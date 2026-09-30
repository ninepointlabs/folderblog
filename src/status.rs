//! Per-blog status in `.blog/status.json`, read by `folderblog status`.

use crate::state::STATE_DIR;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct BuildStatus {
    pub time: String,
    pub ok: bool,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub outputs: usize,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DeployStatus {
    pub time: String,
    pub ok: bool,
    pub target: String,
    pub detail: Option<String>,
    pub error: Option<String>,
    /// Hash of the output tree last deployed successfully.
    pub hash: Option<String>,
    /// Set when the failure is a login problem the user must fix.
    #[serde(default)]
    pub auth_problem: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Status {
    pub build: Option<BuildStatus>,
    pub last_good_build: Option<String>,
    pub deploy: Option<DeployStatus>,
    pub last_good_deploy: Option<String>,
    pub deployed_hash: Option<String>,
}

pub fn load(blog_dir: &Path) -> Status {
    std::fs::read_to_string(blog_dir.join(STATE_DIR).join("status.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(blog_dir: &Path, st: &Status) -> Result<()> {
    let dir = blog_dir.join(STATE_DIR);
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join("status.json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(st)?)?;
    std::fs::rename(tmp, dir.join("status.json"))?;
    Ok(())
}

pub fn healthy(st: &Status) -> bool {
    st.build.as_ref().is_none_or(|b| b.ok) && st.deploy.as_ref().is_none_or(|d| d.ok)
}

/// Human summary for one blog.
pub fn describe(name: &str, st: &Status) -> String {
    let mut s = String::new();
    let mark = if healthy(st) { "ok  " } else { "FAIL" };
    s.push_str(&format!("{mark} {name}\n"));
    match &st.build {
        Some(b) if b.ok => s.push_str(&format!("     build   ok at {} ({} files, {} ms)\n", b.time, b.outputs, b.duration_ms)),
        Some(b) => {
            s.push_str(&format!("     build   FAILED at {}\n", b.time));
            for line in b.error.as_deref().unwrap_or("").lines().take(15) {
                s.push_str(&format!("             {line}\n"));
            }
            if let Some(g) = &st.last_good_build {
                s.push_str(&format!("             last good build {g} is still what is deployed\n"));
            }
        }
        None => s.push_str("     build   never built\n"),
    }
    for w in st.build.iter().flat_map(|b| b.warnings.iter()).take(5) {
        s.push_str(&format!("     warning {w}\n"));
    }
    match &st.deploy {
        Some(d) if d.ok => s.push_str(&format!(
            "     deploy  ok at {} via {}{}\n",
            d.time,
            d.target,
            d.detail.as_ref().map(|x| format!(" ({x})")).unwrap_or_default()
        )),
        Some(d) => {
            s.push_str(&format!("     deploy  FAILED at {} via {}\n", d.time, d.target));
            if let Some(p) = &d.auth_problem {
                s.push_str(&format!("             LOGIN: {p}\n             (posts keep building and publish once you log in again)\n"));
            }
            for line in d.error.as_deref().unwrap_or("").lines().take(10) {
                s.push_str(&format!("             {line}\n"));
            }
        }
        None => s.push_str("     deploy  never deployed\n"),
    }
    s
}
