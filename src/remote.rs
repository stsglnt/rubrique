//! Remote folders over SSH (optional, off by default).
//!
//! Rubrique runs the system `ssh` program, so the user's `~/.ssh/config`,
//! keys, ssh-agent and jump hosts all work. Each remote file has a local
//! *mirror path* (`<config>/remote/<host>/<remote path>`), so the rest of the
//! app keeps working with plain paths. Files read from the host are also
//! written to their mirror path; the host always has the real file.

use std::path::{Path, PathBuf};
use std::sync::{PoisonError, RwLock};

pub mod cksum;
pub mod commands;
pub mod connection;
pub mod mirror;
pub mod ssh_config;
pub mod task;

pub use mirror::{RemotePath, display, from_mirror, is_mirror, to_mirror};

/// Settings for remote folders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteSettings {
    /// Show "Add remote folder" and open remote folders.
    pub enabled: bool,
    /// The `ssh` program to run.
    pub ssh_program: String,
}

impl Default for RemoteSettings {
    fn default() -> Self {
        RemoteSettings { enabled: false, ssh_program: DEFAULT_SSH_PROGRAM.to_string() }
    }
}

const DEFAULT_SSH_PROGRAM: &str = "ssh";

impl RemoteSettings {
    /// Read one `key=value` line; true when the key was ours.
    pub fn parse_line(&mut self, key: &str, value: &str) -> bool {
        match key {
            "remote_enabled" => self.enabled = value == "true",
            "remote_ssh_program" if !value.is_empty() => self.ssh_program = value.to_string(),
            "remote_ssh_program" => {}
            _ => return false,
        }
        true
    }

    /// The lines [`RemoteSettings::parse_line`] reads back.
    pub fn to_lines(&self) -> String {
        format!("remote_enabled={}\nremote_ssh_program={}\n", self.enabled, self.ssh_program)
    }
}

/// The `ssh` program used for every remote command.
static SSH_PROGRAM: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Use `program` for remote commands from now on.
pub fn set_ssh_program(program: &str) {
    let program = if program.trim().is_empty() { DEFAULT_SSH_PROGRAM } else { program.trim() };
    *SSH_PROGRAM.write().unwrap_or_else(PoisonError::into_inner) = Some(PathBuf::from(program));
}

/// What a look for the ssh program setting found on this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgramCheck {
    /// Where the program is.
    Found(PathBuf),
    /// Why it cannot run, in words for the user.
    Problem(String),
}

/// Look for the program the ssh setting names: a path, or a name searched
/// in `PATH`. Never connects anywhere.
pub fn check_ssh_program(program: &str) -> ProgramCheck {
    let program = program.trim();
    let program = if program.is_empty() { DEFAULT_SSH_PROGRAM } else { program };
    if program.chars().any(char::is_whitespace) {
        return ProgramCheck::Problem(format!(
            "Not a program: \"{program}\". Enter the program only, without a computer or options: ssh, or a path such as /opt/homebrew/bin/ssh."
        ));
    }
    let found = if program.contains('/') {
        Path::new(program).is_file().then(|| PathBuf::from(program))
    } else {
        find_in_path(program)
    };
    match found {
        Some(path) => ProgramCheck::Found(path),
        None => ProgramCheck::Problem(format!(
            "No program \"{program}\" on this computer. Enter ssh, or the path of an OpenSSH client such as /opt/homebrew/bin/ssh."
        )),
    }
}

/// The first file called `name` in the folders of `PATH`.
fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|folder| folder.join(name)).find(|candidate| candidate.is_file())
}

/// The `ssh` program to run.
pub fn ssh_program() -> PathBuf {
    SSH_PROGRAM
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SSH_PROGRAM))
}

/// Text for a path in the settings file: `ssh://host/remote/path` for a
/// mirror path, the path itself otherwise.
pub fn settings_text(path: &Path) -> String {
    match from_mirror(path) {
        Some(remote) => format!("ssh://{}{}", remote.host, remote.path.display()),
        None => path.display().to_string(),
    }
}

/// The path a settings value stands for (see [`settings_text`]).
pub fn from_settings_text(value: &str) -> Option<PathBuf> {
    if value.is_empty() {
        return None;
    }
    let Some(rest) = value.strip_prefix("ssh://") else {
        return Some(PathBuf::from(value));
    };
    let (host, path) = rest.split_once('/')?;
    to_mirror(host, Path::new(&format!("/{path}")))
}

#[cfg(test)]
pub mod testing {
    //! A fake `ssh` that runs the remote command on this computer, so the
    //! remote code can be tested without a server.

    use std::path::PathBuf;
    use std::sync::OnceLock;

    /// Point the remote code at the fake `ssh` and a test mirror folder.
    /// Safe to call from every test; the setup happens once per process.
    pub fn use_fake_ssh() {
        static SETUP: OnceLock<()> = OnceLock::new();
        SETUP.get_or_init(|| {
            let folder = std::env::temp_dir().join(format!("rubrique-fake-ssh-{}", std::process::id()));
            std::fs::create_dir_all(&folder).expect("test folder");
            let script = folder.join("ssh");
            // Skip the options up to `--`, then the host; the remote login
            // shell gets the rest as one command line, like real ssh.
            std::fs::write(
                &script,
                "#!/bin/sh\nwhile [ \"$1\" != \"--\" ]; do shift; done\nshift 2\nexec sh -c \"$*\"\n",
            )
            .expect("fake ssh");
            std::fs::set_permissions(
                &script,
                <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o755),
            )
            .expect("chmod");
            // Another test thread starting a process at the moment the script
            // was written can hold it open for a moment ("text file busy").
            for _ in 0..200 {
                let ran = std::process::Command::new(&script).args(["--", "host", "true"]).status();
                if ran.is_ok_and(|status| status.success()) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            super::set_ssh_program(&script.display().to_string());
            super::mirror::set_mirror_root(folder.join("mirror"));
        });
    }

    /// The mirror path of `remote` on the test host `host`.
    pub fn mirror_of(host: &str, remote: &std::path::Path) -> PathBuf {
        use_fake_ssh();
        super::to_mirror(host, remote).expect("valid host")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_lines_round_trip() {
        let settings = RemoteSettings { enabled: true, ssh_program: "/opt/bin/ssh".to_string() };
        let mut back = RemoteSettings::default();
        for line in settings.to_lines().lines() {
            let (key, value) = line.split_once('=').unwrap();
            assert!(back.parse_line(key, value));
        }
        assert_eq!(back, settings);
        assert!(!back.parse_line("theme", "dark"));
    }

    #[test]
    fn checks_the_ssh_program_setting() {
        assert!(matches!(check_ssh_program("sh"), ProgramCheck::Found(path) if path.ends_with("sh")));
        assert!(matches!(check_ssh_program("/bin/sh"), ProgramCheck::Found(path) if path == Path::new("/bin/sh")));
        let ProgramCheck::Problem(command_line) = check_ssh_program("ssh gpu-workstation") else {
            panic!("a command line is not a program");
        };
        assert!(command_line.starts_with("Not a program: \"ssh gpu-workstation\""), "{command_line}");
        assert!(matches!(check_ssh_program("no-such-program-rubrique"), ProgramCheck::Problem(_)));
        assert!(matches!(check_ssh_program("/no/such/ssh"), ProgramCheck::Problem(_)));
        // Empty means the default.
        assert_eq!(check_ssh_program("  "), check_ssh_program("ssh"));
    }

    #[test]
    fn settings_text_round_trip() {
        testing::use_fake_ssh();
        let mirror = to_mirror("box", Path::new("/home/me/my notes")).unwrap();
        assert_eq!(settings_text(&mirror), "ssh://box/home/me/my notes");
        assert_eq!(from_settings_text("ssh://box/home/me/my notes"), Some(mirror));
        assert_eq!(from_settings_text("/local/notes"), Some(PathBuf::from("/local/notes")));
        assert_eq!(from_settings_text(""), None);
        assert_eq!(from_settings_text("ssh://-oProxyCommand=x/a"), None);
    }
}
