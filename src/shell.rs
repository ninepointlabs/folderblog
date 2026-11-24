//! Finding `sh` for build hooks and `type = "command"` deploys.
//!
//! Linux looks on `PATH`, as before. Windows also looks in a normal Git for Windows
//! install, because the default installer puts `git` on `PATH` and leaves `sh` off it.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn on_path(name: &str) -> bool {
    program_in_path(name).is_some()
}

fn program_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

/// What to tell someone when a command deploy or a build hook cannot start.
pub fn sh_missing_message() -> String {
    #[cfg(windows)]
    {
        "could not find `sh`. Command deploys and build hooks need a Unix shell. \
Install Git for Windows and choose \"Git from the command line and also from 3rd-party software\". \
If Git is already installed, add its bin folder (usually C:\\Program Files\\Git\\bin) to PATH"
            .into()
    }
    #[cfg(not(windows))]
    {
        "could not find `sh` on PATH".into()
    }
}

/// `sh` on `PATH`, or Git for Windows' `sh.exe` when that is installed but not on `PATH`.
pub fn sh_program() -> Result<PathBuf> {
    if let Some(p) = program_in_path("sh") {
        return Ok(p);
    }
    #[cfg(windows)]
    {
        let mut candidates = vec![
            PathBuf::from(r"C:\Program Files\Git\bin\sh.exe"),
            PathBuf::from(r"C:\Program Files\Git\usr\bin\sh.exe"),
            PathBuf::from(r"C:\Program Files (x86)\Git\bin\sh.exe"),
        ];
        if let Some(pf) = std::env::var_os("ProgramFiles") {
            candidates.push(PathBuf::from(pf).join("Git").join("bin").join("sh.exe"));
        }
        for c in candidates {
            if c.is_file() {
                return Ok(c);
            }
        }
    }
    bail!("{}", sh_missing_message())
}

/// Run `command` with `sh -c` in `dir`. `FOLDERBLOG_BLOG` and `FOLDERBLOG_OUT` are set
/// to shell-safe paths.
pub fn run(dir: &Path, command: &str, blog: &Path, out: &Path) -> Result<Output> {
    let sh = sh_program()?;
    Command::new(&sh)
        .arg("-c")
        .arg(command)
        .current_dir(dir)
        .env("FOLDERBLOG_BLOG", crate::paths::shell_path(blog))
        .env("FOLDERBLOG_OUT", crate::paths::shell_path(out))
        .output()
        .with_context(|| format!("starting {command:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_hint_names_git_for_windows() {
        let msg = {
            #[cfg(windows)]
            {
                sh_missing_message()
            }
            #[cfg(not(windows))]
            {
                "could not find `sh`. Command deploys and build hooks need a Unix shell. \
Install Git for Windows and choose \"Git from the command line and also from 3rd-party software\". \
If Git is already installed, add its bin folder (usually C:\\Program Files\\Git\\bin) to PATH"
                    .to_string()
            }
        };
        assert!(msg.contains("Git for Windows"), "{msg}");
        assert!(msg.contains("sh"), "{msg}");
    }
}
