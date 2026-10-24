//! Tests only: a local S3 + Cloudflare API stand-in on 127.0.0.1, so storage code is
//! tested without the network. It checks every request's SigV4 signature.

use crate::storage::{Credentials, SignInput, sha256_hex, sign_v4};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

pub const TOKEN: &str = "good-cloudflare-token";
pub const TOKEN_ID: &str = "tokenid0000000000000000000abcd";
pub const R2_DEV: &str = "pub-test.r2.dev";

#[derive(Debug, Clone)]
pub struct Req {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: BTreeMap<String, String>,
    pub body_len: usize,
}

#[derive(Default)]
pub struct State {
    pub objects: HashMap<String, Vec<u8>>,
    pub requests: Vec<Req>,
    /// Answer PUTs with this status instead of storing.
    pub fail_puts: Option<u16>,
    /// Answer object DELETEs with this status instead of deleting.
    pub fail_deletes: Option<u16>,
    /// Store objects shorter than sent (to test HEAD checks).
    pub truncate: bool,
    /// Answer every HEAD as compressed, so the client never sees a Content-Length.
    pub head_without_length: bool,
    parts: HashMap<String, BTreeMap<u32, Vec<u8>>>,
    pub buckets: Vec<String>,
    pub bad_signatures: Vec<String>,
}

pub struct Mock {
    pub url: String,
    pub state: Arc<Mutex<State>>,
}

fn parse_query(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (decode(k), decode(v))
        })
        .collect()
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(x) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(x);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Mock {
    /// `creds` are what S3 requests must be signed with.
    pub fn start(creds: Credentials) -> Mock {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}", server.server_addr().to_ip().unwrap());
        let state = Arc::new(Mutex::new(State::default()));
        let st = state.clone();
        let c = creds.clone();
        std::thread::spawn(move || {
            for mut req in server.incoming_requests() {
                let method = req.method().as_str().to_string();
                let (path, query) = req.url().split_once('?').map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or((req.url().to_string(), String::new()));
                let headers: BTreeMap<String, String> = req.headers().iter().map(|h| (h.field.as_str().as_str().to_lowercase(), h.value.as_str().to_string())).collect();
                let mut body = vec![];
                let _ = req.as_reader().read_to_end(&mut body);
                let (status, out, extra) = handle(&st, &c, &method, &path, &query, &headers, &body);
                st.lock().unwrap().requests.push(Req { method, path, query, headers, body_len: body.len() });
                let mut resp = tiny_http::Response::from_data(out).with_status_code(status);
                for (k, v) in extra {
                    resp = resp.with_header(tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap());
                }
                let _ = req.respond(resp);
            }
        });
        Mock { url, state }
    }

    pub fn requests(&self) -> Vec<Req> {
        self.state.lock().unwrap().requests.clone()
    }
}

fn json(v: serde_json::Value) -> (u16, Vec<u8>, Vec<(String, String)>) {
    (200, v.to_string().into_bytes(), vec![("Content-Type".into(), "application/json".into())])
}

/// Like R2: a client that accepts gzip gets a compressed reply, and ureq then drops
/// Content-Length (along with Content-Encoding) before the caller sees it.
fn head_headers(s: &State, req: &BTreeMap<String, String>, len: usize) -> Vec<(String, String)> {
    // tiny_http drops HEAD bodies but keeps the length header we give.
    let mut h = vec![("Content-Length".to_string(), len.to_string())];
    if s.head_without_length || req.get("accept-encoding").is_some_and(|e| e.contains("gzip")) {
        h.push(("Content-Encoding".into(), "gzip".into()));
    }
    h
}

fn handle(
    st: &Arc<Mutex<State>>,
    creds: &Credentials,
    method: &str,
    path: &str,
    query: &str,
    headers: &BTreeMap<String, String>,
    body: &[u8],
) -> (u16, Vec<u8>, Vec<(String, String)>) {
    // Cloudflare API.
    if let Some(api) = path.strip_prefix("/client/v4") {
        if headers.get("authorization").map(String::as_str) != Some(&format!("Bearer {TOKEN}")) {
            return (401, br#"{"success":false,"errors":[{"code":1000,"message":"Invalid API Token"}]}"#.to_vec(), vec![]);
        }
        let parts: Vec<&str> = api.trim_matches('/').split('/').collect();
        return match (method, parts.as_slice()) {
            ("GET", ["user", "tokens", "verify"]) => json(serde_json::json!({"success": true, "result": {"id": TOKEN_ID, "status": "active"}})),
            ("POST", ["accounts", _, "r2", "buckets"]) => {
                let v: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
                let name = v["name"].as_str().unwrap_or("").to_string();
                let mut s = st.lock().unwrap();
                if s.buckets.contains(&name) {
                    return (409, br#"{"success":false,"errors":[{"code":10004,"message":"The bucket you tried to create already exists, and you own it."}]}"#.to_vec(), vec![]);
                }
                s.buckets.push(name.clone());
                json(serde_json::json!({"success": true, "result": {"name": name}}))
            }
            ("PUT", ["accounts", _, "r2", "buckets", _, "domains", "managed"]) => {
                json(serde_json::json!({"success": true, "result": {"domain": R2_DEV, "enabled": true, "bucketId": "b1"}}))
            }
            ("POST", ["accounts", _, "r2", "buckets", _, "domains", "custom"]) => {
                let v: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
                json(serde_json::json!({"success": true, "result": {"domain": v["domain"], "enabled": true}}))
            }
            _ => (404, b"{}".to_vec(), vec![]),
        };
    }
    // Public downloads: /pub/<key> serves from the first bucket.
    if let Some(key) = path.strip_prefix("/pub/") {
        let s = st.lock().unwrap();
        let found = s.objects.iter().find(|(k, _)| k.split_once('/').map(|(_, rest)| rest) == Some(&decode(key)));
        return match found {
            Some((_, v)) if method == "HEAD" => (200, vec![], head_headers(&s, headers, v.len())),
            Some((_, v)) if method == "GET" => (200, v.clone(), vec![]),
            _ => (404, vec![], vec![]),
        };
    }
    // S3: check the signature like the real thing would.
    let auth = headers.get("authorization").cloned().unwrap_or_default();
    let signed: Vec<String> = auth.split("SignedHeaders=").nth(1).and_then(|s| s.split(',').next()).unwrap_or("").split(';').map(str::to_string).collect();
    let mut hs: Vec<(String, String)> = signed.iter().filter_map(|h| headers.get(h).map(|v| (h.clone(), v.clone()))).collect();
    hs.sort();
    let payload = headers.get("x-amz-content-sha256").cloned().unwrap_or_default();
    let amz_date = headers.get("x-amz-date").cloned().unwrap_or_default();
    let region = auth.split('/').nth(2).unwrap_or("auto").to_string();
    let q = parse_query(query);
    let expected = if amz_date.len() >= 8 {
        sign_v4(
            &SignInput { method, canonical_uri: path, query: &q, headers: &hs, payload_hash: &payload, amz_date: &amz_date, region: &region, service: "s3" },
            creds,
        )
    } else {
        String::new()
    };
    let mut s = st.lock().unwrap();
    if auth != expected || (!body.is_empty() && payload != sha256_hex(body)) {
        s.bad_signatures.push(format!("{method} {path}"));
        return (403, b"<Error><Code>SignatureDoesNotMatch</Code><Message>no</Message></Error>".to_vec(), vec![]);
    }
    let key = decode(path.trim_start_matches('/'));
    let qmap: HashMap<String, String> = q.into_iter().collect();
    match method {
        "PUT" => {
            if let Some(code) = s.fail_puts {
                return (code, b"<Error><Code>InternalError</Code><Message>try again</Message></Error>".to_vec(), vec![]);
            }
            if let (Some(n), Some(id)) = (qmap.get("partNumber"), qmap.get("uploadId")) {
                s.parts.entry(id.clone()).or_default().insert(n.parse().unwrap(), body.to_vec());
                return (200, vec![], vec![("ETag".into(), format!("\"etag-{n}\""))]);
            }
            let mut b = body.to_vec();
            if s.truncate {
                b.truncate(b.len() / 2);
            }
            s.objects.insert(key, b);
            (200, vec![], vec![("ETag".into(), "\"x\"".into())])
        }
        "POST" if qmap.contains_key("uploads") => {
            s.parts.insert("upload-1".into(), BTreeMap::new());
            (200, b"<InitiateMultipartUploadResult><UploadId>upload-1</UploadId></InitiateMultipartUploadResult>".to_vec(), vec![])
        }
        "POST" if qmap.contains_key("uploadId") => {
            let parts = s.parts.remove(&qmap["uploadId"]).unwrap_or_default();
            let all: Vec<u8> = parts.into_values().flatten().collect();
            s.objects.insert(key, all);
            (200, b"<CompleteMultipartUploadResult></CompleteMultipartUploadResult>".to_vec(), vec![])
        }
        "DELETE" if qmap.contains_key("uploadId") => {
            s.parts.remove(&qmap["uploadId"]);
            (204, vec![], vec![])
        }
        "HEAD" => match s.objects.get(&key) {
            Some(v) => (200, vec![], head_headers(&s, headers, v.len())),
            None => (404, vec![], vec![]),
        },
        "GET" => match s.objects.get(&key) {
            Some(v) => (200, v.clone(), vec![]),
            None => (404, b"<Error><Code>NoSuchKey</Code></Error>".to_vec(), vec![]),
        },
        "DELETE" => match s.fail_deletes {
            Some(code) => (code, b"<Error><Code>InternalError</Code></Error>".to_vec(), vec![]),
            None => {
                s.objects.remove(&key);
                (204, vec![], vec![])
            }
        },
        _ => (400, vec![], vec![]),
    }
}
