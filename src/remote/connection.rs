//! Run one shell script on a host through the system `ssh`.
//!
//! All commands to a host share one connection (OpenSSH `ControlMaster`),
//! so only the first one pays for the login. `BatchMode` turns password
//! prompts off: rubrique has no terminal to type them in.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io::{self, Read, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use super::mirror::is_valid_host;

/// Longest wait for a host to answer at all.
const CONNECT_TIMEOUT_SECONDS: u32 = 6;
/// How long the shared connection stays open after the last command.
const PERSIST_SECONDS: u32 = 600;
/// Longest a normal command may take before it is stopped.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
/// Unix socket paths are limited to about 104 bytes; stay well below.
const MAX_SOCKET_PATH: usize = 80;

/// After a host failed to connect, commands to it fail at once for this
/// long instead of each waiting for the timeout (an offline server with
/// several open tabs would otherwise freeze the window again and again).
const RETRY_AFTER: Duration = Duration::from_secs(15);

/// When each host last failed to connect.
static FAILED: Mutex<Option<HashMap<String, (Instant, String)>>> = Mutex::new(None);

/// Exit status the scripts use for "no such file".
pub const EXIT_NOT_FOUND: i32 = 3;
/// Exit status the scripts use for "the target already exists".
pub const EXIT_EXISTS: i32 = 5;
/// Exit status `ssh` uses for its own errors (cannot connect, refused, ...).
const EXIT_SSH_ERROR: i32 = 255;

/// Run `script` with `sh` on `host`, with `args` as `$1`, `$2`, ... and
/// `input` on its standard input. Returns what it printed.
///
/// # Errors
///
/// `NotFound` / `AlreadyExists` for the scripts' exit statuses 3 / 5,
/// `TimedOut` when it ran too long, and `Other` with ssh's or the script's
/// message for anything else.
pub fn run(host: &str, script: &str, args: &[&OsStr], input: Option<&[u8]>, timeout: Duration) -> io::Result<Vec<u8>> {
    if !is_valid_host(host) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("not a host name: {host}")));
    }
    if let Some(message) = recent_failure(host) {
        return Err(io::Error::other(message));
    }
    let result = run_now(host, script, args, input, timeout);
    if let Err(error) = &result
        && matches!(error.kind(), io::ErrorKind::TimedOut | io::ErrorKind::ConnectionRefused)
    {
        remember_failure(host, error.to_string());
    }
    result
}

/// Let the next command to `host` try to connect again at once.
pub fn forget_failure(host: &str) {
    if let Some(failed) = FAILED.lock().unwrap_or_else(PoisonError::into_inner).as_mut() {
        failed.remove(host);
    }
}

fn recent_failure(host: &str) -> Option<String> {
    let failed = FAILED.lock().unwrap_or_else(PoisonError::into_inner);
    let (when, message) = failed.as_ref()?.get(host)?;
    (when.elapsed() < RETRY_AFTER).then(|| message.clone())
}

fn remember_failure(host: &str, message: String) {
    let mut failed = FAILED.lock().unwrap_or_else(PoisonError::into_inner);
    failed.get_or_insert_with(HashMap::new).insert(host.to_string(), (Instant::now(), message));
}

fn run_now(host: &str, script: &str, args: &[&OsStr], input: Option<&[u8]>, timeout: Duration) -> io::Result<Vec<u8>> {
    let mut command = Command::new(super::ssh_program());
    command.args(ssh_options(host)).arg("--").arg(host).arg(remote_command(script, args));
    command.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() });
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        io::Error::new(error.kind(), format!("cannot run {}: {error}", super::ssh_program().display()))
    })?;

    let writer = match (input, child.stdin.take()) {
        (Some(input), Some(mut stdin)) => {
            let input = input.to_vec();
            // A write error shows up as the script's own failure below.
            Some(thread::spawn(move || stdin.write_all(&input)))
        }
        _ => None,
    };
    let stdout = child.stdout.take().map(read_all);
    let stderr = child.stderr.take().map(read_all);

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(io::ErrorKind::TimedOut, format!("{host} did not answer in time")));
        }
        thread::sleep(Duration::from_millis(5));
    };
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    let output = stdout.and_then(|reader| reader.join().ok()).unwrap_or_default();
    let errors = stderr.and_then(|reader| reader.join().ok()).unwrap_or_default();
    match status.code() {
        Some(0) => Ok(output),
        Some(EXIT_NOT_FOUND) => Err(io::Error::new(io::ErrorKind::NotFound, "no such file or folder")),
        Some(EXIT_EXISTS) => Err(io::Error::new(io::ErrorKind::AlreadyExists, "a file with that name exists")),
        Some(EXIT_SSH_ERROR) => Err(io::Error::new(io::ErrorKind::ConnectionRefused, connect_error(host, &errors))),
        _ => {
            let message = last_line(&errors);
            Err(io::Error::other(if message.is_empty() { format!("command failed on {host}") } else { message }))
        }
    }
}

/// Read a pipe to its end on a thread of its own, so a full pipe never blocks the other.
fn read_all(mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        bytes
    })
}

/// The `-o` options for every command.
fn ssh_options(host: &str) -> Vec<String> {
    let mut options = vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        format!("ConnectTimeout={CONNECT_TIMEOUT_SECONDS}"),
        "-o".to_string(),
        "ServerAliveInterval=5".to_string(),
        "-o".to_string(),
        "ServerAliveCountMax=3".to_string(),
    ];
    if let Some(socket) = control_socket(host) {
        options.extend([
            "-o".to_string(),
            "ControlMaster=auto".to_string(),
            "-o".to_string(),
            format!("ControlPath={}", socket.display()),
            "-o".to_string(),
            format!("ControlPersist={PERSIST_SECONDS}"),
        ]);
    }
    options
}

/// Where the shared connection's socket goes: a private folder (only we may
/// use it) with a short path. `None` turns connection sharing off.
fn control_socket(host: &str) -> Option<PathBuf> {
    let base = if cfg!(target_os = "macos") {
        // A per-user private folder on macOS.
        std::env::var_os("TMPDIR").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)
    };
    let folder = base.or_else(super::mirror::mirror_root)?.join("rubrique-ssh");
    std::fs::create_dir_all(&folder).ok()?;
    // Only the owner may change the mode, so this also proves the folder is ours.
    std::fs::set_permissions(&folder, <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700))
        .ok()?;
    let socket = folder.join(format!("{:016x}", fnv_hash(host.as_bytes())));
    (socket.as_os_str().len() <= MAX_SOCKET_PATH).then_some(socket)
}

/// FNV-1a: a short, stable name for a host's socket.
fn fnv_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

/// The command line the host's login shell runs: `sh -c '<script>' sh '<arg>'...`.
/// Everything is single-quoted, so whatever the login shell is, it hands the
/// script and arguments to `sh` untouched.
fn remote_command(script: &str, args: &[&OsStr]) -> OsString {
    let mut line = b"sh -c ".to_vec();
    line.extend(quote(script.as_bytes()));
    line.extend(b" sh");
    for arg in args {
        line.push(b' ');
        line.extend(quote(arg.as_bytes()));
    }
    OsString::from_vec(line)
}

/// `bytes` in single quotes, each `'` written as `'\''`.
pub fn quote(bytes: &[u8]) -> Vec<u8> {
    let mut quoted = vec![b'\''];
    for &byte in bytes {
        if byte == b'\'' {
            quoted.extend(b"'\\''");
        } else {
            quoted.push(byte);
        }
    }
    quoted.push(b'\'');
    quoted
}

/// The last non-empty line of an error output.
fn last_line(errors: &[u8]) -> String {
    String::from_utf8_lossy(errors).lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("").trim().to_string()
}

/// A message for an ssh failure that says what to do about it.
fn connect_error(host: &str, errors: &[u8]) -> String {
    let message = last_line(errors);
    if message.contains("Permission denied") {
        format!(
            "Cannot log in to {host}: {message}. rubrique cannot type passwords; use an SSH key or ssh-agent (check with `ssh {host}` in a terminal)."
        )
    } else if message.contains("Host key verification failed") {
        format!(
            "Cannot connect to {host}: the host key is not known yet. Run `ssh {host}` in a terminal once to accept it."
        )
    } else if message.is_empty() {
        format!("Cannot connect to {host}")
    } else {
        format!("Cannot connect to {host}: {message}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::testing::use_fake_ssh;

    fn run_text(script: &str, args: &[&str]) -> io::Result<String> {
        use_fake_ssh();
        let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
        run("test-host", script, &args, None, COMMAND_TIMEOUT).map(|bytes| String::from_utf8(bytes).unwrap())
    }

    #[test]
    fn arguments_arrive_untouched() {
        let nasty = ["it's", "$HOME", "a b", "-n", "line\nbreak", "`date`", "\\", "*"];
        for arg in nasty {
            assert_eq!(run_text("printf '%s' \"$1\"", &[arg]).unwrap(), arg);
        }
    }

    #[test]
    fn script_quotes_survive() {
        assert_eq!(run_text("printf '%s|' 'one' \"two\" three", &[]).unwrap(), "one|two|three|");
    }

    #[test]
    fn input_reaches_the_script() {
        use_fake_ssh();
        let output = run("test-host", "cat", &[], Some(b"hello\nthere"), COMMAND_TIMEOUT).unwrap();
        assert_eq!(output, b"hello\nthere");
    }

    #[test]
    fn exit_statuses_become_errors() {
        assert_eq!(run_text("exit 3", &[]).unwrap_err().kind(), io::ErrorKind::NotFound);
        assert_eq!(run_text("exit 5", &[]).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        let error = run_text("echo 'disk full' >&2; exit 1", &[]).unwrap_err();
        assert_eq!(error.to_string(), "disk full");
        use_fake_ssh();
        let error =
            run("denied-host", "echo 'Permission denied (publickey).' >&2; exit 255", &[], None, COMMAND_TIMEOUT)
                .unwrap_err();
        assert!(error.to_string().contains("SSH key or ssh-agent"), "{error}");
    }

    #[test]
    fn slow_commands_time_out() {
        use_fake_ssh();
        let error = run("slow-host", "sleep 5", &[], None, Duration::from_millis(200)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn failed_hosts_fail_fast_until_forgotten() {
        use_fake_ssh();
        let error =
            run("down-host", "echo 'Connection refused' >&2; exit 255", &[], None, COMMAND_TIMEOUT).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::ConnectionRefused);
        // Would succeed, but is not even tried.
        let again = run("down-host", "true", &[], None, COMMAND_TIMEOUT).unwrap_err();
        assert!(again.to_string().contains("Connection refused"), "{again}");
        forget_failure("down-host");
        assert!(run("down-host", "true", &[], None, COMMAND_TIMEOUT).is_ok());
    }

    #[test]
    fn bad_hosts_never_reach_ssh() {
        let error = run("-oProxyCommand=touch /tmp/x", "true", &[], None, COMMAND_TIMEOUT).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn quoting() {
        assert_eq!(quote(b"plain"), b"'plain'");
        assert_eq!(quote(b"it's"), b"'it'\\''s'");
        assert_eq!(quote(b""), b"''");
    }
}
