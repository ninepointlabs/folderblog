mod build;
mod check;
mod cli;
mod config;
mod content;
mod contract;
mod deploy;
mod feed;
mod fixtures;
mod markdown;
mod preview;
mod render;
mod site;
mod state;
mod status;
mod templates;
mod util;
mod watch;

#[cfg(test)]
mod tests;

use anyhow::{Context, Result, bail};
use clap::Parser;
use cli::{Cli, Cmd};
use site::LoadOptions;
use std::path::{Path, PathBuf};

pub const APP: &str = "folderblog";

fn default_root(root: Option<PathBuf>) -> PathBuf {
    root.unwrap_or_else(|| home().join("Blogs"))
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

fn blog_dir(p: Option<PathBuf>) -> Result<PathBuf> {
    let p = p.unwrap_or_else(|| PathBuf::from("."));
    if !p.join(config::CONFIG_FILE).is_file() {
        bail!("{} is not a blog folder (no {})", p.display(), config::CONFIG_FILE);
    }
    Ok(p.canonicalize()?)
}

fn main() {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
    }
}

fn run(cli: Cli) -> Result<i32> {
    match cli.cmd {
        Cmd::New { name, root } => {
            let dir = if name.contains('/') { PathBuf::from(&name) } else { default_root(root).join(&name) };
            new_blog(&dir)?;
            println!("created {}", dir.display());
            println!("next: write posts/*.md, then `folderblog preview {}`; configure [deploy] in blog.toml", dir.display());
            Ok(0)
        }
        Cmd::Build { blog, out, drafts, fixtures } => {
            let blog = blog_dir(blog)?;
            let record = out.is_none() && !drafts && fixtures.is_none();
            let persist = fixtures.is_none();
            let opts = LoadOptions { drafts, fixtures, persist_state: persist, ..Default::default() };
            let r = build::build(&blog, out.as_deref(), opts, record)?;
            for w in &r.warnings {
                eprintln!("warning: {w}");
            }
            println!("built {} files into {}", r.outputs, r.out_dir.display());
            Ok(0)
        }
        Cmd::Preview { blog, port, fixtures } => {
            preview::serve(&blog_dir(blog)?, port, fixtures)?;
            Ok(0)
        }
        Cmd::Check { blog, fixtures } => {
            let blog = blog_dir(blog)?;
            let rep = check::check(&blog, fixtures.clone())?;
            for w in &rep.warnings {
                println!("warning: {w}");
            }
            for e in &rep.errors {
                println!("error: {e}");
            }
            let label = fixtures.map(|f| format!(" (fixtures: {f})")).unwrap_or_default();
            if rep.errors.is_empty() {
                println!("ok: {} files rendered, feed valid, no broken internal links{label}", rep.outputs);
                Ok(0)
            } else {
                println!("{} error(s){label}", rep.errors.len());
                Ok(1)
            }
        }
        Cmd::Data { url, blog, drafts, fixtures } => {
            let blog = blog_dir(blog)?;
            let site = site::Site::load(&blog, LoadOptions { drafts, fixtures, ..Default::default() })?;
            let plan = site.plan()?;
            match url {
                None => {
                    for o in &plan.outputs {
                        println!("{}\t{}", o.url, o.origin);
                    }
                }
                Some(u) => {
                    let u = if u.starts_with('/') { u } else { format!("/{u}") };
                    let want = templates::prefix_base(&site.base_path, &u);
                    let candidates = [want.clone(), format!("{want}/"), want.trim_end_matches("index.html").to_string()];
                    let o = plan
                        .outputs
                        .iter()
                        .find(|o| candidates.contains(&o.url))
                        .with_context(|| format!("no output at {want}; run `folderblog data` to list URLs"))?;
                    match &o.content {
                        render::Content::Render { ctx, .. } => println!("{}", serde_json::to_string_pretty(ctx)?),
                        _ => bail!("{} is not rendered from a template ({})", o.url, o.origin),
                    }
                }
            }
            Ok(0)
        }
        Cmd::Watch { root } => {
            let root = default_root(root);
            std::fs::create_dir_all(&root)?;
            watch::watch(&root)?;
            Ok(0)
        }
        Cmd::Deploy { blog, force } => {
            let blog = blog_dir(blog)?;
            build::build(&blog, None, LoadOptions { persist_state: true, ..Default::default() }, true)?;
            match deploy::deploy(&blog, force)? {
                deploy::Outcome::NoTarget => bail!("no [deploy] section in blog.toml"),
                deploy::Outcome::Unchanged => println!("unchanged since last deploy (use --force to redeploy)"),
                deploy::Outcome::Deployed(d) => println!("deployed: {d}"),
            }
            Ok(0)
        }
        Cmd::Status { root, json } => {
            let root = default_root(root);
            let blogs = watch::list_blogs(&root);
            let daemon = std::process::Command::new("systemctl")
                .args(["--user", "is-active", APP])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|_| "unknown".into());
            if json {
                let v: serde_json::Map<String, serde_json::Value> = blogs
                    .iter()
                    .map(|b| (b.file_name().unwrap().to_string_lossy().to_string(), serde_json::to_value(status::load(b)).unwrap()))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({"root": root, "daemon": daemon, "blogs": v}))?);
            } else {
                println!("root {}  (service: {daemon})", root.display());
                if blogs.is_empty() {
                    println!("no blogs (folders containing blog.toml)");
                }
                for b in &blogs {
                    print!("{}", status::describe(&b.file_name().unwrap().to_string_lossy(), &status::load(b)));
                }
            }
            let bad = blogs.iter().any(|b| !status::healthy(&status::load(b)));
            Ok(if bad { 1 } else { 0 })
        }
        Cmd::InstallService { root, no_start, print } => install_service(&default_root(root), no_start, print),
        Cmd::AgentsMd { blog, write } => {
            let blog = blog_dir(blog)?;
            let path = blog.join("AGENTS.md");
            let existing = std::fs::read_to_string(&path).ok();
            let text = contract::update(existing.as_deref());
            if write {
                std::fs::write(&path, text)?;
                println!("wrote {}", path.display());
            } else {
                print!("{text}");
            }
            Ok(0)
        }
    }
}

pub fn new_blog(dir: &Path) -> Result<()> {
    if dir.join(config::CONFIG_FILE).exists() {
        bail!("{} already has a blog.toml", dir.display());
    }
    let name = dir.file_name().context("bad path")?.to_string_lossy().to_string();
    for d in ["posts", "pages", "drafts", "static", "data", "theme"] {
        std::fs::create_dir_all(dir.join(d))?;
    }
    fixtures::DEFAULT_THEME.extract(dir.join("theme"))?;
    std::fs::write(dir.join(config::CONFIG_FILE), config::default_config(&name))?;
    std::fs::write(dir.join("AGENTS.md"), contract::update(None))?;
    std::fs::write(dir.join(".gitignore"), ".blog/\n")?;
    std::fs::write(
        dir.join("drafts").join("welcome.md"),
        "# Welcome\n\nFiles in `drafts/` only show up in `folderblog preview`. Move a file into `posts/` to publish it.\n\nNo front matter needed: the first heading is the title, the filename is the slug, and the date is when folderblog first sees the post.\n",
    )?;
    Ok(())
}

fn install_service(root: &Path, no_start: bool, print: bool) -> Result<i32> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let unit = format!(
        "[Unit]\nDescription=folderblog: publish blogs from {root}\nAfter=default.target\n\n[Service]\nType=simple\nExecStart={exe} watch --root {root}\nRestart=on-failure\nRestartSec=5\nEnvironment=RUST_BACKTRACE=1\n\n[Install]\nWantedBy=default.target\n",
        root = root.display(),
        exe = exe.display()
    );
    if print {
        print!("{unit}");
        return Ok(0);
    }
    let dir = home().join(".config/systemd/user");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{APP}.service"));
    std::fs::write(&path, &unit)?;
    println!("wrote {}", path.display());
    if exe.to_string_lossy().contains("/target/") {
        println!("note: the unit points at a cargo build directory ({}); `cargo install --path .` gives a stable path", exe.display());
    }
    if no_start {
        return Ok(0);
    }
    for args in [vec!["--user", "daemon-reload"], vec!["--user", "enable", "--now", APP]] {
        let st = std::process::Command::new("systemctl").args(&args).status()?;
        if !st.success() {
            bail!("systemctl {} failed", args.join(" "));
        }
    }
    println!("started; logs: journalctl --user -u {APP} -f");
    Ok(0)
}
