//! XML pretty-printer: one element per line, 2-space indent. An element that
//! holds only text stays on one line. Attributes, comments, CDATA and
//! processing instructions are copied as written.

use super::FormatError;

const INDENT: &str = "  ";

/// One piece of an XML document.
#[derive(Debug)]
enum Node<'a> {
    /// `<name ...>`; `raw` is the whole tag as written.
    Open { name: &'a str, raw: &'a str },
    /// `</name>`.
    Close { name: &'a str, raw: &'a str, at: usize },
    /// `<name ... />`, comments, `<?...?>`, `<!DOCTYPE ...>`, CDATA.
    Single(&'a str),
    /// Text between tags (not just whitespace), trimmed.
    Text(&'a str),
}

/// Pretty-print `text`, or say where its tags stop matching.
///
/// # Errors
///
/// An unclosed construct or a closing tag that does not match.
pub fn format_xml(text: &str) -> Result<String, FormatError> {
    let nodes = tokenize(text)?;
    let mut out = String::with_capacity(text.len() * 2);
    let mut open: Vec<&str> = Vec::new();
    let mut index = 0;
    while index < nodes.len() {
        let depth = open.len();
        match &nodes[index] {
            Node::Open { name, raw } => {
                // <a>text</a> and <a></a> stay on one line.
                if let (Some(Node::Text(content)), Some(Node::Close { name: closing, raw: close, .. })) =
                    (nodes.get(index + 1), nodes.get(index + 2))
                    && closing == name
                {
                    line(&mut out, depth, &format!("{raw}{content}{close}"));
                    index += 3;
                    continue;
                }
                if let Some(Node::Close { name: closing, raw: close, .. }) = nodes.get(index + 1)
                    && closing == name
                {
                    line(&mut out, depth, &format!("{raw}{close}"));
                    index += 2;
                    continue;
                }
                line(&mut out, depth, raw);
                open.push(name);
            }
            Node::Close { name, raw, at } => {
                match open.pop() {
                    Some(expected) if expected == *name => {}
                    Some(expected) => {
                        return Err(FormatError::at(text, *at, &format!("expected </{expected}>, found </{name}>")));
                    }
                    None => return Err(FormatError::at(text, *at, &format!("</{name}> closes nothing"))),
                }
                line(&mut out, open.len(), raw);
            }
            Node::Single(raw) | Node::Text(raw) => line(&mut out, depth, raw),
        }
        index += 1;
    }
    if let Some(name) = open.last() {
        return Err(FormatError::at(text, text.len(), &format!("<{name}> is never closed")));
    }
    Ok(out)
}

fn line(out: &mut String, depth: usize, content: &str) {
    for _ in 0..depth {
        out.push_str(INDENT);
    }
    out.push_str(content);
    out.push('\n');
}

/// Split into tags and text; whitespace-only text is dropped.
fn tokenize(text: &str) -> Result<Vec<Node<'_>>, FormatError> {
    let mut nodes = Vec::new();
    let mut pos = 0;
    while pos < text.len() {
        let rest = &text[pos..];
        if !rest.starts_with('<') {
            let end = rest.find('<').map_or(text.len(), |offset| pos + offset);
            let content = text[pos..end].trim();
            if !content.is_empty() {
                nodes.push(Node::Text(content));
            }
            pos = end;
            continue;
        }
        let (terminator, what) = if rest.starts_with("<!--") {
            ("-->", "comment")
        } else if rest.starts_with("<![CDATA[") {
            ("]]>", "CDATA section")
        } else if rest.starts_with("<?") {
            ("?>", "processing instruction")
        } else {
            (">", "tag")
        };
        let end = tag_end(rest, terminator)
            .ok_or_else(|| FormatError::at(text, pos, &format!("{what} is never closed")))?
            + pos;
        let raw = &text[pos..end];
        nodes.push(classify(raw, pos, text)?);
        pos = end;
    }
    Ok(nodes)
}

/// End (exclusive) of the construct starting at `rest`, skipping `>` inside
/// quoted attribute values for ordinary tags.
fn tag_end(rest: &str, terminator: &str) -> Option<usize> {
    if terminator != ">" {
        return rest.find(terminator).map(|offset| offset + terminator.len());
    }
    let mut quote = None;
    for (offset, character) in rest.char_indices() {
        match (quote, character) {
            (None, '"' | '\'') => quote = Some(character),
            (Some(open), _) if character == open => quote = None,
            (None, '>') => return Some(offset + 1),
            _ => {}
        }
    }
    None
}

fn classify<'a>(raw: &'a str, at: usize, text: &str) -> Result<Node<'a>, FormatError> {
    if raw.starts_with("<!") || raw.starts_with("<?") || raw.ends_with("/>") {
        return Ok(Node::Single(raw));
    }
    let closing = raw.starts_with("</");
    let inner = raw.trim_start_matches("</").trim_start_matches('<');
    let name_end = inner.find(|character: char| character.is_whitespace() || character == '>' || character == '/');
    let name = &inner[..name_end.unwrap_or(inner.len())];
    if name.is_empty() {
        return Err(FormatError::at(text, at, "tag has no name"));
    }
    Ok(if closing { Node::Close { name, raw, at } } else { Node::Open { name, raw } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nests_elements_and_keeps_text_inline() {
        let text = r#"<?xml version="1.0"?><notes><note id="1"><title>Plan</title><body/></note><!-- end --></notes>"#;
        assert_eq!(
            format_xml(text).unwrap(),
            "<?xml version=\"1.0\"?>\n<notes>\n  <note id=\"1\">\n    <title>Plan</title>\n    <body/>\n  </note>\n  <!-- end -->\n</notes>\n"
        );
    }

    #[test]
    fn keeps_attributes_with_angle_brackets_and_empty_elements() {
        assert_eq!(format_xml("<a b=\"x>y\"></a>").unwrap(), "<a b=\"x>y\"></a>\n");
    }

    #[test]
    fn formatting_twice_changes_nothing() {
        let once = format_xml("<a><b>1</b><c><d/></c></a>").unwrap();
        assert_eq!(format_xml(&once).unwrap(), once);
    }

    #[test]
    fn reports_mismatched_and_unclosed_tags() {
        let error = format_xml("<a>\n<b></a>").unwrap_err();
        assert_eq!(error.message, "expected </b>, found </a>");
        assert_eq!((error.line, error.column), (2, 4));
        assert!(format_xml("<a>").is_err());
        assert!(format_xml("<a><!-- x </a>").is_err());
    }
}
