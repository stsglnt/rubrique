//! The login shell's `PATH`, for an app started from Finder or a launcher.
//!
//! Started from a terminal, Rubrique gets the shell's `PATH`, with the
//! folders that `.zprofile`, `.profile` or `.bash_profile` add: `~/.local/bin`,
//! Homebrew, npm's global folder. Started from Finder, the Dock or a desktop
//! launcher it gets the system default (`/usr/bin:/bin:/usr/sbin:/sbin` on
//! macOS), so `claude`, `codex` or `ollama` are "not found" although they
//! are installed. At startup Rubrique therefore asks the login shell for its
//! `PATH` once and adds the folders that are missing, plus the usual install
//! folders in case the shell cannot be run. The folders already on `PATH`
//! keep their order and come first, so a terminal start changes nothing.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How long the login shell may take to print its environment. A shell
/// whose profile waits (for a password, a network drive) is given up on,
/// and Rubrique starts with the fallback folders only.
const SHELL_LIMIT: Duration = Duration::from_secs(3);

/// Add the login shell's `PATH` folders, and the usual install folders, to
/// this process's `PATH`. Call once, at the start of `main`, before any
/// other thread runs.
#[expect(unsafe_code, reason = "`set_var` is unsafe; called once in `main` before any other thread exists")]
pub fn extend_path_from_login_shell() {
    let current = std::env::var_os("PATH").unwrap_or_default();
    let shell = std::env::var_os("SHELL").map_or_else(|| PathBuf::from("/bin/sh"), PathBuf::from);
    let from_shell = login_shell_path(&shell, SHELL_LIMIT).unwrap_or_default();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let merged = merged_path(&current, &from_shell, &usual_folders(home.as_deref()));
    if merged != current {
        // SAFETY: `main` calls this before starting the window or any thread,
        // so nothing reads the environment concurrently.
        unsafe { std::env::set_var("PATH", merged) };
    }
}

/// The `PATH` of a login `shell`, read from the `env` it prints. `None`
/// when the shell cannot run, fails, prints no `PATH`, or takes longer
/// than `limit`.
fn login_shell_path(shell: &Path, limit: Duration) -> Option<OsString> {
    // `env` prints the environment the same from every shell (zsh, bash,
    // fish), unlike `echo $PATH`; a login shell (`-l`) reads the profile
    // files where `PATH` is usually set. Not interactive: `.zshrc` and
    // `.bashrc` may start programs or print, and may assume a terminal.
    let mut child = Command::new(shell)
        .args(["-l", "-c", "env"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    // Read on a thread so the wait below can give up after `limit` even if
    // the shell keeps the pipe open; the thread ends when the pipe closes.
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut output = Vec::new();
        let _ = stdout.read_to_end(&mut output);
        let _ = sender.send(output);
    });
    let deadline = Instant::now() + limit;
    let Ok(output) = receiver.recv_timeout(limit) else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    // Reap the shell; after its output ended it is done or nearly so.
    while child.try_wait().ok()?.is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    path_from_env_output(&output)
}

/// The value of the `PATH=` line in what `env` printed.
fn path_from_env_output(output: &[u8]) -> Option<OsString> {
    use std::os::unix::ffi::OsStrExt;
    output
        .split(|byte| *byte == b'\n')
        .find_map(|line| line.strip_prefix(b"PATH="))
        .filter(|value| !value.is_empty())
        .map(|value| OsStr::from_bytes(value).to_os_string())
}

/// Where tools usually get installed, for when the shell gives no answer:
/// the Claude Code and Codex installers (`~/.local/bin`), Homebrew, the
/// classic local folder, and npm's documented user-owned global folder.
fn usual_folders(home: Option<&Path>) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Some(home) = home {
        folders.push(home.join(".local/bin"));
    }
    folders.push(PathBuf::from("/opt/homebrew/bin"));
    folders.push(PathBuf::from("/usr/local/bin"));
    if let Some(home) = home {
        folders.push(home.join(".npm-global/bin"));
    }
    folders
}

/// `current` with the folders of `from_shell` it lacks appended in the
/// shell's order, then those of `usual` that exist and are still missing.
fn merged_path(current: &OsStr, from_shell: &OsStr, usual: &[PathBuf]) -> OsString {
    let mut folders: Vec<PathBuf> = std::env::split_paths(current).collect();
    for folder in std::env::split_paths(from_shell) {
        if !folder.as_os_str().is_empty() && !folders.contains(&folder) {
            folders.push(folder);
        }
    }
    for folder in usual {
        if !folders.contains(folder) && folder.is_dir() {
            folders.push(folder.clone());
        }
    }
    // Only `:` inside a folder name can fail, and such a folder cannot be
    // on a `PATH` anyway; keep what we had rather than lose it.
    std::env::join_paths(folders).unwrap_or_else(|_| current.to_os_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempFolder;

    #[test]
    fn reads_path_from_env_output() {
        let output = b"HOME=/Users/me\nPATH=/Users/me/.local/bin:/usr/bin\nSHELL=/bin/zsh\n";
        assert_eq!(path_from_env_output(output), Some(OsString::from("/Users/me/.local/bin:/usr/bin")));
        assert_eq!(path_from_env_output(b"HOME=/Users/me\n"), None);
        assert_eq!(path_from_env_output(b"PATH=\n"), None);
        // A variable whose value mentions PATH= is not the PATH line.
        assert_eq!(path_from_env_output(b"NOTE=PATH=/x\nPATH=/y"), Some(OsString::from("/y")));
    }

    #[test]
    fn keeps_current_order_and_appends_missing_folders() {
        let temp = TempFolder::new();
        let usual_present = temp.make_folder("usual");
        let usual_absent = temp.path().join("absent");
        let merged = merged_path(
            OsStr::new("/usr/bin:/bin"),
            OsStr::new("/Users/me/.local/bin:/usr/bin::/opt/homebrew/bin"),
            &[usual_present.clone(), usual_absent],
        );
        let folders: Vec<PathBuf> = std::env::split_paths(&merged).collect();
        assert_eq!(
            folders,
            [
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
                PathBuf::from("/Users/me/.local/bin"),
                PathBuf::from("/opt/homebrew/bin"),
                usual_present,
            ]
        );
    }

    #[test]
    fn nothing_to_add_leaves_path_as_it_was() {
        let current = OsStr::new("/a:/b");
        assert_eq!(merged_path(current, OsStr::new("/b:/a"), &[]), current);
        assert_eq!(merged_path(current, OsStr::new(""), &[PathBuf::from("/nonexistent-rubrique")]), current);
    }

    #[test]
    fn asks_a_real_login_shell() {
        let path = login_shell_path(Path::new("/bin/sh"), Duration::from_secs(10)).expect("sh -l -c env");
        assert!(std::env::split_paths(&path).any(|folder| folder == Path::new("/usr/bin")), "{path:?}");
    }

    #[test]
    fn gives_up_on_a_slow_or_broken_shell() {
        let temp = TempFolder::new();
        let slow = temp.write("slow", "#!/bin/sh\nsleep 30\n");
        set_executable(&slow);
        let started = Instant::now();
        assert_eq!(login_shell_path(&slow, Duration::from_millis(200)), None);
        assert!(started.elapsed() < Duration::from_secs(5));

        let failing = temp.write("failing", "#!/bin/sh\nexit 1\n");
        set_executable(&failing);
        assert_eq!(login_shell_path(&failing, Duration::from_secs(5)), None);
        assert_eq!(login_shell_path(&temp.path().join("missing"), Duration::from_secs(5)), None);
    }

    fn set_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
