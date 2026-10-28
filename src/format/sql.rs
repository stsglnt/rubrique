//! SQL pretty-printer in the style of pgFormatter, the usual `PostgreSQL`
//! formatter: keywords upper case, data types lower case, identifiers and
//! function names as written; 4-space indent; every clause on its own line
//! with one item per line and trailing commas; `AND` / `OR` leading their
//! lines; subqueries as `FROM (` … `) AS alias`; a blank line between
//! statements. Strings, quoted identifiers, `$$` bodies and comments are kept
//! exactly. It never fails: unknown syntax is laid out word by word.

const INDENT: usize = 4;

/// Words written in upper case.
const KEYWORDS: &[&str] = &[
    "ADD",
    "ALL",
    "ALTER",
    "AND",
    "ANY",
    "AS",
    "ASC",
    "BEGIN",
    "BETWEEN",
    "BY",
    "CASCADE",
    "CASE",
    "CHECK",
    "COLUMN",
    "COMMIT",
    "CONFLICT",
    "CONSTRAINT",
    "CREATE",
    "CROSS",
    "CURRENT_DATE",
    "CURRENT_TIMESTAMP",
    "DEFAULT",
    "DELETE",
    "DESC",
    "DISTINCT",
    "DO",
    "DROP",
    "ELSE",
    "END",
    "EXCEPT",
    "EXISTS",
    "EXTENSION",
    "FALSE",
    "FETCH",
    "FILTER",
    "FIRST",
    "FOR",
    "FOREIGN",
    "FROM",
    "FULL",
    "FUNCTION",
    "GRANT",
    "GROUP",
    "HAVING",
    "IF",
    "ILIKE",
    "IN",
    "INDEX",
    "INNER",
    "INSERT",
    "INTERSECT",
    "INTO",
    "IS",
    "JOIN",
    "KEY",
    "LAST",
    "LATERAL",
    "LEFT",
    "LIKE",
    "LIMIT",
    "MATERIALIZED",
    "NATURAL",
    "NOT",
    "NOTHING",
    "NULL",
    "NULLS",
    "OFFSET",
    "ON",
    "ONLY",
    "OR",
    "ORDER",
    "OUTER",
    "OVER",
    "PARTITION",
    "PRIMARY",
    "REFERENCES",
    "REPLACE",
    "RETURNING",
    "RETURNS",
    "REVOKE",
    "RIGHT",
    "ROLLBACK",
    "ROWS",
    "SCHEMA",
    "SELECT",
    "SEQUENCE",
    "SET",
    "SIMILAR",
    "TABLE",
    "THEN",
    "TO",
    "TRIGGER",
    "TRUE",
    "TRUNCATE",
    "UNION",
    "UNIQUE",
    "UPDATE",
    "USING",
    "VALUES",
    "VIEW",
    "WHEN",
    "WHERE",
    "WINDOW",
    "WITH",
    "WITHIN",
];

/// Data types, written in lower case.
const TYPES: &[&str] = &[
    "bigint",
    "bigserial",
    "bool",
    "boolean",
    "bytea",
    "char",
    "character",
    "date",
    "decimal",
    "double",
    "float",
    "float4",
    "float8",
    "inet",
    "int",
    "int2",
    "int4",
    "int8",
    "integer",
    "interval",
    "json",
    "jsonb",
    "numeric",
    "precision",
    "real",
    "serial",
    "smallint",
    "text",
    "time",
    "timestamp",
    "timestamptz",
    "uuid",
    "varchar",
    "varying",
    "zone",
];

/// Keywords that may be followed by `(` and still keep a space before it.
const SPACED_BEFORE_PAREN: &[&str] = &[
    "AS",
    "IN",
    "VALUES",
    "EXISTS",
    "ANY",
    "ALL",
    "ON",
    "USING",
    "OVER",
    "FILTER",
    "WITHIN",
    "AND",
    "OR",
    "NOT",
    "WHEN",
    "THEN",
    "ELSE",
    "FROM",
    "JOIN",
    "INTO",
    "TABLE",
    "REFERENCES",
    "KEY",
    "CHECK",
    "UNIQUE",
    "WHERE",
    "SELECT",
    "BY",
    "RETURNING",
    "SET",
    "DO",
    "CONFLICT",
    "LATERAL",
    "IF",
];

/// Clauses whose items go one per line under them.
const LIST_CLAUSES: &[&str] =
    &["SELECT", "FROM", "WHERE", "GROUP BY", "ORDER BY", "HAVING", "SET", "RETURNING", "VALUES", "WINDOW"];
/// Clauses that start a line but keep the rest on it.
const INLINE_CLAUSES: &[&str] = &[
    "LIMIT",
    "OFFSET",
    "FETCH",
    "UNION",
    "UNION ALL",
    "INTERSECT",
    "EXCEPT",
    "INSERT INTO",
    "UPDATE",
    "DELETE FROM",
    "WITH",
    "ON CONFLICT",
];
/// First words of a join.
const JOIN_STARTS: &[&str] = &["JOIN", "LEFT", "RIGHT", "FULL", "INNER", "CROSS", "NATURAL"];

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token<'a> {
    Word(&'a str),
    /// Strings, quoted identifiers, numbers, `$1`: copied as written.
    Literal(&'a str),
    /// `-- ...`; `trailing` when it shares a line with the code before it.
    LineComment {
        text: &'a str,
        trailing: bool,
    },
    BlockComment(&'a str),
    /// Operators, `.`, `::`, `,`, `;`, brackets.
    Symbol(&'a str),
}

/// Pretty-print SQL.
pub fn format_sql(text: &str) -> String {
    let tokens = tokenize(text);
    let mut formatter = Formatter::default();
    for (index, token) in tokens.iter().enumerate() {
        formatter.token(&tokens, index, token);
    }
    formatter.finish()
}

/// What a parenthesis holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParenKind {
    /// A whole query: its own clause lines, indented.
    Subquery,
    /// A condition group inside WHERE / ON: AND / OR break inside it.
    Condition,
    /// `CREATE TABLE t (` column list: one column per line.
    Columns,
    /// Function arguments, IN lists, tuples: kept on one line.
    Inline,
}

#[derive(Clone, Debug)]
struct Frame {
    kind: ParenKind,
    /// Indent of the line the paren was opened on.
    line_indent: usize,
    /// Clause state outside the paren, restored when it closes.
    clause: Option<&'static str>,
    base: usize,
}

#[derive(Default)]
struct Formatter {
    out: String,
    /// Indent of the line being written.
    line_indent: usize,
    /// Indent of clause keywords in the current query.
    base: usize,
    /// The clause we are in, at the current query level.
    clause: Option<&'static str>,
    frames: Vec<Frame>,
    /// Indents of open CASE expressions.
    cases: Vec<usize>,
    /// Start the next token on a new line at this indent.
    pending_break: Option<usize>,
    /// The last emitted token, for spacing.
    previous: Option<String>,
    /// An `AND` that belongs to `BETWEEN` comes next.
    between: bool,
    /// Inside `ON` of a join (its AND / OR indent one more level).
    in_join_on: bool,
    /// Saw `CREATE ... TABLE` and wait for its column list.
    create_table: bool,
    /// Where the last `--` comment starts in `out`, while it ends the line.
    comment_at: Option<usize>,
}

impl Formatter {
    fn token(&mut self, tokens: &[Token], index: usize, token: &Token) {
        match token {
            Token::Word(word) => self.word(tokens, index, word),
            Token::Literal(text) => self.emit(text),
            Token::LineComment { text, trailing } => self.line_comment(text, *trailing),
            Token::BlockComment(comment) => {
                if comment.contains('\n') {
                    let indent = self.pending_break.take().unwrap_or(self.line_indent);
                    self.break_line(indent);
                    self.out.push_str(&reindent_block_comment(comment, indent));
                    self.previous = Some("*/".to_string());
                    self.pending_break = Some(indent);
                } else {
                    self.emit(comment);
                }
            }
            Token::Symbol(symbol) => self.symbol(tokens, index, symbol),
        }
    }

    /// A `--` comment stays at the end of its line when it was there in the
    /// source (even after a comma that wants a line break), else gets its own line.
    fn line_comment(&mut self, text: &str, trailing: bool) {
        let pending = if trailing { self.pending_break.take() } else { None };
        if !trailing && !self.out.is_empty() && self.previous.is_some() {
            let indent = self.pending_break.take().unwrap_or(self.line_indent);
            self.break_line(indent);
        }
        if self.previous.is_some() {
            self.out.push(' ');
        }
        self.comment_at = Some(self.out.len());
        self.out.push_str(text);
        self.previous = Some("--".to_string());
        self.pending_break = Some(pending.unwrap_or(self.line_indent));
    }

    fn at_query_level(&self) -> bool {
        self.frames.last().is_none_or(|frame| frame.kind == ParenKind::Subquery)
    }

    fn item_indent(&self) -> usize {
        self.base + INDENT
    }

    fn word(&mut self, tokens: &[Token], index: usize, word: &str) {
        let upper = word.to_ascii_uppercase();
        let next_word = next_significant(tokens, index + 1).and_then(|token| match token {
            Token::Word(next) => Some(next.to_ascii_uppercase()),
            _ => None,
        });
        let followed_by_paren = matches!(tokens.get(index + 1), Some(Token::Symbol("(")));
        let is_function = followed_by_paren && !SPACED_BEFORE_PAREN.contains(&upper.as_str());
        let shown = if is_function {
            word.to_string()
        } else if KEYWORDS.contains(&upper.as_str()) {
            upper.clone()
        } else if TYPES.contains(&word.to_ascii_lowercase().as_str()) {
            word.to_ascii_lowercase()
        } else {
            word.to_string()
        };

        if !is_function && self.at_query_level() && self.clause_word(&upper, next_word.as_deref(), &shown) {
            return;
        }
        match upper.as_str() {
            "CASE" => {
                self.emit(&shown);
                self.cases.push(self.line_indent);
                return;
            }
            "WHEN" | "ELSE" if !self.cases.is_empty() => {
                let indent = self.cases.last().copied().unwrap_or(0) + INDENT;
                self.break_line(indent);
                self.emit(&shown);
                return;
            }
            "END" if !self.cases.is_empty() => {
                let indent = self.cases.pop().unwrap_or(0);
                self.break_line(indent);
                self.emit(&shown);
                return;
            }
            "BETWEEN" => self.between = true,
            "AND" if self.between => {
                self.between = false;
                self.emit(&shown);
                return;
            }
            "AND" | "OR" => {
                if let Some(indent) = self.condition_break() {
                    self.break_line(indent);
                }
            }
            "TABLE"
                if self.previous_is("CREATE")
                    || self.previous_is("TEMP")
                    || self.previous_is("TEMPORARY")
                    || self.previous_is("UNLOGGED") =>
            {
                self.create_table = true;
            }
            _ => {}
        }
        self.emit(&shown);
    }

    /// Clause keywords and joins at query level. Returns true when handled.
    fn clause_word(&mut self, upper: &str, next: Option<&str>, shown: &str) -> bool {
        // Two-word clauses start on their first word.
        let pair = next.map(|next| format!("{upper} {next}"));
        let pair_clause = pair.as_deref().filter(|pair| LIST_CLAUSES.contains(pair) || INLINE_CLAUSES.contains(pair));
        if let Some(pair) = pair_clause
            && !(upper == "UNION" && pair != "UNION ALL")
        {
            self.start_clause(pair_static(pair), shown);
            return true;
        }
        // The second word of a pair that was started already.
        if matches!(
            (self.previous.as_deref(), upper),
            (Some("GROUP" | "ORDER"), "BY")
                | (Some("INSERT"), "INTO")
                | (Some("DELETE"), "FROM")
                | (Some("UNION"), "ALL")
                | (Some("ON"), "CONFLICT")
        ) {
            self.emit(shown);
            if let Some(clause) = self.clause
                && LIST_CLAUSES.contains(&clause)
            {
                self.pending_break = Some(self.item_indent());
            }
            return true;
        }
        if upper == "DISTINCT" && self.clause == Some("SELECT") && self.pending_break.is_some() {
            let pending = self.pending_break.take();
            self.emit(shown);
            self.pending_break = pending;
            return true;
        }
        if let Some(clause) = LIST_CLAUSES.iter().chain(INLINE_CLAUSES).find(|clause| **clause == upper) {
            // UPDATE inside "ON CONFLICT DO UPDATE" and SET after it stay inline.
            if upper == "UPDATE" && self.previous_is("DO") {
                return false;
            }
            self.start_clause(clause, shown);
            return true;
        }
        if self.clause == Some("FROM")
            && JOIN_STARTS.contains(&upper)
            && (upper == "JOIN" || next.is_some_and(|next| matches!(next, "JOIN" | "OUTER" | "INNER")))
        {
            if !matches!(
                self.previous.as_deref(),
                Some("LEFT" | "RIGHT" | "FULL" | "INNER" | "CROSS" | "NATURAL" | "OUTER")
            ) {
                self.in_join_on = false;
                let indent = self.item_indent();
                self.break_line(indent);
            }
            self.emit(shown);
            return true;
        }
        if upper == "ON" && self.clause == Some("FROM") {
            self.in_join_on = true;
        }
        false
    }

    fn start_clause(&mut self, clause: &'static str, first_word: &str) {
        self.clause = Some(clause);
        self.in_join_on = false;
        self.between = false;
        let base = self.base;
        if !self.out.is_empty() {
            self.break_line(base);
        }
        self.emit(first_word);
        // Two-word clauses get their item break after the second word.
        if LIST_CLAUSES.contains(&clause) && !clause.contains(' ') {
            self.pending_break = Some(self.item_indent());
        }
    }

    /// Where an AND / OR should start a line, if at all.
    fn condition_break(&self) -> Option<usize> {
        match self.frames.last() {
            Some(frame) if frame.kind == ParenKind::Condition => Some(frame.line_indent + INDENT),
            Some(frame) if frame.kind != ParenKind::Subquery => None,
            _ => match self.clause {
                Some("WHERE" | "HAVING") => Some(self.item_indent()),
                Some("FROM") if self.in_join_on => Some(self.item_indent() + INDENT),
                _ => None,
            },
        }
    }

    fn symbol(&mut self, tokens: &[Token], index: usize, symbol: &str) {
        match symbol {
            "(" => self.open_paren(tokens, index),
            ")" => self.close_paren(),
            "," => {
                // `a -- note` then `,`: the comma goes before the comment, like pgFormatter.
                if self.previous.as_deref() == Some("--")
                    && let Some(at) = self.comment_at.take()
                {
                    let at = self.out[..at].trim_end().len();
                    self.out.insert(at, ',');
                    self.pending_break = None;
                    self.previous = Some(",".to_string());
                } else {
                    self.emit(",");
                }
                if let Some(frame) = self.frames.last()
                    && frame.kind == ParenKind::Columns
                {
                    self.pending_break = Some(frame.line_indent + INDENT);
                } else if self.at_query_level() {
                    match self.clause {
                        Some("WITH") => self.pending_break = Some(self.base),
                        Some(clause) if LIST_CLAUSES.contains(&clause) => self.pending_break = Some(self.item_indent()),
                        _ => {}
                    }
                }
            }
            ";" => {
                self.pending_break = None;
                self.emit(";");
                self.out.push_str("\n\n");
                *self = Formatter { out: std::mem::take(&mut self.out), ..Formatter::default() };
            }
            _ => self.emit(symbol),
        }
    }

    fn open_paren(&mut self, tokens: &[Token], index: usize) {
        let next = next_significant(tokens, index + 1);
        let starts_query = matches!(next, Some(Token::Word(word)) if matches!(word.to_ascii_uppercase().as_str(), "SELECT" | "WITH" | "VALUES"));
        let kind = if starts_query {
            ParenKind::Subquery
        } else if self.create_table && self.frames.is_empty() {
            self.create_table = false;
            ParenKind::Columns
        } else if self.after_condition_start() {
            ParenKind::Condition
        } else {
            ParenKind::Inline
        };
        // A subquery right after a clause keyword opens on the keyword's line.
        if kind == ParenKind::Subquery && self.pending_break.is_some() {
            self.pending_break = None;
        }
        self.emit("(");
        self.frames.push(Frame { kind, line_indent: self.line_indent, clause: self.clause, base: self.base });
        match kind {
            ParenKind::Subquery => {
                self.base = self.line_indent + INDENT;
                self.clause = None;
                self.pending_break = Some(self.base);
            }
            ParenKind::Columns => self.pending_break = Some(self.line_indent + INDENT),
            ParenKind::Condition | ParenKind::Inline => {}
        }
    }

    fn close_paren(&mut self) {
        let Some(frame) = self.frames.pop() else {
            self.emit(")");
            return;
        };
        self.pending_break = None;
        if frame.kind == ParenKind::Columns {
            self.break_line(frame.line_indent);
        }
        self.emit(")");
        if frame.kind == ParenKind::Subquery {
            self.clause = frame.clause;
            self.base = frame.base;
        }
    }

    /// The `(` opens a condition group: it follows AND / OR / NOT / WHERE / ON / HAVING / WHEN or another condition paren.
    fn after_condition_start(&self) -> bool {
        let in_conditions = matches!(self.clause, Some("WHERE" | "HAVING")) || self.in_join_on;
        in_conditions
            && match self.previous.as_deref() {
                None => true,
                Some(previous) => {
                    matches!(previous, "AND" | "OR" | "NOT" | "WHERE" | "ON" | "HAVING" | "WHEN")
                        || (previous == "("
                            && self.frames.last().is_some_and(|frame| frame.kind == ParenKind::Condition))
                }
            }
    }

    fn previous_is(&self, word: &str) -> bool {
        self.previous.as_deref() == Some(word)
    }

    /// End the current line and start one at `indent`.
    fn break_line(&mut self, indent: usize) {
        self.pending_break = None;
        while self.out.ends_with(' ') {
            self.out.pop();
        }
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
        self.out.push_str(&" ".repeat(indent));
        self.line_indent = indent;
        self.previous = None;
    }

    /// Write a token with the right spacing before it.
    fn emit(&mut self, text: &str) {
        if let Some(indent) = self.pending_break.take() {
            self.break_line(indent);
        }
        if self.needs_space_before(text) {
            self.out.push(' ');
        }
        self.out.push_str(text);
        self.previous = Some(text.to_string());
    }

    fn needs_space_before(&self, text: &str) -> bool {
        let Some(previous) = self.previous.as_deref() else {
            return false;
        };
        if matches!(text, "," | ";" | ")" | "]" | "." | "::" | "[") || matches!(previous, "(" | "[" | "." | "::") {
            return false;
        }
        if text == "(" {
            // Function calls hug their parenthesis; keywords and table names in INSERT / CREATE do not.
            let previous_upper = previous.to_ascii_uppercase();
            let is_word =
                previous.chars().next().is_some_and(|first| first.is_alphanumeric() || first == '_' || first == '"');
            return !is_word || SPACED_BEFORE_PAREN.contains(&previous_upper.as_str()) || self.paren_after_name();
        }
        true
    }

    /// `INSERT INTO t (`, `CREATE TABLE t (`, `REFERENCES t (`: a space before the column list.
    fn paren_after_name(&self) -> bool {
        let before = self.out.trim_end();
        let words: Vec<&str> = before.rsplit(|character: char| character.is_whitespace()).take(2).collect();
        matches!(
            words.get(1).map(|word| word.to_ascii_uppercase()).as_deref(),
            Some("INTO" | "TABLE" | "REFERENCES" | "EXISTS")
        ) || self.create_table
    }

    fn finish(self) -> String {
        let mut out = self.out.trim_end().to_string();
        out.push('\n');
        out
    }
}

/// The clause names as `'static` strings.
fn pair_static(pair: &str) -> &'static str {
    LIST_CLAUSES.iter().chain(INLINE_CLAUSES).find(|clause| **clause == pair).copied().unwrap_or("")
}

/// A multi-line block comment with its continuation lines moved to `indent`
/// (one more space before a leading `*`, so the stars line up).
fn reindent_block_comment(comment: &str, indent: usize) -> String {
    let pad = " ".repeat(indent);
    let mut lines = comment.lines();
    let mut out = lines.next().unwrap_or("").trim_end().to_string();
    for line in lines {
        let trimmed = line.trim();
        out.push('\n');
        out.push_str(&pad);
        if trimmed.starts_with('*') {
            out.push(' ');
        }
        out.push_str(trimmed);
    }
    out
}

fn next_significant<'a, 'b>(tokens: &'b [Token<'a>], from: usize) -> Option<&'b Token<'a>> {
    tokens[from.min(tokens.len())..]
        .iter()
        .find(|token| !matches!(token, Token::LineComment { .. } | Token::BlockComment(_)))
}

/// Split SQL into tokens, dropping whitespace.
fn tokenize(text: &str) -> Vec<Token<'_>> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        let rest = &text[pos..];
        let byte = bytes[pos];
        let length = if byte.is_ascii_whitespace() {
            pos += 1;
            continue;
        } else if rest.starts_with("--") {
            let length = rest.find('\n').unwrap_or(rest.len());
            let line_start = text[..pos].rfind('\n').map_or(0, |newline| newline + 1);
            let trailing = !text[line_start..pos].trim().is_empty();
            tokens.push(Token::LineComment { text: rest[..length].trim_end(), trailing });
            length
        } else if rest.starts_with("/*") {
            let length = rest.find("*/").map_or(rest.len(), |end| end + 2);
            tokens.push(Token::BlockComment(&rest[..length]));
            length
        } else if byte == b'\''
            || (matches!(byte, b'E' | b'e' | b'B' | b'b' | b'X' | b'x' | b'N' | b'n') && rest[1..].starts_with('\''))
        {
            let start = usize::from(byte != b'\'');
            let length = quoted_length(&rest[start..], '\'') + start;
            tokens.push(Token::Literal(&rest[..length]));
            length
        } else if byte == b'"' {
            let length = quoted_length(rest, '"');
            tokens.push(Token::Literal(&rest[..length]));
            length
        } else if byte == b'$'
            && let Some(length) = dollar_quoted_length(rest)
        {
            tokens.push(Token::Literal(&rest[..length]));
            length
        } else if byte.is_ascii_digit() || (byte == b'$' && rest[1..].starts_with(|c: char| c.is_ascii_digit())) {
            let length = rest[1..]
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '_'))
                .map_or(rest.len(), |end| end + 1);
            tokens.push(Token::Literal(&rest[..length]));
            length
        } else if byte.is_ascii_alphabetic() || byte == b'_' || !byte.is_ascii() {
            let length = rest.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).unwrap_or(rest.len());
            tokens.push(Token::Word(&rest[..length]));
            length
        } else if rest.starts_with("::") {
            tokens.push(Token::Symbol("::"));
            2
        } else if matches!(byte, b'(' | b')' | b',' | b';' | b'.' | b'[' | b']') {
            tokens.push(Token::Symbol(&rest[..1]));
            1
        } else {
            let length = rest.find(|c: char| !"+-*/<>=~!@#%^&|`?:".contains(c)).unwrap_or(rest.len()).max(1);
            let length = rest.char_indices().nth(length).map_or(rest.len(), |(offset, _)| offset);
            tokens.push(Token::Symbol(&rest[..length]));
            length
        };
        pos += length;
    }
    tokens
}

/// Length of a quoted token starting at `rest[0] == quote`, with doubled quotes inside.
fn quoted_length(rest: &str, quote: char) -> usize {
    let mut chars = rest.char_indices().skip(1).peekable();
    while let Some((offset, character)) = chars.next() {
        if character == '\\' && quote == '\'' {
            chars.next();
        } else if character == quote {
            if chars.peek().is_some_and(|(_, next)| *next == quote) {
                chars.next();
            } else {
                return offset + 1;
            }
        }
    }
    rest.len()
}

/// Length of `$tag$ ... $tag$` (or `$$ ... $$`), if `rest` starts one.
fn dollar_quoted_length(rest: &str) -> Option<usize> {
    let tag_end = rest[1..].find('$')? + 2;
    let tag = &rest[..tag_end];
    if !tag[1..tag.len() - 1].chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let body_end = rest[tag_end..].find(tag)?;
    Some(tag_end + body_end + tag.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_with_conditions_and_order() {
        assert_eq!(
            format_sql("select a, b from t where x = 1 and y = 2 order by a desc;"),
            "SELECT\n    a,\n    b\nFROM\n    t\nWHERE\n    x = 1\n    AND y = 2\nORDER BY\n    a DESC;\n"
        );
    }

    #[test]
    fn joins_with_on_conditions() {
        assert_eq!(
            format_sql(
                "select * from a left outer join b on a.id = b.id and b.x > 0 join c using (id) where a.y is not null"
            ),
            "SELECT\n    *\nFROM\n    a\n    LEFT OUTER JOIN b ON a.id = b.id\n        AND b.x > 0\n    JOIN c USING (id)\nWHERE\n    a.y IS NOT NULL\n"
        );
    }

    #[test]
    fn subquery_opens_on_the_clause_line() {
        assert_eq!(
            format_sql("select x from (select y as x from t where z in (1,2)) as s"),
            "SELECT\n    x\nFROM (\n    SELECT\n        y AS x\n    FROM\n        t\n    WHERE\n        z IN (1, 2)) AS s\n"
        );
    }

    #[test]
    fn nested_conditions_indent_inside_parentheses() {
        assert_eq!(
            format_sql("select 1 from t where a < 'R45' or (b = 'R46' and c = 6) and d between 1 and 2"),
            "SELECT\n    1\nFROM\n    t\nWHERE\n    a < 'R45'\n    OR (b = 'R46'\n        AND c = 6)\n    AND d BETWEEN 1 AND 2\n"
        );
    }

    #[test]
    fn functions_casts_and_case() {
        assert_eq!(
            format_sql(
                "select count(*)::int as n, coalesce(a,0), case when a > 1 then 'big' else 'small' end as size from t group by a"
            ),
            "SELECT\n    count(*)::int AS n,\n    coalesce(a, 0),\n    CASE\n        WHEN a > 1 THEN 'big'\n        ELSE 'small'\n    END AS size\nFROM\n    t\nGROUP BY\n    a\n"
        );
    }

    #[test]
    fn insert_update_and_returning() {
        assert_eq!(
            format_sql(
                "insert into t (a,b) values (1,'x'),(2,'y') returning id; update t set a = 1, b = 2 where id = 3;"
            ),
            "INSERT INTO t (a, b)\nVALUES\n    (1, 'x'),\n    (2, 'y')\nRETURNING\n    id;\n\nUPDATE t\nSET\n    a = 1,\n    b = 2\nWHERE\n    id = 3;\n"
        );
    }

    #[test]
    fn create_table_lists_one_column_per_line() {
        assert_eq!(
            format_sql(
                "create table notes (id bigserial primary key, title TEXT not null, created timestamptz default now());"
            ),
            "CREATE TABLE notes (\n    id bigserial PRIMARY KEY,\n    title text NOT NULL,\n    created timestamptz DEFAULT now()\n);\n"
        );
    }

    #[test]
    fn with_queries() {
        assert_eq!(
            format_sql("with a as (select 1 as x), b as (select 2 as y) select * from a, b"),
            "WITH a AS (\n    SELECT\n        1 AS x),\nb AS (\n    SELECT\n        2 AS y)\nSELECT\n    *\nFROM\n    a,\n    b\n"
        );
    }

    #[test]
    fn strings_identifiers_and_comments_are_kept() {
        let text = "-- top note\nselect \"Mixed Case\", 'it''s -- not a comment', $$ body ; $$ /* why */ from t";
        assert_eq!(
            format_sql(text),
            "-- top note\nSELECT\n    \"Mixed Case\",\n    'it''s -- not a comment',\n    $$ body ; $$ /* why */\nFROM\n    t\n"
        );
    }

    #[test]
    fn formatting_twice_changes_nothing() {
        let once = format_sql("select a, b from x join y on x.id = y.id where a = 1 and (b = 2 or c = 3) order by a");
        assert_eq!(format_sql(&once), once);
    }

    #[test]
    fn trailing_comments_stay_on_their_line() {
        assert_eq!(
            format_sql("select a, -- first\n b -- second\n , c from t"),
            "SELECT\n    a, -- first\n    b, -- second\n    c\nFROM\n    t\n"
        );
    }

    #[test]
    fn block_comments_are_reindented() {
        assert_eq!(
            format_sql("select a,\n      /* one\n         * two\n         */\n b from t"),
            "SELECT\n    a,\n    /* one\n     * two\n     */\n    b\nFROM\n    t\n"
        );
    }
}
