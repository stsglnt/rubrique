//! Search: matches of a query in a text (case-insensitive), and in every
//! text file under the explorer's folders. Pure functions, apart from the
//! file reading in [`search_files`].

use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// Files larger than this are skipped in an all-files search.
const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// Longest excerpt shown for a hit, in characters.
const EXCERPT_CHARS: usize = 100;

/// Byte ranges of every case-insensitive occurrence of `query` in `text`,
/// in order, without overlaps. An empty query matches nothing.
pub fn find_all(text: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let lowered = Lowered::new(text);
    let query = query.to_lowercase();
    lowered
        .text
        .match_indices(query.as_str())
        .map(|(start, found)| lowered.original(start..start + found.len()))
        .collect()
}

/// `text` in lower case, with the way back to the original byte offsets
/// (lower-casing can change the byte length of a character).
struct Lowered {
    text: String,
    /// For each byte of `text`, the original byte it came from; one more
    /// entry for the end.
    origin: Vec<usize>,
}

impl Lowered {
    fn new(text: &str) -> Self {
        let mut lowered = String::with_capacity(text.len());
        let mut origin = Vec::with_capacity(text.len() + 1);
        for (index, character) in text.char_indices() {
            for lower in character.to_lowercase() {
                let before = lowered.len();
                lowered.push(lower);
                origin.extend(std::iter::repeat_n(index, lowered.len() - before));
            }
        }
        origin.push(text.len());
        Lowered { text: lowered, origin }
    }

    /// The original byte range for a range in the lowered text.
    fn original(&self, range: Range<usize>) -> Range<usize> {
        self.origin[range.start]..self.origin[range.end]
    }
}

/// Where a query was found in a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileHit {
    pub path: PathBuf,
    /// The explorer folder the file was found under.
    pub root: PathBuf,
    pub kind: HitKind,
}

/// What matched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HitKind {
    /// The file's name contains the query.
    Name,
    /// A line of the file contains the query.
    Text {
        /// 1-based line number.
        line: usize,
        /// The line, trimmed and cut to [`EXCERPT_CHARS`].
        excerpt: String,
        /// Byte range of the match in the whole file.
        range: Range<usize>,
    },
}

/// An all-files search.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchResults {
    pub hits: Vec<FileHit>,
    pub files_searched: usize,
    /// Stopped at `limit` hits; there may be more.
    pub truncated: bool,
}

impl SearchResults {
    /// Files with at least one hit.
    pub fn file_count(&self) -> usize {
        let mut paths: Vec<&Path> = self.hits.iter().map(|hit| hit.path.as_path()).collect();
        paths.dedup();
        paths.len()
    }
}

/// Search the text files under `roots` (hidden entries skipped) for `query`,
/// stopping after `limit` hits. Files come in explorer order; name hits
/// before text hits within a file.
pub fn search_files(roots: &[PathBuf], query: &str, limit: usize) -> SearchResults {
    let mut results = SearchResults::default();
    if query.trim().is_empty() {
        return results;
    }
    let query = query.to_lowercase();
    for root in roots {
        search_folder(root, root, &query, limit, &mut results);
        if results.truncated {
            break;
        }
    }
    results
}

fn search_folder(folder: &Path, root: &Path, query: &str, limit: usize, results: &mut SearchResults) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let mut entries: Vec<fs::DirEntry> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());
    for entry in entries {
        if results.truncated {
            return;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            // Links to folders are not followed, so a loop cannot happen.
            if !entry.file_type().is_ok_and(|kind| kind.is_symlink()) {
                search_folder(&path, root, query, limit, results);
            }
            continue;
        }
        if name.to_lowercase().contains(query) {
            push_hit(results, FileHit { path: path.clone(), root: root.to_path_buf(), kind: HitKind::Name }, limit);
        }
        if metadata.len() > MAX_FILE_BYTES {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        // Encrypted scratches: only their names are searched.
        if text.contains('\0') || crate::crypto::is_encrypted(text.as_bytes()) {
            continue;
        }
        results.files_searched += 1;
        for range in find_all(&text, query) {
            if results.truncated {
                return;
            }
            let (line, excerpt) = line_of(&text, range.start);
            let kind = HitKind::Text { line, excerpt, range };
            push_hit(results, FileHit { path: path.clone(), root: root.to_path_buf(), kind }, limit);
        }
    }
}

fn push_hit(results: &mut SearchResults, hit: FileHit, limit: usize) {
    if results.hits.len() >= limit {
        results.truncated = true;
    } else {
        results.hits.push(hit);
    }
}

/// The 1-based line number of byte `at` and that line as an excerpt.
fn line_of(text: &str, at: usize) -> (usize, String) {
    let line_start = text[..at].rfind('\n').map_or(0, |index| index + 1);
    let line_end = text[at..].find('\n').map_or(text.len(), |index| at + index);
    let line = text[..at].matches('\n').count() + 1;
    let excerpt: String = text[line_start..line_end].trim().chars().take(EXCERPT_CHARS).collect();
    (line, excerpt)
}

/// The character index of byte `at` in `text`.
pub fn char_index(text: &str, at: usize) -> usize {
    text[..at.min(text.len())].chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempFolder;

    #[test]
    fn finds_without_case_and_without_overlaps() {
        assert_eq!(find_all("Ab ab AB", "ab"), vec![0..2, 3..5, 6..8]);
        assert_eq!(find_all("aaaa", "aa"), vec![0..2, 2..4]);
        assert_eq!(find_all("text", ""), Vec::<Range<usize>>::new());
        assert_eq!(find_all("", "x"), Vec::<Range<usize>>::new());
    }

    #[test]
    fn lower_casing_keeps_original_offsets() {
        // İ (2 bytes) lower-cases to i̇ (3 bytes); the match after it still
        // points into the original text.
        let text = "İstanbul ÉTÉ";
        let ranges = find_all(text, "été");
        assert_eq!(ranges.len(), 1);
        assert_eq!(&text[ranges[0].clone()], "ÉTÉ");
        assert_eq!(&text[find_all(text, "i̇stan")[0].clone()], "İstan");
    }

    #[test]
    fn line_numbers_and_excerpts() {
        let text = "first\n  second line here \nthird";
        assert_eq!(line_of(text, 0), (1, "first".to_string()));
        assert_eq!(line_of(text, 10), (2, "second line here".to_string()));
        assert_eq!(line_of(text, 27), (3, "third".to_string()));
        assert_eq!(char_index("héllo", 3), 2);
    }

    #[test]
    fn searches_files_by_name_and_text() {
        let folder = TempFolder::new();
        folder.write("notes/Plan.md", "# Plan\nbuy milk\nMilk is white\n");
        folder.write("notes/.hidden.md", "milk\n");
        folder.write("other.txt", "no match\n");
        folder.write("image.png", "\0\0milk");
        let roots = vec![folder.path().to_path_buf()];
        let results = search_files(&roots, "milk", 10);
        let kinds: Vec<&HitKind> = results.hits.iter().map(|hit| &hit.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &HitKind::Text { line: 2, excerpt: "buy milk".into(), range: 11..15 },
                &HitKind::Text { line: 3, excerpt: "Milk is white".into(), range: 16..20 },
            ]
        );
        assert_eq!(results.hits[0].path, folder.path().join("notes/Plan.md"));
        assert_eq!(results.hits[0].root, folder.path());
        assert_eq!(results.file_count(), 1);
        assert_eq!(results.files_searched, 2);

        let by_name = search_files(&roots, "plan", 10);
        assert_eq!(by_name.hits[0].kind, HitKind::Name);
        assert_eq!(by_name.hits[1].kind, HitKind::Text { line: 1, excerpt: "# Plan".into(), range: 2..6 });

        let capped = search_files(&roots, "milk", 1);
        assert_eq!(capped.hits.len(), 1);
        assert!(capped.truncated);
        assert!(search_files(&roots, "  ", 10).hits.is_empty());
    }

    #[test]
    fn skips_the_text_of_encrypted_scratches() {
        let folder = TempFolder::new();
        // Real ones are rarely valid UTF-8; this one is, to show the check.
        folder.write("secret-milk.md", "RUBRIQUE-ENC1\nmilk");
        let results = search_files(&[folder.path().to_path_buf()], "milk", 10);
        assert_eq!(results.hits.len(), 1);
        assert_eq!(results.hits[0].kind, HitKind::Name);
    }
}
