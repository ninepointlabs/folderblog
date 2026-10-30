//! Where folderblog keeps itself on disk.
//!
//! Linux is unchanged: `$HOME`, `$XDG_CONFIG_HOME`, `~/.config/folderblog`, `~/Blogs`.
//! Windows uses the user profile and `%APPDATA%\folderblog`, and never the current
//! directory when those are missing.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// `$HOME`, or `.` when it is unset. This is the Linux rule; do not "improve" it.
#[cfg_attr(windows, allow(dead_code))]
pub fn linux_home(home: Option<&OsStr>) -> PathBuf {
    home.map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `$FOLDERBLOG_CONFIG_DIR`, else `$XDG_CONFIG_HOME/folderblog`, else
/// `$HOME/.config/folderblog`. An unset `HOME` joins onto an empty path, matching the
/// historical Linux result (`.config/folderblog`, not `./.config/folderblog`).
#[cfg_attr(windows, allow(dead_code))]
pub fn linux_config_dir(
    explicit: Option<&OsStr>,
    xdg: Option<&OsStr>,
    home: Option<&OsStr>,
) -> PathBuf {
    if let Some(d) = explicit {
        return PathBuf::from(d);
    }
    let base = match xdg {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from(home.unwrap_or(OsStr::new(""))).join(".config"),
    };
    base.join("folderblog")
}

/// The user's home folder. Linux reads `HOME` exactly as before. Windows uses the
/// profile (`USERPROFILE` via the `dirs` crate) and refuses to fall back to the
/// current directory.
pub fn home() -> anyhow::Result<PathBuf> {
    #[cfg(windows)]
    {
        dirs::home_dir().ok_or_else(|| {
            anyhow::anyhow!(
                "could not find your user profile (USERPROFILE is unset). folderblog will not use the current directory"
            )
        })
    }
    #[cfg(not(windows))]
    {
        Ok(linux_home(std::env::var_os("HOME").as_deref()))
    }
}

/// GitHub token and podcast storage keys.
///
/// `FOLDERBLOG_CONFIG_DIR` wins on every OS. Otherwise Linux stays on
/// `$XDG_CONFIG_HOME` / `~/.config/folderblog`, and Windows uses `%APPDATA%\folderblog`.
pub fn config_dir() -> PathBuf {
    let explicit = std::env::var_os("FOLDERBLOG_CONFIG_DIR");
    if let Some(d) = explicit.as_deref() {
        return PathBuf::from(d);
    }
    #[cfg(windows)]
    {
        match dirs::config_dir() {
            Some(dir) => dir.join("folderblog"),
            None => {
                eprintln!(
                    "error: could not find AppData (APPDATA is unset). folderblog will not store GitHub tokens in the current directory."
                );
                std::process::exit(1);
            }
        }
    }
    #[cfg(not(windows))]
    {
        linux_config_dir(
            None,
            std::env::var_os("XDG_CONFIG_HOME").as_deref(),
            std::env::var_os("HOME").as_deref(),
        )
    }
}

/// `%LOCALAPPDATA%\folderblog` on Windows, `~/.local/share/folderblog` on Linux.
/// The background log lives here. Linux `watch` does not set it unless asked.
pub fn local_data_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|p| p.join("folderblog"))
}

/// Drop a Windows `\\?\` prefix when the path does not need it. On Linux this is
/// the path unchanged.
pub fn simplify(path: &Path) -> PathBuf {
    dunce::simplified(path).to_path_buf()
}

/// `path` relative to `root`, ignoring ASCII case. A root of `C:\Blogs` does not
/// match `C:\BlogsExtra`. Used for watch events on case-insensitive Windows disks.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn strip_prefix_ignore_ascii_case(path: &Path, root: &Path) -> Option<PathBuf> {
    let path_s = path.to_string_lossy();
    let mut root_s = root.to_string_lossy().into_owned();
    while root_s.ends_with(['\\', '/']) && root_s.len() > 1 {
        root_s.pop();
    }
    let path_b = path_s.as_bytes();
    let root_b = root_s.as_bytes();
    if root_b.is_empty()
        || path_b.len() < root_b.len()
        || !path_b[..root_b.len()].eq_ignore_ascii_case(root_b)
    {
        return None;
    }
    if path_b.len() == root_b.len() {
        return Some(PathBuf::new());
    }
    let next = path_b[root_b.len()];
    if next != b'\\' && next != b'/' {
        return None;
    }
    Some(PathBuf::from(&path_s[root_b.len() + 1..]))
}

/// Exact `strip_prefix` on Linux. On Windows, simplify both sides, try an exact
/// match, then an ASCII case-insensitive one.
pub fn strip_prefix_flex(path: &Path, root: &Path) -> Option<PathBuf> {
    let path = simplify(path);
    let root = simplify(root);
    if let Ok(rel) = path.strip_prefix(&root) {
        return Some(rel.to_path_buf());
    }
    #[cfg(windows)]
    {
        return strip_prefix_ignore_ascii_case(&path, &root);
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// A path safe to expand inside `sh`. Git Bash treats backslashes as escapes, so
/// Windows paths are given with forward slashes (`C:/Users/...`). Linux is unchanged.
pub fn shell_path(path: &Path) -> String {
    let s = path.display().to_string();
    #[cfg(windows)]
    {
        s.replace('\\', "/")
    }
    #[cfg(not(windows))]
    {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_home_and_config_stay_put() {
        assert_eq!(
            linux_home(Some(OsStr::new("/home/tim"))),
            PathBuf::from("/home/tim")
        );
        assert_eq!(linux_home(None), PathBuf::from("."));
        assert_eq!(
            linux_config_dir(
                Some(OsStr::new("/override")),
                Some(OsStr::new("/xdg")),
                Some(OsStr::new("/home/tim"))
            ),
            PathBuf::from("/override")
        );
        assert_eq!(
            linux_config_dir(
                None,
                Some(OsStr::new("/xdg")),
                Some(OsStr::new("/home/tim"))
            ),
            PathBuf::from("/xdg/folderblog")
        );
        assert_eq!(
            linux_config_dir(None, None, Some(OsStr::new("/home/tim"))),
            PathBuf::from("/home/tim/.config/folderblog")
        );
        assert_eq!(
            linux_config_dir(None, None, None),
            PathBuf::from(".config/folderblog")
        );
    }

    #[test]
    fn windows_prefix_ignores_ascii_case_and_stops_at_a_boundary() {
        let root = Path::new(r"C:\Blogs");
        assert_eq!(
            strip_prefix_ignore_ascii_case(Path::new(r"c:\blogs\a.com\posts\x.md"), root).unwrap(),
            PathBuf::from(r"a.com\posts\x.md")
        );
        assert!(strip_prefix_ignore_ascii_case(Path::new(r"C:\BlogsExtra\x"), root).is_none());
        assert!(
            strip_prefix_ignore_ascii_case(Path::new(r"C:\Blogs"), root)
                .unwrap()
                .as_os_str()
                .is_empty()
        );
        assert_eq!(
            strip_prefix_ignore_ascii_case(Path::new(r"C:\Blogs\a.com"), Path::new(r"C:\Blogs\"))
                .unwrap(),
            PathBuf::from("a.com")
        );
    }

    #[test]
    fn shell_paths_keep_slashes_on_unix() {
        let p = Path::new("/home/tim/Blogs");
        assert_eq!(shell_path(p), "/home/tim/Blogs");
    }
}
