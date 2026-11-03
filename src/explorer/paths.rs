//! Path strings for the "Copy path" actions in the explorer.

use std::path::Path;

/// Full path as shown to the user, e.g. `/home/a/notes/todo.md`, or
/// `server:/home/a/notes/todo.md` for a remote file.
pub fn absolute_path_text(path: &Path) -> String {
    crate::remote::display(path)
}

/// Path relative to the explorer root, e.g. `docs/todo.md`.
///
/// The root itself becomes `.`. A path outside the root (should not happen
/// in the explorer) falls back to the absolute path.
pub fn relative_path_text(path: &Path, root: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(relative) if relative.as_os_str().is_empty() => ".".to_string(),
        Ok(relative) => relative.display().to_string(),
        Err(_) => absolute_path_text(path),
    }
}

/// Last path component, e.g. `todo.md`. The filesystem root `/` has no
/// name, so it falls back to the full path.
pub fn name_text(path: &Path) -> String {
    match path.file_name() {
        Some(name) => name.to_string_lossy().into_owned(),
        None => absolute_path_text(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn absolute_keeps_spaces_and_unicode() {
        let path = PathBuf::from("/home/a/my notes/привіт.md");
        assert_eq!(absolute_path_text(&path), "/home/a/my notes/привіт.md");
    }

    #[test]
    fn relative_to_root() {
        let root = Path::new("/home/a/notes");
        assert_eq!(relative_path_text(Path::new("/home/a/notes/docs/todo.md"), root), "docs/todo.md");
        assert_eq!(relative_path_text(Path::new("/home/a/notes/todo.md"), root), "todo.md");
    }

    #[test]
    fn relative_of_root_is_dot() {
        let root = Path::new("/home/a/notes");
        assert_eq!(relative_path_text(root, root), ".");
    }

    #[test]
    fn relative_outside_root_falls_back_to_absolute() {
        let root = Path::new("/home/a/notes");
        assert_eq!(relative_path_text(Path::new("/etc/hosts"), root), "/etc/hosts");
    }

    #[test]
    fn name_of_file_folder_and_fs_root() {
        assert_eq!(name_text(Path::new("/home/a/notes/todo.md")), "todo.md");
        assert_eq!(name_text(Path::new("/home/a/notes")), "notes");
        assert_eq!(name_text(Path::new("/")), "/");
    }
}
