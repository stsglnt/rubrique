//! Edit a pipe table in its Markdown source: find one cell's text, add and
//! remove rows and columns. The preview edits table cells in place with
//! these; the source given is one table block, as the parser found it.

use std::ops::Range;

use super::blocks::{cell_spans, delimiter_align};

/// One cell of a pipe table. Row 0 is the header row and row `n` is body
/// row `n - 1`: the delimiter line between them is not a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableCell {
    pub row: usize,
    pub column: usize,
}

/// A change to a table's shape, next to one cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableAction {
    /// A new empty row under the cell's row (under the header: a first body row).
    AddRow,
    /// Remove the cell's row. The header row stays.
    RemoveRow,
    /// A new empty column after the cell's column.
    AddColumn,
    /// Remove the cell's column. The last column stays.
    RemoveColumn,
}

/// A table's new source after a [`TableAction`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableEdit {
    pub source: String,
    /// The new cell to edit next, when the action made one.
    pub next: Option<TableCell>,
}

/// What the delimiter row shows for a new column.
const NEW_DELIMITER_CELL: &str = "---";

/// Byte range in `source` of `cell`'s text, without the spaces around it.
/// `None` when `source` is not a table or the row has no such cell (the
/// parser pads short rows with empty cells that have no text to point at).
pub fn cell_range(source: &str, cell: TableCell) -> Option<Range<usize>> {
    let lines = table_lines(source)?;
    let (offset, line) = *lines.get(line_of(cell.row))?;
    let span = cell_spans(line).into_iter().nth(cell.column)?;
    let raw = &line[span.clone()];
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        // An empty cell: new text goes after the first space, so `|  |`
        // becomes `| x |`.
        let at = offset + span.start + raw.len().min(1);
        return Some(at..at);
    }
    let start = span.start + (raw.len() - raw.trim_start().len());
    Some(offset + start..offset + start + trimmed.len())
}

/// Text typed for one cell, made safe inside a row: pipes escaped, line
/// breaks turned into spaces, spaces around it dropped.
pub fn escape_cell(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 2);
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            // Already escaped: keep as is.
            '\\' if chars.peek() == Some(&'|') => {
                escaped.push_str("\\|");
                chars.next();
            }
            '|' => escaped.push_str("\\|"),
            '\n' => escaped.push(' '),
            '\r' => {}
            other => escaped.push(other),
        }
    }
    escaped.trim().to_string()
}

/// Apply `action` next to `cell` in the table `source`. `None` when
/// `source` is not a table, the cell is outside it, or the action makes no
/// sense there (removing the header row or the only column).
pub fn apply(source: &str, cell: TableCell, action: TableAction) -> Option<TableEdit> {
    let line_break = if source.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = table_lines(source)?.into_iter().map(|(_, line)| line.to_string()).collect();
    let columns = cell_spans(&lines[1]).len();
    let line_index = line_of(cell.row);
    if line_index >= lines.len() || cell.column >= columns {
        return None;
    }
    let next = match action {
        TableAction::AddRow => {
            // Under the header means under its delimiter line.
            let at = if cell.row == 0 { 2 } else { line_index + 1 };
            lines.insert(at, join_row(&vec![String::new(); columns]));
            Some(TableCell { row: at - 1, column: cell.column })
        }
        TableAction::RemoveRow => {
            if cell.row == 0 {
                return None;
            }
            lines.remove(line_index);
            None
        }
        TableAction::AddColumn => {
            for (index, line) in lines.iter_mut().enumerate() {
                let filler = if index == 1 { NEW_DELIMITER_CELL } else { "" };
                let mut cells = row_cells(line, columns);
                cells.insert(cell.column + 1, filler.to_string());
                *line = join_row(&cells);
            }
            Some(TableCell { row: cell.row, column: cell.column + 1 })
        }
        TableAction::RemoveColumn => {
            if columns <= 1 {
                return None;
            }
            for line in &mut lines {
                let mut cells = row_cells(line, columns);
                cells.remove(cell.column);
                *line = join_row(&cells);
            }
            None
        }
    };
    Some(TableEdit { source: lines.join(line_break), next })
}

/// The lines of a table's source as (byte offset, text without the line
/// break), when the first two are a header row and a delimiter row.
fn table_lines(source: &str) -> Option<Vec<(usize, &str)>> {
    let mut lines = Vec::new();
    let mut offset = 0;
    for raw in source.split('\n') {
        lines.push((offset, raw.trim_end_matches('\r')));
        offset += raw.len() + 1;
    }
    let (_, header) = *lines.first()?;
    let (_, delimiter) = *lines.get(1)?;
    let is_delimiter_row = delimiter.contains('|')
        && cell_spans(delimiter).into_iter().all(|span| delimiter_align(delimiter[span].trim()).is_some());
    (header.contains('|') && is_delimiter_row).then_some(lines)
}

/// Index of a row's line: the header is line 0, body rows start at line 2.
fn line_of(row: usize) -> usize {
    if row == 0 { 0 } else { row + 1 }
}

/// A row's cells as written (pipes still escaped), trimmed, padded with
/// empty cells to `columns`; cells past that are dropped, as the parser
/// drops them.
fn row_cells(line: &str, columns: usize) -> Vec<String> {
    let mut cells: Vec<String> = cell_spans(line).into_iter().map(|span| line[span].trim().to_string()).collect();
    cells.resize_with(columns, String::new);
    cells
}

/// A row line in the usual style: `| a | b |`.
fn join_row(cells: &[String]) -> String {
    format!("| {} |", cells.join(" | "))
}

#[cfg(test)]
mod tests {
    use super::{TableAction, TableCell, TableEdit, apply, cell_range, escape_cell};

    const TABLE: &str = "| File | Size |\n| :-- | --: |\n| a.md | 1 |\nb.md | x \\| y";

    fn at(row: usize, column: usize) -> TableCell {
        TableCell { row, column }
    }

    #[test]
    fn cell_range_points_at_the_trimmed_cell_text() {
        assert_eq!(&TABLE[cell_range(TABLE, at(0, 1)).unwrap()], "Size");
        assert_eq!(&TABLE[cell_range(TABLE, at(1, 0)).unwrap()], "a.md");
        assert_eq!(&TABLE[cell_range(TABLE, at(2, 0)).unwrap()], "b.md");
        assert_eq!(&TABLE[cell_range(TABLE, at(2, 1)).unwrap()], "x \\| y");
    }

    #[test]
    fn cell_range_skips_the_delimiter_line_and_crlf() {
        let table = "| a | b |\r\n| - | - |\r\n| 1 | 2 |";
        assert_eq!(&table[cell_range(table, at(1, 1)).unwrap()], "2");
    }

    #[test]
    fn cell_range_of_an_empty_cell_is_inside_its_spaces() {
        let table = "| a | b |\n| - | - |\n|  | 2 |\n||";
        let range = cell_range(table, at(1, 0)).unwrap();
        assert!(range.is_empty());
        let mut edited = table.to_string();
        edited.replace_range(range, "x");
        assert!(edited.contains("| x | 2 |"));
        let range = cell_range(table, at(2, 0)).unwrap();
        assert_eq!(range.start, table.len() - 1);
    }

    #[test]
    fn cell_range_is_none_outside_the_table() {
        assert_eq!(cell_range(TABLE, at(3, 0)), None);
        // The parser pads short rows: there is no text for the padding.
        assert_eq!(cell_range("| a | b |\n| - | - |\n| 1 |", at(1, 1)), None);
        assert_eq!(cell_range("not | a table\nreally", at(0, 0)), None);
        assert_eq!(cell_range("Title\n---", at(0, 0)), None);
    }

    #[test]
    fn escape_cell_keeps_a_cell_on_one_line() {
        assert_eq!(escape_cell("  a | b \\| c\nd\r\n"), "a \\| b \\| c d");
        assert_eq!(escape_cell(""), "");
    }

    #[test]
    fn add_row_under_the_cell_or_under_the_header() {
        let edited = apply(TABLE, at(1, 1), TableAction::AddRow).unwrap();
        assert_eq!(
            edited,
            TableEdit {
                source: "| File | Size |\n| :-- | --: |\n| a.md | 1 |\n|  |  |\nb.md | x \\| y".to_string(),
                next: Some(at(2, 1)),
            }
        );
        let edited = apply(TABLE, at(0, 0), TableAction::AddRow).unwrap();
        assert!(edited.source.starts_with("| File | Size |\n| :-- | --: |\n|  |  |\n| a.md"));
        assert_eq!(edited.next, Some(at(1, 0)));
    }

    #[test]
    fn remove_row_keeps_the_header() {
        let edited = apply(TABLE, at(1, 0), TableAction::RemoveRow).unwrap();
        assert_eq!(edited.source, "| File | Size |\n| :-- | --: |\nb.md | x \\| y");
        assert_eq!(edited.next, None);
        assert_eq!(apply(TABLE, at(0, 0), TableAction::RemoveRow), None);
    }

    #[test]
    fn add_column_rewrites_every_line_and_keeps_escapes() {
        let edited = apply(TABLE, at(2, 0), TableAction::AddColumn).unwrap();
        assert_eq!(edited.source, "| File |  | Size |\n| :-- | --- | --: |\n| a.md |  | 1 |\n| b.md |  | x \\| y |");
        assert_eq!(edited.next, Some(at(2, 1)));
    }

    #[test]
    fn remove_column_pads_short_rows_and_keeps_the_last_column() {
        let table = "| a | b | c |\n| - | - | - |\n| 1 |";
        let edited = apply(table, at(0, 1), TableAction::RemoveColumn).unwrap();
        assert_eq!(edited.source, "| a | c |\n| - | - |\n| 1 |  |");
        assert_eq!(apply("| a |\n| - |", at(0, 0), TableAction::RemoveColumn), None);
    }

    #[test]
    fn apply_keeps_crlf_line_breaks() {
        let edited = apply("| a |\r\n| - |", at(0, 0), TableAction::AddRow).unwrap();
        assert_eq!(edited.source, "| a |\r\n| - |\r\n|  |");
    }

    #[test]
    fn apply_is_none_for_a_cell_outside_the_table() {
        assert_eq!(apply(TABLE, at(0, 2), TableAction::AddRow), None);
        assert_eq!(apply(TABLE, at(5, 0), TableAction::AddRow), None);
        assert_eq!(apply("plain text", at(0, 0), TableAction::AddRow), None);
    }
}
