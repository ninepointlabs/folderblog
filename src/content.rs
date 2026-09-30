//! Content discovery and the content rules: title, slug and date resolution.
//! Content files are only ever read, never written.

use crate::state::State;
use crate::util::{slugify, title_from_slug};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, TimeZone};
use serde_json::{Map, Value as Json};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Post,
    Page,
}

#[derive(Debug, Clone)]
pub enum Body {
    Markdown(String),
    /// An .html file in pages/: rendered as a template with the full context.
    Html(String),
}

#[derive(Debug, Clone)]
pub struct Source {
    pub kind: Kind,
    /// Path relative to the content root, e.g. "posts/hello.md".
    pub rel: PathBuf,
    pub abs: PathBuf,
    /// For folder items (`posts/trip/index.md`), the folder whose other files are published with it.
    pub bundle_dir: Option<PathBuf>,
    /// Filename-derived stem: folder name for bundles, file stem otherwise.
    pub stem: String,
    /// Directory of the item relative to its section, e.g. "" or "projects" for pages/projects/x.md.
    pub section_subdir: PathBuf,
    pub draft: bool,
    pub front: Map<String, Json>,
    pub body: Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DateSource {
    FrontMatter,
    Filename,
    FirstSeen,
}

#[derive(Debug, Default)]
pub struct Discovered {
    pub sources: Vec<Source>,
    /// Non-content files under posts/ and pages/ to publish: (absolute source, section-relative path).
    pub loose_files: Vec<(PathBuf, Kind, PathBuf)>,
    pub warnings: Vec<String>,
}

pub fn is_markdown(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("md" | "markdown" | "mdown" | "mkd")
    )
}

fn is_html(p: &Path) -> bool {
    matches!(p.extension().and_then(|e| e.to_str()), Some("html" | "htm"))
}

/// Files that editors, sync tools and OSes leave behind. Never content, never published.
pub fn is_debris(name: &str) -> bool {
    name.starts_with('.')
        || name.starts_with("~$")
        || name.ends_with('~')
        || name.ends_with(".swp")
        || name.ends_with(".swx")
        || name.ends_with(".swo")
        || name.ends_with(".tmp")
        || name.ends_with(".part")
        || name.ends_with(".crdownload")
        || name.ends_with(".kate-swp")
        || name == "4913"
        || name == "Thumbs.db"
        || name == "desktop.ini"
        || name.starts_with("#") && name.ends_with('#')
}

/// Walk posts/, pages/ and (optionally) drafts/ under `root`.
pub fn discover(root: &Path, include_drafts: bool) -> Result<Discovered> {
    let mut out = Discovered::default();
    let mut sections = vec![("posts", Kind::Post, false), ("pages", Kind::Page, false)];
    if include_drafts {
        sections.push(("drafts", Kind::Post, true));
    }
    for (dir, kind, draft) in sections {
        let base = root.join(dir);
        if !base.is_dir() {
            continue;
        }
        walk_section(root, &base, &base, kind, draft, None, &mut out)?;
    }
    out.sources.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(out)
}

fn walk_section(
    root: &Path,
    base: &Path,
    dir: &Path,
    kind: Kind,
    draft: bool,
    bundle: Option<&Path>,
    out: &mut Discovered,
) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if is_debris(&name) {
            continue;
        }
        let ft = entry.file_type()?;
        if ft.is_dir() {
            // A folder containing index.md is a bundle: one item plus its files.
            let index = ["index.md", "index.markdown", "index.html"]
                .iter()
                .map(|n| path.join(n))
                .find(|p| p.is_file());
            if bundle.is_none() {
                if let Some(index) = index {
                    if !(kind == Kind::Post && is_html(&index)) {
                        load_item(root, base, &index, kind, draft, Some(&path), out)?;
                        walk_section(root, base, &path, kind, draft, Some(&path), out)?;
                        continue;
                    }
                }
            }
            walk_section(root, base, &path, kind, draft, bundle, out)?;
            continue;
        }
        if let Some(b) = bundle {
            // Inside a bundle, everything except the index is an attached file.
            let is_index = path.parent() == Some(b)
                && matches!(name.as_str(), "index.md" | "index.markdown" | "index.html");
            if !is_index && !draft {
                out.loose_files.push((path.clone(), kind, path.strip_prefix(base)?.to_path_buf()));
            }
            continue;
        }
        if is_markdown(&path) || (kind == Kind::Page && is_html(&path)) {
            load_item(root, base, &path, kind, draft, None, out)?;
        } else if !draft {
            out.loose_files.push((path.clone(), kind, path.strip_prefix(base)?.to_path_buf()));
        }
    }
    Ok(())
}

fn load_item(
    root: &Path,
    base: &Path,
    path: &Path,
    kind: Kind,
    draft: bool,
    bundle: Option<&Path>,
    out: &mut Discovered,
) -> Result<()> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text).to_string();
    let rel = path.strip_prefix(root)?.to_path_buf();
    // A malformed file fails the build rather than publishing something half-right.
    let (front, _, body) =
        split_front_matter(&text).with_context(|| format!("{}: bad front matter", rel.display()))?;
    let stem = match bundle {
        Some(b) => b.file_name().unwrap().to_string_lossy().to_string(),
        None => path.file_stem().unwrap().to_string_lossy().to_string(),
    };
    let item_dir = bundle.map(|b| b.parent().unwrap()).unwrap_or_else(|| path.parent().unwrap());
    let section_subdir = item_dir.strip_prefix(base)?.to_path_buf();
    let fm_draft = front.get("draft").and_then(|v| v.as_bool()).unwrap_or(false);
    out.sources.push(Source {
        kind,
        rel,
        abs: path.to_path_buf(),
        bundle_dir: bundle.map(|b| b.to_path_buf()),
        stem,
        section_subdir,
        draft: draft || fm_draft,
        front,
        body: if is_html(path) { Body::Html(body) } else { Body::Markdown(body) },
    });
    Ok(())
}

/// Split YAML (`---`) or TOML (`+++`) front matter off the top of a file.
/// Returns (front matter, number of lines it occupied, body).
pub fn split_front_matter(text: &str) -> Result<(Map<String, Json>, usize, String)> {
    for (fence, is_yaml) in [("---", true), ("+++", false)] {
        let first = text.lines().next().unwrap_or("");
        if first.trim_end() != fence {
            continue;
        }
        let mut offset = first.len() + 1;
        let mut lines = 1;
        for line in text[offset.min(text.len())..].split_inclusive('\n') {
            lines += 1;
            if line.trim_end() == fence {
                let raw = &text[first.len() + 1..offset];
                let body = text[(offset + line.len()).min(text.len())..].to_string();
                let value: Json = if is_yaml {
                    if raw.trim().is_empty() {
                        Json::Object(Map::new())
                    } else {
                        serde_yaml::from_str(raw).context("invalid YAML front matter")?
                    }
                } else {
                    let t: toml::Table = toml::from_str(raw).context("invalid TOML front matter")?;
                    toml_to_json(toml::Value::Table(t))
                };
                return match value {
                    Json::Object(m) => Ok((m, lines, body)),
                    Json::Null => Ok((Map::new(), lines, body)),
                    _ => bail!("front matter is not a mapping"),
                };
            }
            offset += line.len();
        }
        // An opening fence with no closing fence is just content (e.g. a horizontal rule).
        return Ok((Map::new(), 0, text.to_string()));
    }
    Ok((Map::new(), 0, text.to_string()))
}

pub fn toml_to_json(v: toml::Value) -> Json {
    match v {
        toml::Value::String(s) => Json::String(s),
        toml::Value::Integer(i) => Json::from(i),
        toml::Value::Float(f) => Json::from(f),
        toml::Value::Boolean(b) => Json::Bool(b),
        toml::Value::Datetime(d) => Json::String(d.to_string()),
        toml::Value::Array(a) => Json::Array(a.into_iter().map(toml_to_json).collect()),
        toml::Value::Table(t) => Json::Object(t.into_iter().map(|(k, v)| (k, toml_to_json(v))).collect()),
    }
}

/// Split a leading `YYYY-MM-DD-` (or `YYYY-MM-DD_`/`YYYY-MM-DD `) date off a filename stem.
pub fn split_date_prefix(stem: &str) -> (Option<NaiveDate>, &str) {
    if stem.len() >= 10 && stem.is_char_boundary(10) {
        if let Ok(d) = NaiveDate::parse_from_str(&stem[..10], "%Y-%m-%d") {
            let rest = stem[10..].trim_start_matches(['-', '_', ' ']);
            return (Some(d), if rest.is_empty() { &stem[..10] } else { rest });
        }
    }
    (None, stem)
}

pub fn slug_for(src: &Source) -> String {
    if let Some(s) = src.front.get("slug").and_then(|v| v.as_str()) {
        let s = slugify(s);
        if !s.is_empty() {
            return s;
        }
    }
    let (_, rest) = split_date_prefix(&src.stem);
    let s = slugify(rest);
    if s.is_empty() { "untitled".into() } else { s }
}

/// Title rule: front matter, else the first heading (removed from the body by the
/// Markdown renderer), else the filename.
pub fn title_fallback(src: &Source) -> String {
    let (_, rest) = split_date_prefix(&src.stem);
    title_from_slug(rest)
}

/// Parse a date value from front matter. Date-only values are midnight local time.
pub fn parse_date(s: &str) -> Option<DateTime<FixedOffset>> {
    let s = s.trim();
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return Some(d);
    }
    for fmt in ["%Y-%m-%d %H:%M:%S %z", "%Y-%m-%d %H:%M %z", "%Y-%m-%dT%H:%M:%S%z", "%Y-%m-%dT%H:%M%z"] {
        if let Ok(d) = DateTime::parse_from_str(s, fmt) {
            return Some(d);
        }
    }
    for fmt in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M", "%Y-%m-%dT%H:%M:%S%.f"] {
        if let Ok(n) = NaiveDateTime::parse_from_str(s, fmt) {
            return local(n);
        }
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().and_then(|d| local(d.and_hms_opt(0, 0, 0)?))
}

fn local(n: NaiveDateTime) -> Option<DateTime<FixedOffset>> {
    Local.from_local_datetime(&n).earliest().map(|d| d.fixed_offset())
}

/// Date rule: front matter `date`, else filename prefix, else first-seen (recorded in state).
/// Drafts never record a first-seen date, so a post's date is when it was published.
pub fn resolve_date(
    src: &Source,
    slug: &str,
    state: &mut State,
) -> Result<Option<(DateTime<FixedOffset>, DateSource)>> {
    if let Some(v) = src.front.get("date") {
        let parsed = match v {
            Json::String(s) => parse_date(s),
            _ => None,
        };
        // An unreadable date is an error, not a silent fallback: falling back would
        // give the post a date (and URL) that changes once the typo is fixed.
        return match parsed {
            Some(d) => Ok(Some((d, DateSource::FrontMatter))),
            None => bail!("{}: unrecognised date {v} (use YYYY-MM-DD or RFC 3339)", src.rel.display()),
        };
    }
    if let (Some(d), _) = split_date_prefix(&src.stem) {
        return Ok(local(d.and_hms_opt(0, 0, 0).unwrap()).map(|d| (d, DateSource::Filename)));
    }
    if src.kind == Kind::Page {
        return Ok(None);
    }
    if src.draft {
        return Ok(Some((Local::now().fixed_offset(), DateSource::FirstSeen)));
    }
    Ok(Some((state.first_seen(slug), DateSource::FirstSeen)))
}

pub fn tags_of(front: &Map<String, Json>) -> Vec<String> {
    let mut tags: Vec<String> = match front.get("tags") {
        Some(Json::Array(a)) => a
            .iter()
            .filter_map(|v| match v {
                Json::String(s) => Some(s.trim().to_string()),
                Json::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .collect(),
        Some(Json::String(s)) => s.split(',').map(|t| t.trim().to_string()).collect(),
        _ => vec![],
    };
    tags.retain(|t| !t.is_empty());
    let mut seen = std::collections::HashSet::new();
    tags.retain(|t| seen.insert(slugify(t)));
    tags
}
