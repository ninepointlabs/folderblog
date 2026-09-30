//! Engine state under `.blog/`: first-seen dates. Keyed by slug so that moving
//! `posts/x.md` to `posts/x/index.md` keeps its date.

use anyhow::{Context, Result};
use chrono::{DateTime, FixedOffset, Local};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const STATE_DIR: &str = ".blog";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub first_seen: BTreeMap<String, String>,
    #[serde(skip)]
    dirty: bool,
    #[serde(skip)]
    path: Option<PathBuf>,
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
        st.path = Some(path);
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

    pub fn save(&mut self) -> Result<()> {
        if !self.dirty || !self.persist {
            return Ok(());
        }
        let path = self.path.as_ref().expect("state path");
        std::fs::create_dir_all(path.parent().unwrap())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        self.dirty = false;
        Ok(())
    }
}
