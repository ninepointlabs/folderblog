//! The daemon: watch a root of blogs, debounce bursts of saves, build and deploy.
//! Each blog builds on its own thread, so a hung hook or failed build in one blog
//! never blocks or affects another.

use crate::content::is_debris;
use crate::config::{CONFIG_FILE, Config};
use crate::site::LoadOptions;
use crate::state::STATE_DIR;
use anyhow::Result;
use notify::{RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub const QUIET: Duration = Duration::from_millis(700);
pub const MAX_WAIT: Duration = Duration::from_secs(8);

pub fn log(msg: impl AsRef<str>) {
    // stderr goes to the journal under systemd.
    eprintln!("[folderblog] {}", msg.as_ref());
}

pub fn notify_desktop(summary: &str, body: &str, critical: bool) {
    if std::env::var_os("FOLDERBLOG_NO_NOTIFY").is_some() {
        return;
    }
    let _ = std::process::Command::new("notify-send")
        .args(["-a", "folderblog", "-u", if critical { "critical" } else { "normal" }, summary, body])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut c| c.wait());
}

pub fn list_blogs(root: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.join(CONFIG_FILE).is_file())
        .collect();
    v.sort();
    v
}

/// Which blog (if any) a changed path belongs to, and whether it should trigger a rebuild.
pub fn relevant(root: &Path, path: &Path) -> Option<PathBuf> {
    let rel = path.strip_prefix(root).ok()?;
    let mut comps = rel.components();
    let blog = root.join(comps.next()?.as_os_str());
    let inner: PathBuf = comps.collect();
    if inner.as_os_str().is_empty() {
        return None;
    }
    // Engine state, VCS metadata, and the agent contract never trigger builds.
    let first = inner.components().next()?.as_os_str().to_string_lossy().to_string();
    if first == STATE_DIR || first == ".git" || first == "AGENTS.md" || first == "node_modules" {
        return None;
    }
    let name = path.file_name()?.to_string_lossy().to_string();
    if is_debris(&name) || inner.iter().any(|c| {
        let c = c.to_string_lossy();
        c.starts_with('.') && c != ".well-known"
    }) {
        return None;
    }
    if !blog.join(CONFIG_FILE).is_file() {
        return None;
    }
    if let Ok(cfg) = Config::load(&blog) {
        let inner_s = inner.to_string_lossy().replace('\\', "/");
        if cfg.watch.ignore.iter().any(|ig| inner_s.starts_with(ig.trim_start_matches("./"))) {
            return None;
        }
    }
    Some(blog)
}

/// Build then deploy one blog, recording status and notifying on failure/recovery.
pub fn build_and_deploy(blog: &Path) -> bool {
    let name = blog.file_name().unwrap().to_string_lossy().to_string();
    let was_healthy = crate::status::healthy(&crate::status::load(blog));
    let started = Instant::now();
    let opts = LoadOptions { persist_state: true, ..Default::default() };
    let built = std::panic::catch_unwind(|| crate::build::build(blog, None, opts, true));
    let built = match built {
        Ok(r) => r,
        Err(_) => Err(anyhow::anyhow!("internal error: build panicked")),
    };
    match built {
        Err(e) => {
            let msg = format!("{e:#}");
            // A panic is not recorded by build(); make sure status shows it.
            let mut st = crate::status::load(blog);
            if st.build.as_ref().is_none_or(|b| b.ok) {
                st.build = Some(crate::status::BuildStatus {
                    time: crate::util::now_rfc3339(),
                    ok: false,
                    error: Some(msg.clone()),
                    ..Default::default()
                });
                let _ = crate::status::save(blog, &st);
            }
            log(format!("{name}: build FAILED, live site unchanged:\n{msg}"));
            notify_desktop(&format!("folderblog: {name} build failed"), msg.lines().take(4).collect::<Vec<_>>().join("\n").as_str(), true);
            false
        }
        Ok(r) => {
            log(format!("{name}: built {} files in {} ms", r.outputs, started.elapsed().as_millis()));
            for w in &r.warnings {
                log(format!("{name}: warning: {w}"));
            }
            match crate::deploy::deploy(blog, false) {
                Ok(crate::deploy::Outcome::NoTarget) => log(format!("{name}: no [deploy] target configured")),
                Ok(crate::deploy::Outcome::Unchanged) => log(format!("{name}: output unchanged, nothing to deploy")),
                Ok(crate::deploy::Outcome::Deployed(d)) => log(format!("{name}: deployed ({d})")),
                Err(e) => {
                    let msg = format!("{e:#}");
                    log(format!("{name}: deploy FAILED:\n{msg}"));
                    notify_desktop(&format!("folderblog: {name} deploy failed"), &msg, true);
                    return false;
                }
            }
            if !was_healthy {
                notify_desktop(&format!("folderblog: {name} recovered"), "Build and deploy succeeded.", false);
            }
            true
        }
    }
}

struct Pending {
    first: Instant,
    last: Instant,
}

pub fn watch(root: &Path) -> Result<()> {
    let root = root.canonicalize()?;
    log(format!("watching {}", root.display()));
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(tx)?;
    watcher.watch(&root, RecursiveMode::Recursive)?;

    let (done_tx, done_rx) = mpsc::channel::<PathBuf>();
    let mut pending: HashMap<PathBuf, Pending> = HashMap::new();
    let mut running: HashMap<PathBuf, bool> = HashMap::new(); // blog -> changed while running

    // Initial pass: build everything so a restart catches up on changes made while stopped.
    for blog in list_blogs(&root) {
        let now = Instant::now();
        pending.insert(blog, Pending { first: now - MAX_WAIT, last: now - QUIET });
    }

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(ev)) => {
                if matches!(ev.kind, notify::EventKind::Access(_)) {
                    continue;
                }
                for p in ev.paths {
                    if let Some(blog) = relevant(&root, &p) {
                        let now = Instant::now();
                        if let Some(dirty) = running.get_mut(&blog) {
                            *dirty = true;
                            continue;
                        }
                        pending.entry(blog).and_modify(|e| e.last = now).or_insert(Pending { first: now, last: now });
                    }
                }
            }
            Ok(Err(e)) => log(format!("watch error: {e}")),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => anyhow::bail!("watcher stopped"),
        }
        while let Ok(blog) = done_rx.try_recv() {
            if running.remove(&blog) == Some(true) {
                let now = Instant::now();
                pending.insert(blog, Pending { first: now, last: now });
            }
        }
        let now = Instant::now();
        let ready: Vec<PathBuf> = pending
            .iter()
            .filter(|(b, p)| !running.contains_key(*b) && (now - p.last >= QUIET || now - p.first >= MAX_WAIT))
            .map(|(b, _)| b.clone())
            .collect();
        for blog in ready {
            pending.remove(&blog);
            running.insert(blog.clone(), false);
            let done = done_tx.clone();
            std::thread::spawn(move || {
                build_and_deploy(&blog);
                let _ = done.send(blog);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relevance_filters_debris_and_state() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let blog = root.join("a.com");
        std::fs::create_dir_all(blog.join("posts")).unwrap();
        std::fs::write(blog.join("blog.toml"), "title='a'\nbase_url='https://a.com'\n[watch]\nignore=['theme/gen/']\n").unwrap();
        assert_eq!(relevant(root, &blog.join("posts/x.md")), Some(blog.clone()));
        assert_eq!(relevant(root, &blog.join("posts/.x.md.swp")), None);
        assert_eq!(relevant(root, &blog.join("posts/x.md~")), None);
        assert_eq!(relevant(root, &blog.join("posts/4913")), None);
        assert_eq!(relevant(root, &blog.join(".blog/public/index.html")), None);
        assert_eq!(relevant(root, &blog.join("theme/gen/out.css")), None);
        assert_eq!(relevant(root, &root.join("notablog/posts/x.md")), None);
    }
}
