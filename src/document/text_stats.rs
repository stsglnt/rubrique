//! Numbers for the status bar: cursor line/column, word and character counts.

/// What the status bar shows about the text and selection.
#[derive(Debug, PartialEq, Eq)]
pub struct TextStats {
    /// 1-based line of the cursor.
    pub line: usize,
    /// 1-based column (in characters) of the cursor.
    pub column: usize,
    /// Counts for the selection, or the whole text when nothing is selected.
    pub words: usize,
    /// Characters (not bytes) counted, like `words`.
    pub characters: usize,
    /// The counts are for a selection rather than the whole text.
    pub has_selection: bool,
}

impl TextStats {
    /// Status bar text, e.g. `Ln 2, Col 7   4 words, 15 chars total`.
    pub fn summary(&self) -> String {
        let scope = if self.has_selection { "selected" } else { "total" };
        format!("Ln {}, Col {}   {} words, {} chars {scope}", self.line, self.column, self.words, self.characters)
    }
}

/// Compute stats. `cursor` is the moving end of the selection and
/// `selection` the sorted character range (empty when nothing is selected).
pub fn text_stats(text: &str, cursor: usize, selection: std::ops::Range<usize>) -> TextStats {
    let before_cursor: String = text.chars().take(cursor).collect();
    let line = before_cursor.matches('\n').count() + 1;
    let column = before_cursor.chars().rev().take_while(|&c| c != '\n').count() + 1;
    let has_selection = !selection.is_empty();
    let counted: String = if has_selection {
        text.chars().skip(selection.start).take(selection.len()).collect()
    } else {
        text.to_string()
    };
    TextStats {
        line,
        column,
        words: counted.split_whitespace().count(),
        characters: counted.chars().count(),
        has_selection,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_text_counts_and_position() {
        let stats = text_stats("one two\nthree ü", 14, 14..14);
        assert_eq!(stats, TextStats { line: 2, column: 7, words: 4, characters: 15, has_selection: false });
    }

    #[test]
    fn selection_counts() {
        let stats = text_stats("one two three", 7, 4..13);
        assert_eq!((stats.words, stats.characters, stats.has_selection), (2, 9, true));
    }

    #[test]
    fn summary_names_the_counted_scope() {
        assert_eq!(text_stats("one two\nthree ü", 14, 14..14).summary(), "Ln 2, Col 7   4 words, 15 chars total");
        assert_eq!(text_stats("one two three", 7, 4..13).summary(), "Ln 1, Col 8   2 words, 9 chars selected");
    }

    #[test]
    fn empty_text() {
        let stats = text_stats("", 0, 0..0);
        assert_eq!((stats.line, stats.column, stats.words, stats.characters), (1, 1, 0, 0));
    }
}
