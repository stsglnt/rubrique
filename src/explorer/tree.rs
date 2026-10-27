//! File tree model: lazy loading, sorting, Markdown filter, symlink loops.
//!
//! No egui here; `view.rs` draws this model.

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

/// Extensions treated as Markdown (compared case-insensitively).
const MARKDOWN_EXTENSIONS: [&str; 5] = ["md", "markdown", "mdown", "mkd", "mdx"];

/// Extension-less file names that are usually Markdown.
const MARKDOWN_BARE_NAMES: [&str; 4] = ["README", "CHANGELOG", "LICENSE", "CONTRIBUTING"];

/// True when a file should get the Markdown preview and pass the
/// "Markdown only" filter.
pub fn is_markdown_path(path: &Path) -> bool {
    if let Some(extension) = path.extension() {
        let extension = extension.to_string_lossy();
        return MARKDOWN_EXTENSIONS.iter().any(|known| extension.eq_ignore_ascii_case(known));
    }
    let name = path.file_name().map(|name| name.to_string_lossy()).unwrap_or_default();
    MARKDOWN_BARE_NAMES.iter().any(|known| name.eq_ignore_ascii_case(known))
}

/// Children of a folder, once someone expanded it.
#[derive(Debug)]
pub enum Children {
    /// Never expanded; nothing read from disk yet.
    NotLoaded,
    /// Entries read from disk, sorted for display.
    Loaded(Vec<FsNode>),
    /// Reading the folder failed (permissions, deleted, ...).
    Failed(String),
}

/// What kind of entry a node is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// A file, or a broken symlink (it has no target to tell otherwise).
    File,
    /// A folder, or a symlink to one.
    Folder,
    /// A symlinked folder pointing at itself or one of its ancestors.
    /// Shown, but never expanded, so the tree cannot recurse forever.
    SymlinkLoop,
}

/// One file or folder in the tree.
#[derive(Debug)]
pub struct FsNode {
    /// Path as listed under the root (symlinks are not resolved).
    pub path: PathBuf,
    /// File name shown in the tree.
    pub name: String,
    /// File, folder, or looping folder link.
    pub kind: NodeKind,
    /// The entry itself is a symlink (shown with a link marker).
    pub is_symlink: bool,
    /// Folder is open in the tree.
    pub expanded: bool,
    /// Entries of a folder, loaded on first expand.
    pub children: Children,
}

impl FsNode {
    /// Root node for a folder; starts expanded with its children loaded.
    pub fn new_root(path: PathBuf) -> Self {
        let name = super::paths::name_text(&path);
        let mut root = FsNode {
            path,
            name,
            kind: NodeKind::Folder,
            is_symlink: false,
            expanded: true,
            children: Children::NotLoaded,
        };
        root.load_children();
        root
    }

    /// True for folders, including looping folder links.
    pub fn is_folder(&self) -> bool {
        matches!(self.kind, NodeKind::Folder | NodeKind::SymlinkLoop)
    }

    /// Expand or collapse a folder, reading it from disk on first expand.
    pub fn toggle_expanded(&mut self) {
        if self.kind != NodeKind::Folder {
            return;
        }
        self.expanded = !self.expanded;
        if self.expanded && matches!(self.children, Children::NotLoaded) {
            self.load_children();
        }
    }

    /// Read this folder's entries from disk, replacing any loaded children.
    pub fn load_children(&mut self) {
        self.children = match read_folder(&self.path) {
            Ok(children) => Children::Loaded(children),
            Err(error) => Children::Failed(error.to_string()),
        };
    }

    /// Re-read every loaded folder, keeping which folders are expanded.
    pub fn refresh(&mut self) {
        if matches!(self.children, Children::NotLoaded) {
            return;
        }
        let previous = std::mem::replace(&mut self.children, Children::NotLoaded);
        self.load_children();
        let (Children::Loaded(mut old_children), Children::Loaded(new_children)) = (previous, &mut self.children)
        else {
            return;
        };
        // Carry over each folder's previous state, then refresh it in turn.
        for new_child in new_children.iter_mut().filter(|child| child.kind == NodeKind::Folder) {
            let Some(index) =
                old_children.iter().position(|old| old.path == new_child.path && old.kind == NodeKind::Folder)
            else {
                continue;
            };
            let old_child = old_children.swap_remove(index);
            new_child.expanded = old_child.expanded;
            new_child.children = old_child.children;
            new_child.refresh();
        }
    }
}

/// Read one folder: every entry, folders first, then case-insensitive by name.
///
/// # Errors
///
/// Fails when the folder cannot be listed (missing, permissions, ...).
pub fn read_folder(folder: &Path) -> std::io::Result<Vec<FsNode>> {
    // Canonical path of the folder being read, used for symlink loop checks.
    let canonical_folder = fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf());
    let mut nodes = Vec::new();
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        nodes.push(node_from_entry(&entry, &canonical_folder));
    }
    nodes.sort_by(compare_nodes);
    Ok(nodes)
}

/// Build an unexpanded node for one directory entry.
fn node_from_entry(entry: &fs::DirEntry, canonical_parent: &Path) -> FsNode {
    let path = entry.path();
    let is_symlink = entry.file_type().is_ok_and(|file_type| file_type.is_symlink());
    // `metadata` follows symlinks, so a link to a folder counts as a folder.
    // A broken link has no metadata and is shown as a file.
    let is_folder = fs::metadata(&path).is_ok_and(|metadata| metadata.is_dir());
    let kind = if !is_folder {
        NodeKind::File
    } else if is_symlink && points_to_ancestor(&path, canonical_parent) {
        NodeKind::SymlinkLoop
    } else {
        NodeKind::Folder
    };
    FsNode {
        name: entry.file_name().to_string_lossy().into_owned(),
        path,
        kind,
        is_symlink,
        expanded: false,
        children: Children::NotLoaded,
    }
}

/// True when the symlink's target is the parent folder or one of its
/// ancestors. Expanding such a link would show the same tree again forever.
fn points_to_ancestor(link: &Path, canonical_parent: &Path) -> bool {
    match fs::canonicalize(link) {
        Ok(target) => canonical_parent.starts_with(&target),
        Err(_) => false,
    }
}

/// Display order: folders (including looping links) first, then by name
/// ignoring case, with an exact-case tie-break so the order is stable.
fn compare_nodes(left: &FsNode, right: &FsNode) -> Ordering {
    right
        .is_folder()
        .cmp(&left.is_folder())
        .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        .then_with(|| left.name.cmp(&right.name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempFolder;

    fn names(nodes: &[FsNode]) -> Vec<&str> {
        nodes.iter().map(|node| node.name.as_str()).collect()
    }

    fn loaded(node: &FsNode) -> &[FsNode] {
        match &node.children {
            Children::Loaded(children) => children,
            other => panic!("expected loaded children, got {other:?}"),
        }
    }

    #[test]
    fn markdown_detection() {
        assert!(is_markdown_path(Path::new("a/notes.md")));
        assert!(is_markdown_path(Path::new("a/NOTES.MarkDown")));
        assert!(is_markdown_path(Path::new("README")));
        assert!(is_markdown_path(Path::new("x/license")));
        assert!(!is_markdown_path(Path::new("main.rs")));
        assert!(!is_markdown_path(Path::new("Makefile")));
        assert!(!is_markdown_path(Path::new(".gitignore")));
    }

    #[test]
    fn lists_all_files_folders_first_case_insensitive() {
        let temp = TempFolder::new();
        temp.write("b.md", "");
        temp.write("A.txt", "");
        temp.write(".hidden", "");
        temp.write("Makefile", "");
        temp.make_folder("zeta");
        temp.make_folder("Alpha");

        let nodes = read_folder(temp.path()).unwrap();
        assert_eq!(names(&nodes), ["Alpha", "zeta", ".hidden", "A.txt", "b.md", "Makefile"]);
    }

    #[test]
    fn folders_load_lazily() {
        let temp = TempFolder::new();
        temp.write("docs/inner.md", "");
        let mut root = FsNode::new_root(temp.path().to_path_buf());
        let docs = &mut match &mut root.children {
            Children::Loaded(children) => children,
            _ => unreachable!(),
        }[0];
        assert!(matches!(docs.children, Children::NotLoaded));
        docs.toggle_expanded();
        assert_eq!(names(loaded(docs)), ["inner.md"]);
    }

    #[test]
    fn refresh_picks_up_new_files_and_keeps_expansion() {
        let temp = TempFolder::new();
        temp.write("docs/one.md", "");
        let mut root = FsNode::new_root(temp.path().to_path_buf());
        if let Children::Loaded(children) = &mut root.children {
            children[0].toggle_expanded();
        }
        temp.write("docs/two.md", "");
        temp.write("top.md", "");
        root.refresh();

        assert_eq!(names(loaded(&root)), ["docs", "top.md"]);
        let docs = &loaded(&root)[0];
        assert!(docs.expanded);
        assert_eq!(names(loaded(docs)), ["one.md", "two.md"]);
    }

    #[test]
    fn unreadable_folder_reports_error() {
        let mut node = FsNode::new_root(PathBuf::from("/definitely/not/here"));
        assert!(matches!(node.children, Children::Failed(_)));
        node.refresh();
        assert!(matches!(node.children, Children::Failed(_)));
    }

    #[test]
    fn symlinked_folder_is_followed_and_loop_detected() {
        let temp = TempFolder::new();
        temp.write("real/file.md", "");
        temp.make_folder("inner");
        std::os::unix::fs::symlink(temp.path().join("real"), temp.path().join("link")).unwrap();
        std::os::unix::fs::symlink(temp.path(), temp.path().join("inner/back_to_root")).unwrap();

        let nodes = read_folder(temp.path()).unwrap();
        let link = nodes.iter().find(|node| node.name == "link").unwrap();
        assert!(link.kind == NodeKind::Folder && link.is_symlink);

        let inner = read_folder(&temp.path().join("inner")).unwrap();
        assert_eq!(inner[0].kind, NodeKind::SymlinkLoop);
        let mut looping = FsNode { expanded: false, ..inner.into_iter().next().unwrap() };
        looping.toggle_expanded();
        assert!(!looping.expanded);
    }
}
