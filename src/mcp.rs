//! `folderblog mcp`: an MCP server (stdio) so an agent can turn an audio file into a
//! published episode: inspect it, write the draft episode, fill in the Podcasting 2.0
//! details, check it, and publish once the user says yes.
//!
//! The server never calls an AI, never returns secrets, and writes only inside
//! `<blog>/podcast/`. Episodes it creates are drafts until `publish_episode` is called
//! with `confirm: true`.

use crate::config::Config;
use crate::content::split_front_matter;
use crate::podcast::{self, Chapter};
use crate::site::{LoadOptions, Site};
use anyhow::{Context, Result, anyhow, bail};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ErrorData, ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde_json::{Map, Value as Json, json};
use std::path::{Path, PathBuf};

/// Where the server finds blogs by name, and the saved storage keys.
#[derive(Clone)]
pub struct Ctx {
    pub root: PathBuf,
    pub config_dir: PathBuf,
}

const TRANSCRIPT_LIMIT: usize = 20_000;

// ---------- finding things ----------

/// A blog by path, or by name under the root.
pub fn resolve_blog(ctx: &Ctx, blog: &str) -> Result<PathBuf> {
    let p = Path::new(blog.trim());
    let dir = if p.is_absolute() || blog.contains('/') { p.to_path_buf() } else { ctx.root.join(p) };
    if !dir.join(crate::config::CONFIG_FILE).is_file() {
        bail!("{} is not a blog (no blog.toml); pass a blog folder, or the name of one in {}", dir.display(), ctx.root.display());
    }
    Ok(dir.canonicalize()?)
}

fn podcast_on(blog: &Path) -> Result<Config> {
    let cfg = Config::load(blog)?;
    if !cfg.podcast.enabled {
        bail!("the podcast is off for this blog; ask the user to run `folderblog podcast on {}`", blog.display());
    }
    Ok(cfg)
}

/// The folder of the episode with this slug (or folder name), inside `podcast/`.
fn episode_dir(blog: &Path, slug: &str) -> Result<PathBuf> {
    let base = blog.join(podcast::PODCAST_DIR);
    let mut found = vec![];
    for e in std::fs::read_dir(&base).with_context(|| format!("reading {}", base.display()))?.flatten() {
        let path = e.path();
        if !path.join("index.md").is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        let fm_slug = read_episode(&path).ok().and_then(|(f, _)| f.get("slug").and_then(|v| v.as_str()).map(crate::util::slugify));
        let (_, rest) = crate::content::split_date_prefix(&name);
        let slug_of = fm_slug.unwrap_or_else(|| crate::util::slugify(rest));
        if name == slug || slug_of == slug {
            found.push(path);
        }
    }
    match found.as_slice() {
        [one] => Ok(one.clone()),
        [] => bail!("no episode with the slug {slug:?}; list_episodes shows them"),
        _ => bail!("more than one episode folder matches {slug:?}; use the folder name"),
    }
}

fn read_episode(dir: &Path) -> Result<(Map<String, Json>, String)> {
    let text = std::fs::read_to_string(dir.join("index.md"))?.replace("\r\n", "\n");
    let (front, _, body) = split_front_matter(&text)?;
    Ok((front, body))
}

/// Write `index.md` back with YAML front matter. Only ever inside `podcast/`.
fn write_episode(blog: &Path, dir: &Path, front: &Map<String, Json>, body: &str) -> Result<()> {
    let yaml = serde_yaml::to_string(front)?;
    let body = body.trim_start_matches('\n');
    write_episode_text(blog, dir, &format!("---\n{yaml}---\n{body}"))
}

/// `text` (an episode's `index.md`) with its front matter's `draft: true` line changed to
/// `draft: false` and every other byte kept: comments, key order, quoting, line endings.
/// Unchanged if the episode isn't a draft. An error if the flag isn't on a line of its own.
pub fn set_draft_false(text: &str) -> Result<String> {
    let (front, _, _) = split_front_matter(&text.replace("\r\n", "\n"))?;
    if front.get("draft").and_then(Json::as_bool) != Some(true) {
        return Ok(text.to_string());
    }
    let fence = if text.starts_with("+++") { "+++" } else { "---" };
    let line_re = regex::Regex::new(r#"^((?:draft|"draft"|'draft')[ \t]*[:=][ \t]*)true([ \t]*(?:#.*)?)$"#).unwrap();
    let mut out = String::with_capacity(text.len() + 1);
    let (mut changed, mut in_front) = (0, true);
    for (i, line) in text.split_inclusive('\n').enumerate() {
        let content = line.trim_end_matches('\n').trim_end_matches('\r');
        if i > 0 && in_front && content.trim_end() == fence {
            in_front = false;
        } else if i > 0 && in_front && line_re.is_match(content) {
            out.push_str(&line_re.replace(content, "${1}false${2}"));
            out.push_str(&line[content.len()..]);
            changed += 1;
            continue;
        }
        out.push_str(line);
    }
    let (after, _, _) = split_front_matter(&out.replace("\r\n", "\n"))?;
    let mut want = front.clone();
    want.insert("draft".into(), json!(false));
    if changed != 1 || after != want {
        bail!("couldn't find the `draft: true` line in the front matter to change it; set `draft: false` by hand, then publish");
    }
    Ok(out)
}

/// Replace an episode's `index.md` with `text`. Only ever inside `podcast/`.
fn write_episode_text(blog: &Path, dir: &Path, text: &str) -> Result<()> {
    let podcast_dir = blog.join(podcast::PODCAST_DIR).canonicalize()?;
    let dir = dir.canonicalize()?;
    if !dir.starts_with(&podcast_dir) || dir == podcast_dir {
        bail!("refusing to write outside {}", podcast_dir.display());
    }
    let tmp = dir.join(".index.md.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, dir.join("index.md"))?;
    Ok(())
}

fn copy_into(blog: &Path, dir: &Path, src: &Path, name: &str) -> Result<PathBuf> {
    let podcast_dir = blog.join(podcast::PODCAST_DIR).canonicalize()?;
    if !dir.canonicalize()?.starts_with(&podcast_dir) || name.contains('/') || name.contains("..") {
        bail!("refusing to write outside {}", podcast_dir.display());
    }
    let to = dir.join(name);
    std::fs::copy(src, &to).with_context(|| format!("copying {}", src.display()))?;
    Ok(to)
}

/// Problems `check` reports that are about this episode folder (or the whole show).
fn problems_for(problems: &[String], folder: &str) -> Vec<String> {
    problems.iter().filter(|p| p.contains(&format!("{folder}/")) || p.starts_with("podcast.") || p.starts_with("podcast:")).cloned().collect()
}

fn load_preview(blog: &Path) -> Result<Site> {
    Site::load(blog, LoadOptions { drafts: true, local_media: true, ..Default::default() })
}

// ---------- tool arguments ----------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BlogArgs {
    /// The blog: its folder, or its name in the blogs folder (e.g. "example.com").
    pub blog: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InspectArgs {
    /// Path to an audio (MP3) or video (MP4) file.
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ChapterArg {
    /// Start time in seconds.
    pub start: f64,
    pub title: String,
    /// An https:// link for this chapter.
    #[serde(default)]
    pub url: Option<String>,
    /// An https:// image for this chapter.
    #[serde(default)]
    pub img: Option<String>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct PersonArg {
    pub name: String,
    /// host, guest, producer… (Podcasting 2.0 roles)
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub href: Option<String>,
    #[serde(default)]
    pub img: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateArgs {
    pub blog: String,
    /// The recording: an MP3 (audio) or MP4 (video) file. It is copied; the original is left alone.
    pub audio_path: String,
    /// Default: the file's embedded title, else its file name.
    #[serde(default)]
    pub title: Option<String>,
    /// Show notes in Markdown, written by you (the agent) for the user.
    #[serde(default)]
    pub notes_markdown: Option<String>,
    /// One line for podcast apps.
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub season: Option<u64>,
    /// Default: one more than the highest episode number so far.
    #[serde(default)]
    pub episode: Option<u64>,
    /// full, trailer or bonus.
    #[serde(default)]
    pub episode_type: Option<String>,
    #[serde(default)]
    pub explicit: Option<bool>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub persons: Option<Vec<PersonArg>>,
    /// A .vtt or .srt transcript to copy in.
    #[serde(default)]
    pub transcript_path: Option<String>,
    #[serde(default)]
    pub chapters: Option<Vec<ChapterArg>>,
    /// Publication date, YYYY-MM-DD or RFC 3339. Default: today.
    #[serde(default)]
    pub date: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateArgs {
    pub blog: String,
    /// The episode's slug (or folder name).
    pub slug: String,
    /// Fields to change: title, summary, season, episode, episode_type, explicit, tags,
    /// persons, date, in_main_feed, notes_markdown (replaces the show notes),
    /// transcript_path (copies a .vtt/.srt in), or draft: true (takes it down). Use
    /// publish_episode to publish, and set_chapters for chapters.
    pub fields: Map<String, Json>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ChaptersArgs {
    pub blog: String,
    pub slug: String,
    pub chapters: Vec<ChapterArg>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ValidateArgs {
    pub blog: String,
    /// Only report problems for this episode (and the show).
    #[serde(default)]
    pub slug: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PublishArgs {
    pub blog: String,
    pub slug: String,
    /// Must be true, and only after the user has said yes in the conversation.
    #[serde(default)]
    pub confirm: bool,
}

// ---------- the tools, as plain functions ----------

pub fn podcast_status(ctx: &Ctx, a: &BlogArgs) -> Result<Json> {
    let blog = resolve_blog(ctx, &a.blog)?;
    let cfg = Config::load(&blog)?;
    let storage = match &cfg.podcast.storage {
        None => json!({ "set_up": false, "hint": "ask the user to run `folderblog podcast setup`" }),
        Some(st) => {
            let key = if st.kind.is_empty() {
                Json::Null
            } else {
                match crate::storage::load_credentials(&ctx.config_dir, &st.kind, &st.bucket) {
                    Ok(c) => json!({ "access_key_id": crate::storage::mask(&c.access_key_id), "secret": "set" }),
                    Err(_) if st.kind == "s3" => json!(format!(
                        "missing on this computer; the user needs to {}",
                        crate::storage::key_hint(&ctx.config_dir, &st.kind, &st.bucket)
                    )),
                    Err(_) => json!("missing on this computer; ask the user to run `folderblog podcast setup`"),
                }
            };
            json!({ "set_up": true, "type": if st.kind.is_empty() { "manual" } else { &st.kind }, "bucket": st.bucket, "public_url": st.public_url, "key": key })
        }
    };
    if !cfg.podcast.enabled {
        return Ok(json!({ "enabled": false, "hint": format!("the user can turn it on with `folderblog podcast on {}`", blog.display()), "storage": storage }));
    }
    let site = load_preview(&blog)?;
    let show = site.podcast.as_ref().context("podcast did not load")?;
    let drafts = show.episodes.iter().filter(|e| e.item.src.draft).count();
    Ok(json!({
        "enabled": true,
        "title": show.title,
        "description": show.description,
        "url": format!("{}{}", cfg.base_url, cfg.podcast.url),
        "feed_url": podcast::feed_url(&cfg),
        "guid": show.guid,
        "category": cfg.podcast.category,
        "explicit": cfg.podcast.explicit,
        "lightning_address": cfg.podcast.lightning_address,
        "agent_may_publish": cfg.podcast.mcp_publish,
        "storage": storage,
        "episodes": { "total": show.episodes.len(), "drafts": drafts, "published": show.episodes.len() - drafts },
        "problems": show.problems,
    }))
}

pub fn list_episodes(ctx: &Ctx, a: &BlogArgs) -> Result<Json> {
    let blog = resolve_blog(ctx, &a.blog)?;
    let cfg = podcast_on(&blog)?;
    let site = load_preview(&blog)?;
    let show = site.podcast.as_ref().context("podcast did not load")?;
    let list: Vec<Json> = show
        .episodes
        .iter()
        .map(|e| {
            let problems = problems_for(&show.problems, &e.folder);
            json!({
                "slug": e.item.slug,
                "folder": e.folder,
                "title": e.item.title,
                "date": e.item.date.as_ref().map(|d| d.0.to_rfc3339()),
                "draft": e.item.src.draft,
                "url": format!("{}{}", cfg.base_url.trim_end_matches(&site.base_path), crate::templates::prefix_base(&site.base_path, &e.item.route)),
                "guid": e.guid,
                "season": e.season,
                "episode": e.number,
                "duration": e.media.as_ref().map(|m| podcast::duration_hms(m.info.duration)),
                "check": if problems.is_empty() { json!("ok") } else { json!(problems) },
            })
        })
        .collect();
    Ok(json!({ "episodes": list }))
}

pub fn inspect_audio(a: &InspectArgs) -> Result<Json> {
    let path = Path::new(&a.path);
    if !path.is_file() {
        bail!("{} is not a file", path.display());
    }
    if let Err(e) = podcast::media_kind(path) {
        return Ok(json!({ "accepted": false, "problem": e }));
    }
    match podcast::inspect(path) {
        Ok(i) => Ok(json!({
            "accepted": true,
            "format": i.kind.ext(),
            "mime": i.kind.mime(),
            "is_video": i.kind.is_video(),
            "bytes": i.bytes,
            "duration_seconds": (i.duration * 10.0).round() / 10.0,
            "duration": podcast::duration_hms(i.duration),
            "bitrate_kbps": ((i.bytes as f64 * 8.0 / i.duration / 1000.0).round() as u64),
            "embedded_title": i.title,
            "embedded_chapters": Json::Array(vec![]),
        })),
        Err(e) => Ok(json!({ "accepted": false, "problem": format!("{e:#}") })),
    }
}

fn person_json(p: &PersonArg) -> Json {
    let mut m = Map::new();
    m.insert("name".into(), json!(p.name));
    for (k, v) in [("role", &p.role), ("href", &p.href), ("img", &p.img)] {
        if let Some(v) = v.as_ref().filter(|v| !v.is_empty()) {
            m.insert(k.into(), json!(v));
        }
    }
    Json::Object(m)
}

fn chapters_of(c: &[ChapterArg]) -> Result<String> {
    let list: Vec<Chapter> = c.iter().map(|c| Chapter { start: c.start, title: c.title.clone(), url: c.url.clone(), img: c.img.clone(), toc: true }).collect();
    let text = podcast::chapters_json(&list);
    podcast::parse_chapters(&text).map_err(|e| anyhow!("chapters: {}", e.join("; ")))?;
    Ok(text)
}

/// Strict checks for fields the agent sets; returns them as front matter values.
fn checked_fields(fields: &Map<String, Json>) -> Result<Vec<(String, Json)>> {
    let mut out = vec![];
    for (k, v) in fields {
        let bad = |what: &str| anyhow!("{k}: {what} (got {v})");
        let val = match k.as_str() {
            "title" | "summary" => match v.as_str().map(str::trim) {
                Some(s) if !s.is_empty() => json!(s),
                _ => return Err(bad("must be a non-empty string")),
            },
            "season" | "episode" => match v.as_u64() {
                Some(n) if n > 0 => json!(n),
                _ => return Err(bad("must be a whole number, 1 or more")),
            },
            "episode_type" => match v.as_str() {
                Some(t @ ("full" | "trailer" | "bonus")) => json!(t),
                _ => return Err(bad("must be full, trailer or bonus")),
            },
            "explicit" | "in_main_feed" => match v {
                Json::Bool(_) => v.clone(),
                _ => return Err(bad("must be true or false")),
            },
            "draft" => match v {
                Json::Bool(true) => v.clone(),
                Json::Bool(false) => bail!("draft: use publish_episode to publish an episode"),
                _ => return Err(bad("must be true")),
            },
            "date" => match v.as_str().and_then(crate::content::parse_date) {
                Some(_) => v.clone(),
                None => return Err(bad("must be YYYY-MM-DD or RFC 3339")),
            },
            "tags" => match v.as_array() {
                Some(a) if a.iter().all(|t| t.is_string()) => v.clone(),
                _ => return Err(bad("must be a list of strings")),
            },
            "persons" => {
                let p: Vec<PersonArg> = serde_json::from_value(v.clone()).map_err(|_| bad("must be a list of {name, role, href, img}"))?;
                for x in &p {
                    for u in [&x.href, &x.img].into_iter().flatten() {
                        if !u.starts_with("https://") {
                            return Err(bad("person links must be https://"));
                        }
                    }
                }
                Json::Array(p.iter().map(person_json).collect())
            }
            "guid" => bail!("guid: an episode's guid never changes once it has one"),
            "notes_markdown" | "transcript_path" => continue,
            other => bail!("{other}: not a field update_episode can change"),
        };
        out.push((k.clone(), val));
    }
    Ok(out)
}

fn copy_transcript(blog: &Path, dir: &Path, path: &str) -> Result<(String, String)> {
    let src = Path::new(path);
    let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if podcast::transcript_type(src).is_none() {
        bail!("transcripts must be .vtt or .srt files");
    }
    let text = std::fs::read_to_string(src).with_context(|| format!("reading {path}"))?;
    podcast::validate_transcript(&text, &ext).map_err(|e| anyhow!("{path}: {e}"))?;
    let name = format!("transcript.{ext}");
    copy_into(blog, dir, src, &name)?;
    Ok((name, text))
}

pub fn create_episode(ctx: &Ctx, a: &CreateArgs) -> Result<Json> {
    let blog = resolve_blog(ctx, &a.blog)?;
    podcast_on(&blog)?;
    let audio = Path::new(&a.audio_path);
    let kind = podcast::media_kind(audio).map_err(anyhow::Error::msg)?;
    let info = podcast::inspect(audio).with_context(|| format!("{}", audio.display()))?;
    let title = a
        .title
        .clone()
        .or_else(|| info.title.clone())
        .unwrap_or_else(|| crate::util::title_from_slug(&audio.file_stem().unwrap_or_default().to_string_lossy()));
    let slug = crate::util::slugify(&title);
    if slug.is_empty() {
        bail!("the title needs some letters or digits");
    }
    let date = match &a.date {
        Some(d) => crate::content::parse_date(d).with_context(|| format!("date {d:?} must be YYYY-MM-DD or RFC 3339"))?,
        None => chrono::Local::now().fixed_offset(),
    };
    let folder = format!("{}-{slug}", date.format("%Y-%m-%d"));
    let base = blog.join(podcast::PODCAST_DIR);
    std::fs::create_dir_all(&base)?;
    let dir = base.join(&folder);
    if dir.exists() {
        bail!("podcast/{folder}/ already exists; choose another title or date, or use update_episode");
    }
    // Numbering: one after the highest episode so far.
    let site = load_preview(&blog)?;
    let show = site.podcast.as_ref().context("podcast did not load")?;
    if show.episodes.iter().any(|e| e.item.slug == slug) {
        bail!("there is already an episode with the slug {slug:?}; choose another title");
    }
    let number = a.episode.or_else(|| Some(show.episodes.iter().filter_map(|e| e.number).max().unwrap_or(0) + 1));
    let season = a.season.or_else(|| show.episodes.iter().filter_map(|e| e.season).max());

    let mut front = Map::new();
    front.insert("title".into(), json!(title));
    front.insert("date".into(), json!(date.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)));
    front.insert("draft".into(), json!(true));
    front.insert("guid".into(), json!(uuid::Uuid::new_v4().to_string()));
    let mut extra = Map::new();
    if let Some(s) = &a.summary {
        extra.insert("summary".into(), json!(s));
    }
    if let Some(s) = season {
        extra.insert("season".into(), json!(s));
    }
    if let Some(n) = number {
        extra.insert("episode".into(), json!(n));
    }
    if let Some(t) = &a.episode_type {
        extra.insert("episode_type".into(), json!(t));
    }
    if let Some(e) = a.explicit {
        extra.insert("explicit".into(), json!(e));
    }
    if let Some(t) = &a.tags {
        extra.insert("tags".into(), json!(t));
    }
    if let Some(p) = &a.persons {
        extra.insert("persons".into(), serde_json::to_value(p.iter().map(person_json).collect::<Vec<_>>())?);
    }
    for (k, v) in checked_fields(&extra)? {
        front.insert(k, v);
    }
    let chapters = a.chapters.as_deref().map(chapters_of).transpose()?;

    // Everything checked: now write, only inside podcast/<folder>/.
    std::fs::create_dir(&dir)?;
    let res = (|| -> Result<Option<String>> {
        copy_into(&blog, &dir, audio, &format!("episode.{}", kind.ext()))?;
        let transcript = match &a.transcript_path {
            Some(p) => Some(copy_transcript(&blog, &dir, p)?.1),
            None => None,
        };
        if let Some(c) = &chapters {
            std::fs::write(dir.join("chapters.json"), c)?;
        }
        write_episode(&blog, &dir, &front, a.notes_markdown.as_deref().unwrap_or(""))?;
        Ok(transcript)
    })();
    let transcript = match res {
        Ok(t) => t,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
    };
    let site = load_preview(&blog)?;
    let show = site.podcast.as_ref().context("podcast did not load")?;
    let rel = format!("{}/{folder}", podcast::PODCAST_DIR);
    let problems: Vec<String> = problems_for(&show.problems, &rel);
    let route = format!("{}{slug}/", Config::load(&blog)?.podcast.url);
    Ok(json!({
        "created": format!("{rel}/"),
        "draft": true,
        "slug": slug,
        "file": format!("episode.{}", kind.ext()),
        "duration": podcast::duration_hms(info.duration),
        "duration_seconds": (info.duration * 10.0).round() / 10.0,
        "bytes": info.bytes,
        "embedded_title": info.title,
        "front_matter": front,
        "preview": format!("run `folderblog preview {}`, then open http://127.0.0.1:4000{}{route}", blog.display(), site.base_path),
        "transcript_text": transcript.map(|t| t.chars().take(TRANSCRIPT_LIMIT).collect::<String>()),
        "problems": problems,
        "next": "Write or improve the show notes with update_episode, add chapters with set_chapters, run validate, then show the user the title, date, notes and feed changes and ask before publish_episode.",
    }))
}

pub fn update_episode(ctx: &Ctx, a: &UpdateArgs) -> Result<Json> {
    let blog = resolve_blog(ctx, &a.blog)?;
    podcast_on(&blog)?;
    let dir = episode_dir(&blog, &a.slug)?;
    let (mut front, mut body) = read_episode(&dir)?;
    let fields = checked_fields(&a.fields)?;
    let notes = match a.fields.get("notes_markdown") {
        None => None,
        Some(Json::String(s)) => Some(s.clone()),
        Some(v) => bail!("notes_markdown must be a string (got {v})"),
    };
    let transcript = match a.fields.get("transcript_path") {
        None => None,
        Some(Json::String(p)) => Some(copy_transcript(&blog, &dir, p)?),
        Some(v) => bail!("transcript_path must be a string (got {v})"),
    };
    for (k, v) in fields {
        front.insert(k, v);
    }
    if let Some((name, _)) = &transcript {
        front.insert("transcript".into(), json!(name));
    }
    if let Some(n) = notes {
        body = n;
    }
    write_episode(&blog, &dir, &front, &body)?;
    let folder = format!("{}/{}", podcast::PODCAST_DIR, dir.file_name().unwrap().to_string_lossy());
    let site = load_preview(&blog)?;
    let problems = problems_for(&site.podcast.as_ref().map(|s| s.problems.clone()).unwrap_or_default(), &folder);
    Ok(json!({ "updated": format!("{folder}/index.md"), "front_matter": front, "problems": problems }))
}

pub fn set_chapters(ctx: &Ctx, a: &ChaptersArgs) -> Result<Json> {
    let blog = resolve_blog(ctx, &a.blog)?;
    podcast_on(&blog)?;
    let dir = episode_dir(&blog, &a.slug)?;
    let text = chapters_of(&a.chapters)?;
    let podcast_dir = blog.join(podcast::PODCAST_DIR).canonicalize()?;
    if !dir.canonicalize()?.starts_with(&podcast_dir) {
        bail!("refusing to write outside {}", podcast_dir.display());
    }
    std::fs::write(dir.join("chapters.json"), &text)?;
    Ok(json!({ "written": format!("{}/{}/chapters.json", podcast::PODCAST_DIR, dir.file_name().unwrap().to_string_lossy()), "chapters": a.chapters.len() }))
}

pub fn validate(ctx: &Ctx, a: &ValidateArgs) -> Result<Json> {
    let blog = resolve_blog(ctx, &a.blog)?;
    let rep = crate::check::check(&blog, None)?;
    let (mut errors, mut warnings) = (rep.errors, rep.warnings);
    // Drafts are checked too, the way they will be once published.
    if let Ok(site) = load_preview(&blog)
        && let Some(show) = &site.podcast
    {
        for p in &show.problems {
            if !errors.contains(p) {
                errors.push(p.clone());
            }
        }
    }
    if let Some(slug) = &a.slug {
        let dir = episode_dir(&blog, slug)?;
        let folder = format!("{}/{}", podcast::PODCAST_DIR, dir.file_name().unwrap().to_string_lossy());
        let keep = |v: &mut Vec<String>| *v = problems_for(v, &folder);
        keep(&mut errors);
        keep(&mut warnings);
    }
    Ok(json!({ "ok": errors.is_empty(), "errors": errors, "warnings": warnings }))
}

/// Set `draft: false`, check, build, upload and deploy. Used by the MCP tool (after the
/// user says yes) and by `folderblog podcast publish`.
pub fn publish(ctx: &Ctx, blog: &Path, slug: &str) -> Result<Json> {
    let cfg = podcast_on(blog)?;
    if cfg.podcast.storage.is_none() {
        bail!("there is nowhere to host the audio yet; ask the user to run `folderblog podcast setup {}` first", blog.display());
    }
    let dir = episode_dir(blog, slug)?;
    let folder = format!("{}/{}", podcast::PODCAST_DIR, dir.file_name().unwrap().to_string_lossy());
    // Only the draft line changes, so the notes and commented-out options stay as written.
    let original = std::fs::read_to_string(dir.join("index.md"))?;
    let published = set_draft_false(&original)?;
    if published != original {
        write_episode_text(blog, &dir, &published)?;
    }
    let undo = |why: anyhow::Error| -> anyhow::Error {
        if published != original {
            let _ = write_episode_text(blog, &dir, &original);
        }
        why
    };
    let rep = crate::check::check(blog, None).map_err(&undo)?;
    let errors: Vec<String> = rep.errors.iter().filter(|e| e.contains(&format!("{folder}/")) || e.starts_with("podcast")).cloned().collect();
    if !errors.is_empty() {
        return Err(undo(anyhow!("not published, and it is a draft again; fix these first:\n- {}", errors.join("\n- "))));
    }
    let site = Site::load(blog, LoadOptions::default()).map_err(&undo)?;
    let ep = site.podcast.as_ref().and_then(|s| s.episodes.iter().find(|e| e.folder == folder)).ok_or_else(|| undo(anyhow!("the episode did not make it into the build")))?;
    let enclosure = ep.media.as_ref().and_then(|m| m.enclosure.clone());
    let page = crate::templates::absolute(&cfg.base_url, &site.base_path, &crate::templates::prefix_base(&site.base_path, &ep.item.route));
    // Waits for a watcher build/deploy already in progress, and holds the watcher off until this one is done.
    let _lock = crate::deploy::lock(blog, || eprintln!("waiting for a build or deploy that is already running"))?;
    crate::build::build(blog, None, LoadOptions { persist_state: true, ..Default::default() }, true)?;
    let detail = match crate::deploy::deploy_with(blog, false, &ctx.config_dir) {
        Ok(crate::deploy::Outcome::Deployed(d)) => d,
        Ok(crate::deploy::Outcome::Unchanged) => "already live".into(),
        Ok(crate::deploy::Outcome::NoTarget) => bail!("the blog has no [deploy] target, so it was built but not published"),
        // The episode stays published in the folder; the watcher (or the next deploy) retries.
        Err(e) => bail!("the episode is marked published, but the deploy didn't finish: {e:#}. It is retried automatically; `folderblog status` shows progress"),
    };
    Ok(json!({
        "published": true,
        "episode_url": page,
        "feed_url": podcast::feed_url(&cfg),
        "enclosure": enclosure,
        "deploy": detail,
        "audio_checked": "the episode file is on the media host and its size matches",
    }))
}

pub fn publish_episode(ctx: &Ctx, a: &PublishArgs) -> Result<Json> {
    let blog = resolve_blog(ctx, &a.blog)?;
    let cfg = podcast_on(&blog)?;
    episode_dir(&blog, &a.slug)?;
    if !a.confirm {
        bail!("not published. Show the user the episode's title, date, show notes and what changes in the feed, ask them, and only after they say yes call publish_episode again with confirm: true");
    }
    if !cfg.podcast.mcp_publish {
        return Ok(json!({
            "published": false,
            "reason": "this blog asks the user to publish episodes themselves ([podcast] mcp_publish = false)",
            "command": format!("folderblog podcast publish --episode {} {}", a.slug, blog.display()),
        }));
    }
    publish(ctx, &blog, &a.slug)
}

// ---------- the MCP server ----------

fn result(r: Result<Json>) -> Result<CallToolResult, ErrorData> {
    Ok(match r {
        Ok(v) => CallToolResult::success(vec![ContentBlock::text(serde_json::to_string_pretty(&v).unwrap_or_default())]),
        Err(e) => CallToolResult::error(vec![ContentBlock::text(format!("{e:#}"))]),
    })
}

#[derive(Clone)]
pub struct Server {
    ctx: Ctx,
    tool_router: ToolRouter<Server>,
}

#[tool_router]
impl Server {
    pub fn new(ctx: Ctx) -> Server {
        Server { ctx, tool_router: Self::tool_router() }
    }

    #[tool(
        description = "Whether the blog's podcast is on, its show settings, whether audio hosting is set up (never any secrets), episode counts and problems.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn podcast_status(&self, Parameters(a): Parameters<BlogArgs>) -> Result<CallToolResult, ErrorData> {
        result(podcast_status(&self.ctx, &a))
    }

    #[tool(description = "Every episode, newest first, with draft state, URL, guid, duration and its check status.", annotations(read_only_hint = true, open_world_hint = false))]
    fn list_episodes(&self, Parameters(a): Parameters<BlogArgs>) -> Result<CallToolResult, ErrorData> {
        result(list_episodes(&self.ctx, &a))
    }

    #[tool(
        description = "Size, duration, format, bitrate and embedded title of an audio or video file, and whether it can be a podcast episode (MP3 for audio, MP4 for video).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn inspect_audio(&self, Parameters(a): Parameters<InspectArgs>) -> Result<CallToolResult, ErrorData> {
        result(inspect_audio(&a))
    }

    #[tool(
        description = "Make a new DRAFT episode from an audio file: copies it (and any transcript) into podcast/<date>-<slug>/, writes index.md with draft: true and a new permanent guid, numbers the episode if no number is given, checks it, and returns the duration, any transcript text, the front matter and problems. You write the title and show notes; nothing is published.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    fn create_episode(&self, Parameters(a): Parameters<CreateArgs>) -> Result<CallToolResult, ErrorData> {
        result(create_episode(&self.ctx, &a))
    }

    #[tool(
        description = "Change an episode's front matter or show notes, or add a transcript. Values are checked strictly and bad ones are refused with a clear message. Cannot publish (use publish_episode) or change the guid.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    fn update_episode(&self, Parameters(a): Parameters<UpdateArgs>) -> Result<CallToolResult, ErrorData> {
        result(update_episode(&self.ctx, &a))
    }

    #[tool(description = "Write a valid Podcasting 2.0 chapters.json for an episode from a list of {start (seconds), title, url?, img?}.", annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false))]
    fn set_chapters(&self, Parameters(a): Parameters<ChaptersArgs>) -> Result<CallToolResult, ErrorData> {
        result(set_chapters(&self.ctx, &a))
    }

    #[tool(description = "Run `folderblog check` with the strict podcast rules (drafts included) and return the errors, for the whole blog or one episode.", annotations(read_only_hint = true, open_world_hint = false))]
    fn validate(&self, Parameters(a): Parameters<ValidateArgs>) -> Result<CallToolResult, ErrorData> {
        result(validate(&self.ctx, &a))
    }

    #[tool(
        description = "PUBLIC: publish an episode to the world. Before calling this, show the user the episode's title, date, show notes and what changes in the feed, and get an explicit yes. Then call with confirm: true. It sets draft: false, uploads the audio, deploys the blog, checks the hosted file, and returns the live episode URL and the feed URL. Refuses without confirm: true.",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = true)
    )]
    fn publish_episode(&self, Parameters(a): Parameters<PublishArgs>) -> Result<CallToolResult, ErrorData> {
        result(publish_episode(&self.ctx, &a))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("folderblog", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "folderblog podcast tools. To put up an episode: inspect_audio, create_episode (a draft), write good show notes with update_episode, set_chapters if useful, validate, then show the user the title, date, notes and feed changes and only after an explicit yes call publish_episode with confirm: true.",
            )
    }
}

/// Serve on stdin/stdout until the client goes away.
pub fn serve(ctx: Ctx) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    rt.block_on(async move {
        use rmcp::ServiceExt;
        let service = Server::new(ctx).serve(rmcp::transport::stdio()).await.map_err(|e| anyhow!("starting the MCP server: {e}"))?;
        service.waiting().await.map_err(|e| anyhow!("MCP server stopped: {e}"))?;
        Ok(())
    })
}
