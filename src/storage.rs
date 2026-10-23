//! Where podcast audio is hosted. The blog folder keeps the originals; the media store
//! holds copies that can always be uploaded again. Cloudflare R2 is the default, spoken
//! to through its S3 API, so any S3-compatible host works the same way.
//!
//! Secrets live only in `config_dir()/storage/<bucket>.json` (mode 0600, folder 0700),
//! never in blog.toml, the blog folder, `.blog/`, logs or error messages.

use crate::config::StorageConfig;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// What a HEAD found. Any 2xx means the file is there, even when the reply has no size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stored {
    Missing,
    Size(u64),
    /// There, but the reply carried no Content-Length.
    UnknownSize,
}

impl Stored {
    fn from_head(status: u16, content_length: Option<&str>) -> Stored {
        match (status, content_length.and_then(|l| l.trim().parse().ok())) {
            (200..=299, Some(n)) => Stored::Size(n),
            (200..=299, None) => Stored::UnknownSize,
            _ => Stored::Missing,
        }
    }
}

pub trait MediaStore {
    /// Upload `file` as `key`. Large files go up in parts.
    fn put(&self, key: &str, file: &Path, content_type: &str) -> Result<()>;
    /// Whether `key` is stored, and its size when the host says.
    fn head(&self, key: &str) -> Result<Stored>;
    fn delete(&self, key: &str) -> Result<()>;
    /// Where listeners download `key` from.
    fn public_url(&self, key: &str) -> String;
}

// ---------- credentials ----------

#[derive(Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Credentials {{ access_key_id: {}, secret: set }}", mask(&self.access_key_id))
    }
}

/// Only the last 4 characters, for showing which key is in use.
pub fn mask(id: &str) -> String {
    let n = id.chars().count();
    if n <= 4 { "…".into() } else { format!("…{}", id.chars().skip(n - 4).collect::<String>()) }
}

pub const ENV_KEY_ID: &str = "FOLDERBLOG_STORAGE_KEY_ID";
pub const ENV_SECRET: &str = "FOLDERBLOG_STORAGE_SECRET";

pub fn credentials_path(config_dir: &Path, bucket: &str) -> PathBuf {
    config_dir.join("storage").join(format!("{bucket}.json"))
}

pub fn save_credentials(config_dir: &Path, bucket: &str, c: &Credentials) -> Result<PathBuf> {
    let dir = config_dir.join("storage");
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let path = credentials_path(config_dir, bucket);
    let tmp = path.with_extension("json.tmp");
    {
        use std::io::Write;
        #[cfg(unix)]
        use std::os::unix::fs::OpenOptionsExt;
        let mut o = std::fs::OpenOptions::new();
        o.write(true).create(true).truncate(true);
        #[cfg(unix)]
        o.mode(0o600);
        let mut f = o.open(&tmp)?;
        f.write_all(serde_json::to_string_pretty(c)?.as_bytes())?;
    }
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// `FOLDERBLOG_STORAGE_KEY_ID` + `FOLDERBLOG_STORAGE_SECRET` (for CI), else the saved file.
pub fn load_credentials(config_dir: &Path, bucket: &str) -> Result<Credentials> {
    if let (Ok(id), Ok(secret)) = (std::env::var(ENV_KEY_ID), std::env::var(ENV_SECRET))
        && !id.is_empty()
        && !secret.is_empty()
    {
        return Ok(Credentials { access_key_id: id, secret_access_key: secret });
    }
    let path = credentials_path(config_dir, bucket);
    let text = std::fs::read_to_string(&path)
        .map_err(|_| anyhow!("no saved key for the bucket {bucket:?}; run `folderblog podcast setup` to connect it"))?;
    serde_json::from_str(&text).map_err(|_| anyhow!("{} is damaged; run `folderblog podcast setup` again", path.display()))
}

// ---------- AWS Signature Version 4 ----------

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn sha256_hex(b: &[u8]) -> String {
    hex(&Sha256::digest(b))
}

pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(k.iter().map(|b| b ^ 0x36).collect::<Vec<u8>>());
    inner.update(msg);
    let mut outer = Sha256::new();
    outer.update(k.iter().map(|b| b ^ 0x5c).collect::<Vec<u8>>());
    outer.update(inner.finalize());
    outer.finalize().to_vec()
}

/// RFC 3986 encoding as SigV4 wants it; `/` is kept in paths.
pub fn uri_encode(s: &str, keep_slash: bool) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => o.push(b as char),
            b'/' if keep_slash => o.push('/'),
            _ => o.push_str(&format!("%{b:02X}")),
        }
    }
    o
}

pub struct SignInput<'a> {
    pub method: &'a str,
    /// Already encoded, e.g. `/bucket/a%20b.mp3`.
    pub canonical_uri: &'a str,
    /// Unencoded pairs.
    pub query: &'a [(String, String)],
    /// Lower-case names. Must include `host`, `x-amz-date` and `x-amz-content-sha256`.
    pub headers: &'a [(String, String)],
    pub payload_hash: &'a str,
    /// `YYYYMMDDTHHMMSSZ`
    pub amz_date: &'a str,
    pub region: &'a str,
    pub service: &'a str,
}

/// The `Authorization` header value for a request.
pub fn sign_v4(i: &SignInput, creds: &Credentials) -> String {
    let mut q: Vec<(String, String)> = i.query.iter().map(|(k, v)| (uri_encode(k, false), uri_encode(v, false))).collect();
    q.sort();
    let query = q.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&");
    let mut h: Vec<(String, String)> = i.headers.iter().map(|(k, v)| (k.to_lowercase(), v.split_whitespace().collect::<Vec<_>>().join(" "))).collect();
    h.sort();
    let canonical_headers: String = h.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    let signed = h.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(";");
    let canonical = format!("{}\n{}\n{query}\n{canonical_headers}\n{signed}\n{}", i.method, i.canonical_uri, i.payload_hash);
    let date = &i.amz_date[..8];
    let scope = format!("{date}/{}/{}/aws4_request", i.region, i.service);
    let to_sign = format!("AWS4-HMAC-SHA256\n{}\n{scope}\n{}", i.amz_date, sha256_hex(canonical.as_bytes()));
    let k = hmac_sha256(format!("AWS4{}", creds.secret_access_key).as_bytes(), date.as_bytes());
    let k = hmac_sha256(&k, i.region.as_bytes());
    let k = hmac_sha256(&k, i.service.as_bytes());
    let k = hmac_sha256(&k, b"aws4_request");
    let sig = hex(&hmac_sha256(&k, to_sign.as_bytes()));
    format!("AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed}, Signature={sig}", creds.access_key_id)
}

// ---------- S3 / R2 ----------

pub const R2_REGION: &str = "auto";
/// Files larger than this go up in parts (S3 allows up to 5 GB in one PUT; parts are kinder to bad connections).
pub const MULTIPART_THRESHOLD: u64 = 100 * 1024 * 1024;
pub const PART_SIZE: u64 = 32 * 1024 * 1024;

/// The S3 API endpoint for an R2 account (`eu`/`fedramp` buckets have their own).
pub fn r2_endpoint(account_id: &str, jurisdiction: &str) -> String {
    match jurisdiction {
        "" | "default" => format!("https://{account_id}.r2.cloudflarestorage.com"),
        j => format!("https://{account_id}.{j}.r2.cloudflarestorage.com"),
    }
}

pub struct S3Store {
    /// `scheme://host[:port]`, no trailing slash.
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub creds: Credentials,
    pub public_base: String,
    pub multipart_threshold: u64,
    pub part_size: u64,
    agent: ureq::Agent,
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(timeout)).build().into()
}

/// A short, plain description of an S3 error response, with what usually fixes it.
/// Never includes request details, so it can't leak keys.
fn s3_error(what: &str, status: u16, body: &str, bucket: &str) -> anyhow::Error {
    let tag = |t: &str| {
        let open = format!("<{t}>");
        body.find(&open).and_then(|i| body[i + open.len()..].split(&format!("</{t}>")).next()).map(str::to_string)
    };
    let code = tag("Code").unwrap_or_default();
    let fix = match (status, code.as_str()) {
        (_, "SignatureDoesNotMatch" | "InvalidAccessKeyId") | (401, _) => {
            "the Access Key ID or Secret is wrong; run `folderblog podcast setup` and paste them again".to_string()
        }
        (_, "AccessDenied") | (403, _) => format!("the key isn't allowed to write to {bucket:?}; give its token Object Read & Write on that bucket"),
        (_, "NoSuchBucket") => format!("there is no bucket called {bucket:?} (EU buckets also need jurisdiction = \"eu\" in [podcast.storage])"),
        (s, _) if s >= 500 => "the storage service had a problem; it will be tried again".to_string(),
        _ => tag("Message").unwrap_or_else(|| format!("HTTP {status}")),
    };
    let code = if code.is_empty() { format!("HTTP {status}") } else { format!("{code}, HTTP {status}") };
    anyhow!("{what} failed ({code}): {fix}")
}

impl S3Store {
    pub fn new(endpoint: &str, bucket: &str, region: &str, creds: Credentials, public_base: &str) -> S3Store {
        S3Store {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            bucket: bucket.to_string(),
            region: region.to_string(),
            creds,
            public_base: public_base.trim_end_matches('/').to_string(),
            multipart_threshold: MULTIPART_THRESHOLD,
            part_size: PART_SIZE,
            agent: agent(Duration::from_secs(30 * 60)),
        }
    }

    pub fn from_config(cfg: &StorageConfig, creds: Credentials) -> Result<S3Store> {
        let endpoint = match cfg.kind.as_str() {
            "r2" => r2_endpoint(&cfg.account_id, &cfg.jurisdiction),
            "s3" => cfg.endpoint.clone(),
            other => bail!("storage type {other:?} can't upload; use \"r2\" or \"s3\""),
        };
        let region = if cfg.region.is_empty() { R2_REGION } else { cfg.region.as_str() };
        Ok(S3Store::new(&endpoint, &cfg.bucket, region, creds, &cfg.public_url))
    }

    fn host(&self) -> String {
        self.endpoint.split_once("://").map(|(_, h)| h).unwrap_or(&self.endpoint).to_string()
    }

    /// Path-style: `<endpoint>/<bucket>/<key>`.
    fn request(&self, method: &str, key: &str, query: &[(String, String)], payload_hash: &str, extra: &[(String, String)]) -> (String, Vec<(String, String)>) {
        let uri = format!("/{}/{}", uri_encode(&self.bucket, false), uri_encode(key, true));
        let amz_date = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let mut headers = vec![
            ("host".to_string(), self.host()),
            ("x-amz-content-sha256".to_string(), payload_hash.to_string()),
            ("x-amz-date".to_string(), amz_date.clone()),
        ];
        headers.extend(extra.iter().cloned());
        let auth = sign_v4(
            &SignInput {
                method,
                canonical_uri: &uri,
                query,
                headers: &headers,
                payload_hash,
                amz_date: &amz_date,
                region: &self.region,
                service: "s3",
            },
            &self.creds,
        );
        headers.push(("authorization".to_string(), auth));
        let mut q: Vec<String> = query.iter().map(|(k, v)| if v.is_empty() { uri_encode(k, false) } else { format!("{}={}", uri_encode(k, false), uri_encode(v, false)) }).collect();
        q.sort();
        let url = if q.is_empty() { format!("{}{uri}", self.endpoint) } else { format!("{}{uri}?{}", self.endpoint, q.join("&")) };
        // ureq sets Host itself.
        headers.retain(|(k, _)| k != "host");
        // Unsigned. ureq asks for gzip by default; R2 then gzips text and its HEAD
        // replies lose Content-Length, so a file that is there looks size-less.
        headers.push(("accept-encoding".to_string(), "identity".to_string()));
        (url, headers)
    }

    fn send(&self, method: &str, key: &str, query: &[(String, String)], body: Vec<u8>, extra: &[(String, String)]) -> Result<(u16, BTreeMap<String, String>, String)> {
        let hash = sha256_hex(&body);
        let (url, headers) = self.request(method, key, query, &hash, extra);
        let mut resp = match method {
            "GET" | "HEAD" | "DELETE" => {
                let mut r = match method {
                    "GET" => self.agent.get(&url),
                    "HEAD" => self.agent.head(&url),
                    _ => self.agent.delete(&url),
                };
                for (k, v) in &headers {
                    r = r.header(k, v);
                }
                r.call()
            }
            _ => {
                let mut r = if method == "POST" { self.agent.post(&url) } else { self.agent.put(&url) };
                for (k, v) in &headers {
                    r = r.header(k, v);
                }
                r.header("content-length", &body.len().to_string()).send(&body[..])
            }
        }
        .map_err(|e| anyhow!("could not reach the storage service: {e}"))?;
        let status = resp.status().as_u16();
        let hdrs = resp.headers().iter().map(|(k, v)| (k.as_str().to_lowercase(), v.to_str().unwrap_or("").to_string())).collect();
        let text = if method == "HEAD" { String::new() } else { resp.body_mut().read_to_string().unwrap_or_default() };
        Ok((status, hdrs, text))
    }

    fn put_single(&self, key: &str, file: &Path, content_type: &str, size: u64) -> Result<()> {
        let mut h = Sha256::new();
        let mut f = std::fs::File::open(file).with_context(|| format!("reading {}", file.display()))?;
        let mut buf = vec![0u8; 1 << 16];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        let hash = hex(&h.finalize());
        let extra = vec![("content-type".to_string(), content_type.to_string())];
        let (url, headers) = self.request("PUT", key, &[], &hash, &extra);
        let mut r = self.agent.put(&url);
        for (k, v) in &headers {
            r = r.header(k, v);
        }
        let f = std::fs::File::open(file)?;
        let mut resp = r
            .header("content-length", &size.to_string())
            .send(f)
            .map_err(|e| anyhow!("could not reach the storage service: {e}"))?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            let text = resp.body_mut().read_to_string().unwrap_or_default();
            return Err(s3_error(&format!("uploading {key}"), status, &text, &self.bucket));
        }
        Ok(())
    }

    fn put_multipart(&self, key: &str, file: &Path, content_type: &str) -> Result<()> {
        let what = format!("uploading {key}");
        let ct = vec![("content-type".to_string(), content_type.to_string())];
        let (status, _, text) = self.send("POST", key, &[("uploads".into(), String::new())], vec![], &ct)?;
        if !(200..300).contains(&status) {
            return Err(s3_error(&what, status, &text, &self.bucket));
        }
        let upload_id = text
            .split("<UploadId>")
            .nth(1)
            .and_then(|s| s.split("</UploadId>").next())
            .map(str::to_string)
            .context("the storage service did not start a multipart upload")?;
        let res = (|| -> Result<()> {
            let mut f = std::fs::File::open(file).with_context(|| format!("reading {}", file.display()))?;
            let size = f.metadata()?.len();
            let parts = size.div_ceil(self.part_size).max(1);
            let mut etags = vec![];
            for n in 1..=parts {
                let mut part = Vec::with_capacity(self.part_size.min(size) as usize);
                (&mut f).take(self.part_size).read_to_end(&mut part)?;
                let q = [("partNumber".to_string(), n.to_string()), ("uploadId".to_string(), upload_id.clone())];
                let (status, hdrs, text) = self.send("PUT", key, &q, part, &[])?;
                if !(200..300).contains(&status) {
                    return Err(s3_error(&format!("{what} (part {n} of {parts})"), status, &text, &self.bucket));
                }
                etags.push((n, hdrs.get("etag").cloned().unwrap_or_default()));
            }
            let mut xml = String::from("<CompleteMultipartUpload>");
            for (n, e) in &etags {
                xml.push_str(&format!("<Part><PartNumber>{n}</PartNumber><ETag>{}</ETag></Part>", crate::util::html_escape(e)));
            }
            xml.push_str("</CompleteMultipartUpload>");
            let (status, _, text) = self.send("POST", key, &[("uploadId".into(), upload_id.clone())], xml.into_bytes(), &[])?;
            // S3 can report a failed completion inside a 200 response.
            if !(200..300).contains(&status) || text.contains("<Error>") {
                return Err(s3_error(&what, if status == 200 { 500 } else { status }, &text, &self.bucket));
            }
            Ok(())
        })();
        if res.is_err() {
            let _ = self.send("DELETE", key, &[("uploadId".into(), upload_id)], vec![], &[]);
        }
        res
    }

    pub fn put_bytes(&self, key: &str, body: &[u8], content_type: &str) -> Result<()> {
        let (status, _, text) = self.send("PUT", key, &[], body.to_vec(), &[("content-type".into(), content_type.into())])?;
        if !(200..300).contains(&status) {
            return Err(s3_error(&format!("uploading {key}"), status, &text, &self.bucket));
        }
        Ok(())
    }
}

impl MediaStore for S3Store {
    fn put(&self, key: &str, file: &Path, content_type: &str) -> Result<()> {
        let size = std::fs::metadata(file).with_context(|| format!("reading {}", file.display()))?.len();
        if size > self.multipart_threshold { self.put_multipart(key, file, content_type) } else { self.put_single(key, file, content_type, size) }
    }

    fn head(&self, key: &str) -> Result<Stored> {
        let (status, hdrs, _) = self.send("HEAD", key, &[], vec![], &[])?;
        match status {
            200..=299 | 404 => Ok(Stored::from_head(status, hdrs.get("content-length").map(String::as_str))),
            s => Err(s3_error(&format!("checking {key}"), s, "", &self.bucket)),
        }
    }

    fn delete(&self, key: &str) -> Result<()> {
        let (status, _, text) = self.send("DELETE", key, &[], vec![], &[])?;
        match status {
            200..=299 | 404 => Ok(()),
            s => Err(s3_error(&format!("deleting {key}"), s, &text, &self.bucket)),
        }
    }

    fn public_url(&self, key: &str) -> String {
        format!("{}/{}", self.public_base, uri_encode(key, true))
    }
}

/// Whether a public URL is there (HEAD), and its size when the reply says.
pub fn public_head(url: &str) -> Result<Stored> {
    // identity: a compressed reply would drop Content-Length (see `S3Store::request`).
    let resp = agent(Duration::from_secs(30)).head(url).header("accept-encoding", "identity").call().map_err(|e| anyhow!("could not reach {url}: {e}"))?;
    match resp.status().as_u16() {
        s @ 200..=299 => Ok(Stored::from_head(s, resp.headers().get("content-length").and_then(|v| v.to_str().ok()))),
        404 | 403 => Ok(Stored::Missing),
        s => bail!("{url} answered HTTP {s}"),
    }
}

pub fn public_get(url: &str) -> Result<(u16, String)> {
    let mut resp = agent(Duration::from_secs(30)).get(url).call().map_err(|e| anyhow!("could not reach {url}: {e}"))?;
    Ok((resp.status().as_u16(), resp.body_mut().read_to_string().unwrap_or_default()))
}

// ---------- uploading at deploy time ----------

/// What has been uploaded: key -> (sha256, bytes). Lives in `.blog/`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub uploaded: BTreeMap<String, Uploaded>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Uploaded {
    pub sha256: String,
    pub bytes: u64,
}

pub const MANIFEST: &str = "podcast-uploads.json";

pub fn load_manifest(blog_dir: &Path) -> Manifest {
    std::fs::read_to_string(blog_dir.join(crate::state::STATE_DIR).join(MANIFEST))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_manifest(blog_dir: &Path, m: &Manifest) -> Result<()> {
    let dir = blog_dir.join(crate::state::STATE_DIR);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(MANIFEST);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(m)?)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

/// Make sure every episode file in the last build is on the media host with the right
/// size: upload what is new (the manifest skips the rest), then HEAD each enclosure.
/// Any failure is an error, so the deploy is held and the feed never points at missing audio.
pub fn sync_media(blog_dir: &Path, cfg: &StorageConfig, media: &[crate::podcast::MediaEntry], store: Option<&dyn MediaStore>) -> Result<String> {
    if media.is_empty() {
        return Ok(String::new());
    }
    let held = |e: anyhow::Error| anyhow!("held: {e:#}; the site was not published, so the feed never points at missing audio");
    let Some(store) = store else {
        // No uploads configured: the files must already be at the public URL.
        for m in media {
            let url = format!("{}/{}", cfg.public_url, uri_encode(&m.key, true));
            match public_head(&url).map_err(held)? {
                // There, but the host didn't say how big; nothing to compare.
                Stored::Size(n) if n == m.bytes => {}
                Stored::UnknownSize => {}
                Stored::Size(n) => return Err(held(anyhow!("{url} is {n} bytes but {} is {} bytes; upload it again", m.folder, m.bytes))),
                Stored::Missing => return Err(held(anyhow!("{} is not at {url} yet; upload it there, or run `folderblog podcast setup` so folderblog does it", m.folder))),
            }
        }
        return Ok(format!("{} episode file(s) checked", media.len()));
    };
    let mut manifest = load_manifest(blog_dir);
    let mut uploaded = 0;
    for m in media {
        let want = Uploaded { sha256: m.sha256.clone(), bytes: m.bytes };
        if manifest.uploaded.get(&m.key) == Some(&want) {
            continue;
        }
        let size = std::fs::metadata(&m.file).map(|x| x.len()).map_err(|e| held(anyhow!("{}: {e}", m.file.display())))?;
        if size != m.bytes {
            return Err(held(anyhow!("{} changed since the last build; it will be uploaded after the next build", m.file.display())));
        }
        store.put(&m.key, &m.file, &m.mime).map_err(held)?;
        manifest.uploaded.insert(m.key.clone(), want);
        save_manifest(blog_dir, &manifest)?;
        uploaded += 1;
    }
    for m in media {
        match store.head(&m.key).map_err(held)? {
            Stored::Size(n) if n == m.bytes => {}
            Stored::UnknownSize => {}
            got => {
                // Forget it, so the next deploy uploads it again.
                manifest.uploaded.remove(&m.key);
                save_manifest(blog_dir, &manifest)?;
                let what = match got {
                    Stored::Size(n) => format!("is {n} bytes, not {}", m.bytes),
                    _ => "is missing".into(),
                };
                return Err(held(anyhow!("{} on the media host {what}; it will be uploaded again", m.key)));
            }
        }
    }
    Ok(match uploaded {
        0 => format!("{} episode file(s) already uploaded", media.len()),
        n => format!("uploaded {n} episode file(s)"),
    })
}

/// The deploy-time step: run `sync_media` for a blog's last build, if it has a podcast.
pub fn sync_for_deploy(blog_dir: &Path, cfg: &crate::config::Config, config_dir: &Path) -> Result<Option<String>> {
    if !cfg.podcast.enabled {
        return Ok(None);
    }
    let Some(st) = &cfg.podcast.storage else { return Ok(None) };
    let snap = crate::podcast::load_snapshot(blog_dir, crate::podcast::BUILT_SNAPSHOT).unwrap_or_default();
    if snap.media.is_empty() {
        return Ok(None);
    }
    let store = match st.kind.as_str() {
        "" => None,
        _ => {
            let creds = load_credentials(config_dir, &st.bucket).map_err(|e| anyhow!("held: {e:#}"))?;
            Some(S3Store::from_config(st, creds)?)
        }
    };
    sync_media(blog_dir, st, &snap.media, store.as_ref().map(|s| s as &dyn MediaStore)).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sigv4_matches_the_aws_example() {
        // GET Object example from the AWS S3 SigV4 documentation.
        let creds = Credentials { access_key_id: "AKIAIOSFODNN7EXAMPLE".into(), secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into() };
        let empty = sha256_hex(b"");
        let headers = vec![
            ("host".to_string(), "examplebucket.s3.amazonaws.com".to_string()),
            ("range".to_string(), "bytes=0-9".to_string()),
            ("x-amz-content-sha256".to_string(), empty.clone()),
            ("x-amz-date".to_string(), "20130524T000000Z".to_string()),
        ];
        let auth = sign_v4(
            &SignInput {
                method: "GET",
                canonical_uri: "/test.txt",
                query: &[],
                headers: &headers,
                payload_hash: &empty,
                amz_date: "20130524T000000Z",
                region: "us-east-1",
                service: "s3",
            },
            &creds,
        );
        assert_eq!(
            auth,
            "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }

    #[test]
    fn hmac_matches_rfc_4231() {
        // Test case 2.
        assert_eq!(hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    }

    #[test]
    fn credentials_are_private_and_never_printed() {
        let tmp = tempfile::tempdir().unwrap();
        let c = Credentials { access_key_id: "AKID1234WXYZ".into(), secret_access_key: "canary-secret".into() };
        let p = save_credentials(tmp.path(), "show-podcast", &c).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
            assert_eq!(std::fs::metadata(p.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
        }
        assert_eq!(load_credentials(tmp.path(), "show-podcast").unwrap().secret_access_key, "canary-secret");
        let shown = format!("{c:?}");
        assert!(!shown.contains("canary") && shown.contains("…WXYZ"), "{shown}");
        assert!(load_credentials(tmp.path(), "other").unwrap_err().to_string().contains("podcast setup"));
        assert_eq!(mask("ab"), "…");
    }

    #[test]
    fn uri_encoding() {
        assert_eq!(uri_encode("a b/c+d~.mp3", true), "a%20b/c%2Bd~.mp3");
        assert_eq!(uri_encode("a/b", false), "a%2Fb");
        assert_eq!(r2_endpoint("abc", ""), "https://abc.r2.cloudflarestorage.com");
        assert_eq!(r2_endpoint("abc", "eu"), "https://abc.eu.r2.cloudflarestorage.com");
    }

    #[test]
    fn s3_errors_are_plain_and_hold_no_request_details() {
        let body = "<Error><Code>SignatureDoesNotMatch</Code><Message>x</Message><AWSAccessKeyId>AKIDSECRETISH</AWSAccessKeyId><StringToSign>s</StringToSign></Error>";
        let e = s3_error("uploading k", 403, body, "b").to_string();
        assert!(e.contains("Access Key ID or Secret is wrong") && !e.contains("AKIDSECRETISH"), "{e}");
        assert!(s3_error("x", 404, "<Error><Code>NoSuchBucket</Code></Error>", "b").to_string().contains("no bucket called \"b\""));
    }
}
