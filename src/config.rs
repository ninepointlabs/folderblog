//! `blog.toml` parsing. Every key accepted here is documented in AGENTS.md via
//! `contract::CONFIG_KEYS`; a test keeps the two in sync.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::Path;

pub const CONFIG_FILE: &str = "blog.toml";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub title: String,
    pub base_url: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_permalink")]
    pub permalink: String,
    #[serde(default)]
    pub feed: FeedConfig,
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default)]
    pub watch: WatchConfig,
    #[serde(default)]
    pub deploy: Option<DeployConfig>,
    #[serde(default)]
    pub params: toml::Table,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedConfig {
    #[serde(default = "default_feed_limit")]
    pub limit: usize,
}

impl Default for FeedConfig {
    fn default() -> Self {
        FeedConfig { limit: default_feed_limit() }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildConfig {
    #[serde(default)]
    pub pre: Vec<String>,
    #[serde(default)]
    pub post: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchConfig {
    /// Paths (relative to the blog folder, prefix match) whose changes never trigger a rebuild.
    #[serde(default)]
    pub ignore: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum DeployConfig {
    Git {
        remote: String,
        #[serde(default = "default_branch")]
        branch: String,
        #[serde(default = "default_true")]
        nojekyll: bool,
        #[serde(default = "default_git_name")]
        author_name: String,
        #[serde(default = "default_git_email")]
        author_email: String,
    },
    Command {
        command: String,
    },
    /// GitHub Pages via the folderblog GitHub App login. Creates the repo and turns on
    /// Pages on first deploy, so a new blog needs no manual GitHub steps.
    Github {
        owner: String,
        repo: String,
        #[serde(default = "default_branch")]
        branch: String,
        #[serde(default)]
        domain: Option<String>,
        #[serde(default)]
        private: bool,
    },
}

fn default_language() -> String {
    "en".into()
}
fn default_permalink() -> String {
    "/{year}/{month}/{slug}/".into()
}
fn default_feed_limit() -> usize {
    20
}
fn default_branch() -> String {
    "gh-pages".into()
}
fn default_true() -> bool {
    true
}
fn default_git_name() -> String {
    "folderblog".into()
}
fn default_git_email() -> String {
    "folderblog@localhost".into()
}


impl Config {
    pub fn load(blog_dir: &Path) -> Result<Config> {
        let path = blog_dir.join(CONFIG_FILE);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("in {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Config> {
        let mut cfg: Config = toml::from_str(text)?;
        cfg.base_url = cfg.base_url.trim_end_matches('/').to_string();
        if !(cfg.base_url.starts_with("http://") || cfg.base_url.starts_with("https://")) {
            bail!("base_url must start with http:// or https:// (got {:?})", cfg.base_url);
        }
        if !cfg.permalink.starts_with('/') || !cfg.permalink.contains("{slug}") {
            bail!("permalink must start with / and contain {{slug}} (got {:?})", cfg.permalink);
        }
        Ok(cfg)
    }

    /// Path part of base_url ("" or "/sub"), so sites can live below a domain root.
    pub fn base_path(&self) -> String {
        match self.base_url.splitn(4, '/').nth(3) {
            Some(p) if !p.is_empty() => format!("/{}", p.trim_end_matches('/')),
            _ => String::new(),
        }
    }
}

/// blog.toml for a new blog. `github` is (owner, repo, domain) when logged in to GitHub.
pub fn default_config(name: &str, github: Option<(&str, &str, Option<&str>)>) -> String {
    let host = if name.contains('.') { name.to_string() } else { format!("{name}.example.com") };
    let (base_url, deploy) = match github {
        Some((owner, repo, domain)) => {
            let base = match domain {
                Some(d) => format!("https://{d}"),
                None => crate::github::pages_url(owner, repo),
            };
            let domain_line = match domain {
                Some(d) => format!("domain = \"{d}\"\n"),
                None => "# domain = \"example.com\"   # custom domain; also set base_url and your DNS\n".into(),
            };
            (
                base,
                format!(
                    "# Published to GitHub Pages. The repo is created and Pages turned on at the first deploy.\n[deploy]\ntype = \"github\"\nowner = \"{owner}\"\nrepo = \"{repo}\"\n{domain_line}"
                ),
            )
        }
        None => (format!("https://{host}"), String::new()),
    };
    let text = format!(
        r#"# folderblog site configuration. See AGENTS.md for every key.
title = "{name}"
base_url = "{base_url}"
description = ""
author = ""
language = "en"
# Tokens: {{year}} {{month}} {{day}} {{slug}}. Changing this changes every post URL.
permalink = "/{{year}}/{{month}}/{{slug}}/"

[feed]
limit = 20

# Optional toolchain hooks, run with the blog folder as working directory.
# Post-build hooks get $FOLDERBLOG_OUT (the fresh build directory) and may modify it.
# [build]
# pre = []
# post = []

{deploy}
# Other deploy targets:
# [deploy]
# type = "git"
# remote = "git@github.com:you/{name}.git"
# branch = "gh-pages"
#
# [deploy]
# type = "command"
# command = 'rsync -a --delete "$FOLDERBLOG_OUT/" host:/var/www/{name}/'

# Free-form values, available to templates as site.params / params.
[params]
"#
    );
    text
}
