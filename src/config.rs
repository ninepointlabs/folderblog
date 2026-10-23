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
    pub gallery: GalleryConfig,
    #[serde(default)]
    pub podcast: PodcastConfig,
    #[serde(default)]
    pub analytics: AnalyticsConfig,
    #[serde(default)]
    pub deploy: Option<DeployConfig>,
    /// When set, deploy only while the blog folder is a git checkout on this branch with no
    /// merge, rebase or other git operation in progress.
    #[serde(default)]
    pub deploy_branch: Option<String>,
    /// Default share image (path or URL) for pages without their own front matter `image`.
    #[serde(default)]
    pub image: Option<String>,
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
#[serde(deny_unknown_fields)]
pub struct GalleryConfig {
    /// Publish every picture in gallery/ as a photo gallery.
    #[serde(default)]
    pub enabled: bool,
    /// Where the gallery lives on the site, e.g. "/gallery/" or "/photos/".
    #[serde(default = "default_gallery_url")]
    pub url: String,
}

impl Default for GalleryConfig {
    fn default() -> Self {
        GalleryConfig { enabled: false, url: default_gallery_url() }
    }
}

/// The podcast: one show per blog, its episodes in `podcast/`. Off by default.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PodcastConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Where the show page lives, e.g. "/podcast/". The feed is `<url>feed.xml`.
    #[serde(default = "default_podcast_url")]
    pub url: String,
    /// Show title; default: the podcast/index.md title, else the site title.
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub owner_name: String,
    /// Public in the feed.
    #[serde(default)]
    pub owner_email: String,
    /// Default: the site language.
    #[serde(default)]
    pub language: String,
    /// An Apple Podcasts category, e.g. "Technology", with an optional subcategory.
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub subcategory: String,
    #[serde(default)]
    pub explicit: bool,
    /// "episodic" or "serial".
    #[serde(default = "default_podcast_type", rename = "type")]
    pub show_type: String,
    /// Show art, a path in the blog folder (inside podcast/) or an https URL.
    #[serde(default)]
    pub image: String,
    /// podcast:guid. Set once by `folderblog podcast on`; never change it.
    #[serde(default)]
    pub guid: String,
    #[serde(default = "default_true")]
    pub locked: bool,
    #[serde(default)]
    pub license: String,
    /// "podcast" or "video"; default: video when every episode is an MP4.
    #[serde(default)]
    pub medium: String,
    #[serde(default)]
    pub funding: Vec<Funding>,
    #[serde(default)]
    pub persons: Vec<Person>,
    /// Prefix enclosure URLs with https://op3.dev/e/ for open download stats.
    #[serde(default)]
    pub op3: bool,
    /// Also list episodes in the main feed and on the home page (front matter `in_main_feed` overrides).
    #[serde(default)]
    pub in_main_feed: bool,
    /// A Lightning address (name@domain) for listener tips, written as podcast:value.
    #[serde(default)]
    pub lightning_address: String,
    /// Name shown for the Lightning recipient; default: the show author.
    #[serde(default)]
    pub lightning_name: String,
    #[serde(default)]
    pub storage: Option<StorageConfig>,
    /// Let an agent publish through `folderblog mcp` (after the user says yes). When false,
    /// `publish_episode` returns the command for the user to run instead.
    #[serde(default = "default_true")]
    pub mcp_publish: bool,
}

impl Default for PodcastConfig {
    fn default() -> Self {
        toml::from_str("").expect("defaults")
    }
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Funding {
    pub url: String,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Person {
    pub name: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub href: String,
    #[serde(default)]
    pub img: String,
}

/// Where episode audio is hosted. Only the public URL is needed to build the feed; the
/// rest lets deploys upload it. Keys live in `~/.config/folderblog/storage/<bucket>.json`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    /// "r2" (Cloudflare R2), "s3" (any S3-compatible host at `endpoint`), or empty: you
    /// upload the files yourself and deploys only check they are there.
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub bucket: String,
    /// R2 only: "eu" or "fedramp" for buckets created in a jurisdiction.
    #[serde(default)]
    pub jurisdiction: String,
    /// s3 only: the API endpoint, e.g. "https://s3.us-west-004.backblazeb2.com".
    #[serde(default)]
    pub endpoint: String,
    /// Signing region (default "auto", which R2 uses).
    #[serde(default)]
    pub region: String,
    /// Public base URL of the media host, e.g. "https://media.example.com" or "https://pub-xxxx.r2.dev".
    pub public_url: String,
}

/// Optional, off by default: cookie-free GoatCounter page counts, added to built HTML only.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyticsConfig {
    /// Base URL of a GoatCounter site, e.g. "https://stats.example.com". Empty means off.
    #[serde(default)]
    pub goatcounter: String,
    /// Count each page as "host/path", so one GoatCounter site can serve several blogs.
    #[serde(default = "default_true")]
    pub host_prefix: bool,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        AnalyticsConfig { goatcounter: String::new(), host_prefix: true }
    }
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
fn default_gallery_url() -> String {
    "/gallery/".into()
}
fn default_podcast_url() -> String {
    "/podcast/".into()
}
fn default_podcast_type() -> String {
    "episodic".into()
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
        let g = cfg.gallery.url.trim().trim_matches('/');
        if g.is_empty() || g.contains("..") || g.contains(['?', '#']) {
            bail!("gallery.url must be a path like \"/gallery/\" (got {:?})", cfg.gallery.url);
        }
        cfg.gallery.url = format!("/{g}/");
        let pu = cfg.podcast.url.trim().trim_matches('/');
        if pu.is_empty() || pu.contains("..") || pu.contains(['?', '#']) {
            bail!("podcast.url must be a path like \"/podcast/\" (got {:?})", cfg.podcast.url);
        }
        cfg.podcast.url = format!("/{pu}/");
        if !matches!(cfg.podcast.show_type.as_str(), "episodic" | "serial") {
            bail!("podcast.type must be \"episodic\" or \"serial\" (got {:?})", cfg.podcast.show_type);
        }
        if !matches!(cfg.podcast.medium.as_str(), "" | "podcast" | "video") {
            bail!("podcast.medium must be \"podcast\" or \"video\" (got {:?})", cfg.podcast.medium);
        }
        if let Some(st) = &mut cfg.podcast.storage {
            let u = st.public_url.trim().trim_end_matches('/');
            if !u.starts_with("https://") || u.len() <= "https://".len() || u.contains(char::is_whitespace) {
                bail!("podcast.storage.public_url must be an https:// URL like \"https://media.example.com\" (got {:?})", st.public_url);
            }
            st.public_url = u.to_string();
            let bucket_ok = |b: &str| (3..=63).contains(&b.len()) && b.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            match st.kind.as_str() {
                "" => {}
                "r2" => {
                    if st.account_id.trim().is_empty() || !bucket_ok(&st.bucket) {
                        bail!("podcast.storage type \"r2\" needs account_id and bucket (3-63 lowercase letters, digits or -); run `folderblog podcast setup`");
                    }
                    if !matches!(st.jurisdiction.as_str(), "" | "default" | "eu" | "fedramp") {
                        bail!("podcast.storage.jurisdiction must be \"eu\" or \"fedramp\" when set (got {:?})", st.jurisdiction);
                    }
                }
                "s3" => {
                    if !(st.endpoint.starts_with("https://") || st.endpoint.starts_with("http://")) || st.bucket.trim().is_empty() {
                        bail!("podcast.storage type \"s3\" needs endpoint (an http(s) URL) and bucket");
                    }
                }
                other => bail!("podcast.storage.type must be \"r2\" or \"s3\" (got {other:?})"),
            }
        }
        let gc = cfg.analytics.goatcounter.trim().trim_end_matches('/');
        let host = gc.strip_prefix("https://").unwrap_or("");
        if !gc.is_empty()
            && (host.is_empty() || host.starts_with('/') || gc.contains(|c: char| c.is_whitespace() || "\"'<>`?#\\".contains(c)))
        {
            bail!("analytics.goatcounter must be an https:// URL like \"https://stats.example.com\" (got {:?})", cfg.analytics.goatcounter);
        }
        cfg.analytics.goatcounter = gc.to_string();
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

# Photo gallery: every picture in gallery/ (subfolders are albums). Or run `folderblog gallery on`.
# [gallery]
# enabled = true
# url = "/gallery/"

# Podcast: episodes in podcast/, a feed at /podcast/feed.xml. Or run `folderblog podcast on`.
# [podcast]
# enabled = true

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
