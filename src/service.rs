//! The Windows logon task. Linux keeps its systemd unit in `main`.
//!
//! The task runs as the signed-in user, with no admin rights, hidden, and restarts
//! if `folderblog watch` exits. Logs go to `%LOCALAPPDATA%\folderblog\watch.log`.

#[cfg(windows)]
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::Command;

#[cfg_attr(not(windows), allow(dead_code))]
pub const TASK_NAME: &str = "folderblog";

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn quote_arg(s: &str) -> String {
    if s.chars().any(|c| c.is_whitespace()) {
        format!("\"{s}\"")
    } else {
        s.to_string()
    }
}

/// Task Scheduler XML for a per-user logon task. Pure, so tests can read it on Linux.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn task_xml(exe: &Path, root: &Path) -> String {
    let args = xml_escape(&format!(
        "watch --root {}",
        quote_arg(&root.display().to_string())
    ));
    let exe = xml_escape(&exe.display().to_string());
    let desc = xml_escape(&format!(
        "folderblog: publish blogs from {}",
        root.display()
    ));
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>{desc}</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>true</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
    <RestartOnFailure>
      <Interval>PT1M</Interval>
      <Count>999</Count>
    </RestartOnFailure>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
      <Arguments>{args}</Arguments>
    </Exec>
  </Actions>
</Task>
"#
    )
}

/// Map a Task Scheduler state (`Running`, `Ready`, …) to the words `status` prints.
/// `None` means the task is not installed.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn label_for_task_state(state: Option<&str>) -> String {
    match state.map(str::trim).filter(|s| !s.is_empty()) {
        None => "not installed".into(),
        Some(s) if s.eq_ignore_ascii_case("running") => "running".into(),
        Some(s) if s.eq_ignore_ascii_case("ready") => "installed, starts at logon".into(),
        Some(s) if s.eq_ignore_ascii_case("disabled") => "disabled".into(),
        Some(s) => s.to_string(),
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn task_is_running(state: Option<&str>) -> bool {
    matches!(state.map(str::trim), Some(s) if s.eq_ignore_ascii_case("running"))
}

pub fn log_file() -> Option<PathBuf> {
    crate::paths::local_data_dir().map(|d| d.join("watch.log"))
}

#[cfg(windows)]
unsafe extern "system" {
    fn FreeConsole() -> i32;
}

/// Called at the start of `watch` on Windows. Turns on the log file, and hides the
/// console when there isn't one (the logon task), so a window doesn't sit on the desktop.
#[cfg(windows)]
pub fn prepare_windows_watch() {
    // Edition 2024: set_var is unsafe because it races other threads. This runs at the
    // start of `watch`, before any worker threads exist.
    unsafe { std::env::set_var("FOLDERBLOG_BACKGROUND", "1") };
    if let Some(dir) = crate::paths::local_data_dir() {
        let _ = std::fs::create_dir_all(dir);
    }
    let interactive = std::io::IsTerminal::is_terminal(&std::io::stdin())
        || std::io::IsTerminal::is_terminal(&std::io::stdout());
    if !interactive {
        unsafe { FreeConsole() };
    }
}

#[cfg(windows)]
fn write_utf16(path: &Path, xml: &str) -> Result<()> {
    let mut bytes = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    std::fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

#[cfg(windows)]
fn query_task_state() -> Option<String> {
    let out = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-ScheduledTask -TaskName 'folderblog' -ErrorAction SilentlyContinue).State",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

#[cfg(windows)]
pub fn task_status_label() -> String {
    label_for_task_state(query_task_state().as_deref())
}

#[cfg(windows)]
pub fn task_running() -> bool {
    task_is_running(query_task_state().as_deref())
}

#[cfg(windows)]
pub fn install_windows(root: &Path, no_start: bool, print: bool) -> Result<i32> {
    let exe = crate::paths::simplify(&std::env::current_exe()?.canonicalize()?);
    let root = crate::paths::simplify(root);
    let xml = task_xml(&exe, &root);
    if print {
        print!("{xml}");
        return Ok(0);
    }
    if let Some(dir) = crate::paths::local_data_dir() {
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let xml_path = std::env::temp_dir().join("folderblog-task.xml");
    write_utf16(&xml_path, &xml)?;
    let created = Command::new("schtasks")
        .args([
            "/Create",
            "/TN",
            TASK_NAME,
            "/XML",
            &xml_path.display().to_string(),
            "/F",
        ])
        .status()
        .context("running schtasks /Create")?;
    let _ = std::fs::remove_file(&xml_path);
    if !created.success() {
        bail!(
            "could not register the folderblog logon task. This does not need an administrator account; \
if Windows asked for one, the task was not created"
        );
    }
    println!("installed the per-user logon task \"{TASK_NAME}\" (no administrator rights)");
    println!("watches {}", root.display());
    if let Some(log) = log_file() {
        println!("log: {}", log.display());
    }
    if no_start {
        println!("not started; it starts the next time you sign in");
        println!("start it now with: schtasks /Run /TN {TASK_NAME}");
        return Ok(0);
    }
    let ran = Command::new("schtasks")
        .args(["/Run", "/TN", TASK_NAME])
        .status()
        .context("running schtasks /Run")?;
    if !ran.success() {
        bail!(
            "the task is installed, but starting it now failed. It will still start the next time you sign in"
        );
    }
    println!("started");
    Ok(0)
}

#[cfg(windows)]
pub fn uninstall_windows() -> Result<i32> {
    let _ = Command::new("schtasks")
        .args(["/End", "/TN", TASK_NAME])
        .status();
    let deleted = Command::new("schtasks")
        .args(["/Delete", "/TN", TASK_NAME, "/F"])
        .status()
        .context("running schtasks /Delete")?;
    if !deleted.success() {
        bail!(
            "could not remove the folderblog logon task. It may not be installed (folderblog status shows the state)"
        );
    }
    println!("removed the folderblog logon task");
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_xml_is_a_hidden_per_user_logon_task() {
        let xml = task_xml(
            Path::new(r"C:\Tools\folderblog.exe"),
            Path::new(r"C:\Users\Tim\Blogs"),
        );
        assert!(xml.contains("<LogonTrigger>"), "{xml}");
        assert!(xml.contains("<RunLevel>LeastPrivilege</RunLevel>"), "{xml}");
        assert!(xml.contains("<Hidden>true</Hidden>"), "{xml}");
        assert!(xml.contains("<RestartOnFailure>"), "{xml}");
        assert!(xml.contains("<Interval>PT1M</Interval>"), "{xml}");
        assert!(xml.contains(r"C:\Tools\folderblog.exe"), "{xml}");
        assert!(xml.contains("watch --root"), "{xml}");
        assert!(xml.contains(r"C:\Users\Tim\Blogs"), "{xml}");
        assert!(!xml.contains("HighestAvailable"), "{xml}");
    }

    #[test]
    fn task_xml_quotes_paths_with_spaces() {
        let xml = task_xml(
            Path::new(r"C:\Program Files\folderblog.exe"),
            Path::new(r"C:\Users\Tim Smith\Blogs"),
        );
        assert!(
            xml.contains("&quot;C:\\Users\\Tim Smith\\Blogs&quot;"),
            "{xml}"
        );
    }

    #[test]
    fn task_state_labels() {
        assert_eq!(label_for_task_state(None), "not installed");
        assert_eq!(label_for_task_state(Some("")), "not installed");
        assert_eq!(label_for_task_state(Some("Running")), "running");
        assert_eq!(
            label_for_task_state(Some("Ready")),
            "installed, starts at logon"
        );
        assert_eq!(label_for_task_state(Some("Disabled")), "disabled");
        assert!(task_is_running(Some("Running")));
        assert!(!task_is_running(Some("Ready")));
        assert!(!task_is_running(None));
    }
}
