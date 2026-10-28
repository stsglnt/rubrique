//! Line-based three-way merge: the text both sides started from (base), the
//! user's buffer (mine) and the file on disk (theirs).
//!
//! Lines keep their line breaks, so joining any choice of lines gives back
//! exact text.

/// Above this many base × side lines (after trimming equal ends), the middle
/// is treated as one changed block instead of being diffed line by line.
const MAX_DIFF_CELLS: usize = 4_000_000;

/// What happened to a stretch of lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkKind {
    /// Nobody changed it.
    Same,
    /// Only my buffer changed it; my lines win.
    Mine,
    /// Only the file on disk changed it; their lines win.
    Theirs,
    /// Both made the same change.
    Both,
    /// Both changed it differently: the user decides.
    Conflict,
}

/// One stretch of the merge, with each version's lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub kind: ChunkKind,
    pub base: Vec<String>,
    pub mine: Vec<String>,
    pub theirs: Vec<String>,
}

impl Chunk {
    /// The merged text of a chunk that needs no decision (`None` for a conflict).
    pub fn auto_text(&self) -> Option<String> {
        match self.kind {
            ChunkKind::Same => Some(self.base.concat()),
            ChunkKind::Mine | ChunkKind::Both => Some(self.mine.concat()),
            ChunkKind::Theirs => Some(self.theirs.concat()),
            ChunkKind::Conflict => None,
        }
    }
}

/// Split into lines that keep their `\n`.
pub fn lines(text: &str) -> Vec<String> {
    text.split_inclusive('\n').map(str::to_string).collect()
}

/// Merge `mine` and `theirs`, both edited from `base`.
pub fn merge(base: &str, mine: &str, theirs: &str) -> Vec<Chunk> {
    let (base, mine, theirs) = (lines(base), lines(mine), lines(theirs));
    let to_mine = matching(&base, &mine);
    let to_theirs = matching(&base, &theirs);

    let mut chunks: Vec<Chunk> = Vec::new();
    let (mut b, mut m, mut t) = (0, 0, 0);
    loop {
        // Next base line kept by both sides: an anchor everyone agrees on.
        let anchor = (b..base.len()).find_map(|index| Some((index, to_mine[index]?, to_theirs[index]?)));
        let (b_end, m_end, t_end) = anchor.unwrap_or((base.len(), mine.len(), theirs.len()));
        push_changed(&mut chunks, &base[b..b_end], &mine[m..m_end], &theirs[t..t_end]);
        let Some((b_anchor, m_anchor, t_anchor)) = anchor else {
            break;
        };
        push_same(&mut chunks, &base[b_anchor]);
        (b, m, t) = (b_anchor + 1, m_anchor + 1, t_anchor + 1);
    }
    chunks
}

/// The finished text when every conflict has a resolution (one per chunk,
/// `None` for chunks that need none or are still open).
pub fn result_text(chunks: &[Chunk], resolutions: &[Option<String>]) -> Option<String> {
    let mut text = String::new();
    for (index, chunk) in chunks.iter().enumerate() {
        match chunk.auto_text() {
            Some(auto) => text.push_str(&auto),
            None => text.push_str(resolutions.get(index)?.as_deref()?),
        }
    }
    Some(text)
}

/// An open merge: the chunks, the user's choice for each conflict, and the
/// disk text it was made from.
#[derive(Clone, Debug)]
pub struct MergeSession {
    pub chunks: Vec<Chunk>,
    /// Result text per chunk; only used (and only `Some` once decided) for conflicts.
    pub resolutions: Vec<Option<String>>,
    /// The file on disk when the merge started; the new starting point after Apply.
    pub disk_text: String,
    /// Show my version on the left and theirs on the right.
    pub swapped: bool,
}

impl MergeSession {
    pub fn new(base: &str, mine: &str, disk_text: String) -> Self {
        let chunks = merge(base, mine, &disk_text);
        let resolutions = vec![None; chunks.len()];
        MergeSession { chunks, resolutions, disk_text, swapped: false }
    }

    /// Conflicts that still need a decision.
    pub fn conflicts_left(&self) -> usize {
        self.chunks
            .iter()
            .zip(&self.resolutions)
            .filter(|(chunk, resolution)| chunk.kind == ChunkKind::Conflict && resolution.is_none())
            .count()
    }

    /// The merged text, once every conflict is decided.
    pub fn result(&self) -> Option<String> {
        result_text(&self.chunks, &self.resolutions)
    }

    /// Decide every open conflict for one side.
    pub fn take_all(&mut self, theirs: bool) {
        for (chunk, resolution) in self.chunks.iter().zip(self.resolutions.iter_mut()) {
            if chunk.kind == ChunkKind::Conflict && resolution.is_none() {
                let lines = if theirs { &chunk.theirs } else { &chunk.mine };
                *resolution = Some(lines.concat());
            }
        }
    }
}

/// Classify one stretch between anchors and add it, unless it is empty.
fn push_changed(chunks: &mut Vec<Chunk>, base: &[String], mine: &[String], theirs: &[String]) {
    if base.is_empty() && mine.is_empty() && theirs.is_empty() {
        return;
    }
    let kind = match (mine == base, theirs == base) {
        (true, true) => ChunkKind::Same,
        (true, false) => ChunkKind::Theirs,
        (false, true) => ChunkKind::Mine,
        (false, false) if mine == theirs => ChunkKind::Both,
        (false, false) => ChunkKind::Conflict,
    };
    if kind == ChunkKind::Same {
        for line in base {
            push_same(chunks, line);
        }
        return;
    }
    chunks.push(Chunk { kind, base: base.to_vec(), mine: mine.to_vec(), theirs: theirs.to_vec() });
}

/// Add an unchanged line, joining it to a previous unchanged chunk.
fn push_same(chunks: &mut Vec<Chunk>, line: &str) {
    if let Some(last) = chunks.last_mut().filter(|chunk| chunk.kind == ChunkKind::Same) {
        for side in [&mut last.base, &mut last.mine, &mut last.theirs] {
            side.push(line.to_string());
        }
        return;
    }
    let lines = vec![line.to_string()];
    chunks.push(Chunk { kind: ChunkKind::Same, base: lines.clone(), mine: lines.clone(), theirs: lines });
}

/// For each line of `a`, the index of the same line in `b` on a longest
/// common subsequence (`None` when it was removed or changed).
fn matching(a: &[String], b: &[String]) -> Vec<Option<usize>> {
    let mut result = vec![None; a.len()];
    // Equal lines at both ends need no table.
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..].iter().rev().zip(b[prefix..].iter().rev()).take_while(|(x, y)| x == y).count();
    for (index, slot) in result.iter_mut().enumerate().take(prefix) {
        *slot = Some(index);
    }
    for offset in 1..=suffix {
        result[a.len() - offset] = Some(b.len() - offset);
    }
    let a_mid = &a[prefix..a.len() - suffix];
    let b_mid = &b[prefix..b.len() - suffix];
    if a_mid.is_empty() || b_mid.is_empty() || a_mid.len() * b_mid.len() > MAX_DIFF_CELLS {
        return result;
    }

    // Classic LCS table: lengths[i][j] = LCS of a_mid[i..] and b_mid[j..].
    let width = b_mid.len() + 1;
    let mut lengths = vec![0_u32; (a_mid.len() + 1) * width];
    for i in (0..a_mid.len()).rev() {
        for j in (0..b_mid.len()).rev() {
            lengths[i * width + j] = if a_mid[i] == b_mid[j] {
                lengths[(i + 1) * width + j + 1] + 1
            } else {
                lengths[(i + 1) * width + j].max(lengths[i * width + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < a_mid.len() && j < b_mid.len() {
        if a_mid[i] == b_mid[j] {
            result[prefix + i] = Some(prefix + j);
            i += 1;
            j += 1;
        } else if lengths[(i + 1) * width + j] >= lengths[i * width + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(chunks: &[Chunk]) -> Vec<ChunkKind> {
        chunks.iter().map(|chunk| chunk.kind).collect()
    }

    #[test]
    fn nothing_changed_is_one_same_chunk() {
        let chunks = merge("a\nb\n", "a\nb\n", "a\nb\n");
        assert_eq!(kinds(&chunks), [ChunkKind::Same]);
        assert_eq!(result_text(&chunks, &[]), Some("a\nb\n".to_string()));
    }

    #[test]
    fn changes_on_different_lines_merge_by_themselves() {
        let base = "one\ntwo\nthree\nfour\n";
        let mine = "ONE\ntwo\nthree\nfour\n";
        let theirs = "one\ntwo\nthree\nFOUR\n";
        let chunks = merge(base, mine, theirs);
        assert_eq!(kinds(&chunks), [ChunkKind::Mine, ChunkKind::Same, ChunkKind::Theirs]);
        assert_eq!(result_text(&chunks, &[]), Some("ONE\ntwo\nthree\nFOUR\n".to_string()));
    }

    #[test]
    fn same_line_changed_twice_is_a_conflict() {
        let chunks = merge("a\nb\nc\n", "a\nmine\nc\n", "a\ntheirs\nc\n");
        assert_eq!(kinds(&chunks), [ChunkKind::Same, ChunkKind::Conflict, ChunkKind::Same]);
        let conflict = &chunks[1];
        assert_eq!(
            (conflict.base.concat(), conflict.mine.concat(), conflict.theirs.concat()),
            ("b\n".into(), "mine\n".into(), "theirs\n".into())
        );
        assert_eq!(result_text(&chunks, &[None, None, None]), None);
        let resolved = [None, Some("both\n".to_string()), None];
        assert_eq!(result_text(&chunks, &resolved), Some("a\nboth\nc\n".to_string()));
    }

    #[test]
    fn identical_changes_are_not_a_conflict() {
        let chunks = merge("a\nb\n", "a\nB\n", "a\nB\n");
        assert_eq!(kinds(&chunks), [ChunkKind::Same, ChunkKind::Both]);
        assert_eq!(result_text(&chunks, &[]), Some("a\nB\n".to_string()));
    }

    #[test]
    fn added_and_deleted_lines() {
        let base = "a\nb\nc\n";
        let mine = "a\nb\nc\nmine added\n";
        let theirs = "a\nc\n";
        let chunks = merge(base, mine, theirs);
        assert_eq!(result_text(&chunks, &[]), Some("a\nc\nmine added\n".to_string()));
    }

    #[test]
    fn last_line_without_newline_is_kept_exactly() {
        let chunks = merge("a\nb", "a\nb", "a\nb!");
        assert_eq!(result_text(&chunks, &[]), Some("a\nb!".to_string()));
    }

    #[test]
    fn empty_texts() {
        assert_eq!(merge("", "", ""), Vec::<Chunk>::new());
        let chunks = merge("", "mine\n", "theirs\n");
        assert_eq!(kinds(&chunks), [ChunkKind::Conflict]);
    }

    #[test]
    fn matching_follows_a_longest_common_subsequence() {
        let a = lines("x\na\nb\nc\ny\n");
        let b = lines("a\nz\nb\nc\n");
        assert_eq!(matching(&a, &b), [None, Some(0), Some(2), Some(3), None]);
    }

    #[test]
    fn session_counts_and_resolves_conflicts() {
        let mut session = MergeSession::new("a\nb\n", "a\nmine\n", "a\ntheirs\n".to_string());
        assert_eq!(session.conflicts_left(), 1);
        assert_eq!(session.result(), None);
        session.take_all(true);
        assert_eq!(session.conflicts_left(), 0);
        assert_eq!(session.result(), Some("a\ntheirs\n".to_string()));
    }
}
