//! `@tmux-` mentions: hand a step of any document to a terminal.
//!
//! A line like `@tmux-ticket-1-qa: check the login form` assigns work to
//! the terminal `ticket-1-qa` (a `.tmux` file or a running tmux session).
//! Sending pastes the instruction into that terminal, with the step the
//! mention sits in as context and the document's path, so the agent there
//! knows what to do and where it came from. A sent mention gets a hidden
//! comment after it, so it is not sent twice by accident.
//!
//! No egui here: finding mentions and building the message are plain text work.

use std::ops::Range;
use std::path::Path;

/// What every terminal mention starts with, after `@`: `@tmux-qa`.
pub const MENTION_PREFIX: &str = "tmux-";
/// The start of the hidden mark left after a sent mention.
const SENT_MARK: &str = "<!-- sent to ";
/// A fenced code block starts or ends with this.
const FENCE: &str = "```";

/// One `@tmux-` mention in a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delegation {
    /// The terminal's handle: what follows `@tmux-`.
    pub handle: String,
    /// What to do: the text after `: `, up to the end of the line.
    pub instruction: String,
    /// The step the mention sits in (its list item or paragraph), without
    /// the mention itself.
    pub context: String,
    /// Byte range of the mention's line, without its line break.
    pub line: Range<usize>,
    /// The line already carries a "sent" mark.
    pub sent: bool,
}

/// A terminal's handle from its name: letters, digits and `-` (what a
/// mention can hold), with other characters turned into `-`.
pub fn handle_for(name: &str) -> String {
    name.trim().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' }).collect()
}

/// Every `@tmux-` mention in `text`, in order, one per line (the first on
/// it). Mentions in fenced code blocks are examples, not assignments.
pub fn find_delegations(text: &str) -> Vec<Delegation> {
    let lines = line_ranges(text);
    let mut in_code = false;
    let mut found = Vec::new();
    for (index, range) in lines.iter().enumerate() {
        let line = &text[range.clone()];
        if line.trim_start().starts_with(FENCE) {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let Some((mention, handle)) = mention_in(line) else {
            continue;
        };
        found.push(Delegation {
            handle: handle.to_string(),
            instruction: instruction_after(&line[mention.end..]),
            context: context_around(text, &lines, index, mention),
            line: range.clone(),
            sent: line.contains(SENT_MARK),
        });
    }
    found
}

/// The first `@tmux-<handle>` in `line` that starts a word: its byte range
/// and the handle.
fn mention_in(line: &str) -> Option<(Range<usize>, &str)> {
    let marker = format!("@{MENTION_PREFIX}");
    let mut from = 0;
    while let Some(offset) = line[from..].find(&marker) {
        let start = from + offset;
        let handle_start = start + marker.len();
        let handle_end = line[handle_start..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .map_or(line.len(), |end| handle_start + end);
        let starts_word = line[..start].chars().next_back().is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let handle = line[handle_start..handle_end].trim_end_matches('-');
        if starts_word && !handle.is_empty() {
            return Some((start..handle_start + handle.len(), handle));
        }
        from = handle_start;
    }
    None
}

/// The instruction after a mention: the text after a `:` (or after the
/// mention when there is none), without any hidden comment.
fn instruction_after(rest: &str) -> String {
    let rest = rest.split(SENT_MARK).next().unwrap_or_default();
    let rest = rest.split("<!--").next().unwrap_or_default();
    rest.trim().trim_start_matches(':').trim().to_string()
}

/// The step around line `index`: up to the line that starts its list item
/// or paragraph, and down to the next item, heading or blank line. The
/// mention line itself counts only for the text before its mention.
fn context_around(text: &str, lines: &[Range<usize>], index: usize, mention: Range<usize>) -> String {
    let line_text = |line: usize| &text[lines[line].clone()];
    let mut first = index;
    while first > 0 && !starts_step(line_text(first)) && !line_text(first - 1).trim().is_empty() {
        first -= 1;
    }
    let mut last = index;
    while last + 1 < lines.len() && !line_text(last + 1).trim().is_empty() && !starts_step(line_text(last + 1)) {
        last += 1;
    }
    let mut parts = Vec::new();
    for line in first..=last {
        let part = if line == index { &line_text(line)[..mention.start] } else { line_text(line) };
        if !part.trim().is_empty() {
            parts.push(part.trim_end());
        }
    }
    parts.join("\n")
}

/// True for a line that starts a step: a list item or a heading.
fn starts_step(line: &str) -> bool {
    let line = line.trim_start();
    if line.starts_with('#') || line.starts_with("- ") || line.starts_with("* ") || line.starts_with("+ ") {
        return true;
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && (line[digits..].starts_with(". ") || line[digits..].starts_with(") "))
}

/// The byte range of every line of `text`, without line breaks.
fn line_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        ranges.push(start..start + content.len());
        start += line.len();
    }
    ranges
}

/// What is pasted into the terminal: the instruction, then the step it is
/// about and the document it came from.
pub fn message(delegation: &Delegation, document: &Path) -> String {
    let mut message = delegation.instruction.clone();
    if !delegation.context.is_empty() {
        if !message.is_empty() {
            message.push_str("\n\n");
        }
        message.push_str("The step this is about:\n");
        message.push_str(&delegation.context);
    }
    message.push_str("\n\n(From ");
    message.push_str(&document.display().to_string());
    message.push(')');
    message
}

/// `text` with a hidden "sent" mark at the end of the delegation's line.
pub fn mark_sent(text: &str, delegation: &Delegation, when: &str) -> String {
    let end = delegation.line.end;
    format!("{} {SENT_MARK}{MENTION_PREFIX}{} at {when} -->{}", &text[..end], delegation.handle, &text[end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# Plan\n\n1. Set up the project\n2. Build the login form\n   with remember-me\n   @tmux-ticket-1-qa: work on this topic\n3. Ship it\n\n```\n@tmux-example: not real\n```\n";

    #[test]
    fn finds_a_mention_with_its_step_and_instruction() {
        let found = find_delegations(DOC);
        assert_eq!(found.len(), 1);
        let delegation = &found[0];
        assert_eq!(delegation.handle, "ticket-1-qa");
        assert_eq!(delegation.instruction, "work on this topic");
        assert_eq!(delegation.context, "2. Build the login form\n   with remember-me");
        assert_eq!(&DOC[delegation.line.clone()], "   @tmux-ticket-1-qa: work on this topic");
        assert!(!delegation.sent);
    }

    #[test]
    fn a_mention_inside_its_step_line_keeps_the_text_before_it() {
        let found = find_delegations("- Fix the tests @tmux-qa\n- Next\n");
        assert_eq!(found[0].context, "- Fix the tests");
        assert_eq!(found[0].instruction, "");
    }

    #[test]
    fn ignores_emails_and_bare_prefixes() {
        assert_eq!(find_delegations("mail me@tmux-lab.com\n@tmux- alone\n"), []);
    }

    #[test]
    fn marking_sent_is_seen_and_kept_out_of_the_instruction() {
        let delegation = find_delegations(DOC).remove(0);
        let marked = mark_sent(DOC, &delegation, "14:02");
        assert!(marked.contains("work on this topic <!-- sent to tmux-ticket-1-qa at 14:02 -->\n3. Ship it"));
        let again = find_delegations(&marked).remove(0);
        assert!(again.sent);
        assert_eq!(again.instruction, "work on this topic");
    }

    #[test]
    fn the_message_has_the_instruction_step_and_document() {
        let delegation = find_delegations(DOC).remove(0);
        let text = message(&delegation, Path::new("/notes/plan.md"));
        assert_eq!(
            text,
            "work on this topic\n\nThe step this is about:\n2. Build the login form\n   with remember-me\n\n(From /notes/plan.md)"
        );
    }

    #[test]
    fn handles_come_from_names() {
        assert_eq!(handle_for("ticket_1 qa"), "ticket-1-qa");
        assert_eq!(handle_for("term-1"), "term-1");
    }
}
