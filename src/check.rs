//! `folderblog check`: render everything, validate the feed, find broken internal
//! links and path collisions. Never touches `.blog/public` or content.

use crate::render::BuildErrors;
use crate::site::{LoadOptions, Site};
use crate::state::STATE_DIR;
use crate::templates::prefix_base;
use crate::util::{has_scheme, resolve_relative};
use anyhow::Result;
use regex::Regex;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Default)]
pub struct Report {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub outputs: usize,
}

pub fn check(blog: &Path, fixtures: Option<String>) -> Result<Report> {
    let mut rep = Report::default();
    let opts = LoadOptions { fixtures: fixtures.clone(), ..Default::default() };
    let site = match Site::load(blog, opts.clone()) {
        Ok(s) => s,
        Err(e) => {
            rep.errors.push(format!("{e:#}"));
            return Ok(rep);
        }
    };
    rep.warnings.extend(site.warnings.iter().cloned());
    let plan = site.plan()?;
    let tmp = site.blog_dir.join(STATE_DIR).join(format!("check-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    match site.write_to(&plan, &tmp) {
        Ok(n) => rep.outputs = n,
        Err(e) => match e.downcast::<BuildErrors>() {
            Ok(be) => rep.errors.extend(be.0),
            Err(e) => rep.errors.push(format!("{e:#}")),
        },
    }

    // Feed.
    match std::fs::read_to_string(tmp.join(crate::feed::FEED_PATH)) {
        Ok(xml) => {
            if let Err(e) = crate::feed::validate_rss(&xml) {
                rep.errors.push(format!("feed.xml: {e:#}"));
            }
        }
        Err(e) => rep.errors.push(format!("feed.xml was not written: {e}")),
    }
    if site.cfg.base_url.contains("example.com") {
        rep.warnings.push(format!("blog.toml: base_url is still a placeholder ({})", site.cfg.base_url));
    }

    // Internal links.
    let attr = Regex::new(r#"(?i)\s(?:href|src|poster)\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap();
    let mut broken = BTreeSet::new();
    let render_failed = !rep.errors.is_empty();
    if render_failed {
        rep.warnings.push("broken-link check skipped until the errors below are fixed".into());
    }
    for o in plan.outputs.iter().filter(|_| !render_failed) {
        if !o.path.ends_with(".html") {
            continue;
        }
        let Ok(html) = std::fs::read_to_string(tmp.join(&o.path)) else { continue };
        let dir = if o.url.ends_with('/') { o.url.clone() } else { o.url.rsplit_once('/').map(|(d, _)| format!("{d}/")).unwrap_or("/".into()) };
        for c in attr.captures_iter(&html) {
            let v = c.get(1).or(c.get(2)).unwrap().as_str().trim();
            if v.is_empty() || v.starts_with('#') || v.starts_with("//") || has_scheme(v) || v.contains("{{") || v.contains("${") {
                continue;
            }
            let abs = if v.starts_with('/') { v.to_string() } else { resolve_relative(&dir, v).unwrap_or_default() };
            let path = abs.split(['?', '#']).next().unwrap_or("").to_string();
            let path = percent_decode(&path);
            let Some(inner) = strip_base(&site, &path) else {
                broken.insert(format!("{}: link {v:?} points outside the site's base path {}", o.url, site.base_path));
                continue;
            };
            let target = tmp.join(inner.trim_start_matches('/'));
            let ok = if path.ends_with('/') { target.join("index.html").is_file() } else { target.is_file() || target.join("index.html").is_file() };
            if !ok {
                broken.insert(format!("{} ({}): broken link {v:?}", o.url, o.origin));
            }
        }
    }
    rep.errors.extend(broken);

    // Undefined values printed by templates: usually a typo in a variable name.
    if rep.errors.is_empty() {
        if let Ok(strict) = Site::load(blog, LoadOptions { strict_undefined: true, ..opts }) {
            if let Ok(plan) = strict.plan() {
                let mut seen = BTreeSet::new();
                for o in &plan.outputs {
                    if let crate::render::Content::Render { .. } = o.content {
                        if let Err(e) = strict.render_output(o) {
                            let first = e.lines().next().unwrap_or("").to_string();
                            if seen.insert(first.clone()) && seen.len() <= 10 {
                                rep.warnings.push(format!("undefined value used (renders as empty) at {first}"));
                            }
                        }
                    }
                }
            }
        }
    }

    // AGENTS.md drift.
    if fixtures.is_none() {
        let agents = site.blog_dir.join("AGENTS.md");
        match std::fs::read_to_string(&agents) {
            Ok(text) if !crate::contract::is_current(&text) => rep.warnings.push(
                "AGENTS.md is out of date with this folderblog version; run `folderblog agents-md --write`".into(),
            ),
            Err(_) => rep.warnings.push("AGENTS.md is missing; run `folderblog agents-md --write`".into()),
            _ => {}
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(rep)
}

fn strip_base(site: &Site, path: &str) -> Option<String> {
    if site.base_path.is_empty() {
        return Some(path.to_string());
    }
    let _ = prefix_base;
    path.strip_prefix(&site.base_path).map(|p| if p.is_empty() { "/".into() } else { p.to_string() })
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(x) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(x);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
