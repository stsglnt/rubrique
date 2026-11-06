//! One picture of a terminal: its lines with colors, its size and where the
//! cursor is, read from what `tmux capture-pane` prints.

use super::ansi::{CellStyle, Span, parse_line};

/// What `display-message` prints on the first line before the capture:
/// cursor column and row, pane width and height, and whether the cursor shows.
pub const CAPTURE_FORMAT: &str = "#{cursor_x} #{cursor_y} #{pane_width} #{pane_height} #{cursor_flag}";

/// A terminal screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Screen {
    /// One entry per row, top to bottom.
    pub lines: Vec<Vec<Span>>,
    /// Column and row of the cursor; `None` when the program hides it.
    pub cursor: Option<(u16, u16)>,
    pub columns: u16,
    pub rows: u16,
}

impl Screen {
    /// The text without colors, one row per line.
    #[cfg(test)]
    pub fn plain_text(&self) -> String {
        let rows: Vec<String> =
            self.lines.iter().map(|line| line.iter().map(|span| span.text.as_str()).collect()).collect();
        rows.join("\n")
    }
}

/// Read the header line and the captured rows. `None` when the header is
/// not the five numbers asked for.
pub fn parse_capture(output: &str) -> Option<Screen> {
    let (header, body) = output.split_once('\n').unwrap_or((output, ""));
    let numbers: Vec<u16> = header.split_whitespace().filter_map(|word| word.parse().ok()).collect();
    let [cursor_x, cursor_y, columns, rows, cursor_shown] = numbers[..] else {
        return None;
    };
    let mut style = CellStyle::default();
    let lines = body.lines().take(usize::from(rows)).map(|line| parse_line(line, &mut style)).collect();
    Some(Screen { lines, cursor: (cursor_shown == 1).then_some((cursor_x, cursor_y)), columns, rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_size_cursor_and_rows() {
        let screen = parse_capture("2 1 80 3 1\n$ ls\n\u{1b}[34mdocs\u{1b}[39m\n\n").unwrap();
        assert_eq!((screen.columns, screen.rows), (80, 3));
        assert_eq!(screen.cursor, Some((2, 1)));
        assert_eq!(screen.plain_text(), "$ ls\ndocs\n");
    }

    #[test]
    fn a_hidden_cursor_and_bad_headers() {
        assert_eq!(parse_capture("0 0 10 1 0\nx").unwrap().cursor, None);
        assert_eq!(parse_capture("no session"), None);
    }
}
