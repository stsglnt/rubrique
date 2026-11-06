//! Mention suggestions while typing `@ai-…` (agents) or `@tmux-…`
//! (terminals) in a text box: a small list under the cursor with the
//! matches. Up and Down move through it, Enter or Tab puts the mention in,
//! Esc hides it, and a click picks one.
//!
//! The list is drawn after its text box, but must take Enter, Tab and the
//! arrows before the box sees them (the box would insert a new line or a
//! tab). So each text box calls [`take_key`] before it is drawn and
//! [`show`] after, and whether the list is open is kept between frames.

use std::ops::Range;

use eframe::egui::text::{CCursor, CCursorRange};
use eframe::egui::text_edit::{TextEditOutput, TextEditState};
use eframe::egui::{self, Context, Id, Key, Modifiers, RichText, Ui};

use super::agents::{Agent, MENTION_PREFIX};
use crate::terminal::mentions::MENTION_PREFIX as TERMINAL_PREFIX;

/// Widest a suggestion row gets; long descriptions are cut short.
const LIST_WIDTH: f32 = 360.0;
/// The mention kinds a text box suggests: agents and terminals.
const PREFIXES: [&str; 2] = [MENTION_PREFIX, TERMINAL_PREFIX];

/// One thing the list can put in: an agent (`@ai-ed`) or a terminal
/// (`@tmux-qa`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    /// The whole mention, `@` included.
    pub mention: String,
    /// The name shown next to it.
    pub title: String,
    /// A short description, shown weak.
    pub detail: String,
}

impl Suggestion {
    /// The suggestion for an agent.
    pub fn agent(agent: &Agent) -> Self {
        Suggestion { mention: agent.mention(), title: agent.name.clone(), detail: agent.description.clone() }
    }
}

/// The suggestions whose mention starts with `typed` (without `@`), or
/// whose name starts with what follows the mention kind (`ai-` or `tmux-`).
fn matching<'a>(suggestions: &'a [Suggestion], typed: &str) -> Vec<&'a Suggestion> {
    let typed = typed.to_lowercase();
    let bare = PREFIXES.iter().find_map(|prefix| typed.strip_prefix(prefix)).unwrap_or(&typed);
    suggestions
        .iter()
        .filter(|suggestion| {
            let mention = suggestion.mention.trim_start_matches('@').to_lowercase();
            let same_kind = PREFIXES.iter().any(|prefix| typed.starts_with(prefix) && mention.starts_with(prefix));
            same_kind && (mention.starts_with(&typed) || suggestion.title.to_lowercase().starts_with(bare))
        })
        .collect()
}

/// An `@ai-…` or `@tmux-…` word at the text cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedMention {
    /// Byte range of the whole word, `@` included: what a picked agent replaces.
    pub range: Range<usize>,
    /// What is typed after `@`, up to the cursor (`ai-ed`).
    pub typed: String,
}

/// The mention word starting with one of `prefixes` (after `@`) that the
/// cursor (a char index, as egui counts) is in or right after. `None` for
/// other words, and for e-mail addresses (`me@ai-lab.com`): the `@` must
/// start a word.
pub fn typed_mention(text: &str, cursor_char: usize, prefixes: &[&str]) -> Option<TypedMention> {
    let cursor = text.char_indices().nth(cursor_char).map_or(text.len(), |(byte, _)| byte);
    let before = &text[..cursor];
    let word_start = before.rfind(|c: char| !is_handle_char(c)).map_or(0, |index| index + 1);
    let at = word_start.checked_sub(1).filter(|&at| before[at..].starts_with('@'))?;
    let starts_word = before[..at].chars().next_back().is_none_or(|c| !c.is_alphanumeric() && c != '_');
    let typed = &before[word_start..];
    let lower = typed.to_lowercase();
    if !starts_word || !prefixes.iter().any(|prefix| lower.starts_with(prefix)) {
        return None;
    }
    // A pick replaces the rest of the word after the cursor too.
    let word_end = text[cursor..].find(|c: char| !is_handle_char(c)).map_or(text.len(), |index| cursor + index);
    Some(TypedMention { range: at..word_end, typed: typed.to_string() })
}

/// Characters a handle can have (as typed, so upper case too). All ASCII,
/// so a byte index next to one is always a char boundary.
fn is_handle_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}

/// Put `mention` and one space over `range`. Returns the char index just
/// after the space, for the cursor.
pub fn insert_mention(text: &mut String, range: Range<usize>, mention: &str) -> usize {
    // Reuse a space that is already there rather than doubling it.
    let space = if text[range.end..].starts_with(' ') { "" } else { " " };
    let start = range.start;
    text.replace_range(range, &format!("{mention}{space}"));
    let cursor = start + mention.len() + 1;
    text[..cursor].chars().count()
}

/// `text` without the mention word at `range`, with the words around it
/// joined by one space (for the Ask bar, where a picked agent becomes a chip).
pub fn remove_mention(text: &str, range: Range<usize>) -> String {
    let before = text[..range.start].trim_end();
    let after = text[range.end..].trim_start();
    if before.is_empty() || after.is_empty() { format!("{before}{after}") } else { format!("{before} {after}") }
}

/// The key the list took from its text box this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListKey {
    #[default]
    Nothing,
    Up,
    Down,
    /// Enter or Tab: put the selected suggestion in.
    Pick,
    /// Esc: hide the list for this mention.
    Close,
}

/// What the list remembers between frames, per text box.
#[derive(Clone, Debug, Default)]
struct ListState {
    open: bool,
    selected: usize,
    /// Esc hid the list for the mention starting at this byte.
    dismissed_at: Option<usize>,
}

fn list_state_id(field: Id) -> Id {
    field.with("agent-mention-list")
}

fn load_state(ctx: &Context, field: Id) -> ListState {
    ctx.data(|data| data.get_temp::<ListState>(list_state_id(field))).unwrap_or_default()
}

fn store_state(ctx: &Context, field: Id, state: ListState) {
    ctx.data_mut(|data| data.insert_temp(list_state_id(field), state));
}

/// Take the list's key before the text box `field` is drawn, while the
/// list is open. Nothing is taken while it is closed.
pub fn take_key(ctx: &Context, field: Id) -> ListKey {
    if !load_state(ctx, field).open {
        return ListKey::Nothing;
    }
    ctx.input_mut(|input| {
        let mut take = |key| input.consume_key(Modifiers::NONE, key);
        if take(Key::Escape) {
            ListKey::Close
        } else if take(Key::Enter) || take(Key::Tab) {
            ListKey::Pick
        } else if take(Key::ArrowUp) {
            ListKey::Up
        } else if take(Key::ArrowDown) {
            ListKey::Down
        } else {
            ListKey::Nothing
        }
    })
}

/// Show the suggestions that match the mention word at the cursor of the
/// text box `field` (just drawn, as `output`), and put a picked one into
/// `text`. Returns true when `text` changed.
pub fn show(
    ui: &Ui,
    field: Id,
    output: &TextEditOutput,
    text: &mut String,
    suggestions: &[Suggestion],
    key: ListKey,
) -> bool {
    let ctx = ui.ctx();
    let mut state = load_state(ctx, field);
    // A click on the list takes focus from the box in the same frame.
    let focused = output.response.has_focus() || output.response.lost_focus();
    let cursor = output.cursor_range.or_else(|| output.state.cursor.char_range()).map(|range| range.primary);
    let mention = cursor.filter(|_| focused).and_then(|cursor| typed_mention(text, cursor.index.0, &PREFIXES));
    let Some((mention, cursor)) = mention.zip(cursor) else {
        store_state(ctx, field, ListState::default());
        return false;
    };
    let matches = matching(suggestions, &mention.typed);
    if matches.is_empty() || state.dismissed_at == Some(mention.range.start) {
        state.open = false;
        store_state(ctx, field, state);
        return false;
    }
    if key == ListKey::Close {
        store_state(ctx, field, ListState { dismissed_at: Some(mention.range.start), ..ListState::default() });
        return false;
    }
    state.selected = moved_selection(state.selected, key, matches.len());
    let mut picked = (key == ListKey::Pick).then(|| matches[state.selected]);
    let below_cursor = output.galley_pos + output.galley.pos_from_cursor(cursor).left_bottom().to_vec2();
    // Above the dialogs, so it also works in the note dialog.
    egui::Area::new(field.with("agent-mention-area")).order(egui::Order::Tooltip).fixed_pos(below_cursor).show(
        ctx,
        |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(LIST_WIDTH);
                for (index, suggestion) in matches.iter().enumerate() {
                    if suggestion_row(ui, suggestion, index == state.selected) {
                        picked = Some(suggestion);
                    }
                }
            });
        },
    );
    let Some(suggestion) = picked else {
        state.open = true;
        store_state(ctx, field, state);
        return false;
    };
    let new_cursor = insert_mention(text, mention.range, &suggestion.mention);
    let mut edit_state = TextEditState::load(ctx, field).unwrap_or_default();
    edit_state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(new_cursor))));
    edit_state.store(ctx, field);
    ctx.memory_mut(|memory| memory.request_focus(field));
    store_state(ctx, field, ListState::default());
    true
}

/// The selected row after Up or Down, kept inside the list.
fn moved_selection(selected: usize, key: ListKey, count: usize) -> usize {
    let last = count.saturating_sub(1);
    let selected = selected.min(last);
    match key {
        ListKey::Up => selected.saturating_sub(1),
        ListKey::Down => (selected + 1).min(last),
        _ => selected,
    }
}

/// One suggestion: its mention, name and description. True when clicked.
fn suggestion_row(ui: &mut Ui, suggestion: &Suggestion, selected: bool) -> bool {
    ui.horizontal(|ui| {
        let clicked = ui.selectable_label(selected, RichText::new(&suggestion.mention).monospace()).clicked();
        ui.label(&suggestion.title);
        ui.add(egui::Label::new(RichText::new(&suggestion.detail).small().weak()).truncate());
        clicked
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn agent(handle: &str) -> Agent {
        Agent {
            name: handle.to_string(),
            handle: handle.to_string(),
            description: String::new(),
            tool: None,
            model: None,
            command: None,
            skills: Vec::new(),
            instructions: String::new(),
            path: PathBuf::new(),
        }
    }

    fn mention_at_end(text: &str) -> Option<TypedMention> {
        typed_mention(text, text.chars().count(), &PREFIXES)
    }

    #[test]
    fn finds_the_mention_being_typed() {
        let found = mention_at_end("Fix this @ai-ed").unwrap();
        assert_eq!(found, TypedMention { range: 9..15, typed: "ai-ed".to_string() });
        assert_eq!(mention_at_end("@ai-").unwrap().range, 0..4);
        assert_eq!(mention_at_end("(@AI-Ed").unwrap().typed, "AI-Ed");
    }

    #[test]
    fn ignores_other_words_and_emails() {
        assert_eq!(mention_at_end("hello @john"), None);
        assert_eq!(mention_at_end("@ai"), None);
        assert_eq!(mention_at_end("mail me@ai-lab"), None);
        assert_eq!(mention_at_end("@ai-ed and more"), None);
    }

    #[test]
    fn covers_the_whole_word_when_the_cursor_is_inside_it() {
        // Cursor after "@ai-e" in "é @ai-editor!": char indices, not bytes.
        let found = typed_mention("é @ai-editor!", 7, &PREFIXES).unwrap();
        assert_eq!(found.typed, "ai-e");
        assert_eq!(found.range, 3..13);
    }

    #[test]
    fn inserts_the_mention_and_one_space() {
        let mut text = "Ask @ai-e".to_string();
        let cursor = insert_mention(&mut text, 4..9, &agent("editor").mention());
        assert_eq!(text, "Ask @ai-editor ");
        assert_eq!(cursor, text.chars().count());

        let mut text = "é @ai-e fix".to_string();
        let cursor = insert_mention(&mut text, 3..8, &agent("editor").mention());
        assert_eq!(text, "é @ai-editor fix");
        assert_eq!(cursor, "é @ai-editor ".chars().count());
    }

    #[test]
    fn removes_the_mention_word_and_its_extra_space() {
        assert_eq!(remove_mention("make it @ai-ed short", 8..14), "make it short");
        assert_eq!(remove_mention("shorter @ai-ed", 8..14), "shorter");
        assert_eq!(remove_mention("@ai-ed", 0..6), "");
    }

    #[test]
    fn finds_terminal_mentions_too() {
        assert_eq!(mention_at_end("Do it @tmux-q").unwrap().typed, "tmux-q");
        assert_eq!(typed_mention("@tmux-q", 7, &[MENTION_PREFIX]), None);
    }

    #[test]
    fn matches_only_the_kind_being_typed() {
        let suggestions = [
            Suggestion::agent(&agent("qa")),
            Suggestion { mention: "@tmux-qa".to_string(), title: "qa".to_string(), detail: String::new() },
        ];
        let mentions = |typed: &str| -> Vec<String> {
            matching(&suggestions, typed).iter().map(|found| found.mention.clone()).collect()
        };
        assert_eq!(mentions("tmux-"), ["@tmux-qa"]);
        assert_eq!(mentions("ai-q"), ["@ai-qa"]);
        assert_eq!(mentions("tmux-x"), Vec::<String>::new());
    }

    #[test]
    fn up_and_down_stay_inside_the_list() {
        assert_eq!(moved_selection(0, ListKey::Down, 3), 1);
        assert_eq!(moved_selection(2, ListKey::Down, 3), 2);
        assert_eq!(moved_selection(0, ListKey::Up, 3), 0);
        // The list got shorter while typing.
        assert_eq!(moved_selection(5, ListKey::Nothing, 2), 1);
    }
}
