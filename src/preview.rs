//! Local preview server with drafts and live reload. The live-reload snippet is
//! injected into HTML responses at serve time; it never reaches built output.

use crate::site::LoadOptions;
use crate::state::STATE_DIR;
use crate::util::html_escape;
use crate::watch::log;
use anyhow::Result;
use notify::{RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

pub const VERSION_PATH: &str = "/__folderblog/version";

pub const LIVE_RELOAD: &str = r#"<script>/* folderblog preview live reload */(function(){var v=null;function poll(){fetch("/__folderblog/version",{cache:"no-store"}).then(function(r){return r.text()}).then(function(t){if(v!==null&&t!==v){location.reload()}v=t}).catch(function(){}).finally(function(){setTimeout(poll,600)})}poll()})();</script>"#;

struct Shared {
    version: u64,
    error: Option<String>,
}

pub fn inject(html: &str) -> String {
    match html.rfind("</body>") {
        Some(i) => format!("{}{LIVE_RELOAD}{}", &html[..i], &html[i..]),
        None => format!("{html}{LIVE_RELOAD}"),
    }
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "xml" => "application/xml",
        "rss" => "application/rss+xml",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "webmanifest" => "application/manifest+json",
        _ => "application/octet-stream",
    }
}

fn build_preview(blog: &Path, out: &Path, fixtures: &Option<String>) -> Result<(), String> {
    let opts = LoadOptions { drafts: true, fixtures: fixtures.clone(), persist_state: fixtures.is_none(), ..Default::default() };
    crate::build::build(blog, Some(out), opts, false).map(|_| ()).map_err(|e| format!("{e:#}"))
}

pub fn serve(blog: &Path, port: u16, fixtures: Option<String>) -> Result<()> {
    let blog = blog.canonicalize()?;
    let out_name = match &fixtures {
        Some(f) => format!("preview-fixtures-{f}"),
        None => "preview".into(),
    };
    let out = blog.join(STATE_DIR).join(out_name);
    let shared = Arc::new(Mutex::new(Shared { version: 1, error: build_preview(&blog, &out, &fixtures).err() }));
    if let Some(e) = &shared.lock().unwrap().error {
        log(format!("build failed:\n{e}"));
    }

    let what = if fixtures.is_some() { "fixture content, theme of" } else { "drafts included," };
    let label = fixtures.as_deref().map(|f| format!(" [fixtures: {f}]")).unwrap_or_default();
    // Rebuild on change.
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx)?;
    watcher.watch(&blog, RecursiveMode::Recursive)?;
    let root = blog.parent().unwrap().to_path_buf();
    {
        let shared = shared.clone();
        let blog = blog.clone();
        let out = out.clone();
        std::thread::spawn(move || {
            let _watcher = watcher;
            loop {
                let Ok(ev) = rx.recv() else { return };
                // Access events are ignored: reading files (including blog.toml inside
                // `relevant`) produces them, which would retrigger forever.
                let hit = |ev: &notify::Result<notify::Event>| {
                    ev.as_ref()
                        .map(|e| {
                            !matches!(e.kind, notify::EventKind::Access(_))
                                && e.paths.iter().any(|p| crate::watch::relevant(&root, p).is_some())
                        })
                        .unwrap_or(false)
                };
                if !hit(&ev) {
                    continue;
                }
                // Debounce: wait for a quiet period.
                while rx.recv_timeout(Duration::from_millis(250)).is_ok() {}
                let res = build_preview(&blog, &out, &fixtures);
                let mut s = shared.lock().unwrap();
                s.version += 1;
                match &res {
                    Ok(()) => log("rebuilt"),
                    Err(e) => log(format!("build failed:\n{e}")),
                }
                s.error = res.err();
            }
        });
    }

    let server = tiny_http::Server::http(("127.0.0.1", port)).map_err(|e| anyhow::anyhow!("cannot listen on port {port}: {e}"))?;
    // Sites under a subfolder (e.g. GitHub Pages project sites at /myblog/) are served
    // under the same path, so their links work exactly as they will live.
    let base_path = crate::config::Config::load(&blog).map(|c| c.base_path()).unwrap_or_default();
    log(format!("preview ({what} {}) at http://127.0.0.1:{port}{base_path}/{label}", blog.display()));
    for req in server.incoming_requests() {
        let url = req.url().split(['?', '#']).next().unwrap_or("/").to_string();
        let url = percent_decode(&url);
        let url = if base_path.is_empty() || url == VERSION_PATH {
            url
        } else if url == "/" || url == base_path {
            let _ = req.respond(
                tiny_http::Response::from_data(vec![]).with_status_code(302).with_header(header("Location", &format!("{base_path}/"))),
            );
            continue;
        } else if let Some(rest) = url.strip_prefix(&format!("{base_path}/")) {
            format!("/{rest}")
        } else {
            // Outside the site's path: what the live site would 404 on.
            "/__outside_base_path__".into()
        };
        let (version, error) = {
            let s = shared.lock().unwrap();
            (s.version, s.error.clone())
        };
        if url == VERSION_PATH {
            let _ = req.respond(tiny_http::Response::from_string(version.to_string()));
            continue;
        }
        let resp = respond(&out, &url, error.as_deref());
        let _ = req.respond(resp);
    }
    Ok(())
}

fn header(k: &str, v: &str) -> tiny_http::Header {
    tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap()
}

fn respond(out: &Path, url: &str, error: Option<&str>) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let resolved = resolve(out, url);
    let is_html_req = resolved.as_ref().map(|p| p.extension().is_some_and(|e| e == "html")).unwrap_or(true);
    if let (Some(e), true) = (error, is_html_req) {
        let body = format!(
            "<!doctype html><meta charset=utf-8><title>Build failed</title><body style=\"font:14px/1.5 monospace;padding:2rem;white-space:pre-wrap\"><h1>folderblog: build failed</h1>{}</body>",
            html_escape(e)
        );
        return tiny_http::Response::from_data(inject(&body).into_bytes())
            .with_status_code(500)
            .with_header(header("Content-Type", "text/html; charset=utf-8"));
    }
    if let Some(Redirect(to)) = needs_slash(out, url) {
        return tiny_http::Response::from_data(vec![]).with_status_code(301).with_header(header("Location", &to));
    }
    let (path, code) = match resolved {
        Some(p) => (p, 200),
        None => (out.join("404.html"), 404),
    };
    match std::fs::read(&path) {
        Ok(bytes) => {
            let name = path.to_string_lossy().to_string();
            let ct = content_type(&name);
            let bytes = if ct.starts_with("text/html") { inject(&String::from_utf8_lossy(&bytes)).into_bytes() } else { bytes };
            tiny_http::Response::from_data(bytes)
                .with_status_code(code)
                .with_header(header("Content-Type", ct))
                .with_header(header("Cache-Control", "no-store"))
        }
        Err(_) => tiny_http::Response::from_data(inject("<!doctype html><title>404</title><p>Not found (and the theme has no 404.html).</p>").into_bytes())
            .with_status_code(404)
            .with_header(header("Content-Type", "text/html; charset=utf-8")),
    }
}

struct Redirect(String);

fn needs_slash(out: &Path, url: &str) -> Option<Redirect> {
    if url.ends_with('/') {
        return None;
    }
    let p = out.join(url.trim_start_matches('/'));
    if p.is_dir() && p.join("index.html").is_file() { Some(Redirect(format!("{url}/"))) } else { None }
}

fn resolve(out: &Path, url: &str) -> Option<PathBuf> {
    if url.contains("..") {
        return None;
    }
    let rel = url.trim_start_matches('/');
    let p = out.join(rel);
    if p.is_file() {
        return Some(p);
    }
    if p.join("index.html").is_file() {
        return Some(p.join("index.html"));
    }
    let html = out.join(format!("{}.html", rel.trim_end_matches('/')));
    if html.is_file() { Some(html) } else { None }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
