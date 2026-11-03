//! Mirror paths: where a remote file lives on this computer.
//!
//! `host:/home/me/notes/a.md` is mirrored at
//! `<mirror root>/host/home/me/notes/a.md`. Pure path functions, no I/O.

use std::path::{Component, Path, PathBuf};
use std::sync::{PoisonError, RwLock};

/// Set by tests; otherwise the `remote` folder in the config folder.
static MIRROR_ROOT: RwLock<Option<PathBuf>> = RwLock::new(None);

/// A file or folder on a remote host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemotePath {
    /// The name given to `ssh`: a `Host` from `~/.ssh/config` or `user@host`.
    pub host: String,
    /// Absolute path on the host.
    pub path: PathBuf,
}

/// Use `root` as the mirror root (tests only use a temporary folder).
#[cfg(test)]
pub fn set_mirror_root(root: PathBuf) {
    *MIRROR_ROOT.write().unwrap_or_else(PoisonError::into_inner) = Some(root);
}

/// The folder all mirror paths are under.
pub fn mirror_root() -> Option<PathBuf> {
    let set = MIRROR_ROOT.read().unwrap_or_else(PoisonError::into_inner).clone();
    set.or_else(crate::settings::remote_mirror_folder)
}

/// True for a name that is safe to give to `ssh` as the host and to use as a
/// folder name: letters, digits and `. _ - @`, not starting with `-` or `.`
/// (a leading `-` would be read as an ssh option).
pub fn is_valid_host(host: &str) -> bool {
    !host.is_empty()
        && !host.starts_with(['-', '.'])
        && host.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | '@'))
}

/// The mirror path of `remote` on `host`. `None` for an invalid host, a
/// relative path or one with `..` in it.
pub fn to_mirror(host: &str, remote: &Path) -> Option<PathBuf> {
    if !is_valid_host(host) || !remote.is_absolute() {
        return None;
    }
    let mut mirror = mirror_root()?.join(host);
    for component in remote.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(part) => mirror.push(part),
            Component::ParentDir | Component::Prefix(_) => return None,
        }
    }
    Some(mirror)
}

/// The remote file a mirror path stands for; `None` for a local path.
pub fn from_mirror(path: &Path) -> Option<RemotePath> {
    let root = mirror_root()?;
    let rest = path.strip_prefix(&root).ok()?;
    let mut components = rest.components();
    let Some(Component::Normal(host)) = components.next() else {
        return None;
    };
    let host = host.to_str()?;
    if !is_valid_host(host) {
        return None;
    }
    let mut remote = PathBuf::from("/");
    for component in components {
        match component {
            Component::Normal(part) => remote.push(part),
            _ => return None,
        }
    }
    Some(RemotePath { host: host.to_string(), path: remote })
}

/// True when `path` is a mirror path (a file on a remote host).
pub fn is_mirror(path: &Path) -> bool {
    from_mirror(path).is_some()
}

/// A path for people: `host:/remote/path` for a remote file, the path
/// itself otherwise.
pub fn display(path: &Path) -> String {
    match from_mirror(path) {
        Some(remote) => format!("{}:{}", remote.host, remote.path.display()),
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::testing::use_fake_ssh;

    #[test]
    fn hosts_are_checked() {
        assert!(is_valid_host("box"));
        assert!(is_valid_host("me@notes.example.com"));
        assert!(is_valid_host("my_host-2"));
        assert!(!is_valid_host(""));
        assert!(!is_valid_host("-oProxyCommand=evil"));
        assert!(!is_valid_host(".hidden"));
        assert!(!is_valid_host("a/b"));
        assert!(!is_valid_host("a b"));
        assert!(!is_valid_host("host;rm"));
    }

    #[test]
    fn mirror_paths_go_both_ways() {
        use_fake_ssh();
        let mirror = to_mirror("box", Path::new("/home/me/notes/a b.md")).unwrap();
        assert!(mirror.starts_with(mirror_root().unwrap()));
        assert!(mirror.ends_with("box/home/me/notes/a b.md"));
        let back = from_mirror(&mirror).unwrap();
        assert_eq!(back, RemotePath { host: "box".to_string(), path: PathBuf::from("/home/me/notes/a b.md") });
        assert_eq!(display(&mirror), "box:/home/me/notes/a b.md");
        assert!(is_mirror(&mirror));
    }

    #[test]
    fn host_root_is_slash() {
        use_fake_ssh();
        let mirror = to_mirror("box", Path::new("/")).unwrap();
        assert_eq!(from_mirror(&mirror).unwrap().path, PathBuf::from("/"));
    }

    #[test]
    fn bad_paths_are_refused() {
        use_fake_ssh();
        assert_eq!(to_mirror("box", Path::new("relative/a.md")), None);
        assert_eq!(to_mirror("box", Path::new("/a/../../etc")), None);
        assert_eq!(to_mirror("-x", Path::new("/a")), None);
    }

    #[test]
    fn local_paths_are_not_mirrors() {
        use_fake_ssh();
        assert!(!is_mirror(Path::new("/home/me/notes/a.md")));
        assert_eq!(display(Path::new("/home/me/a.md")), "/home/me/a.md");
        let root = mirror_root().unwrap();
        assert!(!is_mirror(&root));
        assert!(!is_mirror(&root.join("-bad").join("x")));
    }
}
