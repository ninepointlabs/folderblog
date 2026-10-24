//! `folderblog podcast setup`: connect a blog's podcast to Cloudflare R2, step by step,
//! in plain words. Works for someone with no Cloudflare account yet, and also runs
//! without questions (`--token-from-stdin` or `--secret-from-stdin`) for scripts and
//! agents. Secrets are only ever read with no echo or from stdin, never from arguments.

use crate::config::Config;
use crate::storage::{self, Credentials, MediaStore, S3Store, Stored};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value as Json, json};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const CF_API: &str = "https://api.cloudflare.com/client/v4";
pub const SIGNUP_URL: &str = "https://dash.cloudflare.com/sign-up";
pub const R2_OVERVIEW_URL: &str = "https://dash.cloudflare.com/?to=/:account/r2/overview";
pub const R2_NEW_BUCKET_URL: &str = "https://dash.cloudflare.com/?to=/:account/r2/new";
pub const R2_TOKENS_URL: &str = "https://dash.cloudflare.com/?to=/:account/r2/api-tokens";
/// A user API token form pre-filled with one permission, "Workers R2 Storage: Edit"
/// (Cloudflare's token template link, documented at
/// developers.cloudflare.com/fundamentals/api/how-to/account-owned-token-template/).
pub const TOKEN_TEMPLATE_URL: &str = "https://dash.cloudflare.com/profile/api-tokens?permissionGroupKeys=%5B%7B%22key%22%3A%22workers_r2%22%2C%22type%22%3A%22edit%22%7D%5D&accountId=*&zoneId=all&name=folderblog%20podcast";

#[derive(Default, Clone)]
pub struct Opts {
    pub account_id: Option<String>,
    pub bucket: Option<String>,
    pub public_url: Option<String>,
    pub access_key_id: Option<String>,
    pub jurisdiction: Option<String>,
    pub domain: Option<String>,
    pub zone_id: Option<String>,
    pub lightning_address: Option<String>,
    pub secret_from_stdin: bool,
    pub token_from_stdin: bool,
    pub show: bool,
    pub no_browser: bool,
}

pub struct Io<'a> {
    pub input: &'a mut dyn BufRead,
    pub out: &'a mut dyn Write,
    /// Read a secret without showing it.
    pub hidden: &'a mut dyn FnMut(&str) -> Result<String>,
    pub open: &'a mut dyn FnMut(&str),
    pub cf_api: String,
    pub config_dir: PathBuf,
    /// Tests point R2 at a local mock.
    pub r2_endpoint: Option<String>,
    /// How long to keep trying the public URL (a new r2.dev address can take a moment).
    pub public_wait: Duration,
    /// GET a public URL: (status, body). Tests answer from a local mock.
    pub public_get: &'a dyn Fn(&str) -> Result<(u16, String)>,
}

macro_rules! say {
    ($io:expr, $($t:tt)*) => {{ let _ = writeln!($io.out, $($t)*); }};
}

fn ask(io: &mut Io, q: &str, default: &str) -> Result<String> {
    if default.is_empty() {
        let _ = write!(io.out, "{q} ");
    } else {
        let _ = write!(io.out, "{q} [{default}] ");
    }
    let _ = io.out.flush();
    let mut line = String::new();
    if io.input.read_line(&mut line)? == 0 {
        bail!("stopped: no answer to {q:?}");
    }
    let a = line.trim();
    Ok(if a.is_empty() { default.to_string() } else { a.to_string() })
}

fn yes(io: &mut Io, q: &str, default_yes: bool) -> Result<bool> {
    let a = ask(io, &format!("{q} ({})", if default_yes { "Y/n" } else { "y/N" }), "")?;
    Ok(match a.to_lowercase().as_str() {
        "" => default_yes,
        a => a.starts_with('y'),
    })
}

fn open(io: &mut Io, url: &str, no_browser: bool) {
    say!(io, "  {url}");
    if !no_browser {
        (io.open)(url);
    }
}

fn read_stdin_secret(io: &mut Io, what: &str) -> Result<String> {
    let mut line = String::new();
    io.input.read_line(&mut line)?;
    let s = line.trim().to_string();
    if s.is_empty() {
        bail!("expected the {what} on stdin");
    }
    Ok(s)
}

/// `<blog>-podcast`, as a valid bucket name.
pub fn default_bucket(blog_name: &str) -> String {
    let mut s: String = blog_name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    s.truncate(52);
    let s = s.trim_matches('-').to_string();
    format!("{}-podcast", if s.is_empty() { "blog" } else { &s })
}

fn valid_bucket(b: &str) -> bool {
    (3..=63).contains(&b.len()) && b.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') && !b.starts_with('-') && !b.ends_with('-')
}

/// `media.<site domain>`.
pub fn default_domain(cfg: &Config) -> String {
    let host = cfg.base_url.split("://").nth(1).unwrap_or("").split('/').next().unwrap_or("");
    format!("media.{}", host.trim_start_matches("www."))
}

// ---------- the Cloudflare API ----------

fn cf(io: &Io, token: &str, method: &str, path: &str, body: Option<Json>, jurisdiction: &str) -> Result<Json> {
    let url = format!("{}{path}", io.cf_api);
    let a: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(Duration::from_secs(30))).build().into();
    let auth = format!("Bearer {token}");
    let jur = if jurisdiction.is_empty() { "default" } else { jurisdiction };
    let resp = match method {
        "GET" => a.get(&url).header("Authorization", &auth).header("cf-r2-jurisdiction", jur).call(),
        "POST" => a.post(&url).header("Authorization", &auth).header("cf-r2-jurisdiction", jur).send_json(body.unwrap_or(json!({}))),
        "PUT" => a.put(&url).header("Authorization", &auth).header("cf-r2-jurisdiction", jur).send_json(body.unwrap_or(json!({}))),
        m => bail!("unsupported method {m}"),
    };
    let mut resp = resp.map_err(|e| anyhow!("could not reach Cloudflare: {e}"))?;
    let status = resp.status().as_u16();
    let j: Json = serde_json::from_str(&resp.body_mut().read_to_string().unwrap_or_default()).unwrap_or(Json::Null);
    if !(200..300).contains(&status) || j["success"] == json!(false) {
        let msgs: Vec<String> = j["errors"]
            .as_array()
            .map(|a| a.iter().map(|e| format!("{} (code {})", e["message"].as_str().unwrap_or("?"), e["code"])).collect())
            .unwrap_or_default();
        bail!("Cloudflare said no (HTTP {status}): {}", if msgs.is_empty() { "no details".into() } else { msgs.join("; ") });
    }
    Ok(j)
}

// ---------- the check ----------

/// PUT a tiny file, HEAD it, GET it through the public URL, DELETE it. Each step is
/// reported in plain words, with what usually fixes it.
pub fn verify(io: &mut Io, store: &S3Store) -> Result<()> {
    let key = format!("folderblog-check-{}.txt", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let body = format!("folderblog storage check {}\n", crate::util::now_rfc3339());
    say!(io, "\nChecking that everything works:");
    let step = |io: &mut Io, what: &str, r: Result<()>, fix: &str| -> Result<()> {
        match r {
            Ok(()) => {
                say!(io, "  ok    {what}");
                Ok(())
            }
            Err(e) => {
                say!(io, "  FAIL  {what}: {e:#}");
                if !fix.is_empty() {
                    say!(io, "        {fix}");
                }
                Err(anyhow!("the check failed at: {what}"))
            }
        }
    };
    let eu = "If the bucket was made in the EU, run setup again with --jurisdiction eu.";
    step(io, "upload a tiny test file", store.put_bytes(&key, body.as_bytes(), "text/plain"), eu)?;
    let head = store.head(&key).and_then(|h| match h {
        Stored::Size(n) if n == body.len() as u64 => Ok(()),
        Stored::Size(n) => Err(anyhow!("it is {n} bytes, expected {}", body.len())),
        // It's there; the public download below checks the contents.
        Stored::UnknownSize => Ok(()),
        Stored::Missing => Err(anyhow!("it isn't there")),
    });
    if let Err(e) = step(io, "find it again", head, eu) {
        let _ = store.delete(&key);
        return Err(e);
    }
    let url = store.public_url(&key);
    let started = std::time::Instant::now();
    let public = loop {
        let r = (io.public_get)(&url).and_then(|(s, text)| match s {
            200 if text == body => Ok(()),
            200 => Err(anyhow!("got a different file back")),
            s => Err(anyhow!("HTTP {s}")),
        });
        if r.is_ok() || started.elapsed() >= io.public_wait {
            break r;
        }
        std::thread::sleep(Duration::from_secs(3).min(io.public_wait));
    };
    let public_fix = "The bucket isn't public at that address yet. In the Cloudflare dashboard, open the bucket, then Settings: turn on the Public Development URL (r2.dev), or connect your custom domain. Then run setup again.";
    let public = step(io, &format!("download it from {url}"), public, public_fix);
    // Setup never comes back for it, so say where it is.
    let leftover = format!("The test file {key} is still in the bucket; you can delete it in your storage dashboard.");
    let del = step(io, "remove the test file", store.delete(&key), &leftover);
    public.and(del)
}

// ---------- writing the settings ----------

fn write_storage(blog: &Path, kind: &str, account_id: &str, bucket: &str, jurisdiction: &str, public_url: &str, lightning: Option<&str>) -> Result<()> {
    let path = blog.join(crate::config::CONFIG_FILE);
    let text = std::fs::read_to_string(&path)?;
    let mut doc: toml_edit::DocumentMut = text.parse().context("blog.toml does not parse")?;
    let pod = doc["podcast"].or_insert(toml_edit::table());
    if let Some(l) = lightning {
        pod["lightning_address"] = toml_edit::value(l);
    }
    let st = pod["storage"].or_insert(toml_edit::table());
    st["type"] = toml_edit::value(kind);
    st["account_id"] = toml_edit::value(account_id);
    st["bucket"] = toml_edit::value(bucket);
    if jurisdiction.is_empty() {
        if let Some(t) = st.as_table_mut() {
            t.remove("jurisdiction");
        }
    } else {
        st["jurisdiction"] = toml_edit::value(jurisdiction);
    }
    st["public_url"] = toml_edit::value(public_url);
    let out = doc.to_string();
    Config::parse(&out).context("blog.toml would not parse after the change; edit [podcast.storage] by hand")?;
    std::fs::write(&path, out)?;
    Ok(())
}

pub fn show(io: &mut Io, cfg: &Config) -> Result<i32> {
    let Some(st) = &cfg.podcast.storage else {
        say!(io, "audio hosting is not set up; run `folderblog podcast setup`");
        return Ok(0);
    };
    let kind = match st.kind.as_str() {
        "r2" => "Cloudflare R2",
        "s3" => "S3-compatible storage",
        _ => "files you upload yourself",
    };
    say!(io, "audio host: {kind}");
    if !st.account_id.is_empty() {
        say!(io, "account id: {}", st.account_id);
    }
    if !st.bucket.is_empty() {
        say!(io, "bucket: {}", st.bucket);
    }
    if !st.jurisdiction.is_empty() {
        say!(io, "jurisdiction: {}", st.jurisdiction);
    }
    say!(io, "public url: {}", st.public_url);
    if !st.kind.is_empty() {
        match storage::load_credentials(&io.config_dir, &st.kind, &st.bucket) {
            Ok(c) => say!(io, "access key id: {}\nsecret: set", storage::mask(&c.access_key_id)),
            Err(_) => say!(io, "key: not saved on this computer; {}", storage::key_hint(&io.config_dir, &st.kind, &st.bucket)),
        }
    }
    if st.public_url.contains(".r2.dev") {
        say!(io, "note: r2.dev addresses are rate-limited and Cloudflare says they are not for production; a custom domain is better once the show grows");
    }
    Ok(0)
}

// ---------- the flow ----------

pub fn run(blog: &Path, o: Opts, io: &mut Io) -> Result<i32> {
    let cfg = Config::load(blog)?;
    if o.show {
        return show(io, &cfg);
    }
    let scripted = o.secret_from_stdin || o.token_from_stdin;
    let blog_name = blog.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let jurisdiction = o.jurisdiction.clone().unwrap_or_default();
    if !matches!(jurisdiction.as_str(), "" | "eu" | "fedramp") {
        bail!("--jurisdiction must be eu or fedramp");
    }
    let nb = o.no_browser;

    if !scripted {
        say!(io, "Let's set up hosting for your podcast's audio, on Cloudflare R2.");
        say!(io, "Podcast apps download each episode from there. Your recordings stay in your blog");
        say!(io, "folder; R2 only holds copies, which folderblog uploads when it publishes.\n");
        say!(io, "Step 1 of 5: a Cloudflare account");
        if !yes(io, "Do you have a Cloudflare account?", false)? {
            say!(io, "Opening the sign-up page. Make an account, then click the link in the email Cloudflare sends you.");
            open(io, SIGNUP_URL, nb);
            ask(io, "Press Enter once your email is verified.", "")?;
        }
        say!(io, "\nStep 2 of 5: turn on R2");
        say!(io, "R2 is free for up to 10 GB stored each month, and downloads are always free.");
        say!(io, "Cloudflare does ask for a card or PayPal to turn R2 on, even on the free plan.");
        say!(io, "Opening R2. If it asks, add a payment method and turn R2 on.");
        open(io, R2_OVERVIEW_URL, nb);
        ask(io, "Press Enter once you see the R2 Overview page.", "")?;
    }

    // Which path: one token that lets folderblog do the rest, or everything by hand.
    let quick = if o.token_from_stdin {
        true
    } else if o.secret_from_stdin {
        false
    } else {
        say!(io, "\nStep 3 of 5: connect folderblog");
        say!(io, "  1. Quick (recommended): you make one API token and folderblog does the rest.");
        say!(io, "     That token can manage every R2 bucket in your Cloudflare account.");
        say!(io, "  2. Careful: you make the bucket and a key for just that bucket yourself. A few more clicks.");
        ask(io, "Which one?", "1")? != "2"
    };

    let account_id = match &o.account_id {
        Some(a) => a.clone(),
        None if scripted => bail!("--account-id is needed"),
        None => {
            say!(io, "Your Account ID is on the R2 Overview page, under Account Details on the right.");
            ask(io, "Account ID:", "")?
        }
    };
    if account_id.is_empty() || !account_id.chars().all(|c| c.is_ascii_alphanumeric()) {
        bail!("that doesn't look like a Cloudflare Account ID (32 letters and digits)");
    }
    let suggested = default_bucket(&blog_name);
    let bucket = match &o.bucket {
        Some(b) => b.clone(),
        None if scripted => suggested,
        None => ask(io, "Bucket name:", &suggested)?,
    };
    if !valid_bucket(&bucket) {
        bail!("bucket names are 3 to 63 lowercase letters, digits and dashes (got {bucket:?})");
    }

    let (creds, public_url) = if quick {
        let token = if o.token_from_stdin {
            read_stdin_secret(io, "API token")?
        } else {
            say!(io, "Opening a ready-made token form (permission: Workers R2 Storage, Edit).");
            say!(io, "Click Continue to summary, then Create Token, and copy the token.");
            open(io, TOKEN_TEMPLATE_URL, nb);
            (io.hidden)("Paste the token (it won't show):")?
        };
        let v = cf(io, &token, "GET", "/user/tokens/verify", None, "").map_err(|e| anyhow!("Cloudflare didn't accept that token: {e:#}"))?;
        let id = v["result"]["id"].as_str().context("Cloudflare didn't say which token this is")?.to_string();
        // R2's S3 keys for a token: its id, and the SHA-256 of its value.
        let creds = Credentials { access_key_id: id, secret_access_key: storage::sha256_hex(token.trim().as_bytes()) };
        match cf(io, &token, "POST", &format!("/accounts/{account_id}/r2/buckets"), Some(json!({ "name": bucket })), &jurisdiction) {
            Ok(_) => say!(io, "  made the bucket {bucket}"),
            Err(e) if format!("{e:#}").to_lowercase().contains("already exists") => say!(io, "  the bucket {bucket} already exists; using it"),
            Err(e) => return Err(e.context(format!("making the bucket {bucket}"))),
        }
        let domain = match (&o.domain, scripted) {
            (Some(d), _) => Some(d.clone()),
            (None, true) => None,
            (None, false) => {
                say!(io, "\nWhere should listeners download from?");
                say!(io, "Your own address, like {}, is best for a real show. It needs that domain", default_domain(&cfg));
                say!(io, "to be on Cloudflare in this account. Or start with a free r2.dev address: fine for");
                say!(io, "getting going, but rate-limited, and Cloudflare says it isn't meant for production.");
                say!(io, "You can switch later; podcast apps keep following the show.");
                if yes(io, "Use your own domain?", false)? { Some(ask(io, "Address:", &default_domain(&cfg))?) } else { None }
            }
        };
        let public_url = match domain {
            Some(d) => {
                let zone = match (&o.zone_id, scripted) {
                    (Some(z), _) => z.clone(),
                    (None, true) => bail!("--zone-id is needed with --domain (it's on the domain's Overview page in Cloudflare)"),
                    (None, false) => ask(io, "Zone ID of that domain (on its Overview page in Cloudflare):", "")?,
                };
                cf(io, &token, "POST", &format!("/accounts/{account_id}/r2/buckets/{bucket}/domains/custom"), Some(json!({ "domain": d, "zoneId": zone, "enabled": true })), &jurisdiction)
                    .context("connecting the domain")?;
                say!(io, "  connected {d}");
                format!("https://{d}")
            }
            None => {
                let r = cf(io, &token, "PUT", &format!("/accounts/{account_id}/r2/buckets/{bucket}/domains/managed"), Some(json!({ "enabled": true })), &jurisdiction)
                    .context("turning on the r2.dev address")?;
                let d = r["result"]["domain"].as_str().context("Cloudflare didn't return the r2.dev address")?;
                say!(io, "  turned on {d}");
                format!("https://{d}")
            }
        };
        (creds, public_url)
    } else {
        if !scripted {
            say!(io, "\nMake the bucket: click Create bucket, name it {bucket}, and create it.");
            open(io, R2_NEW_BUCKET_URL, nb);
            say!(io, "Then make it public: open the bucket, go to Settings, and either turn on the");
            say!(io, "Public Development URL (r2.dev) or connect your own domain under Custom Domains.");
            ask(io, "Press Enter when that's done.", "")?;
            say!(io, "\nNow a key for just this bucket: click Create API token, choose Object Read & Write,");
            say!(io, "pick Apply to specific buckets only, choose {bucket}, and create it.");
            open(io, R2_TOKENS_URL, nb);
        }
        let public_url = match &o.public_url {
            Some(u) => u.clone(),
            None if scripted => bail!("--public-url is needed"),
            None => ask(io, "The bucket's public address (https://pub-….r2.dev or your domain):", "")?,
        };
        let public_url = if public_url.starts_with("https://") { public_url } else { format!("https://{public_url}") };
        let id = match &o.access_key_id {
            Some(k) => k.clone(),
            None if scripted => bail!("--access-key-id is needed"),
            None => ask(io, "Access Key ID:", "")?,
        };
        let secret = if o.secret_from_stdin { read_stdin_secret(io, "Secret Access Key")? } else { (io.hidden)("Secret Access Key (it won't show):")? };
        (Credentials { access_key_id: id, secret_access_key: secret }, public_url)
    };
    let public_url = public_url.trim_end_matches('/').to_string();

    let lightning = match (&o.lightning_address, scripted) {
        (Some(l), _) => Some(l.trim().to_string()).filter(|l| !l.is_empty()),
        (None, true) => None,
        (None, false) => {
            say!(io, "\nStep 4 of 5: tips (optional)");
            say!(io, "Listeners in apps like Fountain can send small Bitcoin tips over Lightning while they");
            say!(io, "listen. New to Lightning? Rizful (https://rizful.com) is an easy way to get an address.");
            Some(ask(io, "Lightning address for tips? (optional, Enter to skip)", "")?).filter(|l| !l.is_empty())
        }
    };
    if let Some(l) = &lightning
        && !crate::podcast::is_lightning_address(l)
    {
        bail!("{l:?} isn't a Lightning address; they look like name@rizful.com");
    }

    if !scripted {
        say!(io, "\nStep 5 of 5: check");
    }
    let endpoint = io.r2_endpoint.clone().unwrap_or_else(|| storage::r2_endpoint(&account_id, &jurisdiction));
    let store = S3Store::new(&endpoint, &bucket, storage::R2_REGION, creds.clone(), &public_url);
    verify(io, &store).map_err(|e| anyhow!("{e:#}; nothing was saved, so you can run setup again"))?;

    let cred_path = storage::save_credentials(&io.config_dir, &bucket, &creds)?;
    write_storage(blog, "r2", &account_id, &bucket, &jurisdiction, &public_url, lightning.as_deref())?;
    say!(io, "\nAll set. Episodes are uploaded to {bucket} when the blog publishes.");
    say!(io, "  settings: [podcast.storage] in {}", blog.join(crate::config::CONFIG_FILE).display());
    say!(io, "  key: {} (only you can read it; access key {})", cred_path.display(), storage::mask(&creds.access_key_id));
    if public_url.contains(".r2.dev") {
        say!(io, "\nA note on r2.dev: it's fine to start with, but it is rate-limited and Cloudflare says");
        say!(io, "it isn't for production. When the show grows, connect your own domain and run setup");
        say!(io, "again. Episode guids and the show's guid stay the same, so podcast apps keep following.");
    }
    Ok(0)
}
