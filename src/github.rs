//! GitHub: device-flow login, token storage and renewal, and repo/Pages setup.
//!
//! folderblog authenticates as a GitHub App user (device flow, no client secret).
//! Access tokens last 8 hours and are renewed automatically with a refresh token that
//! lasts about six months. Refresh tokens are single-use, so every read-and-maybe-renew
//! happens under an exclusive file lock shared by all folderblog processes.

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The folderblog GitHub App (public client ID; device flow needs no secret).
pub const DEFAULT_CLIENT_ID: &str = "Iv23liotOZjRQl8Wv1tJ";
const API: &str = "https://api.github.com";
/// Renew when the access token has less than this left.
const RENEW_MARGIN: u64 = 10 * 60;
/// Warn this long before the login itself (the refresh token) runs out.
pub const WARN_BEFORE: u64 = 7 * 24 * 3600;

pub fn client_id() -> String {
    std::env::var("FOLDERBLOG_GITHUB_CLIENT_ID").unwrap_or_else(|_| DEFAULT_CLIENT_ID.to_string())
}

pub fn config_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("FOLDERBLOG_CONFIG_DIR") {
        return PathBuf::from(d);
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    base.join("folderblog")
}

fn token_path() -> PathBuf {
    config_dir().join("github.json")
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

/// A login problem the user has to fix by logging in again. Deploy failures caused by
/// this are reported (and notified) differently from ordinary failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthProblem {
    NotLoggedIn,
    Expired,
    Rejected(String),
}

impl std::fmt::Display for AuthProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthProblem::NotLoggedIn => write!(f, "not logged in to GitHub; run `folderblog login github`"),
            AuthProblem::Expired => write!(f, "GitHub login expired; run `folderblog login github`"),
            AuthProblem::Rejected(why) => {
                write!(f, "GitHub rejected folderblog's login ({why}); run `folderblog login github`")
            }
        }
    }
}
impl std::error::Error for AuthProblem {}

/// Find an AuthProblem anywhere in an error chain.
pub fn auth_problem(e: &anyhow::Error) -> Option<AuthProblem> {
    e.chain().find_map(|c| c.downcast_ref::<AuthProblem>().cloned())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub client_id: String,
    pub login: String,
    pub access_token: String,
    pub access_expires_at: u64,
    pub refresh_token: String,
    pub refresh_expires_at: u64,
}

impl Token {
    pub fn login_days_left(&self) -> i64 {
        (self.refresh_expires_at as i64 - now() as i64) / 86400
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    refresh_token_expires_in: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
    interval: Option<u64>,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into()
}

fn post_form(url: &str, form: &[(&str, &str)]) -> Result<TokenResponse> {
    let mut r = agent()
        .post(url)
        .header("Accept", "application/json")
        .send_form(form.iter().copied())
        .with_context(|| format!("contacting {url}"))?;
    r.body_mut().read_json().context("reading GitHub's response")
}

fn token_from(resp: TokenResponse, client_id: &str, login: String) -> Result<Token> {
    let t = now();
    Ok(Token {
        client_id: client_id.to_string(),
        login,
        access_token: resp.access_token.context("no access token in response")?,
        access_expires_at: t + resp.expires_in.unwrap_or(8 * 3600),
        refresh_token: resp.refresh_token.context(
            "GitHub returned no refresh token; enable \"Expire user authorization tokens\" on the GitHub App",
        )?,
        refresh_expires_at: t + resp.refresh_token_expires_in.unwrap_or(180 * 86400),
    })
}

fn save(tok: &Token) -> Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join("github.json.tmp");
    {
        use std::io::Write;
        #[cfg(unix)]
        use std::os::unix::fs::OpenOptionsExt;
        let mut o = OpenOptions::new();
        o.write(true).create(true).truncate(true);
        #[cfg(unix)]
        o.mode(0o600);
        let mut f = o.open(&tmp)?;
        f.write_all(serde_json::to_string_pretty(tok)?.as_bytes())?;
    }
    std::fs::rename(tmp, token_path())?;
    Ok(())
}

pub fn load() -> Option<Token> {
    std::fs::read_to_string(token_path()).ok().and_then(|t| serde_json::from_str(&t).ok())
}

/// Run `f` while holding the token lock (shared by every folderblog process).
fn with_lock<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let lock = OpenOptions::new().create(true).truncate(false).write(true).open(dir.join("github.lock"))?;
    lock.lock()?;
    let r = f();
    let _ = lock.unlock();
    r
}

/// Interactive device-flow login. `show` is called with (verification URL, user code).
pub fn login(show: impl Fn(&str, &str)) -> Result<Token> {
    let cid = client_id();
    #[derive(Deserialize)]
    struct Device {
        device_code: String,
        user_code: String,
        verification_uri: String,
        expires_in: u64,
        interval: u64,
    }
    let mut r = agent()
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .send_form([("client_id", cid.as_str())])
        .context("contacting GitHub")?;
    let body = r.body_mut().read_to_string()?;
    let d: Device = serde_json::from_str(&body).map_err(|_| {
        anyhow!("GitHub refused to start a login: {body}\n(is Device Flow enabled on the GitHub App?)")
    })?;
    show(&d.verification_uri, &d.user_code);
    let deadline = now() + d.expires_in;
    let mut interval = d.interval.max(1);
    while now() < deadline {
        std::thread::sleep(Duration::from_secs(interval));
        let resp = post_form(
            "https://github.com/login/oauth/access_token",
            &[
                ("client_id", cid.as_str()),
                ("device_code", d.device_code.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ],
        )?;
        match resp.error.as_deref() {
            None => {
                let mut tok = token_from(resp, &cid, String::new())?;
                let user = api_with(&tok.access_token, "GET", "/user", None)?;
                tok.login = user.1["login"].as_str().unwrap_or_default().to_string();
                with_lock(|| save(&tok))?;
                return Ok(tok);
            }
            Some("authorization_pending") => {}
            Some("slow_down") => interval = resp.interval.unwrap_or(interval + 5),
            Some("expired_token") => bail!("the login code expired before it was approved; run the command again"),
            Some("access_denied") => bail!("the login was cancelled on GitHub"),
            Some(other) => bail!("GitHub login failed: {other}: {}", resp.error_description.unwrap_or_default()),
        }
    }
    bail!("the login code expired before it was approved; run the command again")
}

pub fn logout() -> Result<bool> {
    with_lock(|| match std::fs::remove_file(token_path()) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    })
}

/// A usable access token, renewing it first if it is about to expire.
pub fn access_token() -> Result<Token> {
    with_lock(|| {
        let tok = load().ok_or(AuthProblem::NotLoggedIn)?;
        let t = now();
        if tok.access_expires_at > t + RENEW_MARGIN {
            return Ok(tok);
        }
        if tok.refresh_expires_at <= t {
            return Err(AuthProblem::Expired.into());
        }
        let resp = post_form(
            "https://github.com/login/oauth/access_token",
            &[
                ("client_id", tok.client_id.as_str()),
                ("grant_type", "refresh_token"),
                ("refresh_token", tok.refresh_token.as_str()),
            ],
        )?;
        if let Some(err) = resp.error {
            return Err(match err.as_str() {
                "bad_refresh_token" => AuthProblem::Expired.into(),
                _ => AuthProblem::Rejected(format!("{err}: {}", resp.error_description.unwrap_or_default())).into(),
            });
        }
        let fresh = token_from(resp, &tok.client_id, tok.login.clone())?;
        save(&fresh)?;
        Ok(fresh)
    })
}

fn api_with(token: &str, method: &str, path: &str, body: Option<Json>) -> Result<(u16, Json)> {
    let url = format!("{API}{path}");
    let a = agent();
    let auth = format!("Bearer {token}");
    macro_rules! hdrs {
        ($r:expr) => {
            $r.header("Authorization", &auth)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "folderblog")
        };
    }
    let mut resp = match (method, body) {
        ("GET", _) => hdrs!(a.get(&url)).call(),
        ("DELETE", _) => hdrs!(a.delete(&url)).call(),
        ("POST", b) => hdrs!(a.post(&url)).send_json(b.unwrap_or(json!({}))),
        ("PUT", b) => hdrs!(a.put(&url)).send_json(b.unwrap_or(json!({}))),
        (m, _) => bail!("unsupported method {m}"),
    }
    .with_context(|| format!("contacting GitHub ({method} {path})"))?;
    let status = resp.status().as_u16();
    let text = resp.body_mut().read_to_string().unwrap_or_default();
    let json: Json = serde_json::from_str(&text).unwrap_or(Json::Null);
    if status == 401 {
        return Err(AuthProblem::Rejected(json["message"].as_str().unwrap_or("401").to_string()).into());
    }
    Ok((status, json))
}

pub fn api(method: &str, path: &str, body: Option<Json>) -> Result<(u16, Json)> {
    let tok = access_token()?;
    api_with(&tok.access_token, method, path, body)
}

fn fail(what: &str, status: u16, j: &Json) -> anyhow::Error {
    anyhow!("GitHub: {what} failed ({status}): {}", j["message"].as_str().unwrap_or(&j.to_string()))
}

/// Check the login works (renewing if needed). Returns the token so callers can warn
/// before the login runs out.
pub fn check() -> Result<Token> {
    let tok = access_token()?;
    let (s, j) = api_with(&tok.access_token, "GET", "/user", None)?;
    if s != 200 {
        return Err(fail("checking login", s, &j));
    }
    Ok(tok)
}

/// Make sure the repo exists (creating it if not). Idempotent.
pub fn ensure_repo(owner: &str, repo: &str, private: bool, description: &str) -> Result<bool> {
    let (s, j) = api("GET", &format!("/repos/{owner}/{repo}"), None)?;
    match s {
        200 => Ok(false),
        404 => {
            let tok = load().ok_or(AuthProblem::NotLoggedIn)?;
            let body = json!({"name": repo, "description": description, "private": private, "has_issues": false, "has_wiki": false});
            let (s, j) = if tok.login.eq_ignore_ascii_case(owner) {
                api("POST", "/user/repos", Some(body))?
            } else {
                api("POST", &format!("/orgs/{owner}/repos"), Some(body))?
            };
            if s != 201 {
                return Err(fail(&format!("creating repo {owner}/{repo}"), s, &j));
            }
            Ok(true)
        }
        _ => Err(fail(&format!("looking up repo {owner}/{repo}"), s, &j)),
    }
}

/// Make sure Pages serves `branch`, with the custom domain (or none). Idempotent.
pub fn ensure_pages(owner: &str, repo: &str, branch: &str, domain: Option<&str>) -> Result<bool> {
    let path = format!("/repos/{owner}/{repo}/pages");
    let (s, j) = api("GET", &path, None)?;
    let mut enabled = false;
    let current = match s {
        200 => j,
        404 => {
            let (s, j) = api("POST", &path, Some(json!({"build_type": "legacy", "source": {"branch": branch, "path": "/"}})))?;
            enabled = s == 201;
            // GitHub turns Pages on by itself when a gh-pages branch first appears, and a
            // request racing that returns 409 or even 500. Give it a moment and look again.
            let mut found = None;
            for attempt in 0..10 {
                let (gs, gj) = api("GET", &path, None)?;
                if gs == 200 {
                    found = Some(gj);
                    break;
                }
                if attempt == 9 {
                    return Err(fail("enabling Pages", s, &j));
                }
                std::thread::sleep(Duration::from_secs(3));
            }
            found.unwrap()
        }
        _ => return Err(fail("reading Pages settings", s, &j)),
    };
    let want_cname = domain.unwrap_or("");
    let have_cname = current["cname"].as_str().unwrap_or("");
    let have_branch = current["source"]["branch"].as_str().unwrap_or("");
    if have_branch != branch || have_cname != want_cname {
        let body = json!({
            "source": {"branch": branch, "path": "/"},
            "cname": if want_cname.is_empty() { Json::Null } else { Json::from(want_cname) },
        });
        let (s, j) = api("PUT", &path, Some(body))?;
        if s != 204 && s != 200 {
            return Err(fail("updating Pages settings", s, &j));
        }
    }
    // HTTPS can only be enforced once GitHub has issued a certificate; keep trying on later deploys.
    if !current["https_enforced"].as_bool().unwrap_or(false) {
        let _ = api("PUT", &path, Some(json!({"https_enforced": true})));
    }
    Ok(enabled)
}

/// Latest Pages build status, for `folderblog status`.
pub fn pages_status(owner: &str, repo: &str) -> Result<String> {
    let (s, j) = api("GET", &format!("/repos/{owner}/{repo}/pages/builds/latest"), None)?;
    if s != 200 {
        return Ok(format!("no Pages build yet ({s})"));
    }
    let st = j["status"].as_str().unwrap_or("unknown");
    Ok(match j["error"]["message"].as_str() {
        Some(m) if !m.is_empty() => format!("{st}: {m}"),
        _ => st.to_string(),
    })
}

/// git config (passed via environment, so the token never appears in argv or .git/config)
/// that authenticates HTTPS pushes to github.com with `token` and ignores other helpers.
pub fn git_env(token: &str) -> Vec<(String, String)> {
    use base64_lite::encode;
    let basic = encode(format!("x-access-token:{token}").as_bytes());
    vec![
        ("GIT_CONFIG_COUNT".into(), "2".into()),
        ("GIT_CONFIG_KEY_0".into(), "credential.helper".into()),
        ("GIT_CONFIG_VALUE_0".into(), String::new()),
        ("GIT_CONFIG_KEY_1".into(), "http.https://github.com/.extraheader".into()),
        ("GIT_CONFIG_VALUE_1".into(), format!("Authorization: Basic {basic}")),
        ("GIT_TERMINAL_PROMPT".into(), "0".into()),
    ]
}

/// Repo names GitHub accepts: letters, digits, `.`, `-`, `_`.
pub fn repo_name(s: &str) -> String {
    let n: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || ".-_".contains(c) { c } else { '-' }).collect();
    n.trim_matches('-').to_string()
}

/// The URL GitHub Pages serves a repo at, without a custom domain.
pub fn pages_url(owner: &str, repo: &str) -> String {
    let host = format!("{}.github.io", owner.to_lowercase());
    if repo.eq_ignore_ascii_case(&host) { format!("https://{host}") } else { format!("https://{host}/{repo}") }
}

mod base64_lite {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    pub fn encode(b: &[u8]) -> String {
        let mut o = String::new();
        for c in b.chunks(3) {
            let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
            for i in 0..4 {
                if i <= c.len() {
                    o.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    o.push('=');
                }
            }
        }
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64() {
        assert_eq!(base64_lite::encode(b"x-access-token:abc"), "eC1hY2Nlc3MtdG9rZW46YWJj");
        assert_eq!(base64_lite::encode(b"a"), "YQ==");
        assert_eq!(base64_lite::encode(b"ab"), "YWI=");
    }

    #[test]
    fn urls_and_names() {
        assert_eq!(pages_url("NinePointLabs", "blog"), "https://ninepointlabs.github.io/blog");
        assert_eq!(pages_url("ninepointlabs", "ninepointlabs.github.io"), "https://ninepointlabs.github.io");
        assert_eq!(repo_name("my blog!"), "my-blog");
        assert_eq!(repo_name("example.com"), "example.com");
    }

    #[test]
    fn auth_problems_found_in_chains() {
        let e = anyhow::Error::new(AuthProblem::Expired).context("deploying");
        assert_eq!(auth_problem(&e), Some(AuthProblem::Expired));
        assert_eq!(auth_problem(&anyhow!("other")), None);
    }

    #[test]
    fn expired_login_is_reported_without_network() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: tests touching FOLDERBLOG_CONFIG_DIR run in this one test only.
        unsafe { std::env::set_var("FOLDERBLOG_CONFIG_DIR", tmp.path()) };
        assert_eq!(auth_problem(&access_token().unwrap_err()), Some(AuthProblem::NotLoggedIn));
        save(&Token {
            client_id: "x".into(),
            login: "me".into(),
            access_token: "a".into(),
            access_expires_at: 1,
            refresh_token: "r".into(),
            refresh_expires_at: 2,
        })
        .unwrap();
        assert_eq!(auth_problem(&access_token().unwrap_err()), Some(AuthProblem::Expired));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(tmp.path().join("github.json")).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "token file must be private");
        }
        unsafe { std::env::remove_var("FOLDERBLOG_CONFIG_DIR") };
    }
}
