//! Bring an existing blog up to date after folderblog itself is updated. Only adds or refreshes
//! what the engine owns: missing sample `templates/` and the generated part of `AGENTS.md`.
//! Never turns a feature on and never touches `theme/`, blog.toml or content.

use crate::state::STATE_DIR;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

/// `.blog/upgrade.json`: the version last applied, and every sample file the blog has had, so
/// one the owner deleted on purpose is not put back.
#[derive(Default, Serialize, Deserialize)]
struct Record {
    #[serde(default)]
    version: String,
    #[serde(default)]
    templates: BTreeSet<String>,
}

const FILE: &str = "upgrade.json";

/// Apply this version's additions to `blog_dir` once. Returns what changed, for the log.
pub fn upgrade(blog_dir: &Path) -> Result<Vec<String>> {
    let path = blog_dir.join(STATE_DIR).join(FILE);
    let mut rec: Record = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let version = env!("CARGO_PKG_VERSION");
    if rec.version == version {
        return Ok(vec![]);
    }
    let mut changed = vec![];
    // Samples already there count as had; so do ones written before.
    for (rel, _) in crate::fixtures::template_files() {
        if blog_dir.join(&rel).exists() {
            rec.templates.insert(rel);
        }
    }
    for rel in crate::fixtures::write_templates_except(blog_dir, |rel| rec.templates.contains(rel))? {
        changed.push(format!("added {rel}"));
        rec.templates.insert(rel);
    }
    let agents = blog_dir.join("AGENTS.md");
    if let Ok(text) = std::fs::read_to_string(&agents)
        && !crate::contract::is_current(&text)
    {
        std::fs::write(&agents, crate::contract::update(Some(&text)))?;
        changed.push("refreshed AGENTS.md".into());
    }
    rec.version = version.into();
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, serde_json::to_string_pretty(&rec)?)?;
    Ok(changed)
}
