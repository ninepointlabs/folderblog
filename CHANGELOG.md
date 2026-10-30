# Changelog

## 0.2.11

Early Windows test build. Linux paths, the systemd user service, and the Linux packages are unchanged.

- On Windows, blogs default to `%USERPROFILE%\Blogs` and the GitHub login and podcast keys live in `%APPDATA%\folderblog`. If the user profile or AppData cannot be found, folderblog stops instead of writing into the current directory. Linux still uses `$HOME`, `$XDG_CONFIG_HOME`, and `~/.config/folderblog`.
- `folderblog install-service` on Windows registers a per-user scheduled task that runs `folderblog watch` at sign-in, hidden, with no administrator rights, and restarts it if it exits. Logs go to `%LOCALAPPDATA%\folderblog\watch.log`. `--uninstall` removes the task. On Linux, `--uninstall` stops the systemd user service.
- Deploy checkouts set `core.autocrlf=false` (and `core.safecrlf=false`) so Git for Windows does not rewrite line endings.
- Watch events still match after Windows `\\?\` paths are simplified, including when the drive letter's case differs.
- Command deploys and build hooks say clearly when `sh` is missing, and look for Git for Windows' `sh.exe`. `guide` and `man` still print the text when `less` or `man` is not installed.
- The release includes an unsigned Windows zip and an unsigned per-user installer, next to the existing Linux artifacts. SmartScreen will warn.

## 0.2.10

Previous release.
