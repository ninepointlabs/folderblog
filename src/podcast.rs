//! The podcast: one show per blog. Each folder in `podcast/` with an `index.md` is an
//! episode (its show notes) beside one MP3 or MP4 file. The engine owns the show page,
//! episode URLs and `<podcast.url>feed.xml` (iTunes and Podcasting 2.0 tags). Audio is
//! never published with the site: the feed points at the media host, and previews play
//! the local file.

use crate::config::{Config, Person};
use crate::content::{self, Kind, Source};
use crate::site::{Image, Item};
use crate::util::html_escape;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const PODCAST_DIR: &str = "podcast";
/// Theme templates the podcast renders with; `folderblog podcast on` installs them.
pub const TEMPLATES: &[&str] = &["_podcast/index.html", "_podcast/episode.html"];
pub const FEED_FILE: &str = "feed.xml";
/// The Podcasting 2.0 namespace for `podcast:guid` (UUIDv5 of the feed URL).
pub const GUID_NAMESPACE: &str = "ead4c236-bf58-58c6-a2c6-a6b28d128cb6";
/// Where previews serve episode files from, outside the built site.
pub const PREVIEW_MEDIA_PATH: &str = "/__folderblog/media/";
/// Enclosure base URL for fixture builds, which have no media host.
const FIXTURE_MEDIA_URL: &str = "https://media.example.com";
pub const VALIDATORS: &[&str] = &["https://validator.truefans.fm", "https://podba.se/validate/"];

/// Bump when MediaInfo changes, so caches rebuild.
const CACHE_VERSION: &str = "1";
/// Files above these sizes still work, but are slow for listeners; warn.
const LARGE_MP3: u64 = 200 * 1024 * 1024;
const LARGE_MP4: u64 = 2 * 1024 * 1024 * 1024;

/// Audio and video extensions. None of these files is ever copied into the built site.
pub const MEDIA_EXTS: &[&str] = &[
    "mp3", "mp4", "m4a", "m4v", "m4b", "mov", "wav", "flac", "aac", "ogg", "oga", "opus", "aif", "aiff", "webm", "mkv", "wma",
];

pub const APPLE_CATEGORIES: &[(&str, &[&str])] = &[
    ("Arts", &["Books", "Design", "Fashion & Beauty", "Food", "Performing Arts", "Visual Arts"]),
    ("Business", &["Careers", "Entrepreneurship", "Investing", "Management", "Marketing", "Non-Profit"]),
    ("Comedy", &["Comedy Interviews", "Improv", "Stand-Up"]),
    ("Education", &["Courses", "How To", "Language Learning", "Self-Improvement"]),
    ("Fiction", &["Comedy Fiction", "Drama", "Science Fiction"]),
    ("Government", &[]),
    ("History", &[]),
    ("Health & Fitness", &["Alternative Health", "Fitness", "Medicine", "Mental Health", "Nutrition", "Sexuality"]),
    ("Kids & Family", &["Education for Kids", "Parenting", "Pets & Animals", "Stories for Kids"]),
    ("Leisure", &["Animation & Manga", "Automotive", "Aviation", "Crafts", "Games", "Hobbies", "Home & Garden", "Video Games"]),
    ("Music", &["Music Commentary", "Music History", "Music Interviews"]),
    ("News", &["Business News", "Daily News", "Entertainment News", "News Commentary", "Politics", "Sports News", "Tech News"]),
    ("Religion & Spirituality", &["Buddhism", "Christianity", "Hinduism", "Islam", "Judaism", "Religion", "Spirituality"]),
    (
        "Science",
        &["Astronomy", "Chemistry", "Earth Sciences", "Life Sciences", "Mathematics", "Natural Sciences", "Nature", "Physics", "Social Sciences"],
    ),
    ("Society & Culture", &["Documentary", "Personal Journals", "Philosophy", "Places & Travel", "Relationships"]),
    (
        "Sports",
        &[
            "Baseball", "Basketball", "Cricket", "Fantasy Sports", "Football", "Golf", "Hockey", "Rugby", "Running", "Soccer", "Swimming",
            "Tennis", "Volleyball", "Wilderness", "Wrestling",
        ],
    ),
    ("Technology", &[]),
    ("True Crime", &[]),
    ("TV & Film", &["After Shows", "Film History", "Film Interviews", "Film Reviews", "TV Reviews"]),
];

fn ext_of(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

pub fn is_media(p: &Path) -> bool {
    MEDIA_EXTS.contains(&ext_of(p).as_str())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Mp3,
    Mp4,
}

impl MediaKind {
    pub fn mime(self) -> &'static str {
        match self {
            MediaKind::Mp3 => "audio/mpeg",
            MediaKind::Mp4 => "video/mp4",
        }
    }
    pub fn ext(self) -> &'static str {
        match self {
            MediaKind::Mp3 => "mp3",
            MediaKind::Mp4 => "mp4",
        }
    }
    pub fn is_video(self) -> bool {
        self == MediaKind::Mp4
    }
}

/// The accepted kind for a file name, or a plain explanation of why it can't be used.
pub fn media_kind(p: &Path) -> Result<MediaKind, String> {
    match ext_of(p).as_str() {
        "mp3" => Ok(MediaKind::Mp3),
        "mp4" => Ok(MediaKind::Mp4),
        "m4a" | "m4b" | "aac" => Err("M4A/AAC audio isn't supported yet; export the episode as MP3".into()),
        "wav" | "flac" | "aif" | "aiff" => Err("uncompressed or lossless audio can't be published as is; export the episode as MP3".into()),
        "m4v" | "mov" | "webm" | "mkv" => Err("video episodes must be MP4 files; export the episode as MP4".into()),
        e => Err(format!(".{e} files aren't supported; podcasts use MP3 (audio) or MP4 (video)")),
    }
}

/// What folderblog learns once from an episode file (cached in `.blog/`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaInfo {
    pub kind: MediaKind,
    pub bytes: u64,
    /// Seconds.
    pub duration: f64,
    pub sha256: String,
    /// An embedded title (ID3 for MP3), if any.
    #[serde(default)]
    pub title: Option<String>,
}

/// The kind of media the file's first bytes say it is, whatever its name.
fn sniff(path: &Path) -> Option<MediaKind> {
    let mut head = [0u8; 12];
    let mut f = std::fs::File::open(path).ok()?;
    let n = f.read(&mut head).ok()?;
    if n >= 8 && &head[4..8] == b"ftyp" {
        return Some(MediaKind::Mp4);
    }
    if n >= 3 && &head[..3] == b"ID3" {
        return Some(MediaKind::Mp3);
    }
    if n >= 2 && head[0] == 0xFF && head[1] & 0xE0 == 0xE0 {
        return Some(MediaKind::Mp3);
    }
    None
}

/// Size, duration, content hash and type of an MP3 or MP4 file.
pub fn inspect(path: &Path) -> Result<MediaInfo> {
    let kind = media_kind(path).map_err(anyhow::Error::msg)?;
    let bytes = std::fs::metadata(path).with_context(|| format!("reading {}", path.display()))?.len();
    if bytes == 0 {
        bail!("the file is empty");
    }
    match sniff(path) {
        Some(k) if k == kind => {}
        Some(k) => bail!("the file is named .{} but is really an {} file; rename it to .{}", kind.ext(), k.ext().to_uppercase(), k.ext()),
        None => bail!("this doesn't look like an {} file", kind.ext().to_uppercase()),
    }
    let (duration, title) = match kind {
        MediaKind::Mp3 => mp3_duration(path)?,
        MediaKind::Mp4 => (mp4_duration(path)?, None),
    };
    if duration.is_nan() || duration <= 0.0 {
        bail!("could not work out how long it is");
    }
    let mut h = Sha256::new();
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    let sha256 = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    Ok(MediaInfo { kind, bytes, duration, sha256, title })
}

fn mp3_duration(path: &Path) -> Result<(f64, Option<String>)> {
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");
    let mut fr = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| anyhow::anyhow!("cannot read it as MP3: {e}"))?;
    let track = fr.default_track(TrackType::Audio).cloned().context("no audio in the file")?;
    let tb = track.time_base.context("the MP3 has no time base")?;
    let title = mp3_title(fr.as_mut());
    if let Some(t) = track.duration.and_then(|d| tb.calc_duration(d)) {
        return Ok((t.as_secs_f64(), title));
    }
    // No Xing/VBRI header: add up the frames.
    let mut total: u64 = 0;
    while let Ok(Some(p)) = fr.next_packet() {
        if p.track_id == track.id {
            total += p.dur.get();
        }
    }
    let secs = tb.calc_duration(symphonia::core::units::Duration::new(total)).map(|t| t.as_secs_f64()).unwrap_or(0.0);
    Ok((secs, title))
}

fn mp3_title(fr: &mut dyn symphonia::core::formats::FormatReader) -> Option<String> {
    use symphonia::core::meta::StandardTag;
    let mut md = fr.metadata();
    let rev = md.skip_to_latest()?;
    rev.media.tags.iter().find_map(|t| match &t.std {
        Some(StandardTag::TrackTitle(s)) => Some(s.to_string()).filter(|s| !s.trim().is_empty()),
        _ => None,
    })
}

/// Duration from the MP4 movie header (`moov/mvhd`), read without loading the file.
fn mp4_duration(path: &Path) -> Result<f64> {
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let (ms, me) = find_box(&mut f, 0, len, b"moov")?.context("the MP4 has no movie header (moov)")?;
    let (hs, _) = find_box(&mut f, ms, me, b"mvhd")?.context("the MP4 has no mvhd box")?;
    f.seek(SeekFrom::Start(hs))?;
    let mut v = [0u8; 1];
    f.read_exact(&mut v)?;
    let (timescale, duration) = if v[0] == 1 {
        let mut b = [0u8; 3 + 8 + 8 + 4 + 8];
        f.read_exact(&mut b)?;
        (u32::from_be_bytes(b[19..23].try_into()?) as u64, u64::from_be_bytes(b[23..31].try_into()?))
    } else {
        let mut b = [0u8; 3 + 4 + 4 + 4 + 4];
        f.read_exact(&mut b)?;
        let d = u32::from_be_bytes(b[15..19].try_into()?);
        (u32::from_be_bytes(b[11..15].try_into()?) as u64, if d == u32::MAX { u64::MAX } else { d as u64 })
    };
    if timescale == 0 || duration == u64::MAX {
        bail!("the MP4 doesn't say how long it is");
    }
    Ok(duration as f64 / timescale as f64)
}

/// The body (start, end) of the first box named `name` between `start` and `end`.
fn find_box(f: &mut std::fs::File, start: u64, end: u64, name: &[u8; 4]) -> Result<Option<(u64, u64)>> {
    let mut pos = start;
    while pos + 8 <= end {
        f.seek(SeekFrom::Start(pos))?;
        let mut h = [0u8; 8];
        f.read_exact(&mut h)?;
        let mut size = u32::from_be_bytes(h[..4].try_into()?) as u64;
        let mut header = 8;
        if size == 1 {
            let mut l = [0u8; 8];
            f.read_exact(&mut l)?;
            size = u64::from_be_bytes(l);
            header = 16;
        } else if size == 0 {
            size = end - pos;
        }
        if size < header || pos + size > end {
            bail!("the MP4 is damaged (a box runs past the end of the file)");
        }
        if &h[4..8] == name {
            return Ok(Some((pos + header, pos + size)));
        }
        pos += size;
    }
    Ok(None)
}

/// Cache key: the file's identity, so an edited file is inspected again.
fn cache_key(abs: &Path) -> String {
    let meta = std::fs::metadata(abs).ok();
    let len = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let mtime = meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut h = Sha256::new();
    h.update(format!("{CACHE_VERSION}|{}|{len}|{mtime}", abs.display()));
    h.finalize().iter().take(12).map(|b| format!("{b:02x}")).collect()
}

pub fn inspect_cached(abs: &Path, cache_dir: &Path) -> Result<MediaInfo> {
    let json = cache_dir.join(format!("{}.json", cache_key(abs)));
    if let Ok(info) = std::fs::read_to_string(&json).map_err(anyhow::Error::from).and_then(|t| Ok(serde_json::from_str::<MediaInfo>(&t)?)) {
        return Ok(info);
    }
    let info = inspect(abs)?;
    std::fs::create_dir_all(cache_dir)?;
    let tmp = json.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string(&info)?)?;
    std::fs::rename(&tmp, &json)?;
    Ok(info)
}

fn prune_cache(dir: &Path, keep: &HashSet<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !keep.contains(name.split('.').next().unwrap_or("")) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

// ---------- transcripts and chapters ----------

pub fn transcript_type(p: &Path) -> Option<&'static str> {
    match ext_of(p).as_str() {
        "vtt" => Some("text/vtt"),
        "srt" => Some("application/x-subrip"),
        _ => None,
    }
}

static VTT_TIME: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"^(?:\d+:)?[0-5]\d:[0-5]\d\.\d{3}$").unwrap());
static SRT_TIME: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"^\d{2,}:[0-5]\d:[0-5]\d[,.]\d{3}$").unwrap());

/// Check a WebVTT (`vtt`) or SubRip (`srt`) transcript. Returns the number of cues.
pub fn validate_transcript(text: &str, ext: &str) -> Result<usize, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n");
    let time_re = if ext == "vtt" { &*VTT_TIME } else { &*SRT_TIME };
    if ext == "vtt" {
        let first = text.lines().next().unwrap_or("");
        if !(first == "WEBVTT" || first.starts_with("WEBVTT ") || first.starts_with("WEBVTT\t")) {
            return Err("a WebVTT file must start with the line WEBVTT".into());
        }
    }
    let mut cues = 0;
    for (n, line) in text.lines().enumerate() {
        if !line.contains("-->") {
            continue;
        }
        let mut parts = line.splitn(2, "-->");
        let a = parts.next().unwrap_or("").trim();
        let b = parts.next().unwrap_or("").split_whitespace().next().unwrap_or("");
        if !time_re.is_match(a) || !time_re.is_match(b) {
            return Err(format!("line {}: bad cue timing {:?}", n + 1, line.trim()));
        }
        cues += 1;
    }
    if ext == "srt" {
        for (i, block) in text.split("\n\n").filter(|b| !b.trim().is_empty()).enumerate() {
            let mut lines = block.trim_matches('\n').lines();
            let idx = lines.next().unwrap_or("").trim();
            let timing = lines.next().unwrap_or("");
            if idx.parse::<u64>().is_err() || !timing.contains("-->") {
                return Err(format!("cue {}: expected a number, then a timing line like 00:00:01,000 --> 00:00:04,000", i + 1));
            }
        }
    }
    if cues == 0 {
        return Err("no cues (timed lines) found".into());
    }
    Ok(cues)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Chapter {
    pub start: f64,
    pub title: String,
    pub url: Option<String>,
    pub img: Option<String>,
    pub toc: bool,
}

/// Parse and check Podcasting 2.0 JSON chapters. All problems are reported together.
pub fn parse_chapters(text: &str) -> Result<Vec<Chapter>, Vec<String>> {
    let v: Json = serde_json::from_str(text).map_err(|e| vec![format!("not valid JSON: {e}")])?;
    let Some(obj) = v.as_object() else { return Err(vec!["must be a JSON object".into()]) };
    let mut errs = vec![];
    if !obj.get("version").is_some_and(|v| v.is_string()) {
        errs.push("\"version\" is required and must be a string, e.g. \"1.2.0\"".into());
    }
    let Some(list) = obj.get("chapters").and_then(|c| c.as_array()) else {
        errs.push("\"chapters\" is required and must be a list".into());
        return Err(errs);
    };
    if list.is_empty() {
        errs.push("\"chapters\" is empty".into());
    }
    let mut out = vec![];
    let mut last = -1.0f64;
    for (i, c) in list.iter().enumerate() {
        let n = i + 1;
        let Some(c) = c.as_object() else {
            errs.push(format!("chapter {n} is not an object"));
            continue;
        };
        let start = match c.get("startTime").and_then(|s| s.as_f64()) {
            Some(s) if s >= 0.0 => s,
            _ => {
                errs.push(format!("chapter {n}: \"startTime\" is required and must be a number of seconds, 0 or more"));
                continue;
            }
        };
        if start < last {
            errs.push(format!("chapter {n} starts before the chapter above it; list chapters in order"));
        }
        last = start;
        if let Some(e) = c.get("endTime")
            && !e.as_f64().is_some_and(|e| e >= start)
        {
            errs.push(format!("chapter {n}: \"endTime\" must be a number not before startTime"));
        }
        let text_field = |k: &str, errs: &mut Vec<String>| -> Option<String> {
            match c.get(k) {
                None => None,
                Some(Json::String(s)) => Some(s.clone()),
                Some(_) => {
                    errs.push(format!("chapter {n}: \"{k}\" must be a string"));
                    None
                }
            }
        };
        let title = text_field("title", &mut errs).unwrap_or_default();
        let url = text_field("url", &mut errs);
        let img = text_field("img", &mut errs);
        for (k, u) in [("url", &url), ("img", &img)] {
            if let Some(u) = u
                && !u.starts_with("https://")
            {
                errs.push(format!("chapter {n}: \"{k}\" must be an https:// URL (got {u:?})"));
            }
        }
        let toc = match c.get("toc") {
            None => true,
            Some(Json::Bool(b)) => *b,
            Some(_) => {
                errs.push(format!("chapter {n}: \"toc\" must be true or false"));
                true
            }
        };
        out.push(Chapter { start, title, url, img, toc });
    }
    if errs.is_empty() { Ok(out) } else { Err(errs) }
}

// ---------- show settings ----------

pub fn check_category(cat: &str, sub: &str) -> Result<(), String> {
    if cat.is_empty() {
        return Err("podcast.category is not set; pick one of Apple's categories, e.g. \"Technology\"".into());
    }
    let Some((_, subs)) = APPLE_CATEGORIES.iter().find(|(c, _)| *c == cat) else {
        let names: Vec<&str> = APPLE_CATEGORIES.iter().map(|c| c.0).collect();
        return Err(format!("podcast.category {cat:?} is not an Apple Podcasts category; use one of: {}", names.join(", ")));
    };
    if !sub.is_empty() && !subs.contains(&sub) {
        return Err(if subs.is_empty() {
            format!("podcast.category {cat:?} has no subcategories; remove podcast.subcategory")
        } else {
            format!("podcast.subcategory {sub:?} is not under {cat:?}; use one of: {}", subs.join(", "))
        });
    }
    Ok(())
}

/// podcast:guid: UUIDv5 of the feed URL without its scheme and trailing slashes.
pub fn feed_guid(feed_url: &str) -> String {
    let u = feed_url.trim();
    let u = u.split_once("://").map(|(_, r)| r).unwrap_or(u).trim_end_matches('/');
    let ns = uuid::Uuid::parse_str(GUID_NAMESPACE).unwrap();
    uuid::Uuid::new_v5(&ns, u.as_bytes()).to_string()
}

static LN_ADDRESS: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"^[a-zA-Z0-9._+-]+@[a-zA-Z0-9-]+(\.[a-zA-Z0-9-]+)+$").unwrap());

pub fn is_lightning_address(s: &str) -> bool {
    LN_ADDRESS.is_match(s)
}

/// The absolute feed URL for a config.
pub fn feed_url(cfg: &Config) -> String {
    format!("{}{}{FEED_FILE}", cfg.base_url, cfg.podcast.url)
}

// ---------- discovery ----------

#[derive(Default)]
pub struct Found {
    /// `podcast/index.md`: the show's title and intro.
    pub index: Option<PathBuf>,
    pub sources: Vec<Source>,
    /// Non-media files to publish: (absolute, Kind::Episode, path relative to `podcast/`).
    pub loose_files: Vec<(PathBuf, Kind, PathBuf)>,
    /// Media files directly inside each episode folder.
    pub media: HashMap<PathBuf, Vec<PathBuf>>,
}

pub fn discover(root: &Path, drafts: bool, warnings: &mut Vec<String>) -> Result<Found> {
    let base = root.join(PODCAST_DIR);
    let mut out = Found::default();
    if !base.is_dir() {
        return Ok(out);
    }
    let mut disc = content::Discovered::default();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&base)?.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if content::is_debris(&name) {
            continue;
        }
        let rel = format!("{PODCAST_DIR}/{name}");
        if path.is_dir() {
            let Some(index) = ["index.md", "index.markdown"].iter().map(|n| path.join(n)).find(|p| p.is_file()) else {
                warnings.push(format!("{rel}/: no index.md, so this folder is not an episode yet"));
                continue;
            };
            content::load_item(root, &base, &index, Kind::Episode, false, Some(&path), &mut disc)?;
            let mut media = vec![];
            for e in walkdir::WalkDir::new(&path)
                .sort_by_file_name()
                .into_iter()
                .filter_entry(|e| e.depth() == 0 || !content::is_debris(&e.file_name().to_string_lossy()))
            {
                let e = e?;
                if !(e.file_type().is_file() || (e.file_type().is_symlink() && e.path().is_file())) || e.path() == index {
                    continue;
                }
                if is_media(e.path()) {
                    if e.depth() == 1 {
                        media.push(e.path().to_path_buf());
                    }
                    continue;
                }
                disc.loose_files.push((e.path().to_path_buf(), Kind::Episode, e.path().strip_prefix(&base)?.to_path_buf()));
            }
            out.media.insert(path.clone(), media);
        } else if name == "index.md" || name == "index.markdown" {
            out.index = Some(path);
        } else if is_media(&path) {
            warnings.push(format!("{rel}: put each episode's audio in its own folder with an index.md (e.g. podcast/2026-10-30-first-episode/)"));
        } else if content::is_markdown(&path) {
            warnings.push(format!("{rel}: only podcast/index.md and episode folders are used; this file is ignored"));
        } else {
            disc.loose_files.push((path.clone(), Kind::Episode, PathBuf::from(&name)));
        }
    }
    if !drafts {
        let hidden: Vec<PathBuf> = disc.sources.iter().filter(|s| s.draft).filter_map(|s| s.bundle_dir.clone()).collect();
        disc.sources.retain(|s| !s.draft);
        disc.loose_files.retain(|(abs, _, _)| !hidden.iter().any(|h| abs.starts_with(h)));
        out.media.retain(|k, _| !hidden.contains(k));
    }
    out.sources = disc.sources;
    out.loose_files = disc.loose_files;
    Ok(out)
}

// ---------- the show ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipient {
    pub name: String,
    pub address: String,
    pub split: u64,
}

pub struct Media {
    pub file: PathBuf,
    pub info: MediaInfo,
    /// Object key on the media host: `<blog>/<slug>-<sha8>.<ext>`.
    pub key: String,
    /// What the episode page plays: the local file in previews, else the hosted file.
    pub url: Option<String>,
    /// The feed's enclosure URL (with the OP3 prefix when on). None until a media host is set up.
    pub enclosure: Option<String>,
}

pub struct Episode {
    pub item: Item,
    /// The episode folder, e.g. `podcast/2026-10-30-first-episode`.
    pub folder: String,
    pub guid: String,
    pub media: Option<Media>,
    pub season: Option<u64>,
    pub number: Option<u64>,
    pub episode_type: String,
    pub explicit: bool,
    /// (site route, MIME type)
    pub transcript: Option<(String, &'static str)>,
    /// (site route, chapters)
    pub chapters: Option<(String, Vec<Chapter>)>,
    pub image: Option<Image>,
    pub persons: Vec<Person>,
    pub value: Vec<Recipient>,
    pub in_main_feed: bool,
}

pub struct Show {
    pub title: String,
    pub description: String,
    pub intro_html: String,
    pub front: Map<String, Json>,
    /// Site routes without base path.
    pub route: String,
    pub feed_route: String,
    pub image: Option<Image>,
    pub guid: String,
    pub medium: &'static str,
    /// Newest first.
    pub episodes: Vec<Episode>,
    /// Everything `folderblog check` reports as an error.
    pub problems: Vec<String>,
}

pub struct ShowArgs<'a> {
    pub cfg: &'a Config,
    pub blog_name: &'a str,
    pub content_root: &'a Path,
    pub cache_dir: &'a Path,
    pub base_path: &'a str,
    /// Previews play the local file and keep episodes whose audio has problems.
    pub preview: bool,
    pub fixtures: bool,
    pub index: Option<&'a Path>,
    pub media: &'a HashMap<PathBuf, Vec<PathBuf>>,
    /// Published file URL (with base path) -> source file.
    pub files_by_url: &'a HashMap<String, PathBuf>,
}

fn opt_uint(front: &Map<String, Json>, k: &str, rel: &str, problems: &mut Vec<String>) -> Option<u64> {
    match front.get(k) {
        None | Some(Json::Null) => None,
        Some(v) => match v.as_u64().filter(|n| *n > 0) {
            Some(n) => Some(n),
            None => {
                problems.push(format!("{rel}: {k} must be a whole number, 1 or more (got {v})"));
                None
            }
        },
    }
}

fn opt_str<'a>(front: &'a Map<String, Json>, k: &str) -> Option<&'a str> {
    front.get(k).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty())
}

/// Square, 1400-3000 px, JPEG or PNG, RGB: what Apple and the other apps require of art.
pub fn check_art(file: &Path, label: &str) -> Result<(), String> {
    use image::ImageDecoder;
    let ext = ext_of(file);
    if !matches!(ext.as_str(), "jpg" | "jpeg" | "png") {
        return Err(format!("{label} must be a JPEG or PNG"));
    }
    let dec = image::ImageReader::open(file)
        .and_then(|r| r.with_guessed_format())
        .map_err(|e| format!("{label}: cannot read it: {e}"))?
        .into_decoder()
        .map_err(|e| format!("{label}: cannot read it: {e}"))?;
    let (w, h) = dec.dimensions();
    let color = dec.color_type();
    if w != h {
        return Err(format!("{label} must be square (it is {w}×{h} px)"));
    }
    if !(1400..=3000).contains(&w) {
        return Err(format!("{label} must be between 1400 and 3000 px on each side (it is {w} px)"));
    }
    if !matches!(color, image::ColorType::Rgb8 | image::ColorType::Rgba8) {
        return Err(format!("{label} must be an 8-bit RGB image (it is {color:?}); export it as RGB"));
    }
    Ok(())
}

pub fn load_show(a: ShowArgs, episodes: Vec<Item>, state: &mut crate::state::State, warnings: &mut Vec<String>) -> Result<Show> {
    let cfg = a.cfg;
    let pc = &cfg.podcast;
    let mut problems: Vec<String> = vec![];
    let abs_url = |route: &str| crate::templates::absolute(&cfg.base_url, a.base_path, &crate::templates::prefix_base(a.base_path, route));

    // Show page text.
    let (mut title, mut intro_html, mut front) = (String::new(), String::new(), Map::new());
    if let Some(p) = a.index {
        let rel = p.strip_prefix(a.content_root).unwrap_or(p).to_string_lossy().replace('\\', "/");
        let text = std::fs::read_to_string(p).with_context(|| format!("reading {rel}"))?.replace("\r\n", "\n");
        let (fm, _, body) = content::split_front_matter(&text).with_context(|| format!("{rel}: bad front matter"))?;
        let fm_title = fm.get("title").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
        let resolve = |u: &str| if u.starts_with('/') && !u.starts_with("//") { crate::templates::prefix_base(a.base_path, u) } else { u.to_string() };
        let r = crate::markdown::render(&body, fm_title.is_none(), None, &resolve).with_context(|| format!("rendering {rel}"))?;
        title = fm_title.or(r.title).unwrap_or_default();
        intro_html = r.html.trim().to_string();
        front = fm;
    }
    if !pc.title.trim().is_empty() {
        title = pc.title.trim().to_string();
    }
    if title.is_empty() {
        title = cfg.title.clone();
    }
    let description = [pc.description.trim(), &crate::util::strip_tags(&intro_html), cfg.description.trim()]
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or("")
        .to_string();
    if description.is_empty() {
        problems.push("podcast: no description; set podcast.description in blog.toml or write an intro in podcast/index.md".into());
    }
    let feed_route = format!("{}{FEED_FILE}", pc.url);
    let guid = if pc.guid.trim().is_empty() {
        problems.push(format!(
            "podcast.guid is not set in blog.toml; add guid = \"{}\" so the show keeps its identity if its address ever changes",
            feed_guid(&feed_url(cfg))
        ));
        feed_guid(&feed_url(cfg))
    } else {
        pc.guid.trim().to_string()
    };
    if !cfg.base_url.starts_with("https://") {
        problems.push(format!("base_url must be https:// for a podcast (got {})", cfg.base_url));
    }
    if let Err(e) = check_category(pc.category.trim(), pc.subcategory.trim()) {
        problems.push(e);
    }
    if pc.owner_email.trim().is_empty() {
        warnings.push("podcast.owner_email is empty; some directories use it to confirm you own the show (it is public in the feed)".into());
    } else if !pc.owner_email.contains('@') {
        problems.push(format!("podcast.owner_email {:?} is not an email address", pc.owner_email));
    }
    for f in &pc.funding {
        if !f.url.starts_with("https://") {
            problems.push(format!("podcast.funding url must be https:// (got {:?})", f.url));
        }
    }
    for p in &pc.persons {
        for u in [&p.href, &p.img] {
            if !u.is_empty() && !u.starts_with("https://") {
                problems.push(format!("podcast.persons {:?}: links must be https:// (got {u:?})", p.name));
            }
        }
    }
    if !pc.lightning_address.is_empty() && !is_lightning_address(pc.lightning_address.trim()) {
        problems.push(format!("podcast.lightning_address {:?} is not a Lightning address like name@getalby.com", pc.lightning_address));
    }

    // Show art: blog.toml `image`, else podcast/cover.jpg or cover.png.
    let image_src = if pc.image.trim().is_empty() {
        ["cover.jpg", "cover.jpeg", "cover.png"].iter().map(|n| format!("{PODCAST_DIR}/{n}")).find(|p| a.content_root.join(p).is_file())
    } else {
        Some(pc.image.trim().to_string())
    };
    let image = match image_src {
        None => {
            problems.push("podcast: no show art; add a square JPEG or PNG (1400-3000 px) as podcast/cover.jpg".into());
            None
        }
        Some(s) if s.starts_with("https://") => Some(Image { url: s, size: None }),
        Some(s) if crate::util::has_scheme(&s) => {
            problems.push(format!("podcast.image must be https:// (got {s:?})"));
            None
        }
        Some(s) => {
            let rel = s.trim_start_matches('/');
            match rel.strip_prefix(&format!("{PODCAST_DIR}/")) {
                Some(inner) if a.content_root.join(rel).is_file() => {
                    let file = a.content_root.join(rel);
                    if let Err(e) = check_art(&file, &format!("show art {rel}")) {
                        problems.push(e);
                    }
                    Some(Image { url: abs_url(&format!("{}{inner}", pc.url)), size: image::image_dimensions(&file).ok() })
                }
                Some(_) => {
                    problems.push(format!("podcast.image {s:?} is not a file in the blog"));
                    None
                }
                None => {
                    problems.push(format!("podcast.image {s:?} must be inside podcast/ (or an https:// URL)"));
                    None
                }
            }
        }
    };

    let media_base = match (&pc.storage, a.fixtures) {
        (_, true) => Some(FIXTURE_MEDIA_URL.to_string()),
        (Some(s), false) => Some(s.public_url.clone()),
        (None, false) => None,
    };
    let channel_value_on = !pc.lightning_address.is_empty();

    let mut used_cache: HashSet<String> = HashSet::new();
    let mut guids: HashMap<String, String> = HashMap::new();
    let mut out: Vec<Episode> = vec![];
    for item in episodes {
        let bundle = item.src.bundle_dir.clone().expect("episodes are folders");
        let folder = format!("{PODCAST_DIR}/{}", bundle.file_name().unwrap().to_string_lossy());
        let rel = item.src.rel.to_string_lossy().replace('\\', "/");
        let front = item.src.front.clone();
        let mut hard: Option<String> = None;

        // The episode file.
        let media_files = a.media.get(&bundle).cloned().unwrap_or_default();
        let chosen = match opt_str(&front, "audio") {
            Some(name) => {
                let p = bundle.join(name);
                if p.is_file() { Ok(p) } else { Err(format!("{rel}: audio {name:?} is not a file in {folder}/")) }
            }
            None => match media_files.as_slice() {
                [] => Err(format!("{rel}: no episode file; add an MP3 (or MP4 for video) to {folder}/")),
                [one] => Ok(one.clone()),
                many => Err(format!(
                    "{rel}: {} media files in {folder}/; set `audio: <file>` in the front matter to say which is the episode",
                    many.len()
                )),
            },
        };
        let mut media = None;
        match chosen {
            Err(e) => hard = Some(e),
            Ok(file) => {
                let name = file.file_name().unwrap().to_string_lossy().to_string();
                used_cache.insert(cache_key(&file));
                match inspect_cached(&file, a.cache_dir) {
                    Err(e) => hard = Some(format!("{folder}/{name}: {e:#}")),
                    Ok(info) => {
                        let limit = if info.kind.is_video() { LARGE_MP4 } else { LARGE_MP3 };
                        if info.bytes > limit {
                            warnings.push(format!(
                                "{folder}/{name} is {} MB; files this large are slow for listeners to download (most episodes are under {} MB)",
                                info.bytes / 1_000_000,
                                limit / 1_000_000 / 2
                            ));
                        }
                        let key = format!("{}/{}-{}.{}", a.blog_name, item.slug, &info.sha256[..8], info.kind.ext());
                        let hosted = media_base.as_ref().map(|b| format!("{b}/{key}"));
                        let enclosure = hosted.as_ref().map(|u| if pc.op3 { format!("https://op3.dev/e/{}", u.trim_start_matches("https://")) } else { u.clone() });
                        if hosted.is_none() && !a.preview {
                            hard = Some(format!(
                                "{rel}: there is nowhere to host the audio yet, so this episode is left out; run `folderblog podcast setup`, or add `draft: true` until you are ready"
                            ));
                        }
                        let url = if a.preview {
                            let src = file.strip_prefix(a.content_root).unwrap_or(&file).to_string_lossy().replace('\\', "/");
                            Some(format!("{PREVIEW_MEDIA_PATH}{src}"))
                        } else {
                            hosted
                        };
                        media = Some(Media { file, info, key, url, enclosure });
                    }
                }
            }
        }

        // Identity.
        let guid = match front.get("guid") {
            Some(Json::String(g)) if !g.trim().is_empty() => g.trim().to_string(),
            Some(v) => {
                problems.push(format!("{rel}: guid must be a non-empty string (got {v})"));
                state.podcast_guid(&folder)
            }
            None => {
                let g = state.podcast_guid(&folder);
                problems.push(format!(
                    "{rel}: no guid; add `guid: \"{g}\"` to the front matter so the episode is never re-released if the folder is renamed"
                ));
                g
            }
        };
        if let Some(prev) = guids.insert(guid.clone(), folder.clone()) {
            let msg = format!("{rel}: guid {guid:?} is already used by {prev}; every episode needs its own");
            problems.push(msg.clone());
            hard.get_or_insert(msg);
        }

        let season = opt_uint(&front, "season", &rel, &mut problems);
        let number = opt_uint(&front, "episode", &rel, &mut problems);
        let episode_type = match opt_str(&front, "episode_type") {
            None => "full".to_string(),
            Some(t @ ("full" | "trailer" | "bonus")) => t.to_string(),
            Some(t) => {
                problems.push(format!("{rel}: episode_type must be full, trailer or bonus (got {t:?})"));
                "full".to_string()
            }
        };
        let explicit = match front.get("explicit") {
            None => pc.explicit,
            Some(Json::Bool(b)) => *b,
            Some(v) => {
                problems.push(format!("{rel}: explicit must be true or false (got {v})"));
                pc.explicit
            }
        };
        let in_main_feed = match front.get("in_main_feed") {
            None => pc.in_main_feed,
            Some(Json::Bool(b)) => *b,
            Some(v) => {
                problems.push(format!("{rel}: in_main_feed must be true or false (got {v})"));
                pc.in_main_feed
            }
        };

        // Files published beside the episode: name -> route.
        let files: HashMap<String, (PathBuf, String)> = item
            .files
            .iter()
            .map(|(abs, out)| (abs.strip_prefix(&bundle).unwrap_or(abs).to_string_lossy().replace('\\', "/"), (abs.clone(), out.clone())))
            .collect();
        let pick = |key: &str, auto: &dyn Fn(&str) -> bool, problems: &mut Vec<String>| -> Option<(PathBuf, String)> {
            match opt_str(&front, key) {
                Some(n) => match files.get(n) {
                    Some(f) => Some(f.clone()),
                    None => {
                        problems.push(format!("{rel}: {key} {n:?} is not a file in {folder}/"));
                        None
                    }
                },
                None => {
                    let mut found: Vec<&String> = files.keys().filter(|k| !k.contains('/') && auto(k)).collect();
                    found.sort();
                    found.first().map(|k| files[*k].clone())
                }
            }
        };
        let transcript = pick("transcript", &|n| transcript_type(Path::new(n)).is_some(), &mut problems).and_then(|(abs, route)| {
            let Some(ty) = transcript_type(&abs) else {
                problems.push(format!("{rel}: the transcript must be a .vtt or .srt file"));
                return None;
            };
            let text = std::fs::read_to_string(&abs).unwrap_or_default();
            match validate_transcript(&text, &ext_of(&abs)) {
                Ok(_) => Some((route, ty)),
                Err(e) => {
                    problems.push(format!("{folder}/{}: {e}", abs.file_name().unwrap().to_string_lossy()));
                    None
                }
            }
        });
        let chapters = pick("chapters", &|n| n == "chapters.json", &mut problems).and_then(|(abs, route)| {
            let text = std::fs::read_to_string(&abs).unwrap_or_default();
            match parse_chapters(&text) {
                Ok(c) => Some((route, c)),
                Err(errs) => {
                    for e in errs {
                        problems.push(format!("{folder}/{}: {e}", abs.file_name().unwrap().to_string_lossy()));
                    }
                    None
                }
            }
        });
        let image = item.image.clone();
        if let Some(img) = &image {
            if !img.url.starts_with("https://") {
                problems.push(format!("{rel}: image must be https:// for the feed (got {})", img.url));
            }
            let local = a.files_by_url.get(img.url.trim_start_matches(&cfg.base_url[..cfg.base_url.len() - a.base_path.len()]));
            if let Some(file) = local
                && let Err(e) = check_art(file, &format!("{rel}: episode art"))
            {
                problems.push(e);
            }
        }
        let persons: Vec<Person> = match front.get("persons") {
            None => vec![],
            Some(v) => match serde_json::from_value::<Vec<Person>>(v.clone()) {
                Ok(p) => p,
                Err(e) => {
                    problems.push(format!("{rel}: persons must be a list of {{name, role, href, img}}: {e}"));
                    vec![]
                }
            },
        };
        for p in &persons {
            for u in [&p.href, &p.img] {
                if !u.is_empty() && !u.starts_with("https://") {
                    problems.push(format!("{rel}: person {:?}: links must be https:// (got {u:?})", p.name));
                }
            }
        }
        let value = match front.get("value") {
            None => vec![],
            Some(Json::String(s)) if s == "inherit" => vec![],
            Some(v) => match serde_json::from_value::<Vec<Recipient>>(v.clone()) {
                Ok(r) => {
                    for x in &r {
                        if !is_lightning_address(&x.address) {
                            problems.push(format!("{rel}: value recipient {:?}: {:?} is not a Lightning address", x.name, x.address));
                        }
                    }
                    if !r.is_empty() && !channel_value_on {
                        warnings.push(format!("{rel}: per-episode value recipients are set, but the show has no podcast.lightning_address"));
                    }
                    r
                }
                Err(_) => {
                    problems.push(format!("{rel}: value must be `inherit` or a list of {{name, address, split}}"));
                    vec![]
                }
            },
        };

        if let Some(h) = &hard {
            problems.push(h.clone());
            if !a.preview {
                warnings.push(h.clone());
                continue;
            }
        }
        out.push(Episode {
            item,
            folder,
            guid,
            media,
            season,
            number,
            episode_type,
            explicit,
            transcript,
            chapters,
            image,
            persons,
            value,
            in_main_feed,
        });
    }
    prune_cache(a.cache_dir, &used_cache);
    problems.dedup();
    out.sort_by(|x, y| {
        let dx = x.item.date.as_ref().map(|d| d.0);
        let dy = y.item.date.as_ref().map(|d| d.0);
        dy.cmp(&dx).then(x.item.slug.cmp(&y.item.slug))
    });
    let medium = match pc.medium.as_str() {
        "video" => "video",
        "podcast" => "podcast",
        _ if !out.is_empty() && out.iter().all(|e| e.media.as_ref().is_some_and(|m| m.info.kind.is_video())) => "video",
        _ => "podcast",
    };
    Ok(Show { title, description, intro_html, front, route: pc.url.clone(), feed_route, image, guid, medium, episodes: out, problems })
}

pub fn duration_hms(secs: f64) -> String {
    let s = secs.round() as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

// ---------- the feed ----------

fn cdata(s: &str) -> String {
    format!("<![CDATA[{}]]>", s.replace("]]>", "]]]]><![CDATA[>"))
}

fn person_xml(p: &Person) -> String {
    let mut attrs = String::new();
    for (k, v) in [("role", &p.role), ("group", &p.group), ("href", &p.href), ("img", &p.img)] {
        if !v.is_empty() {
            attrs.push_str(&format!(" {k}=\"{}\"", html_escape(v)));
        }
    }
    format!("<podcast:person{attrs}>{}</podcast:person>\n", html_escape(&p.name))
}

fn value_xml(recipients: &[Recipient]) -> String {
    let mut x = String::from("<podcast:value type=\"lightning\" method=\"lnaddress\">\n");
    for r in recipients {
        x.push_str(&format!(
            "<podcast:valueRecipient name=\"{}\" type=\"lnaddress\" address=\"{}\" split=\"{}\"/>\n",
            html_escape(&r.name),
            html_escape(&r.address),
            r.split
        ));
    }
    x.push_str("</podcast:value>\n");
    x
}

/// `<podcast.url>feed.xml`: every episode with an enclosure, newest first.
pub fn rss(site: &crate::site::Site, show: &Show) -> String {
    let cfg = &site.cfg;
    let pc = &cfg.podcast;
    let abs = |route: &str| crate::templates::absolute(&cfg.base_url, &site.base_path, &crate::templates::prefix_base(&site.base_path, route));
    let episodes: Vec<&Episode> = show.episodes.iter().filter(|e| !e.item.src.draft && e.media.as_ref().is_some_and(|m| m.enclosure.is_some())).collect();
    let author = [pc.author.trim(), cfg.author.trim()].into_iter().find(|s| !s.is_empty()).unwrap_or("");
    let language = if pc.language.trim().is_empty() { &cfg.language } else { pc.language.trim() };
    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    x.push_str("<rss version=\"2.0\" xmlns:itunes=\"http://www.itunes.com/dtds/podcast-1.0.dtd\" xmlns:podcast=\"https://podcastindex.org/namespace/1.0\" xmlns:atom=\"http://www.w3.org/2005/Atom\" xmlns:content=\"http://purl.org/rss/1.0/modules/content/\">\n<channel>\n");
    x.push_str(&format!("<title>{}</title>\n", html_escape(&show.title)));
    x.push_str(&format!("<link>{}</link>\n", html_escape(&abs(&show.route))));
    x.push_str(&format!("<description>{}</description>\n", html_escape(&show.description)));
    x.push_str(&format!("<language>{}</language>\n", html_escape(language)));
    x.push_str(&format!("<atom:link href=\"{}\" rel=\"self\" type=\"application/rss+xml\"/>\n", html_escape(&abs(&show.feed_route))));
    if let Some(img) = &show.image {
        x.push_str(&format!("<itunes:image href=\"{}\"/>\n", html_escape(&img.url)));
        x.push_str(&format!(
            "<image><url>{}</url><title>{}</title><link>{}</link></image>\n",
            html_escape(&img.url),
            html_escape(&show.title),
            html_escape(&abs(&show.route))
        ));
    }
    if check_category(pc.category.trim(), pc.subcategory.trim()).is_ok() {
        if pc.subcategory.trim().is_empty() {
            x.push_str(&format!("<itunes:category text=\"{}\"/>\n", html_escape(pc.category.trim())));
        } else {
            x.push_str(&format!(
                "<itunes:category text=\"{}\"><itunes:category text=\"{}\"/></itunes:category>\n",
                html_escape(pc.category.trim()),
                html_escape(pc.subcategory.trim())
            ));
        }
    }
    x.push_str(&format!("<itunes:explicit>{}</itunes:explicit>\n", pc.explicit));
    if !author.is_empty() {
        x.push_str(&format!("<itunes:author>{}</itunes:author>\n", html_escape(author)));
    }
    let owner_name = if pc.owner_name.trim().is_empty() { author } else { pc.owner_name.trim() };
    if !owner_name.is_empty() || !pc.owner_email.trim().is_empty() {
        x.push_str("<itunes:owner>");
        if !owner_name.is_empty() {
            x.push_str(&format!("<itunes:name>{}</itunes:name>", html_escape(owner_name)));
        }
        if !pc.owner_email.trim().is_empty() {
            x.push_str(&format!("<itunes:email>{}</itunes:email>", html_escape(pc.owner_email.trim())));
        }
        x.push_str("</itunes:owner>\n");
    }
    x.push_str(&format!("<itunes:type>{}</itunes:type>\n", html_escape(&pc.show_type)));
    x.push_str(&format!("<podcast:guid>{}</podcast:guid>\n", html_escape(&show.guid)));
    let owner = if pc.owner_email.trim().is_empty() { String::new() } else { format!(" owner=\"{}\"", html_escape(pc.owner_email.trim())) };
    x.push_str(&format!("<podcast:locked{owner}>{}</podcast:locked>\n", if pc.locked { "yes" } else { "no" }));
    x.push_str(&format!("<podcast:medium>{}</podcast:medium>\n", show.medium));
    for f in &pc.funding {
        let text = if f.text.trim().is_empty() { "Support the show" } else { f.text.trim() };
        x.push_str(&format!("<podcast:funding url=\"{}\">{}</podcast:funding>\n", html_escape(&f.url), html_escape(text)));
    }
    for p in &pc.persons {
        x.push_str(&person_xml(p));
    }
    if !pc.license.trim().is_empty() {
        x.push_str(&format!("<podcast:license>{}</podcast:license>\n", html_escape(pc.license.trim())));
    }
    if !pc.lightning_address.trim().is_empty() {
        let name = [pc.lightning_name.trim(), author, show.title.as_str()].into_iter().find(|s| !s.is_empty()).unwrap_or("");
        x.push_str(&value_xml(&[Recipient { name: name.into(), address: pc.lightning_address.trim().into(), split: 100 }]));
    }
    x.push_str(&format!("<generator>folderblog {}</generator>\n", env!("CARGO_PKG_VERSION")));
    if let Some(u) = episodes.iter().filter_map(|e| e.item.date.as_ref().map(|d| d.0)).max() {
        x.push_str(&format!("<lastBuildDate>{}</lastBuildDate>\n", u.to_rfc2822()));
    }
    for e in episodes {
        let it = &e.item;
        let m = e.media.as_ref().unwrap();
        x.push_str("<item>\n");
        x.push_str(&format!("<title>{}</title>\n", html_escape(&it.title)));
        x.push_str(&format!("<link>{}</link>\n", html_escape(&abs(&it.route))));
        x.push_str(&format!("<guid isPermaLink=\"false\">{}</guid>\n", html_escape(&e.guid)));
        if let Some((d, _)) = &it.date {
            x.push_str(&format!("<pubDate>{}</pubDate>\n", d.to_rfc2822()));
        }
        x.push_str(&format!("<description>{}</description>\n", cdata(&crate::feed::absolutize(site, &it.excerpt_html))));
        x.push_str(&format!("<content:encoded>{}</content:encoded>\n", cdata(&crate::feed::absolutize(site, &it.html))));
        x.push_str(&format!(
            "<enclosure url=\"{}\" length=\"{}\" type=\"{}\"/>\n",
            html_escape(m.enclosure.as_deref().unwrap()),
            m.info.bytes,
            m.info.kind.mime()
        ));
        x.push_str(&format!("<itunes:duration>{}</itunes:duration>\n", (m.info.duration.round() as u64).max(1)));
        x.push_str(&format!("<itunes:episodeType>{}</itunes:episodeType>\n", e.episode_type));
        x.push_str(&format!("<itunes:explicit>{}</itunes:explicit>\n", e.explicit));
        if let Some(s) = e.season {
            x.push_str(&format!("<itunes:season>{s}</itunes:season>\n<podcast:season>{s}</podcast:season>\n"));
        }
        if let Some(n) = e.number {
            x.push_str(&format!("<itunes:episode>{n}</itunes:episode>\n<podcast:episode>{n}</podcast:episode>\n"));
        }
        if let Some(img) = &e.image {
            x.push_str(&format!("<itunes:image href=\"{}\"/>\n", html_escape(&img.url)));
        }
        if let Some((route, ty)) = &e.transcript {
            x.push_str(&format!("<podcast:transcript url=\"{}\" type=\"{ty}\"/>\n", html_escape(&abs(route))));
        }
        if let Some((route, _)) = &e.chapters {
            x.push_str(&format!("<podcast:chapters url=\"{}\" type=\"application/json+chapters\"/>\n", html_escape(&abs(route))));
        }
        for p in &e.persons {
            x.push_str(&person_xml(p));
        }
        if !e.value.is_empty() {
            x.push_str(&value_xml(&e.value));
        }
        x.push_str("</item>\n");
    }
    x.push_str("</channel>\n</rss>\n");
    x
}

/// The podcast feed rules apps depend on (PSP-1 required tags, https everywhere, known
/// lengths and durations). Returns the number of episodes, or every problem found.
pub fn validate_feed(xml: &str) -> Result<usize, Vec<String>> {
    use quick_xml::Reader;
    use quick_xml::events::{BytesStart, Event};
    let mut r = Reader::from_str(xml);
    let mut stack: Vec<String> = vec![];
    let mut problems = vec![];
    let mut channel: Vec<String> = vec![];
    let mut item: Vec<String> = vec![];
    let mut items = 0;
    let mut text = String::new();
    let attrs = |e: &BytesStart| -> Vec<(String, String)> {
        e.attributes()
            .filter_map(|a| a.ok())
            .map(|a| {
                let k = a.key.into_inner().to_string();
                let v = a.normalized_value(quick_xml::XmlVersion::Explicit1_0).map(|v| v.to_string()).unwrap_or_default();
                (k, v)
            })
            .collect()
    };
    let on_open = |name: &str, at: Vec<(String, String)>, stack: &Vec<String>, items: usize, problems: &mut Vec<String>| {
        let where_ = if stack.iter().any(|s| s == "item") { format!("episode {items}") } else { "channel".to_string() };
        for (k, v) in &at {
            if matches!(k.as_str(), "href" | "url" | "img") && !v.starts_with("https://") {
                problems.push(format!("{where_}: <{name} {k}> must be an https:// URL (got {v:?})"));
            }
        }
        let get = |k: &str| at.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str());
        if name == "enclosure" {
            if !get("length").and_then(|l| l.parse::<u64>().ok()).is_some_and(|l| l > 0) {
                problems.push(format!("{where_}: <enclosure> has no length in bytes"));
            }
            if !matches!(get("type"), Some("audio/mpeg" | "video/mp4")) {
                problems.push(format!("{where_}: <enclosure> type must be audio/mpeg or video/mp4 (got {:?})", get("type").unwrap_or("")));
            }
        }
        if name == "atom:link" && get("rel") != Some("self") {
            problems.push("channel: <atom:link> must have rel=\"self\"".into());
        }
    };
    loop {
        let ev = match r.read_event() {
            Err(e) => return Err(vec![format!("not well-formed XML at byte {}: {e}", r.buffer_position())]),
            Ok(ev) => ev,
        };
        match ev {
            Event::Eof => break,
            Event::Start(e) => {
                let name = e.name().into_inner().to_string();
                if name == "item" {
                    items += 1;
                    item.clear();
                }
                match stack.last().map(String::as_str) {
                    Some("channel") => channel.push(name.clone()),
                    Some("item") => item.push(name.clone()),
                    _ => {}
                }
                on_open(&name, attrs(&e), &stack, items, &mut problems);
                stack.push(name);
                text.clear();
            }
            Event::Empty(e) => {
                let name = e.name().into_inner().to_string();
                match stack.last().map(String::as_str) {
                    Some("channel") => channel.push(name.clone()),
                    Some("item") => item.push(name.clone()),
                    _ => {}
                }
                on_open(&name, attrs(&e), &stack, items, &mut problems);
            }
            Event::Text(t) => text.push_str(&t.into_inner()),
            Event::GeneralRef(g) => {
                let n = g.into_inner().to_string();
                text.push_str(match n.as_str() {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "apos" => "'",
                    _ => "?",
                });
            }
            Event::End(e) => {
                let name = e.name().into_inner().to_string();
                stack.pop();
                let where_ = if stack.iter().any(|s| s == "item") || name == "item" { format!("episode {items}") } else { "channel".to_string() };
                let t = text.trim();
                match name.as_str() {
                    "link" if !t.starts_with("https://") => problems.push(format!("{where_}: <link> must be an https:// URL (got {t:?})")),
                    "itunes:explicit" if t != "true" && t != "false" => {
                        problems.push(format!("{where_}: <itunes:explicit> must be true or false (got {t:?})"))
                    }
                    "itunes:duration" if !t.parse::<u64>().is_ok_and(|d| d > 0) => {
                        problems.push(format!("{where_}: <itunes:duration> must be a number of seconds above 0 (got {t:?})"))
                    }
                    "podcast:locked" if t != "yes" && t != "no" => problems.push(format!("channel: <podcast:locked> must be yes or no (got {t:?})")),
                    "guid" | "title" | "podcast:guid" | "description" if t.is_empty() && !matches!(stack.last().map(String::as_str), Some("image")) => {
                        problems.push(format!("{where_}: <{name}> is empty"))
                    }
                    "item" => {
                        for req in ["title", "enclosure", "guid", "pubDate", "itunes:duration"] {
                            if !item.iter().any(|f| f == req) {
                                problems.push(format!("episode {items}: missing <{req}>"));
                            }
                        }
                    }
                    _ => {}
                }
                text.clear();
            }
            Event::CData(c) => text.push_str(&c.into_inner()),
            _ => {}
        }
    }
    for req in [
        "title", "link", "description", "language", "atom:link", "itunes:image", "itunes:category", "itunes:explicit", "podcast:guid", "podcast:locked",
    ] {
        if !channel.iter().any(|f| f == req) {
            problems.push(format!("channel: missing <{req}>"));
        }
    }
    if problems.is_empty() { Ok(items) } else { Err(problems) }
}

// ---------- what was deployed ----------

/// The show's and episodes' identities, recorded at each build and copied at each
/// deploy, so `check` can tell when a guid changed after listeners saw it.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub guid: String,
    /// Episode folder -> guid.
    pub episodes: BTreeMap<String, String>,
    /// The episode files the feed points at, for uploading at deploy time.
    #[serde(default)]
    pub media: Vec<MediaEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaEntry {
    pub folder: String,
    pub file: PathBuf,
    pub key: String,
    pub bytes: u64,
    pub sha256: String,
    pub mime: String,
}

pub const BUILT_SNAPSHOT: &str = "podcast-built.json";
pub const DEPLOYED_SNAPSHOT: &str = "podcast-deployed.json";

pub fn snapshot(show: &Show) -> Snapshot {
    Snapshot {
        guid: show.guid.clone(),
        episodes: show.episodes.iter().filter(|e| e.media.as_ref().is_some_and(|m| m.enclosure.is_some())).map(|e| (e.folder.clone(), e.guid.clone())).collect(),
        media: show
            .episodes
            .iter()
            .filter(|e| !e.item.src.draft)
            .filter_map(|e| {
                let m = e.media.as_ref().filter(|m| m.enclosure.is_some())?;
                Some(MediaEntry {
                    folder: e.folder.clone(),
                    file: m.file.clone(),
                    key: m.key.clone(),
                    bytes: m.info.bytes,
                    sha256: m.info.sha256.clone(),
                    mime: m.info.kind.mime().to_string(),
                })
            })
            .collect(),
    }
}

pub fn save_snapshot(blog_dir: &Path, name: &str, s: &Snapshot) -> Result<()> {
    let dir = blog_dir.join(crate::state::STATE_DIR);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(name), serde_json::to_string_pretty(s)?)?;
    Ok(())
}

pub fn load_snapshot(blog_dir: &Path, name: &str) -> Option<Snapshot> {
    let text = std::fs::read_to_string(blog_dir.join(crate::state::STATE_DIR).join(name)).ok()?;
    serde_json::from_str(&text).ok()
}

/// After a successful deploy: what was built is now what listeners have.
pub fn mark_deployed(blog_dir: &Path) {
    if let Some(s) = load_snapshot(blog_dir, BUILT_SNAPSHOT) {
        let _ = save_snapshot(blog_dir, DEPLOYED_SNAPSHOT, &s);
    }
}

/// Guids that changed since the last deploy.
pub fn guid_changes(show: &Show, deployed: &Snapshot) -> Vec<String> {
    let mut v = vec![];
    if !deployed.guid.is_empty() && deployed.guid != show.guid {
        v.push(format!(
            "podcast.guid changed from {} (the deployed feed) to {}; apps would treat this as a new show. Put the old value back in blog.toml",
            deployed.guid, show.guid
        ));
    }
    for e in &show.episodes {
        if let Some(old) = deployed.episodes.get(&e.folder)
            && *old != e.guid
        {
            v.push(format!(
                "{}: guid changed from {old} (already published) to {}; apps would download it again as a new episode. Put `guid: \"{old}\"` back",
                e.folder, e.guid
            ));
        }
    }
    v
}

// ---------- `folderblog podcast on|off` ----------

pub const GITIGNORE_BLOCK: &str = "# folderblog podcast: episode audio is hosted separately, never kept in git\npodcast/**/*.mp3\npodcast/**/*.mp4\npodcast/**/*.m4a\npodcast/**/*.wav\npodcast/**/*.flac\n";

/// Turn the podcast on or off in blog.toml text, keeping comments and layout. Turning it
/// on also sets `guid` once, from the feed URL.
pub fn set_enabled(text: &str, on: bool) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = text.parse().context("blog.toml does not parse")?;
    if !doc.contains_key("podcast") {
        let mut t = toml_edit::Table::new();
        t.decor_mut().set_prefix("\n# Podcast: episodes in podcast/, one folder each. See AGENTS.md for every key.\n");
        doc.insert("podcast", toml_edit::Item::Table(t));
    }
    let t = doc["podcast"].as_table_mut().context("[podcast] in blog.toml is not a table")?;
    t["enabled"] = toml_edit::value(on);
    let out = doc.to_string();
    let cfg = Config::parse(&out).context("blog.toml would not parse after the change; edit [podcast] by hand")?;
    if on && cfg.podcast.guid.trim().is_empty() {
        let mut doc: toml_edit::DocumentMut = out.parse()?;
        doc["podcast"]["guid"] = toml_edit::value(feed_guid(&feed_url(&cfg)));
        return Ok(doc.to_string());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn podcast_guid_matches_the_spec_test_vector() {
        assert_eq!(feed_guid("https://mp3s.nashownotes.com/pc20rss.xml"), "917393e3-1b1e-5cef-ace4-edaa54e1f810");
        assert_eq!(feed_guid("mp3s.nashownotes.com/pc20rss.xml/"), "917393e3-1b1e-5cef-ace4-edaa54e1f810");
    }

    #[test]
    fn categories() {
        assert!(check_category("Technology", "").is_ok());
        assert!(check_category("News", "Tech News").is_ok());
        assert!(check_category("Tech", "").unwrap_err().contains("not an Apple"));
        assert!(check_category("Technology", "Gadgets").unwrap_err().contains("no subcategories"));
        assert!(check_category("News", "Gadgets").unwrap_err().contains("Tech News"));
        assert!(check_category("", "").is_err());
    }

    #[test]
    fn formats() {
        assert_eq!(media_kind(Path::new("a.MP3")), Ok(MediaKind::Mp3));
        assert_eq!(media_kind(Path::new("a.mp4")), Ok(MediaKind::Mp4));
        for (f, want) in [("a.m4a", "MP3"), ("a.wav", "MP3"), ("a.flac", "MP3"), ("a.mov", "MP4"), ("a.ogg", "MP3")] {
            let e = media_kind(Path::new(f)).unwrap_err();
            assert!(e.contains(want), "{f}: {e}");
        }
        assert!(is_media(Path::new("x/episode.WAV")) && !is_media(Path::new("chapters.json")));
    }

    #[test]
    fn transcripts() {
        let vtt = "WEBVTT\n\n00:00.000 --> 00:02.500\nHello\n\n01:00:01.000 --> 01:00:03.000 align:start\nLater\n";
        assert_eq!(validate_transcript(vtt, "vtt"), Ok(2));
        assert!(validate_transcript("00:00.000 --> 00:02.500\nHi\n", "vtt").unwrap_err().contains("WEBVTT"));
        assert!(validate_transcript("WEBVTT\n\n0:0 --> 1:0\nHi\n", "vtt").unwrap_err().contains("line 3"));
        assert!(validate_transcript("WEBVTT\n", "vtt").unwrap_err().contains("no cues"));
        let srt = "1\n00:00:00,000 --> 00:00:02,000\nHello\n\n2\n00:00:02,000 --> 00:00:04,000\nAgain\n";
        assert_eq!(validate_transcript(srt, "srt"), Ok(2));
        assert!(validate_transcript("00:00:00,000 --> 00:00:02,000\nHello\n", "srt").is_err());
    }

    #[test]
    fn chapters_schema() {
        let good = r#"{"version":"1.2.0","chapters":[{"startTime":0,"title":"Intro"},{"startTime":61.5,"title":"Talk","url":"https://x.com","toc":false}]}"#;
        let c = parse_chapters(good).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[1].start, 61.5);
        assert!(!c[1].toc);
        let errs = parse_chapters(r#"{"chapters":[{"startTime":5},{"startTime":1,"img":"http://x/a.jpg"},{"title":"no start"}]}"#).unwrap_err();
        let all = errs.join("\n");
        for want in ["\"version\"", "chapter 2 starts before", "chapter 2: \"img\" must be an https", "chapter 3: \"startTime\""] {
            assert!(all.contains(want), "{want}: {all}");
        }
        assert!(parse_chapters("[]").is_err());
        assert!(parse_chapters("{nope").unwrap_err()[0].contains("JSON"));
    }

    #[test]
    fn lightning_addresses() {
        assert!(is_lightning_address("tim@rizful.com"));
        assert!(!is_lightning_address("tim"));
        assert!(!is_lightning_address("tim@localhost"));
        assert!(!is_lightning_address("02abc…pubkey"));
    }

    #[test]
    fn durations() {
        assert_eq!(duration_hms(59.6), "1:00");
        assert_eq!(duration_hms(3725.0), "1:02:05");
    }

    #[test]
    fn toggling_keeps_comments_and_sets_the_guid_once() {
        let base = "# my blog\ntitle = \"t\"\nbase_url = \"https://x.com\"\n\n[params]\na = 1 # keep\n";
        let on = set_enabled(base, true).unwrap();
        assert!(on.contains("# my blog") && on.contains("a = 1 # keep"), "{on}");
        let cfg = Config::parse(&on).unwrap();
        assert!(cfg.podcast.enabled);
        assert_eq!(cfg.podcast.guid, feed_guid("https://x.com/podcast/feed.xml"));
        let off = set_enabled(&on, false).unwrap();
        let cfg_off = Config::parse(&off).unwrap();
        assert!(!cfg_off.podcast.enabled);
        assert_eq!(cfg_off.podcast.guid, cfg.podcast.guid, "guid is kept when turned off");
        // A changed base_url later never recomputes an existing guid.
        let moved = set_enabled(&off.replace("https://x.com", "https://new.example"), true).unwrap();
        assert_eq!(Config::parse(&moved).unwrap().podcast.guid, cfg.podcast.guid);
    }
}
