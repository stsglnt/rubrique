//! Prettifiers for JSON, SQL (pgFormatter style) and XML. Own code, pure.

pub mod json;
pub mod sql;
pub mod xml;

use std::fmt;
use std::path::Path;

/// A language the Format command knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Json,
    Sql,
    Xml,
}

impl Language {
    /// The language of a file, from its extension.
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "json" | "jsonc" | "geojson" => Some(Language::Json),
            "sql" | "psql" | "pgsql" => Some(Language::Sql),
            "xml" | "xsd" | "xsl" | "xslt" | "svg" | "plist" => Some(Language::Xml),
            _ => None,
        }
    }

    /// Guess the language of a snippet: `{` or `[` is JSON, `<` is XML, the rest SQL.
    pub fn guess(text: &str) -> Self {
        match text.trim_start().chars().next() {
            Some('{' | '[') => Language::Json,
            Some('<') => Language::Xml,
            _ => Language::Sql,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Language::Json => "JSON",
            Language::Sql => "SQL",
            Language::Xml => "XML",
        }
    }
}

/// Format `text` as `language`.
///
/// # Errors
///
/// The first syntax error, with its line and column.
pub fn format(language: Language, text: &str) -> Result<String, FormatError> {
    match language {
        Language::Json => json::format_json(text),
        Language::Sql => Ok(sql::format_sql(text)),
        Language::Xml => xml::format_xml(text),
    }
}

/// Why text could not be formatted, and where (1-based line and column).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl FormatError {
    /// An error at byte `offset` of `text`.
    pub fn at(text: &str, offset: usize, message: &str) -> Self {
        let before = &text[..floor_char_boundary(text, offset)];
        let line = before.matches('\n').count() + 1;
        let column = before.rsplit('\n').next().map_or(0, |last| last.chars().count()) + 1;
        FormatError { message: message.to_string(), line, column }
    }
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}, column {}: {}", self.line, self.column, self.message)
    }
}

/// The largest char boundary at or before `index` (clamped to the text).
fn floor_char_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_from_paths_and_snippets() {
        assert_eq!(Language::from_path(Path::new("a/data.JSON")), Some(Language::Json));
        assert_eq!(Language::from_path(Path::new("q.sql")), Some(Language::Sql));
        assert_eq!(Language::from_path(Path::new("pom.xml")), Some(Language::Xml));
        assert_eq!(Language::from_path(Path::new("notes.md")), None);
        assert_eq!(Language::guess("  [1]"), Language::Json);
        assert_eq!(Language::guess("<a/>"), Language::Xml);
        assert_eq!(Language::guess("select 1"), Language::Sql);
    }

    #[test]
    fn error_positions_count_characters() {
        let error = FormatError::at("ab\nçd", 6, "x");
        assert_eq!((error.line, error.column), (2, 3));
        assert_eq!(error.to_string(), "line 2, column 3: x");
    }
}
