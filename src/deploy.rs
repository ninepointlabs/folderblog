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
        DeployConfig::Git { remote, branch, nojekyll, author_name, author_email } => Box::new(GitTarget {
            remote,
            branch,
            nojekyll,
            author_name,
            author_email,
            env: vec![],
            cname: None,
        }),
        DeployConfig::Command { command } => Box::new(CommandTarget { command }),
        DeployConfig::Github { owner, repo, branch, domain, private } => {
            Box::new(GithubTarget { owner, repo, branch, domain, private })
        }
    }
}

pub struct GitTarget {
    pub remote: String,
    pub branch: String,
    pub nojekyll: bool,
    pub author_name: String,
    pub author_email: String,
    /// Extra environment for git (e.g. credentials for the GitHub target).
    pub env: Vec<(String, String)>,
    /// Written as CNAME in the pushed tree, so a Pages custom domain survives redeploys.
    pub cname: Option<String>,
}

fn git(dir: &Path, args: &[&str], env: &[(String, String)]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .envs(env.iter().map(|(k, v)| (k, v)))
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
            git(&work, &["init", "-q"], &self.env)?;
            git(&work, &["remote", "add", "origin", &self.remote], &self.env)?;
        } else {
            git(&work, &["remote", "set-url", "origin", &self.remote], &self.env)?;
        }
        let remote_has_branch = git(&work, &["fetch", "-q", "--depth", "1", "origin", &self.branch], &self.env).is_ok();
        let branch_ref = format!("refs/heads/{}", self.branch);
        if remote_has_branch {
            git(&work, &["checkout", "-q", "-f", "-B", &self.branch, "FETCH_HEAD"], &self.env)?;
        } else {
            git(&work, &["symbolic-ref", "HEAD", &branch_ref], &self.env)?;
            let _ = git(&work, &["read-tree", "--empty"], &self.env);
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
        if let Some(domain) = &self.cname {
            if !out.join("CNAME").exists() {
                std::fs::write(work.join("CNAME"), format!("{domain}\n"))?;
            }
        }
        git(&work, &["add", "-A"], &self.env)?;
        let unchanged = Command::new("git")
            .args(["diff", "--cached", "--quiet"])
            .current_dir(&work)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .status()?
            .success();
        let has_head = git(&work, &["rev-parse", "--verify", "-q", "HEAD"], &self.env).is_ok();
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
                &self.env,
            )?;
        }
        git(&work, &["push", "-q", "origin", &format!("HEAD:{branch_ref}")], &self.env)?;
        let sha = git(&work, &["rev-parse", "--short", "HEAD"], &self.env)?;
        Ok(format!("pushed {sha}"))
    }
}

pub struct GithubTarget {
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub domain: Option<String>,
    pub private: bool,
}

impl Target for GithubTarget {
    fn describe(&self) -> String {
        let url = match &self.domain {
            Some(d) => format!("https://{d}"),
            None => crate::github::pages_url(&self.owner, &self.repo),
        };
        format!("GitHub Pages {}/{} → {url}", self.owner, self.repo)
    }

    fn deploy(&self, blog_dir: &Path, out: &Path) -> Result<String> {
        let tok = crate::github::access_token()?;
        let title = Config::load(blog_dir).map(|c| c.title).unwrap_or_default();
        let created = crate::github::ensure_repo(&self.owner, &self.repo, self.private, &format!("{title} (published by folderblog)"))?;
        let git = GitTarget {
            remote: format!("https://github.com/{}/{}.git", self.owner, self.repo),
            branch: self.branch.clone(),
            nojekyll: true,
            author_name: tok.login.clone(),
            author_email: format!("{}@users.noreply.github.com", tok.login),
            env: crate::github::git_env(&tok.access_token),
            cname: self.domain.clone(),
        };
        let pushed = git.deploy(blog_dir, out).map_err(|e| {
            // git's own message for a bad token is "could not read Username"; ask the API
            // whether the login is the real problem so the user gets the right advice.
            match crate::github::check() {
                Err(auth) if crate::github::auth_problem(&auth).is_some() => auth,
                _ => e,
            }
        })?;
        let enabled = crate::github::ensure_pages(&self.owner, &self.repo, &self.branch, self.domain.as_deref())?;
        let mut notes = vec![];
        if created {
            notes.push("created repo".to_string());
        }
        if enabled {
            notes.push("enabled Pages".to_string());
        }
        notes.push(pushed);
        Ok(notes.join(", "))
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
        auth_problem: res.as_ref().err().and_then(crate::github::auth_problem).map(|p| p.to_string()),
    });
    if res.is_ok() {
        st.deployed_hash = Some(hash);
        st.last_good_deploy = st.deploy.as_ref().map(|d| d.time.clone());
    }
    status::save(blog_dir, &st)?;
    res.map(Outcome::Deployed)
}
