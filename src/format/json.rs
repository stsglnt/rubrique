//! JSON pretty-printer: 2-space indent, one member per line. Numbers and
//! strings are copied exactly as written (no re-encoding, no precision loss).

use super::FormatError;

const INDENT: &str = "  ";

/// Pretty-print `text`, or say where it stops being valid JSON.
///
/// # Errors
///
/// The first syntax error, with its line and column.
pub fn format_json(text: &str) -> Result<String, FormatError> {
    let mut parser = Parser { text, bytes: text.as_bytes(), pos: 0, out: String::with_capacity(text.len() * 2) };
    parser.skip_whitespace();
    parser.value(0)?;
    parser.skip_whitespace();
    if parser.pos < parser.bytes.len() {
        return Err(parser.error("unexpected text after the JSON value"));
    }
    parser.out.push('\n');
    Ok(parser.out)
}

struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
    out: String,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> FormatError {
        FormatError::at(self.text, self.pos, message)
    }

    fn skip_whitespace(&mut self) {
        while self.bytes.get(self.pos).is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn newline(&mut self, depth: usize) {
        self.out.push('\n');
        for _ in 0..depth {
            self.out.push_str(INDENT);
        }
    }

    fn value(&mut self, depth: usize) -> Result<(), FormatError> {
        match self.bytes.get(self.pos) {
            Some(b'{') => self.container(depth, b'}', true),
            Some(b'[') => self.container(depth, b']', false),
            Some(b'"') => self.string(),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(b't') => self.word("true"),
            Some(b'f') => self.word("false"),
            Some(b'n') => self.word("null"),
            Some(_) => Err(self.error("expected a value")),
            None => Err(self.error("unexpected end, expected a value")),
        }
    }

    /// An object (`object` = true) or array, members one per line.
    fn container(&mut self, depth: usize, close: u8, object: bool) -> Result<(), FormatError> {
        self.out.push(char::from(self.bytes[self.pos]));
        self.pos += 1;
        self.skip_whitespace();
        if self.bytes.get(self.pos) == Some(&close) {
            self.pos += 1;
            self.out.push(char::from(close));
            return Ok(());
        }
        loop {
            self.newline(depth + 1);
            if object {
                if self.bytes.get(self.pos) != Some(&b'"') {
                    return Err(self.error("expected a quoted key"));
                }
                self.string()?;
                self.skip_whitespace();
                if self.bytes.get(self.pos) != Some(&b':') {
                    return Err(self.error("expected ':' after the key"));
                }
                self.pos += 1;
                self.out.push_str(": ");
                self.skip_whitespace();
            }
            self.value(depth + 1)?;
            self.skip_whitespace();
            match self.bytes.get(self.pos) {
                Some(b',') => {
                    self.pos += 1;
                    self.out.push(',');
                    self.skip_whitespace();
                }
                Some(&byte) if byte == close => {
                    self.pos += 1;
                    self.newline(depth);
                    self.out.push(char::from(close));
                    return Ok(());
                }
                _ => return Err(self.error(if object { "expected ',' or '}'" } else { "expected ',' or ']'" })),
            }
        }
    }

    /// A string, copied as written after checking it is closed.
    fn string(&mut self) -> Result<(), FormatError> {
        let start = self.pos;
        self.pos += 1;
        loop {
            match self.bytes.get(self.pos) {
                Some(b'"') => break,
                Some(b'\\') => self.pos += 2,
                Some(b'\n') | None => return Err(FormatError::at(self.text, start, "string is not closed")),
                Some(_) => self.pos += 1,
            }
        }
        self.pos += 1;
        self.out.push_str(&self.text[start..self.pos]);
        Ok(())
    }

    fn number(&mut self) -> Result<(), FormatError> {
        let start = self.pos;
        while self
            .bytes
            .get(self.pos)
            .is_some_and(|byte| matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'))
        {
            self.pos += 1;
        }
        let number = &self.text[start..self.pos];
        if number.parse::<f64>().is_err() || number.starts_with('+') {
            return Err(FormatError::at(self.text, start, "not a valid number"));
        }
        self.out.push_str(number);
        Ok(())
    }

    fn word(&mut self, word: &str) -> Result<(), FormatError> {
        if self.text[self.pos..].starts_with(word) {
            self.pos += word.len();
            self.out.push_str(word);
            Ok(())
        } else {
            Err(self.error("expected a value"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_prints_nested_values() {
        let text = r#"{"name":"rubrique","tags":["md",1,-2.5e3,true,null],"empty":{},"none":[]}"#;
        assert_eq!(
            format_json(text).unwrap(),
            "{\n  \"name\": \"rubrique\",\n  \"tags\": [\n    \"md\",\n    1,\n    -2.5e3,\n    true,\n    null\n  ],\n  \"empty\": {},\n  \"none\": []\n}\n"
        );
    }

    #[test]
    fn strings_are_kept_exactly() {
        assert_eq!(format_json(r#"["a\"b\\", "é\u00e9"]"#).unwrap(), "[\n  \"a\\\"b\\\\\",\n  \"é\\u00e9\"\n]\n");
    }

    #[test]
    fn formatting_twice_changes_nothing() {
        let once = format_json("[{\"a\":[1,{\"b\":2}]}]").unwrap();
        assert_eq!(format_json(&once).unwrap(), once);
    }

    #[test]
    fn errors_say_where() {
        let error = format_json("{\n  \"a\": 1,\n  \"b\" 2\n}").unwrap_err();
        assert_eq!((error.line, error.column), (3, 7));
        assert!(error.message.contains("':'"), "{}", error.message);
        assert!(format_json("[1, 2").is_err());
        assert!(format_json("{\"a\": tru}").is_err());
        assert!(format_json("1 2").is_err());
        assert!(format_json("").is_err());
    }
}
