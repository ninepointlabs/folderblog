//! Deploy targets. A target takes a finished output directory and publishes it.
//! To add one: implement `Target` and add a variant to `config::DeployConfig`.

use crate::build::tail;
use crate::config::{Config, DeployConfig};
use crate::state::STATE_DIR;
use crate::status::{self, DeployStatus};
use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

pub trait Target {
    fn describe(&self) -> String;
    /// Publish `out`. Returns a short detail string (e.g. a commit id).
    fn deploy(&self, blog_dir: &Path, out: &Path) -> Result<String>;
}

pub fn target_for(cfg: &DeployConfig) -> Box<dyn Target> {
    match cfg.clone() {
        DeployConfig::Git { remote, branch, nojekyll, author_name, author_email } => {
            Box::new(GitTarget { remote, branch, nojekyll, author_name, author_email })
        }
        DeployConfig::Command { command } => Box::new(CommandTarget { command }),
    }
}

pub struct GitTarget {
    pub remote: String,
    pub branch: String,
    pub nojekyll: bool,
    pub author_name: String,
    pub author_email: String,
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    if !out.status.success() {
        bail!("git {} failed: {}", args.join(" "), tail(&String::from_utf8_lossy(&out.stderr), 10));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

impl Target for GitTarget {
    fn describe(&self) -> String {
        format!("git {} ({})", self.remote, self.branch)
    }

    fn deploy(&self, blog_dir: &Path, out: &Path) -> Result<String> {
        let work = blog_dir.join(STATE_DIR).join("deploy").join("git");
        if !work.join(".git").is_dir() {
            std::fs::create_dir_all(&work)?;
            git(&work, &["init", "-q"])?;
            git(&work, &["remote", "add", "origin", &self.remote])?;
        } else {
            git(&work, &["remote", "set-url", "origin", &self.remote])?;
        }
        let remote_has_branch = git(&work, &["fetch", "-q", "--depth", "1", "origin", &self.branch]).is_ok();
        let branch_ref = format!("refs/heads/{}", self.branch);
        if remote_has_branch {
            git(&work, &["checkout", "-q", "-f", "-B", &self.branch, "FETCH_HEAD"])?;
        } else {
            git(&work, &["symbolic-ref", "HEAD", &branch_ref])?;
            let _ = git(&work, &["read-tree", "--empty"]);
        }
        // Replace the worktree with the build output.
        for e in std::fs::read_dir(&work)? {
            let e = e?;
            if e.file_name() == ".git" {
                continue;
            }
            if e.file_type()?.is_dir() {
                std::fs::remove_dir_all(e.path())?;
            } else {
                std::fs::remove_file(e.path())?;
            }
        }
        crate::util::copy_dir(out, &work)?;
        if self.nojekyll {
            std::fs::write(work.join(".nojekyll"), "")?;
        }
        git(&work, &["add", "-A"])?;
        let unchanged = Command::new("git").args(["diff", "--cached", "--quiet"]).current_dir(&work).status()?.success();
        let has_head = git(&work, &["rev-parse", "--verify", "-q", "HEAD"]).is_ok();
        if unchanged && has_head && remote_has_branch {
            return Ok("no changes".into());
        }
        if !unchanged || !has_head {
            let msg = format!("Publish {}", crate::util::now_rfc3339());
            git(
                &work,
                &[
                    "-c",
                    &format!("user.name={}", self.author_name),
                    "-c",
                    &format!("user.email={}", self.author_email),
                    "commit",
                    "-q",
                    "--allow-empty",
                    "-m",
                    &msg,
                ],
            )?;
        }
        git(&work, &["push", "-q", "origin", &format!("HEAD:{branch_ref}")])?;
        let sha = git(&work, &["rev-parse", "--short", "HEAD"])?;
        Ok(format!("pushed {sha}"))
    }
}

pub struct CommandTarget {
    pub command: String,
}

impl Target for CommandTarget {
    fn describe(&self) -> String {
        format!("command `{}`", self.command)
    }

    fn deploy(&self, blog_dir: &Path, out: &Path) -> Result<String> {
        let output = Command::new("sh")
            .arg("-c")
            .arg(&self.command)
            .current_dir(blog_dir)
            .env("FOLDERBLOG_OUT", out)
            .env("FOLDERBLOG_BLOG", blog_dir)
            .output()
            .context("starting deploy command")?;
        if !output.status.success() {
            bail!(
                "deploy command failed ({}):\n{}",
                output.status,
                tail(&String::from_utf8_lossy(&output.stderr), 20)
            );
        }
        Ok("command succeeded".into())
    }
}

/// Stable hash of a directory tree (paths and contents).
pub fn tree_hash(dir: &Path) -> Result<String> {
    let mut files: Vec<PathBuf> = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect();
    files.sort();
    let mut h = Sha256::new();
    for f in files {
        h.update(f.strip_prefix(dir)?.to_string_lossy().as_bytes());
        h.update([0]);
        h.update(std::fs::read(&f)?);
        h.update([0]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub enum Outcome {
    NoTarget,
    Unchanged,
    Deployed(String),
}

/// Deploy the last good build. Skips if identical to what was last deployed, unless forced.
pub fn deploy(blog_dir: &Path, force: bool) -> Result<Outcome> {
    let cfg = Config::load(blog_dir)?;
    let Some(dcfg) = &cfg.deploy else { return Ok(Outcome::NoTarget) };
    let target = target_for(dcfg);
    let out = crate::build::public_dir(blog_dir);
    if !out.is_dir() {
        bail!("nothing to deploy: no successful build in {}", out.display());
    }
    let hash = tree_hash(&out)?;
    let mut st = status::load(blog_dir);
    if !force && st.deployed_hash.as_deref() == Some(hash.as_str()) && st.deploy.as_ref().is_some_and(|d| d.ok) {
        return Ok(Outcome::Unchanged);
    }
    let res = target.deploy(blog_dir, &out);
    st.deploy = Some(DeployStatus {
        time: crate::util::now_rfc3339(),
        ok: res.is_ok(),
        target: target.describe(),
        detail: res.as_ref().ok().cloned(),
        error: res.as_ref().err().map(|e| format!("{e:#}")),
        hash: Some(hash.clone()),
    });
    if res.is_ok() {
        st.deployed_hash = Some(hash);
        st.last_good_deploy = st.deploy.as_ref().map(|d| d.time.clone());
    }
    status::save(blog_dir, &st)?;
    res.map(Outcome::Deployed)
}
