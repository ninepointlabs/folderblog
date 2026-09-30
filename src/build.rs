//! Building a blog into a fresh directory and swapping it in only on success.

use crate::config::Config;
use crate::site::{LoadOptions, Site};
use crate::state::STATE_DIR;
use crate::status::{self, BuildStatus};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

pub struct BuildResult {
    pub out_dir: PathBuf,
    pub outputs: usize,
    pub warnings: Vec<String>,
}

pub fn public_dir(blog_dir: &Path) -> PathBuf {
    blog_dir.join(STATE_DIR).join("public")
}

pub fn run_hooks(blog_dir: &Path, hooks: &[String], out: &Path, phase: &str) -> Result<()> {
    for cmd in hooks {
        let output = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(blog_dir)
            .env("FOLDERBLOG_BLOG", blog_dir)
            .env("FOLDERBLOG_OUT", out)
            .output()
            .with_context(|| format!("starting {phase} hook {cmd:?}"))?;
        if !output.status.success() {
            let tail = tail(&String::from_utf8_lossy(&output.stderr), 20);
            bail!("{phase} hook {cmd:?} failed ({}):\n{tail}", output.status);
        }
    }
    Ok(())
}

pub fn tail(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Build `blog_dir` into `out` (default `.blog/public`). The previous contents of
/// `out` are replaced only if every step succeeds. Records the result in status.
pub fn build(blog_dir: &Path, out: Option<&Path>, opts: LoadOptions, record: bool) -> Result<BuildResult> {
    let started = Instant::now();
    let result = build_inner(blog_dir, out, opts);
    if record {
        let mut st = status::load(blog_dir);
        st.build = Some(BuildStatus {
            time: crate::util::now_rfc3339(),
            ok: result.is_ok(),
            error: result.as_ref().err().map(|e| format!("{e:#}")),
            duration_ms: started.elapsed().as_millis() as u64,
            outputs: result.as_ref().map(|r| r.outputs).unwrap_or(0),
            warnings: result.as_ref().map(|r| r.warnings.clone()).unwrap_or_default(),
        });
        if result.is_ok() {
            st.last_good_build = st.build.as_ref().map(|b| b.time.clone());
        }
        status::save(blog_dir, &st)?;
    }
    result
}

fn build_inner(blog_dir: &Path, out: Option<&Path>, opts: LoadOptions) -> Result<BuildResult> {
    let cfg = Config::load(blog_dir)?;
    let out = out.map(Path::to_path_buf).unwrap_or_else(|| public_dir(blog_dir));
    let parent = out.parent().context("output directory has no parent")?;
    std::fs::create_dir_all(parent)?;
    let name = out.file_name().unwrap().to_string_lossy();
    let tmp = parent.join(format!(".{name}.tmp-{}", std::process::id()));
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp)?;
    }
    std::fs::create_dir_all(&tmp)?;
    let res = (|| -> Result<BuildResult> {
        if opts.fixtures.is_none() {
            run_hooks(blog_dir, &cfg.build.pre, &tmp, "pre-build")?;
        }
        let site = Site::load(blog_dir, opts.clone())?;
        let plan = site.plan()?;
        let n = site.write_to(&plan, &tmp)?;
        if opts.fixtures.is_none() {
            run_hooks(blog_dir, &cfg.build.post, &tmp, "post-build")?;
        }
        Ok(BuildResult { out_dir: out.clone(), outputs: n, warnings: site.warnings })
    })();
    match res {
        Ok(r) => {
            swap_in(&tmp, &out)?;
            Ok(r)
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            Err(e)
        }
    }
}

/// Replace `out` with `fresh` using renames, so readers never see a half-written tree.
pub fn swap_in(fresh: &Path, out: &Path) -> Result<()> {
    if out.exists() {
        let old = out.with_file_name(format!(".{}.old-{}", out.file_name().unwrap().to_string_lossy(), std::process::id()));
        if old.exists() {
            std::fs::remove_dir_all(&old)?;
        }
        std::fs::rename(out, &old)?;
        std::fs::rename(fresh, out)?;
        std::fs::remove_dir_all(&old)?;
    } else {
        std::fs::rename(fresh, out)?;
    }
    Ok(())
}
