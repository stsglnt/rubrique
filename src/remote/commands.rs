//! The file operations rubrique runs on a host. Each one is a short POSIX
//! `sh` script that works with GNU (Linux) and BSD (macOS) tools. Paths are
//! passed as arguments, never pasted into the script.
//!
//! Scripts are kept on one line: some login shells (csh) refuse a newline
//! inside quotes.

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::connection::{COMMAND_TIMEOUT, run};

/// Searches may read many files; give them longer.
const SEARCH_TIMEOUT: Duration = Duration::from_mins(1);
/// Downloads of images or a video.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_mins(2);

/// One entry of a remote folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    /// A folder, or a link to one.
    pub is_folder: bool,
    /// The entry itself is a symlink.
    pub is_symlink: bool,
    /// A folder link pointing at the folder itself or one of its parents.
    pub is_loop: bool,
}

/// Prints `<type><link>/<name>/` per entry: type `f` file, `d` folder, `o`
/// looping folder link; link `1` for a symlink. Names cannot contain `/`.
const LIST: &str = "[ -d \"$1\" ] || exit 3; cd -- \"$1\" || exit 1; p=$(pwd -P); \
    for f in * .*; do case $f in .|..) continue;; esac; \
    if [ ! -e \"$f\" ] && [ ! -L \"$f\" ]; then continue; fi; \
    t=f; l=0; [ -L \"$f\" ] && l=1; \
    if [ -d \"$f\" ]; then t=d; if [ $l = 1 ]; then \
    g=$(cd -P -- \"$f\" 2>/dev/null && pwd -P) && case \"$p/\" in \"${g%/}\"/*) t=o;; esac; fi; fi; \
    printf '%s%s/%s/' \"$t\" \"$l\" \"$f\"; done";

/// List a remote folder (not sorted).
///
/// # Errors
///
/// `NotFound` when it is not a folder; connection and permission errors.
pub fn list(host: &str, folder: &Path) -> io::Result<Vec<Entry>> {
    let output = run(host, LIST, &[folder.as_os_str()], None, COMMAND_TIMEOUT)?;
    Ok(parse_list(&String::from_utf8_lossy(&output)))
}

fn parse_list(output: &str) -> Vec<Entry> {
    let mut parts = output.split('/');
    let mut entries = Vec::new();
    while let (Some(kind), Some(name)) = (parts.next(), parts.next()) {
        let mut flags = kind.chars();
        let (Some(kind), Some(link)) = (flags.next(), flags.next()) else {
            break;
        };
        entries.push(Entry {
            name: name.to_string(),
            is_folder: matches!(kind, 'd' | 'o'),
            is_symlink: link == '1',
            is_loop: kind == 'o',
        });
    }
    entries
}

/// A file read from a host.
#[derive(Debug, PartialEq, Eq)]
pub enum Read {
    Bytes(Vec<u8>),
    /// Over the limit; holds the size in bytes. Nothing was downloaded.
    TooLarge(u64),
}

/// Prints `<size>/`, then the file unless it is over `$2` bytes.
const READ: &str = "[ -f \"$1\" ] || exit 3; s=$(wc -c < \"$1\") || exit 1; printf '%s/' $s; \
    [ $s -gt \"$2\" ] || cat -- \"$1\"";

/// Download a file of at most `max_bytes`.
///
/// # Errors
///
/// `NotFound` when there is no such file; connection and permission errors.
pub fn read(host: &str, path: &Path, max_bytes: u64) -> io::Result<Read> {
    let max = max_bytes.to_string();
    let output = run(host, READ, &[path.as_os_str(), OsStr::new(&max)], None, DOWNLOAD_TIMEOUT)?;
    let slash = output.iter().position(|&byte| byte == b'/').ok_or_else(|| bad_answer(host))?;
    let size: u64 = String::from_utf8_lossy(&output[..slash]).trim().parse().map_err(|_| bad_answer(host))?;
    if size > max_bytes {
        return Ok(Read::TooLarge(size));
    }
    Ok(Read::Bytes(output[slash + 1..].to_vec()))
}

/// Prints what `cksum` says about the file.
const STAMP: &str = "[ -e \"$1\" ] || exit 3; cksum < \"$1\"";

/// The file's `cksum` line ("checksum size"), `None` when it is gone.
///
/// # Errors
///
/// Connection and permission errors (never for a missing file).
pub fn stamp(host: &str, path: &Path) -> io::Result<Option<String>> {
    match run(host, STAMP, &[path.as_os_str()], None, COMMAND_TIMEOUT) {
        Ok(output) => Ok(Some(String::from_utf8_lossy(&output).trim().to_string())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Safe save, like `write_atomically` here: write a temp file next to the
/// target (through a symlink, to its target), keep the old permissions, check
/// nothing was cut off on the way, then rename it over the target.
const WRITE: &str = "t=$(readlink -f -- \"$1\" 2>/dev/null); [ -n \"$t\" ] || t=$1; \
    d=$(dirname -- \"$t\"); b=$(basename -- \"$t\"); tmp=\"$d/.$b.rubrique-$$.tmp\"; w=1; \
    if [ -e \"$t\" ]; then case $(ls -ld -- \"$t\") in ??-*) w=0;; esac; cp -p -- \"$t\" \"$tmp\" || exit 1; chmod u+w \"$tmp\"; fi; \
    if ! cat > \"$tmp\"; then rm -f \"$tmp\"; exit 1; fi; \
    s=$(wc -c < \"$tmp\"); if [ $s -ne \"$2\" ]; then rm -f \"$tmp\"; echo 'the upload was cut off' >&2; exit 1; fi; \
    [ $w = 1 ] || chmod u-w \"$tmp\"; \
    mv -f -- \"$tmp\" \"$t\" || { rm -f \"$tmp\"; exit 1; }";

/// Replace the file with `contents`, safely (see [`WRITE`]).
///
/// # Errors
///
/// The file is left as it was on any error.
pub fn write(host: &str, path: &Path, contents: &[u8]) -> io::Result<()> {
    let size = contents.len().to_string();
    run(host, WRITE, &[path.as_os_str(), OsStr::new(&size)], Some(contents), DOWNLOAD_TIMEOUT).map(|_| ())
}

const RENAME: &str = "[ -e \"$2\" ] && exit 5; [ -e \"$1\" ] || [ -L \"$1\" ] || exit 3; mv -- \"$1\" \"$2\"";

/// Rename `from` to `to`, refusing to replace an existing file.
///
/// # Errors
///
/// `AlreadyExists`, `NotFound`, connection and permission errors.
pub fn rename(host: &str, from: &Path, to: &Path) -> io::Result<()> {
    run(host, RENAME, &[from.as_os_str(), to.as_os_str()], None, COMMAND_TIMEOUT).map(|_| ())
}

const DELETE: &str = "[ -e \"$1\" ] || [ -L \"$1\" ] || exit 3; rm -f -- \"$1\"";

/// Delete the file at `path` for good: hosts have no shared Trash. `rm`
/// without `-r` refuses folders.
///
/// # Errors
///
/// `NotFound`, a folder, connection and permission errors.
pub fn delete(host: &str, path: &Path) -> io::Result<()> {
    run(host, DELETE, &[path.as_os_str()], None, COMMAND_TIMEOUT).map(|_| ())
}

const EXISTS: &str = "[ -e \"$1\" ] || [ -L \"$1\" ] || exit 3";

/// True when something is at `path` (false when it cannot be checked).
pub fn exists(host: &str, path: &Path) -> bool {
    run(host, EXISTS, &[path.as_os_str()], None, COMMAND_TIMEOUT).is_ok()
}

/// `~` and relative paths start in the home folder, like in a terminal.
const RESOLVE_FOLDER: &str = "p=$1; case $p in '~') p=$HOME;; '~/'*) p=\"$HOME/${p#'~/'}\";; esac; \
    [ -d \"$p\" ] || exit 3; cd -- \"$p\" && pwd -P";

/// The absolute path of the folder `typed` on the host (`~/notes`,
/// `notes`, `/srv/notes`). Also proves the host can be reached.
///
/// # Errors
///
/// `NotFound` when there is no such folder; connection errors.
pub fn resolve_folder(host: &str, typed: &str) -> io::Result<PathBuf> {
    let output = run(host, RESOLVE_FOLDER, &[OsStr::new(typed.trim())], None, COMMAND_TIMEOUT)?;
    let mut path = output;
    if path.last() == Some(&b'\n') {
        path.pop();
    }
    let path = PathBuf::from(<std::ffi::OsString as std::os::unix::ffi::OsStringExt>::from_vec(path));
    if path.is_absolute() { Ok(path) } else { Err(bad_answer(host)) }
}

/// Prints `<size>/<bytes>` per file, or `x/` for one that cannot be read or
/// is over `$1` bytes. Files are copied first so the size always fits.
const READ_MANY: &str = "m=$1; shift; t=$(mktemp) || exit 1; trap 'rm -f \"$t\"' EXIT; \
    for f in \"$@\"; do s=x; if [ -f \"$f\" ] && [ -r \"$f\" ]; then s=$(wc -c < \"$f\"); \
    if [ $s -le $m ] && cat -- \"$f\" > \"$t\"; then s=$(wc -c < \"$t\"); else s=x; fi; fi; \
    if [ \"$s\" = x ]; then printf 'x/'; else printf '%s/' $s; cat \"$t\"; fi; done";

/// Download several files in one go; `None` for each one that is missing,
/// unreadable or over `max_bytes`.
///
/// # Errors
///
/// Connection errors.
pub fn read_many(host: &str, paths: &[PathBuf], max_bytes: u64) -> io::Result<Vec<Option<Vec<u8>>>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let max = max_bytes.to_string();
    let mut args = vec![OsStr::new(&max)];
    args.extend(paths.iter().map(|path| path.as_os_str()));
    let output = run(host, READ_MANY, &args, None, DOWNLOAD_TIMEOUT)?;
    parse_many(&output, paths.len()).ok_or_else(|| bad_answer(host))
}

fn parse_many(mut output: &[u8], count: usize) -> Option<Vec<Option<Vec<u8>>>> {
    let mut files = Vec::with_capacity(count);
    for _ in 0..count {
        let slash = output.iter().position(|&byte| byte == b'/')?;
        let size = std::str::from_utf8(&output[..slash]).ok()?.trim();
        output = &output[slash + 1..];
        if size == "x" {
            files.push(None);
            continue;
        }
        let size: usize = size.parse().ok()?;
        files.push(Some(output.get(..size)?.to_vec()));
        output = &output[size..];
    }
    Some(files)
}

/// Prints every file (hidden ones skipped) one per line, a `/` line, then
/// the files of at most `$2` bytes whose text contains `$1` (any case).
const SEARCH: &str = "q=$1; m=$2; shift 2; \
    for r in \"$@\"; do find \"$r\" -mindepth 1 -name '.*' -prune -o -type f -print; done; echo /; \
    for r in \"$@\"; do find \"$r\" -mindepth 1 -name '.*' -prune -o -type f -size -$((m + 1))c \
    -exec grep -lIiF -e \"$q\" -- {} +; done; exit 0";

/// What a remote search found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Found {
    /// Every file under the folders, for name matches.
    pub files: Vec<PathBuf>,
    /// Files whose text contains the query.
    pub text_matches: Vec<PathBuf>,
}

/// Look for `query` under `folders` on the host.
///
/// # Errors
///
/// Connection errors.
pub fn search(host: &str, folders: &[PathBuf], query: &str, max_bytes: u64) -> io::Result<Found> {
    let max = max_bytes.to_string();
    let mut args = vec![OsStr::new(query), OsStr::new(&max)];
    args.extend(folders.iter().map(|folder| folder.as_os_str()));
    let output = run(host, SEARCH, &args, None, SEARCH_TIMEOUT)?;
    Ok(parse_search(&output))
}

fn parse_search(output: &[u8]) -> Found {
    use std::os::unix::ffi::OsStrExt;
    let mut found = Found::default();
    let mut after_marker = false;
    for line in output.split(|&byte| byte == b'\n').filter(|line| !line.is_empty()) {
        if line == b"/" {
            after_marker = true;
            continue;
        }
        let path = PathBuf::from(OsStr::from_bytes(line));
        if after_marker { found.text_matches.push(path) } else { found.files.push(path) }
    }
    found
}

fn bad_answer(host: &str) -> io::Error {
    io::Error::other(format!("unexpected answer from {host}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::testing::use_fake_ssh;
    use crate::test_support::TempFolder;
    use std::os::unix::fs::PermissionsExt;

    const HOST: &str = "test-host";

    #[test]
    fn scripts_are_one_line() {
        for script in [LIST, READ, STAMP, WRITE, RENAME, EXISTS, RESOLVE_FOLDER, READ_MANY, SEARCH] {
            assert!(!script.contains('\n'), "{script}");
            // fish reads `\\` inside single quotes as one backslash.
            assert!(!script.contains("\\\\"), "{script}");
        }
    }

    #[test]
    fn lists_files_folders_links_and_loops() {
        use_fake_ssh();
        let temp = TempFolder::new();
        temp.write("a.md", "");
        temp.write(".hidden", "");
        temp.write("odd name 'x'.md", "");
        temp.make_folder("docs");
        std::os::unix::fs::symlink(temp.path().join("docs"), temp.path().join("link")).unwrap();
        std::os::unix::fs::symlink(temp.path(), temp.path().join("docs/up")).unwrap();
        std::os::unix::fs::symlink(temp.path().join("gone"), temp.path().join("broken")).unwrap();

        let mut entries = list(HOST, temp.path()).unwrap();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, [".hidden", "a.md", "broken", "docs", "link", "odd name 'x'.md"]);
        let link = entries.iter().find(|entry| entry.name == "link").unwrap();
        assert!(link.is_folder && link.is_symlink && !link.is_loop);
        let broken = entries.iter().find(|entry| entry.name == "broken").unwrap();
        assert!(!broken.is_folder && broken.is_symlink);

        let inner = list(HOST, &temp.path().join("docs")).unwrap();
        assert_eq!(inner, [Entry { name: "up".to_string(), is_folder: true, is_symlink: true, is_loop: true }]);
        assert_eq!(list(HOST, &temp.path().join("nope")).unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn empty_folder_lists_nothing() {
        use_fake_ssh();
        let temp = TempFolder::new();
        assert_eq!(list(HOST, temp.path()).unwrap(), []);
    }

    #[test]
    fn reads_whole_files_and_refuses_large_ones() {
        use_fake_ssh();
        let temp = TempFolder::new();
        let text = "line/one\r\n\nno final newline";
        let path = temp.write("a.md", text);
        assert_eq!(read(HOST, &path, 1000).unwrap(), Read::Bytes(text.as_bytes().to_vec()));
        assert_eq!(read(HOST, &path, 5).unwrap(), Read::TooLarge(text.len() as u64));
        let empty = temp.write("empty.md", "");
        assert_eq!(read(HOST, &empty, 5).unwrap(), Read::Bytes(Vec::new()));
        assert_eq!(read(HOST, &temp.path().join("x"), 5).unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn stamps_match_the_local_checksum() {
        use_fake_ssh();
        let temp = TempFolder::new();
        let path = temp.write("a.md", "hello\n");
        assert_eq!(stamp(HOST, &path).unwrap(), Some(crate::remote::cksum::cksum_text(b"hello\n")));
        assert_eq!(stamp(HOST, &temp.path().join("gone")).unwrap(), None);
    }

    #[test]
    fn writes_safely_and_keeps_mode_and_links() {
        use_fake_ssh();
        let temp = TempFolder::new();
        let path = temp.write("a.md", "old");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        write(HOST, &path, b"new text").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new text");
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o640);

        let link = temp.path().join("link.md");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        write(HOST, &link, b"through the link").unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "through the link");

        let fresh = temp.path().join("new.md");
        write(HOST, &fresh, b"").unwrap();
        assert_eq!(std::fs::read(&fresh).unwrap(), b"");

        // No temp files left behind.
        let names: Vec<String> = std::fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|name| !name.contains("rubrique")), "{names:?}");
    }

    #[test]
    fn writes_read_only_files_like_local_saves() {
        use_fake_ssh();
        let temp = TempFolder::new();
        let path = temp.write("a.md", "old");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
        write(HOST, &path, b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o444);
    }

    #[test]
    fn cut_off_uploads_change_nothing() {
        use_fake_ssh();
        let temp = TempFolder::new();
        let path = temp.write("a.md", "old");
        // Claim more bytes than are sent, as when the connection drops.
        let error =
            run(HOST, WRITE, &[path.as_os_str(), OsStr::new("100")], Some(b"short"), COMMAND_TIMEOUT).unwrap_err();
        assert!(error.to_string().contains("cut off"), "{error}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old");
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn deletes_files_but_not_folders() {
        use_fake_ssh();
        let temp = TempFolder::new();
        let file = temp.write("a.md", "a");
        delete(HOST, &file).unwrap();
        assert!(!exists(HOST, &file));
        assert_eq!(delete(HOST, &file).unwrap_err().kind(), io::ErrorKind::NotFound);
        temp.make_folder("docs");
        assert!(delete(HOST, &temp.path().join("docs")).is_err());
        assert!(temp.path().join("docs").is_dir());
    }

    #[test]
    fn renames_without_replacing() {
        use_fake_ssh();
        let temp = TempFolder::new();
        let from = temp.write("a.md", "a");
        let taken = temp.write("b.md", "b");
        assert_eq!(rename(HOST, &from, &taken).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        let to = temp.path().join("c.md");
        rename(HOST, &from, &to).unwrap();
        assert!(exists(HOST, &to));
        assert!(!exists(HOST, &from));
    }

    #[test]
    fn resolves_typed_folders() {
        use_fake_ssh();
        let temp = TempFolder::new();
        temp.make_folder("notes");
        let typed = temp.path().join("notes").join("..").join("notes");
        assert_eq!(resolve_folder(HOST, &typed.display().to_string()).unwrap(), temp.path().join("notes"));
        let home = resolve_folder(HOST, "~").unwrap();
        assert!(home.is_absolute());
        assert_eq!(resolve_folder(HOST, "/no/such/folder").unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn reads_many_files() {
        use_fake_ssh();
        let temp = TempFolder::new();
        let a = temp.write("a.png", "12/34");
        let big = temp.write("big.png", "0123456789");
        let paths = [a, temp.path().join("missing.png"), big];
        let files = read_many(HOST, &paths, 6).unwrap();
        assert_eq!(files, [Some(b"12/34".to_vec()), None, None]);
    }

    #[test]
    fn searches_names_and_text() {
        use_fake_ssh();
        let temp = TempFolder::new();
        temp.write("Plan.md", "nothing here");
        temp.write("docs/b.md", "the PLAN is set");
        temp.write(".git/plan", "plan");
        temp.write("docs/.secret.md", "plan");
        let found = search(HOST, &[temp.path().to_path_buf()], "plan", 1000).unwrap();
        let mut files = found.files.clone();
        files.sort();
        assert_eq!(files, [temp.path().join("Plan.md"), temp.path().join("docs/b.md")]);
        assert_eq!(found.text_matches, [temp.path().join("docs/b.md")]);
    }
}
