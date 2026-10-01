mod build;
mod check;
mod cli;
mod config;
mod content;
mod contract;
mod deploy;
mod feed;
mod fixtures;
mod gallery;
mod github;
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
        Cmd::New { name, root, domain, repo, no_github } => {
            let dir = if name.contains('/') { PathBuf::from(&name) } else { default_root(root).join(&name) };
            let base = dir.file_name().context("bad path")?.to_string_lossy().to_string();
            let login = if no_github { None } else { github::load() };
            let gh = login.as_ref().map(|t| (t.login.clone(), github::repo_name(repo.as_deref().unwrap_or(&base))));
            new_blog(&dir, gh.as_ref().map(|(o, r)| (o.as_str(), r.as_str(), domain.as_deref())))?;
            println!("created {}", dir.display());
            match &gh {
                Some((owner, repo)) => {
                    let url = domain.as_ref().map(|d| format!("https://{d}/")).unwrap_or_else(|| format!("{}/", github::pages_url(owner, repo)));
                    println!("publishes to GitHub Pages: repo {owner}/{repo} is created on the first deploy, site at {url}");
                    if let Some(d) = &domain {
                        println!("add DNS for {d}: {}", dns_help(d, owner));
                    }
                    println!("the watch service deploys it automatically; or run `folderblog deploy {}`", dir.display());
                }
                None if !no_github => println!("not logged in to GitHub, so no deploy target was set; run `folderblog login github` first, or edit [deploy] in blog.toml"),
                None => println!("no deploy target set; edit [deploy] in blog.toml"),
            }
            Ok(0)
        }
        Cmd::Login { service, client_id } => {
            if service != "github" {
                bail!("only `folderblog login github` is supported");
            }
            let tok = github::login(client_id.as_deref(), |url, code| {
                println!("Open {url} and enter the code:  {code}");
                println!("(waiting for you to approve folderblog on GitHub…)");
            })?;
            println!("logged in to GitHub as {}; this login lasts about {} days and renews itself", tok.login, tok.login_days_left());
            println!("if the watch service is running, anything waiting to publish goes out within 5 minutes");
            Ok(0)
        }
        Cmd::Logout { service } => {
            if service != "github" {
                bail!("only `folderblog logout github` is supported");
            }
            if github::logout()? {
                println!("forgot the GitHub login (to revoke it on GitHub too: Settings → Applications → Authorized GitHub Apps)");
            } else {
                println!("not logged in");
            }
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
                    use std::io::Write;
                    let mut out = std::io::stdout().lock();
                    for o in &plan.outputs {
                        // Piping into `head` closes stdout early; that's not an error.
                        if writeln!(out, "{}\t{}", o.url, o.origin).is_err() {
                            break;
                        }
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
                        render::Content::Render { ctx, .. } => {
                            use std::io::Write;
                            let _ = writeln!(std::io::stdout().lock(), "{}", serde_json::to_string_pretty(ctx)?);
                        }
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
                match github::load() {
                    Some(t) if t.login_days_left() < 0 => println!("GitHub: login EXPIRED; run `folderblog login github`"),
                    Some(t) => println!("GitHub: logged in as {} (login renews itself; re-login needed in {} days)", t.login, t.login_days_left()),
                    None => println!("GitHub: not logged in"),
                }
                if blogs.is_empty() {
                    println!("no blogs (folders containing blog.toml)");
                }
                for b in &blogs {
                    print!("{}", status::describe(&b.file_name().unwrap().to_string_lossy(), &status::load(b)));
                    if let Some(config::DeployConfig::Github { owner, repo, .. }) = config::Config::load(b).ok().and_then(|c| c.deploy) {
                        if github::load().is_some() {
                            match github::pages_status(&owner, &repo) {
                                Ok(s) => println!("     pages   GitHub Pages build: {s}"),
                                Err(e) => println!("     pages   could not ask GitHub: {e:#}"),
                            }
                        }
                    }
                }
            }
            let bad = blogs.iter().any(|b| !status::healthy(&status::load(b)));
            Ok(if bad { 1 } else { 0 })
        }
        Cmd::InstallService { root, no_start, print } => install_service(&default_root(root), no_start, print),
        Cmd::Gallery { state, blog } => gallery_cmd(&blog_dir(blog)?, state.as_deref()),
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

fn gallery_cmd(blog: &Path, state: Option<&str>) -> Result<i32> {
    let cfg_path = blog.join(config::CONFIG_FILE);
    let cfg = config::Config::load(blog)?;
    let page = format!("{}{}", cfg.base_url, cfg.gallery.url);
    let Some(state) = state else {
        let n = std::fs::read_dir(blog.join(gallery::GALLERY_DIR)).map(|d| d.count()).unwrap_or(0);
        match cfg.gallery.enabled {
            true => println!("gallery is on: {page} (from {} entries in gallery/)", n),
            false => println!("gallery is off; `folderblog gallery on` turns it on"),
        }
        return Ok(0);
    };
    let on = state == "on";
    let text = std::fs::read_to_string(&cfg_path)?;
    if cfg.gallery.enabled != on {
        std::fs::write(&cfg_path, gallery::set_enabled(&text, on)?)?;
    }
    if !on {
        println!("gallery is off; gallery/ and the theme's _gallery/ templates were left as they are");
        return Ok(0);
    }
    let dir = blog.join(gallery::GALLERY_DIR);
    if !dir.is_dir() {
        std::fs::create_dir_all(&dir)?;
        println!("created {}", dir.display());
    }
    // Templates come from the default theme; ones the blog already has are never overwritten.
    let src = fixtures::DEFAULT_THEME.get_dir("_gallery").context("default theme has no _gallery/")?;
    for f in src.files() {
        let target = blog.join("theme").join(f.path());
        if !target.exists() {
            std::fs::create_dir_all(target.parent().unwrap())?;
            std::fs::write(&target, f.contents())?;
            println!("added theme/{}", f.path().display());
        }
    }
    let base = std::fs::read_to_string(blog.join("theme/_layouts/base.html")).unwrap_or_default();
    if !base.contains("gallery") {
        println!("note: the theme's navigation doesn't link to the gallery yet; add e.g.");
        println!("  {{% if gallery.enabled and gallery.count %}}<a href=\"{{{{ gallery.url }}}}\">{{{{ gallery.title }}}}</a>{{% endif %}}");
        println!("  to theme/_layouts/base.html");
    }
    println!("gallery is on: drop pictures into {} and they appear at {page}", dir.display());
    println!("subfolders become albums; a sidecar like sunset.md beside sunset.jpg adds a title, caption and tags");
    Ok(0)
}

fn dns_help(domain: &str, owner: &str) -> String {
    if domain.matches('.').count() >= 2 && !domain.starts_with("www.") {
        format!("a CNAME record {domain} → {}.github.io", owner.to_lowercase())
    } else {
        format!(
            "A records for {domain} → 185.199.108.153, 185.199.109.153, 185.199.110.153, 185.199.111.153 (and optionally CNAME www → {}.github.io)",
            owner.to_lowercase()
        )
    }
}

pub fn new_blog(dir: &Path, github: Option<(&str, &str, Option<&str>)>) -> Result<()> {
    if dir.join(config::CONFIG_FILE).exists() {
        bail!("{} already has a blog.toml", dir.display());
    }
    let name = dir.file_name().context("bad path")?.to_string_lossy().to_string();
    for d in ["posts", "pages", "drafts", "static", "data", "theme"] {
        std::fs::create_dir_all(dir.join(d))?;
    }
    fixtures::DEFAULT_THEME.extract(dir.join("theme"))?;
    std::fs::write(dir.join(config::CONFIG_FILE), config::default_config(&name, github))?;
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
    // Installed from a package: the unit already ships in /usr/lib/systemd/user.
    let packaged = Path::new("/usr/lib/systemd/user/folderblog.service");
    if !print && packaged.exists() && exe == Path::new("/usr/bin/folderblog") && *root == home().join("Blogs") {
        if !no_start {
            let st = std::process::Command::new("systemctl").args(["--user", "enable", "--now", APP]).status()?;
            if !st.success() {
                bail!("systemctl --user enable --now {APP} failed");
            }
            println!("enabled the packaged service; logs: journalctl --user -u {APP} -f");
        } else {
            println!("the package already installed the unit; start it with: systemctl --user enable --now {APP}");
        }
        return Ok(0);
    }
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
