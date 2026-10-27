//! Draw the editor and the preview, and apply editor keyboard shortcuts.

use eframe::egui::text::{CCursor, CCursorRange};
use eframe::egui::text_edit::TextEditState;
use eframe::egui::{self, Context, Id, Key, Modifiers, Ui};

use super::line_ops::{Direction, Selection, duplicate_lines, insert_text, move_lines};
use super::text_file::Document;
use super::text_stats::{TextStats, text_stats};
use crate::markdown::blocks::Block;
use crate::markdown::render::render_blocks;

/// Widest the preview text gets, for comfortable reading.
const PREVIEW_MAX_WIDTH: f32 = 860.0;
/// Space (x, y) between the page edge and the editor text.
const EDITOR_MARGIN: (i8, i8) = (16, 12);
/// Space (x, y) between the page edge and the preview text.
const PREVIEW_MARGIN: (i8, i8) = (28, 20);

/// The editor widget's id, shared by drawing, focus and the line shortcuts.
fn editor_id() -> Id {
    Id::unique("rubrique-editor")
}

/// Draw the text editor. Returns status-bar stats for the cursor.
pub fn show_editor(ui: &mut Ui, document: &mut Document) -> Option<TextStats> {
    let available = ui.available_size();
    let output = egui::ScrollArea::both()
        .id_salt("editor-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::TextEdit::multiline(&mut document.text)
                .id(editor_id())
                .code_editor()
                // No border: the editor is the page itself.
                .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(EDITOR_MARGIN.0, EDITOR_MARGIN.1)))
                .lock_focus(true)
                .desired_width(f32::INFINITY)
                .min_size(available)
                .show(ui)
        })
        .inner;
    if output.response.changed() {
        document.on_text_changed();
    }
    let range = output.cursor_range?;
    let sorted = range.as_sorted_char_range();
    Some(text_stats(&document.text, range.primary.index.0, sorted.start.0..sorted.end.0))
}

/// Draw the rendered Markdown.
pub fn show_preview(ui: &mut Ui, blocks: &[Block]) {
    egui::ScrollArea::vertical().id_salt("preview-scroll").auto_shrink([false, false]).show(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(PREVIEW_MARGIN.0, PREVIEW_MARGIN.1)).show(ui, |ui| {
            ui.set_max_width(PREVIEW_MAX_WIDTH.min(ui.available_width()));
            render_blocks(ui, blocks);
        });
    });
}

/// Give keyboard focus to the editor (e.g. after opening a file).
pub fn focus_editor(ctx: &Context) {
    ctx.memory_mut(|memory| memory.request_focus(editor_id()));
}

/// Line-editing commands bound to keys while the editor has focus.
enum EditorCommand {
    MoveLines(Direction),
    DuplicateLines,
    InsertDate,
}

/// Apply editor shortcuts before the editor is drawn, so the editor itself
/// never sees these keys. Returns true when the text changed.
pub fn handle_editor_shortcuts(ctx: &Context, document: &mut Document) -> bool {
    if !ctx.memory(|memory| memory.has_focus(editor_id())) {
        return false;
    }
    let Some(command) = ctx.input_mut(|input| {
        // Check Shift variants first: egui lets Cmd+Shift+D also match Cmd+D.
        if input.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::D) {
            Some(EditorCommand::InsertDate)
        } else if input.consume_key(Modifiers::COMMAND, Key::D) {
            Some(EditorCommand::DuplicateLines)
        } else if input.consume_key(Modifiers::ALT, Key::ArrowUp) {
            Some(EditorCommand::MoveLines(Direction::Up))
        } else if input.consume_key(Modifiers::ALT, Key::ArrowDown) {
            Some(EditorCommand::MoveLines(Direction::Down))
        } else {
            None
        }
    }) else {
        return false;
    };

    let Some(mut state) = TextEditState::load(ctx, editor_id()) else {
        return false;
    };
    let Some(range) = state.cursor.char_range() else {
        return false;
    };
    let sorted = range.as_sorted_char_range();
    let selection = Selection { start: sorted.start.0, end: sorted.end.0 };
    let new_selection = match command {
        EditorCommand::MoveLines(direction) => move_lines(&mut document.text, selection, direction),
        EditorCommand::DuplicateLines => Some(duplicate_lines(&mut document.text, selection)),
        EditorCommand::InsertDate => Some(insert_text(&mut document.text, selection, &crate::date::today_iso())),
    };
    let Some(new_selection) = new_selection else {
        return false;
    };
    let new_range = CCursorRange::two(CCursor::new(new_selection.start), CCursor::new(new_selection.end));
    state.cursor.set_char_range(Some(new_range));
    state.store(ctx, editor_id());
    document.on_text_changed();
    true
}
