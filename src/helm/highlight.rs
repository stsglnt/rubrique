//! Find YAML and Helm template syntax in raw text so the editor can color it.
//!
//! Line-based and forgiving, like the Markdown highlighter: it never fails,
//! it only labels byte ranges. It knows keys, `#` comments, list dashes,
//! `---`, anchors and tags, block scalars (`|`, `>`), and Go template actions
//! (`{{ ... }}`, `{{- ... -}}`, `{{/* ... */}}`), also across lines.

use crate::markdown::highlight::{Kind, Span, Spans};

/// What carries over from one line to the next.
#[derive(Debug, Default)]
struct State {
    /// Inside an action that started on an earlier line; `true` when it is
    /// a `{{/* ... */}}` comment.
    action: Option<bool>,
    /// Inside a block scalar: the indent its lines must go past.
    block: Option<usize>,
}

/// Label every byte of `text`. Spans are in order, touch each other, never
/// empty, and neighbors never share a kind.
pub fn highlight(text: &str) -> Vec<Span> {
    let mut out = Spans::default();
    let mut state = State::default();
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        Line { out: &mut out, start: line_start, text: content }.highlight(&mut state);
        line_start += line.len();
        out.to(line_start, Kind::Plain);
    }
    out.spans
}

/// One line being labeled; positions are byte offsets into the line.
struct Line<'a> {
    out: &'a mut Spans,
    /// Where the line starts in the whole text.
    start: usize,
    text: &'a str,
}

impl<'a> Line<'a> {
    /// Label everything up to `upto` (in the line) as `kind`.
    fn to(&mut self, upto: usize, kind: Kind) {
        self.out.to(self.start + upto, kind);
    }

    fn bytes(&self) -> &'a [u8] {
        self.text.as_bytes()
    }

    fn highlight(mut self, state: &mut State) {
        let mut i = 0;
        if let Some(comment) = state.action {
            i = self.action_body(0, comment, state);
            if state.action.is_some() {
                return;
            }
            if state.block.is_none() {
                self.inline(i, self.text.len(), Kind::Plain, true, state);
                return;
            }
        }
        let indent = self.text.len() - self.text.trim_start_matches(' ').len();
        if let Some(parent) = state.block {
            if i > 0 || self.text.trim().is_empty() || indent > parent {
                // Block scalar text is literal: no comments, but templates still run.
                self.inline(i, self.text.len(), Kind::Plain, false, state);
                return;
            }
            state.block = None;
        }
        self.node(indent, state);
    }

    /// A line of YAML structure, from its first non-space byte `i`.
    fn node(&mut self, mut i: usize, state: &mut State) {
        let rest = &self.bytes()[i..];
        if rest.is_empty() {
            return;
        }
        if rest[0] == b'#' {
            self.to(self.text.len(), Kind::Comment);
            return;
        }
        if i == 0 && (rest.starts_with(b"---") || rest.starts_with(b"...")) && is_end_or_space(rest.get(3)) {
            self.to(3, Kind::Marker);
            self.value(3, 0, state);
            return;
        }
        // List items, possibly nested on one line: `- - a`.
        let mut column = i;
        while self.bytes().get(i) == Some(&b'-') && is_end_or_space(self.bytes().get(i + 1)) {
            column = i;
            self.to(i, Kind::Plain);
            self.to(i + 1, Kind::Marker);
            i = skip_spaces(self.bytes(), i + 1);
        }
        if let Some(colon) = key_end(self.bytes(), i) {
            self.to(i, Kind::Plain);
            self.inline(i, colon, Kind::Key, false, state);
            self.to(colon + 1, Kind::Marker);
            self.value(colon + 1, i, state);
        } else {
            self.value(i, column, state);
        }
    }

    /// A value from `i`; a block scalar here belongs to the node at `column`.
    fn value(&mut self, i: usize, column: usize, state: &mut State) {
        let mut i = skip_spaces(self.bytes(), i);
        // Anchors, aliases and tags: `&name`, `*name`, `!!str`.
        while matches!(self.bytes().get(i), Some(b'&' | b'*' | b'!')) {
            self.to(i, Kind::Plain);
            let end = self.bytes()[i..].iter().position(u8::is_ascii_whitespace).map_or(self.text.len(), |at| i + at);
            self.to(end, Kind::Marker);
            i = skip_spaces(self.bytes(), end);
        }
        if let Some(&(b'|' | b'>')) = self.bytes().get(i) {
            let end =
                i + 1 + self.bytes()[i + 1..].iter().take_while(|b| matches!(b, b'-' | b'+' | b'0'..=b'9')).count();
            let after = skip_spaces(self.bytes(), end);
            if after == self.text.len() || self.bytes()[after] == b'#' {
                self.to(i, Kind::Plain);
                self.to(end, Kind::Marker);
                self.to(after, Kind::Plain);
                self.to(self.text.len(), Kind::Comment);
                state.block = Some(column);
                return;
            }
        }
        self.inline(i, self.text.len(), Kind::Plain, true, state);
    }

    /// Label `from..to` as `base`, with template actions and, when
    /// `comments` is set, a trailing `# comment`.
    fn inline(&mut self, from: usize, to: usize, base: Kind, comments: bool, state: &mut State) {
        let mut quote = None;
        let mut i = from;
        while i < to {
            let bytes = self.bytes();
            if bytes[i..].starts_with(b"{{") {
                self.to(i, base);
                i = self.action(i, state);
                if state.action.is_some() {
                    return;
                }
                continue;
            }
            let byte = bytes[i];
            let after_space = i == from || matches!(bytes[i - 1], b' ' | b'\t');
            match quote {
                Some(open) if byte == open => quote = None,
                None if comments && byte == b'#' && after_space => {
                    self.to(i, base);
                    self.to(self.text.len(), Kind::Comment);
                    return;
                }
                // A quote opens a string only where a scalar starts.
                None if matches!(byte, b'"' | b'\'') && (after_space || matches!(bytes[i - 1], b'[' | b'{' | b',')) => {
                    quote = Some(byte);
                }
                Some(_) | None => {}
            }
            i += 1;
        }
        self.to(to, base);
    }

    /// An action starting with `{{` at `i`. Returns where it ends (the line's
    /// end when it goes on to the next line).
    fn action(&mut self, i: usize, state: &mut State) -> usize {
        let mut body = i + 2;
        if self.bytes().get(body) == Some(&b'-') {
            body += 1;
        }
        let comment = self.bytes()[skip_spaces(self.bytes(), body)..].starts_with(b"/*");
        self.to(body, Kind::Marker);
        self.action_body(body, comment, state)
    }

    /// The inside of an action from `from` up to and including its `}}`.
    fn action_body(&mut self, from: usize, comment: bool, state: &mut State) -> usize {
        let bytes = self.bytes();
        let close = if comment {
            find(bytes, from, b"*/").and_then(|end| find(bytes, end + 2, b"}}"))
        } else {
            find_close(bytes, from)
        };
        let Some(close) = close else {
            self.to(self.text.len(), if comment { Kind::Comment } else { Kind::Template });
            state.action = Some(comment);
            return self.text.len();
        };
        // `-}}` trims space after the action; the `-` belongs to the marker.
        let marker = if close > from && bytes[close - 1] == b'-' { close - 1 } else { close };
        self.to(marker, if comment { Kind::Comment } else { Kind::Template });
        self.to(close + 2, Kind::Marker);
        state.action = None;
        close + 2
    }
}

/// Where `}}` closes an action, skipping `"..."` and `` `...` `` strings.
fn find_close(bytes: &[u8], from: usize) -> Option<usize> {
    let mut quote = None;
    let mut i = from;
    while i < bytes.len() {
        match (quote, bytes[i]) {
            (Some(b'"'), b'\\') => i += 1,
            (Some(open), byte) if byte == open => quote = None,
            (None, byte @ (b'"' | b'`')) => quote = Some(byte),
            (None, b'}') if bytes.get(i + 1) == Some(&b'}') => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// The `:` that ends a mapping key starting at `i`, if the line has one.
fn key_end(bytes: &[u8], i: usize) -> Option<usize> {
    let mut at = i;
    match bytes.get(i)? {
        // A quoted key: `"a b": 1`.
        &quote @ (b'"' | b'\'') => {
            at = i + 1 + bytes[i + 1..].iter().position(|&byte| byte == quote)? + 1;
            at = skip_spaces(bytes, at);
            return (bytes.get(at) == Some(&b':') && is_end_or_space(bytes.get(at + 1))).then_some(at);
        }
        // Flow collections and complex keys are left as values.
        b'{' if bytes.get(i + 1) != Some(&b'{') => return None,
        b'[' | b'?' | b'#' => return None,
        _ => {}
    }
    while at < bytes.len() {
        match bytes[at] {
            b'{' if bytes.get(at + 1) == Some(&b'{') => at = find_close(bytes, at + 2)? + 2,
            b':' if is_end_or_space(bytes.get(at + 1)) => return Some(at),
            b'#' if matches!(bytes[at - 1], b' ' | b'\t') => return None,
            // `- echo "a: b"` is a string, not a key.
            b'"' | b'\'' => return None,
            _ => at += 1,
        }
    }
    None
}

fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    bytes.get(from..)?.windows(needle.len()).position(|window| window == needle).map(|at| from + at)
}

fn skip_spaces(bytes: &[u8], i: usize) -> usize {
    i + bytes.get(i..).map_or(0, |rest| rest.iter().take_while(|byte| matches!(byte, b' ' | b'\t')).count())
}

fn is_end_or_space(byte: Option<&u8>) -> bool {
    byte.is_none_or(|byte| matches!(byte, b' ' | b'\t'))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text of each span with its kind, skipping plain text.
    fn colored(text: &str) -> Vec<(&str, Kind)> {
        highlight(text)
            .into_iter()
            .filter(|span| span.kind != Kind::Plain)
            .map(|span| (&text[span.range], span.kind))
            .collect()
    }

    #[test]
    fn spans_cover_the_text() {
        let text = "a: 1\n- b: {{ x }}\n  c: |\n    # literal\n{{/* multi\nline */}}\nd: é # c\n";
        let spans = highlight(text);
        assert_eq!(spans.first().map(|span| span.range.start), Some(0));
        assert_eq!(spans.last().map(|span| span.range.end), Some(text.len()));
        for pair in spans.windows(2) {
            assert_eq!(pair[0].range.end, pair[1].range.start);
            assert_ne!(pair[0].kind, pair[1].kind);
        }
    }

    #[test]
    fn keys_values_and_comments() {
        use Kind::{Comment, Key, Marker};
        assert_eq!(colored("name: web # the app\n"), [("name", Key), (":", Marker), ("# the app", Comment)]);
        assert_eq!(colored("url: http://a/#top\n"), [("url", Key), (":", Marker)]);
        assert_eq!(colored("- \"a b\": 1\n"), [("-", Marker), ("\"a b\"", Key), (":", Marker)]);
        assert_eq!(colored("# all\n---\n"), [("# all", Comment), ("---", Marker)]);
        assert_eq!(colored("- echo \"a: b\"\n"), [("-", Marker)]);
        let anchors = colored("x: &base\ny: *base\n");
        assert_eq!(
            anchors,
            [("x", Key), (":", Marker), ("&base", Marker), ("y", Key), (":", Marker), ("*base", Marker)]
        );
    }

    #[test]
    fn template_actions() {
        use Kind::{Comment, Key, Marker, Template};
        assert_eq!(
            colored("image: {{ .Values.image | quote }}\n"),
            [("image", Key), (":", Marker), ("{{", Marker), (" .Values.image | quote ", Template), ("}}", Marker),]
        );
        assert_eq!(
            colored("{{- if .Values.on -}}\n"),
            [("{{-", Marker), (" if .Values.on ", Template), ("-}}", Marker)]
        );
        assert_eq!(
            colored("x: {{ \"}}\" }}\n"),
            [("x", Key), (":", Marker), ("{{", Marker), (" \"}}\" ", Template), ("}}", Marker)]
        );
        assert_eq!(
            colored("{{/* a\nb */}}\nk: v\n"),
            [("{{", Marker), ("/* a", Comment), ("b */", Comment), ("}}", Marker), ("k", Key), (":", Marker),]
        );
        assert_eq!(
            colored("{{ .Release.Name }}-web: 1\n"),
            [("{{", Marker), (" .Release.Name ", Template), ("}}", Marker), ("-web", Key), (":", Marker),]
        );
    }

    #[test]
    fn block_scalars_are_literal() {
        use Kind::{Key, Marker, Template};
        let text = "script: |-\n  # not a comment\n  a: b {{ .x }}\n\nnext: 1\n";
        assert_eq!(
            colored(text),
            [
                ("script", Key),
                (":", Marker),
                ("|-", Marker),
                ("{{", Marker),
                (" .x ", Template),
                ("}}", Marker),
                ("next", Key),
                (":", Marker),
            ]
        );
        // In a list item the block belongs to the key, not the dash.
        let item = "- run: >\n    a: b\n  c: d\n";
        assert_eq!(
            colored(item),
            [("-", Marker), ("run", Key), (":", Marker), (">", Marker), ("c", Key), (":", Marker)]
        );
    }
}
