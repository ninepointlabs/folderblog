//! Engine state under `.blog/`, plus first-seen dates. Keyed by slug so that moving
//! `posts/x.md` to `posts/x/index.md` keeps its date.
//!
//! First-seen dates decide undated posts' URLs, so they also live in `dates.toml` in the
//! blog folder, which is not ignored by git and travels with copies of the folder. Older
//! versions kept them only in `.blog/state.json`; those dates are still read (and the
//! earliest date for a slug wins), so no recorded date ever changes. `.blog/state.json`
//! keeps a copy too, so an older folderblog sees the same dates.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, FixedOffset, Local};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const STATE_DIR: &str = ".blog";
/// The tracked file with first-seen dates, in the blog folder.
pub const DATES_FILE: &str = "dates.toml";

const DATES_HEADER: &str = "\
# folderblog: the moment each post (or photo) without a date of its own was first
# published. Undated posts' URLs are made from these dates, so keep this file with your
# blog, in git and in copies. Changing a date here changes that post's URL.
";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub first_seen: BTreeMap<String, String>,
    /// Podcast episodes without a front matter `guid`: the one handed out on first sight,
    /// keyed by episode folder.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub podcast_guids: BTreeMap<String, String>,
    #[serde(skip)]
    dirty: bool,
    #[serde(skip)]
    path: Option<PathBuf>,
    #[serde(skip)]
    dates_path: Option<PathBuf>,
    /// When false (check/data/fixtures), new dates are handed out but never saved.
    #[serde(skip)]
    persist: bool,
}

impl State {
    pub fn load(blog_dir: &Path, persist: bool) -> Result<State> {
        let path = blog_dir.join(STATE_DIR).join("state.json");
        let mut st = if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?
        } else {
            State::default()
        };
        let dates_path = blog_dir.join(DATES_FILE);
        let tracked: BTreeMap<String, String> = if dates_path.exists() {
            let text = std::fs::read_to_string(&dates_path)?;
            let dates: BTreeMap<String, String> = toml::from_str(&text)
                .with_context(|| format!("parsing {DATES_FILE} (each line should look like: my-post = \"2026-03-04T10:00:00+01:00\")"))?;
            if let Some((slug, d)) = dates.iter().find(|(_, d)| DateTime::parse_from_rfc3339(d).is_err()) {
                bail!("{DATES_FILE}: {slug} = {d:?} is not a date like \"2026-03-04T10:00:00+01:00\"");
            }
            dates
        } else {
            BTreeMap::new()
        };
        st.dirty = merge_dates(&mut st.first_seen, &tracked);
        st.path = Some(path);
        st.dates_path = Some(dates_path);
        st.persist = persist;
        Ok(st)
    }

    pub fn ephemeral() -> State {
        State::default()
    }

    pub fn first_seen(&mut self, slug: &str) -> DateTime<FixedOffset> {
        if let Some(d) = self.first_seen.get(slug).and_then(|s| DateTime::parse_from_rfc3339(s).ok()) {
            return d;
        }
        // Whole seconds, so the stored and rendered values are identical.
        let now = Local::now().fixed_offset();
        let now = DateTime::parse_from_rfc3339(&now.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)).unwrap();
        self.first_seen.insert(slug.to_string(), now.to_rfc3339());
        self.dirty = true;
        now
    }

    /// The episode guid for `key`: the one handed out before (older versions picked random
    /// ones, which are kept), else `suggested`, which is then kept.
    pub fn podcast_guid(&mut self, key: &str, suggested: &str) -> String {
        if let Some(g) = self.podcast_guids.get(key) {
            return g.clone();
        }
        let g = suggested.to_string();
        self.podcast_guids.insert(key.to_string(), g.clone());
        self.dirty = true;
        g
    }

    pub fn save(&mut self) -> Result<()> {
        if !self.dirty || !self.persist {
            return Ok(());
        }
        let path = self.path.as_ref().expect("state path");
        std::fs::create_dir_all(path.parent().unwrap())?;
        write_atomic(path, &serde_json::to_string_pretty(self)?)?;
        if let Some(dates) = &self.dates_path {
            let wanted = format!("{DATES_HEADER}{}", toml::to_string(&self.first_seen)?);
            if std::fs::read_to_string(dates).ok().as_deref() != Some(wanted.as_str()) {
                write_atomic(dates, &wanted)?;
            }
        }
        self.dirty = false;
        Ok(())
    }
}

fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Adds `other`'s dates to `into`. Where both have a date for a slug, the earlier one wins:
/// it is when the post was really first seen. True if `into` was missing anything or
/// `other` was missing anything (so both copies need saving).
fn merge_dates(into: &mut BTreeMap<String, String>, other: &BTreeMap<String, String>) -> bool {
    let parse = |s: &str| DateTime::parse_from_rfc3339(s).ok();
    let mut changed = into.keys().any(|k| !other.contains_key(k));
    for (slug, d) in other {
        match into.get(slug) {
            Some(cur) if cur == d => {}
            Some(cur) if parse(cur).is_some() && parse(d).is_none_or(|new| parse(cur).unwrap() <= new) => changed = true,
            _ => {
                into.insert(slug.clone(), d.clone());
                changed = true;
            }
        }
    }
    changed
}
