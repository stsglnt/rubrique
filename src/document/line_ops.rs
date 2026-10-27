//! Line editing on a text buffer: move lines up/down, duplicate lines.
//!
//! Positions are character indices (not bytes), matching egui's cursors.
//! Lines are split on `\n` only, so a `\r` before it stays part of the line
//! and CRLF files keep their line endings.

/// Direction for [`move_lines`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

/// A selection as a character range; `start == end` is a plain cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    /// Character index where the selection begins.
    pub start: usize,
    /// Character index just past the selection.
    pub end: usize,
}

/// The text split into lines, plus which lines the selection touches.
struct LineBlock<'a> {
    lines: Vec<&'a str>,
    /// Whether the text ends with `\n`. The empty string after that final
    /// newline is not a real line and never moves.
    ends_with_newline: bool,
    /// Index of the first line the selection touches.
    first: usize,
    /// Index of the last line the selection touches.
    last: usize,
}

impl<'a> LineBlock<'a> {
    /// Split `text` into lines and find the ones `selection` covers.
    fn new(text: &'a str, selection: Selection) -> Self {
        let mut lines: Vec<&str> = text.split('\n').collect();
        let ends_with_newline = text.ends_with('\n') && lines.len() > 1;
        if ends_with_newline {
            lines.pop();
        }
        let first = line_of(text, selection.start);
        let mut last = line_of(text, selection.end);
        // A selection ending at the very start of a line does not include it.
        if selection.end > selection.start && is_line_start(text, selection.end) {
            last -= 1;
        }
        let max_line = lines.len().saturating_sub(1);
        LineBlock { lines, ends_with_newline, first: first.min(max_line), last: last.min(max_line) }
    }

    /// Put the lines back together, restoring the final newline.
    fn join(&self) -> String {
        let mut text = self.lines.join("\n");
        if self.ends_with_newline {
            text.push('\n');
        }
        text
    }

    /// Characters in a line plus its `\n`.
    fn line_width(&self, index: usize) -> usize {
        self.lines[index].chars().count() + 1
    }
}

/// Move the lines covered by `selection` one line up or down.
///
/// Returns `None` when nothing can move (already at the top or bottom).
pub fn move_lines(text: &mut String, selection: Selection, direction: Direction) -> Option<Selection> {
    let mut block = LineBlock::new(text, selection);
    let (first, last) = (block.first, block.last);
    // The selection moves by the width of the line it jumps over.
    let new_selection = match direction {
        Direction::Up if first > 0 => {
            let width = block.line_width(first - 1);
            block.lines[first - 1..=last].rotate_left(1);
            Selection { start: selection.start.saturating_sub(width), end: selection.end.saturating_sub(width) }
        }
        Direction::Down if last + 1 < block.lines.len() => {
            let width = block.line_width(last + 1);
            block.lines[first..=last + 1].rotate_right(1);
            Selection { start: selection.start + width, end: selection.end + width }
        }
        Direction::Up | Direction::Down => return None,
    };
    let new_text = block.join();
    *text = new_text;
    Some(new_selection)
}

/// Copy the lines covered by `selection` right below them and select the copy.
pub fn duplicate_lines(text: &mut String, selection: Selection) -> Selection {
    let mut block = LineBlock::new(text, selection);
    let (first, last) = (block.first, block.last);
    let shift: usize = (first..=last).map(|index| block.line_width(index)).sum();
    let copies: Vec<&str> = block.lines[first..=last].to_vec();
    let insert_at = last + 1;
    block.lines.splice(insert_at..insert_at, copies);
    let new_text = block.join();
    *text = new_text;
    Selection { start: selection.start + shift, end: selection.end + shift }
}

/// Replace the selection with `inserted` and put the cursor after it.
pub fn insert_text(text: &mut String, selection: Selection, inserted: &str) -> Selection {
    let start = byte_index(text, selection.start);
    let end = byte_index(text, selection.end);
    text.replace_range(start..end, inserted);
    let cursor = selection.start + inserted.chars().count();
    Selection { start: cursor, end: cursor }
}

/// Byte offset of character `index`, clamped to the end of the text.
fn byte_index(text: &str, index: usize) -> usize {
    text.char_indices().nth(index).map_or(text.len(), |(byte, _)| byte)
}

/// Zero-based line number containing character `index`.
fn line_of(text: &str, index: usize) -> usize {
    text.chars().take(index).filter(|&character| character == '\n').count()
}

/// True when character `index` is the first character of a line.
fn is_line_start(text: &str, index: usize) -> bool {
    index == 0 || text.chars().nth(index - 1) == Some('\n')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor(at: usize) -> Selection {
        Selection { start: at, end: at }
    }

    #[test]
    fn move_line_down_and_cursor_follows() {
        let mut text = "one\ntwo\nthree".to_string();
        let moved = move_lines(&mut text, cursor(1), Direction::Down).unwrap();
        assert_eq!(text, "two\none\nthree");
        assert_eq!(moved, cursor(5));
    }

    #[test]
    fn move_line_up() {
        let mut text = "one\ntwo\nthree".to_string();
        let moved = move_lines(&mut text, cursor(9), Direction::Up).unwrap();
        assert_eq!(text, "one\nthree\ntwo");
        assert_eq!(moved, cursor(5));
    }

    #[test]
    fn move_at_edges_does_nothing() {
        let mut text = "one\ntwo\n".to_string();
        assert_eq!(move_lines(&mut text, cursor(0), Direction::Up), None);
        // The empty string after the final newline is not a line to swap with.
        assert_eq!(move_lines(&mut text, cursor(5), Direction::Down), None);
        assert_eq!(text, "one\ntwo\n");
    }

    #[test]
    fn move_keeps_final_newline_and_crlf() {
        let mut text = "a\r\nb\r\n".to_string();
        move_lines(&mut text, cursor(0), Direction::Down).unwrap();
        assert_eq!(text, "b\r\na\r\n");
    }

    #[test]
    fn move_multi_line_selection() {
        let mut text = "a\nb\nc\nd".to_string();
        // Select "b\nc" (ends at the start of "d", which is not included).
        let moved = move_lines(&mut text, Selection { start: 2, end: 6 }, Direction::Up).unwrap();
        assert_eq!(text, "b\nc\na\nd");
        assert_eq!(moved, Selection { start: 0, end: 4 });
    }

    #[test]
    fn move_multi_line_selection_down() {
        let mut text = "a\nb\nc\nd".to_string();
        let moved = move_lines(&mut text, Selection { start: 0, end: 3 }, Direction::Down).unwrap();
        assert_eq!(text, "c\na\nb\nd");
        assert_eq!(moved, Selection { start: 2, end: 5 });
    }

    #[test]
    fn move_with_unicode() {
        let mut text = "привіт\nok".to_string();
        let moved = move_lines(&mut text, cursor(8), Direction::Up).unwrap();
        assert_eq!(text, "ok\nпривіт");
        assert_eq!(moved, cursor(1));
    }

    #[test]
    fn duplicate_line_selects_copy() {
        let mut text = "one\ntwo".to_string();
        let selection = duplicate_lines(&mut text, cursor(5));
        assert_eq!(text, "one\ntwo\ntwo");
        assert_eq!(selection, cursor(9));
    }

    #[test]
    fn insert_replaces_selection() {
        let mut text = "dé: X!".to_string();
        let cursor_after = insert_text(&mut text, Selection { start: 4, end: 5 }, "2026-10-27");
        assert_eq!(text, "dé: 2026-10-27!");
        assert_eq!(cursor_after, cursor(14));
    }

    #[test]
    fn duplicate_in_empty_text() {
        let mut text = String::new();
        duplicate_lines(&mut text, cursor(0));
        assert_eq!(text, "\n");
    }
}
