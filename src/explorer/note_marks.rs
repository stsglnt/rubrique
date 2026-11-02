//! Which files have notes, for the marker in the explorer. A file is checked
//! when its row is drawn, at most every [`CHECK_EVERY`], and read again only
//! when its size or modification time changed.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crate::markdown::notes::find_notes;

/// How long a check is trusted before the file's metadata is read again.
const CHECK_EVERY: Duration = Duration::from_secs(2);
/// Files larger than this are not read; they count as having no notes.
const MAX_LEN: u64 = 8 * 1024 * 1024;

/// What is known about one file.
#[derive(Clone, Copy, Debug)]
struct Entry {
    modified: Option<SystemTime>,
    len: u64,
    has_notes: bool,
    checked: Instant,
}

/// A cache of which files have notes.
#[derive(Debug)]
pub struct NoteMarks {
    cache: RefCell<HashMap<PathBuf, Entry>>,
    check_every: Duration,
}

impl Default for NoteMarks {
    fn default() -> Self {
        NoteMarks { cache: RefCell::new(HashMap::new()), check_every: CHECK_EVERY }
    }
}

impl NoteMarks {
    /// True when the file has at least one note. Cheap to call every frame.
    pub fn has_notes(&self, path: &Path) -> bool {
        let now = Instant::now();
        let mut cache = self.cache.borrow_mut();
        if let Some(entry) = cache.get(path)
            && now.duration_since(entry.checked) < self.check_every
        {
            return entry.has_notes;
        }
        let (modified, len) =
            fs::metadata(path).map_or((None, 0), |metadata| (metadata.modified().ok(), metadata.len()));
        let entry = cache.entry(path.to_path_buf()).or_insert(Entry {
            modified: None,
            len: u64::MAX,
            has_notes: false,
            checked: now,
        });
        if entry.modified != modified || entry.len != len {
            entry.has_notes = len <= MAX_LEN && file_has_notes(path);
            entry.modified = modified;
            entry.len = len;
        }
        entry.checked = now;
        entry.has_notes
    }

    /// Drop what is known, so every file is read again.
    pub fn clear(&self) {
        self.cache.borrow_mut().clear();
    }
}

fn file_has_notes(path: &Path) -> bool {
    fs::read_to_string(path).is_ok_and(|text| !find_notes(&text).is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempFolder;

    fn marks() -> NoteMarks {
        NoteMarks { check_every: Duration::ZERO, ..NoteMarks::default() }
    }

    #[test]
    fn finds_notes_and_follows_changes() {
        let folder = TempFolder::new();
        let plain = folder.write("plain.md", "# Hello\n");
        let noted = folder.write("noted.md", "Call Ann<!-- note on \"Ann\": today -->\n");
        let marks = marks();
        assert!(!marks.has_notes(&plain));
        assert!(marks.has_notes(&noted));
        assert!(!marks.has_notes(&folder.path().join("missing.md")));

        // A different size is noticed at once; the note is gone.
        fs::write(&noted, "Call Ann\n").unwrap();
        assert!(!marks.has_notes(&noted));
        marks.clear();
        assert!(!marks.has_notes(&noted));
    }

    #[test]
    fn trusts_a_recent_check() {
        let folder = TempFolder::new();
        let path = folder.write("a.md", "text\n");
        let marks = NoteMarks::default();
        assert!(!marks.has_notes(&path));
        // Same length, so even a fresh check would read nothing new; the
        // cached answer is returned without touching the disk.
        fs::write(&path, "<!-- note on \"t\": x -->").unwrap();
        assert!(!marks.has_notes(&path));
    }
}
