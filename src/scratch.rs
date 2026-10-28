//! Scratch files: quick, empty notes with a random readable name, like
//! `2026-10-28-brave-otter.md`, in one folder.

use std::collections::hash_map::RandomState;
use std::fs::{self, OpenOptions};
use std::hash::{BuildHasher, Hasher};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

/// First word of a scratch name.
const ADJECTIVES: [&str; 32] = [
    "amber", "bold", "brave", "bright", "calm", "clever", "cosy", "crisp", "eager", "fancy", "gentle", "golden",
    "happy", "honest", "jolly", "kind", "lively", "lucky", "merry", "mighty", "nimble", "proud", "quick", "quiet",
    "rapid", "shiny", "silent", "sunny", "swift", "tidy", "witty", "young",
];
/// Second word of a scratch name.
const NOUNS: [&str; 32] = [
    "badger", "beacon", "breeze", "brook", "canyon", "cedar", "comet", "falcon", "fern", "harbor", "heron", "island",
    "lantern", "maple", "meadow", "otter", "owl", "pebble", "pine", "planet", "river", "robin", "sparrow", "spruce",
    "summit", "thistle", "tiger", "tulip", "valley", "walrus", "willow", "wren",
];
/// Names tried before giving up when they all exist already.
const MAX_ATTEMPTS: u32 = 64;

/// `DATE-adjective-noun.md`, words picked from `seed`.
pub fn scratch_name(date: &str, seed: u64) -> String {
    let adjective = ADJECTIVES[usize::try_from(seed % 32).unwrap_or(0)];
    let noun = NOUNS[usize::try_from((seed / 32) % 32).unwrap_or(0)];
    format!("{date}-{adjective}-{noun}.md")
}

/// A random number, different on every call (from the standard library's
/// randomly seeded hasher, so no extra dependency).
fn random_seed(attempt: u32) -> u64 {
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u32(attempt);
    hasher.finish()
}

/// Create an empty scratch file in `folder` (made if missing) and return its
/// path. Never overwrites: a taken name just gets another try.
///
/// # Errors
///
/// Fails when the folder cannot be created or written, or every tried name exists.
pub fn create_scratch(folder: &Path, date: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(folder)?;
    for attempt in 0..MAX_ATTEMPTS {
        let path = folder.join(scratch_name(date, random_seed(attempt)));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(path),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(ErrorKind::AlreadyExists, "no free scratch name, clean up the scratches folder"))
}

/// The new path for renaming `path` to `new_name` in the same folder.
///
/// # Errors
///
/// A message for the user when the name is empty, holds a path separator,
/// is `.` or `..`, or the target already exists.
pub fn rename_target(path: &Path, new_name: &str) -> Result<PathBuf, String> {
    let name = new_name.trim();
    if name.is_empty() {
        return Err("The name cannot be empty".to_string());
    }
    if name.contains(['/', '\\', '\0']) || name == "." || name == ".." {
        return Err(format!("Not a valid file name: {name}"));
    }
    let target = path.with_file_name(name);
    if target != path && target.exists() {
        return Err(format!("{name} already exists"));
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempFolder;

    #[test]
    fn names_are_date_adjective_noun() {
        assert_eq!(scratch_name("2026-10-28", 0), "2026-10-28-amber-badger.md");
        assert_eq!(scratch_name("2026-10-28", 2 + 32 * 15), "2026-10-28-brave-otter.md");
        assert_eq!(scratch_name("2026-10-28", u64::MAX), "2026-10-28-young-wren.md");
    }

    #[test]
    fn creates_an_empty_file_and_the_folder() {
        let temp = TempFolder::new();
        let folder = temp.path().join("scratches");
        let path = create_scratch(&folder, "2026-10-28").unwrap();
        assert_eq!(path.parent(), Some(folder.as_path()));
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
        let name = path.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with("2026-10-28-"), "{name}");
        assert_eq!(path.extension().and_then(|extension| extension.to_str()), Some("md"));
    }

    #[test]
    fn never_overwrites_an_existing_scratch() {
        let temp = TempFolder::new();
        let mut paths = Vec::new();
        for _ in 0..20 {
            paths.push(create_scratch(temp.path(), "2026-10-28").unwrap());
        }
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), 20);
    }

    #[test]
    fn rename_checks_the_new_name() {
        let temp = TempFolder::new();
        let file = temp.write("a.md", "");
        temp.write("taken.md", "");
        assert_eq!(rename_target(&file, " plan.md "), Ok(temp.path().join("plan.md")));
        assert_eq!(rename_target(&file, "a.md"), Ok(file.clone()));
        assert!(rename_target(&file, "").is_err());
        assert!(rename_target(&file, "sub/x.md").is_err());
        assert!(rename_target(&file, "..").is_err());
        assert_eq!(rename_target(&file, "taken.md"), Err("taken.md already exists".to_string()));
    }
}
