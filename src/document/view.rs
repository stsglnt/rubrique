//! Draw the editor and the preview, and apply editor keyboard shortcuts.

use eframe::egui::text::{CCursor, CCursorRange};
use eframe::egui::text_edit::TextEditState;
use eframe::egui::{self, Context, Id, Key, Modifiers, TextBuffer, Ui};

use super::highlighting::{HighlightStyle, highlighted};
use super::line_ops::{Direction, Selection, duplicate_lines, insert_text, move_lines};
use super::text_file::Document;
use super::text_stats::{TextStats, text_stats};
use crate::markdown::blocks::SourceBlock;
use crate::markdown::render::{CodeAction, render_top_block, take_code_action, top_gap};
use crate::voice::view::{MemoAction, take_memo_action};

/// Widest the preview text gets, for comfortable reading.
const PREVIEW_MAX_WIDTH: f32 = 760.0;
/// Space (x, y) between the page edge and the editor text.
const EDITOR_MARGIN: (i8, i8) = (16, 12);
/// Space (x, y) between the page edge and the preview text.
const PREVIEW_MARGIN: (i8, i8) = (32, 24);

/// The editor widget's id, shared by drawing, focus and the line shortcuts.
fn editor_id() -> Id {
    Id::unique("rubrique-editor")
}

/// Draw the text editor, coloring Markdown syntax when `markdown` is set.
/// Returns status-bar stats for the cursor.
pub fn show_editor(ui: &mut Ui, document: &mut Document, markdown: bool) -> Option<TextStats> {
    let available = ui.available_size();
    let style = HighlightStyle::from_ui(ui);
    let mut layouter = |ui: &Ui, buffer: &dyn TextBuffer, wrap_width: f32| {
        let mut job = highlighted(ui.ctx(), buffer.as_str(), &style);
        job.wrap.max_width = wrap_width;
        ui.ctx().fonts_mut(|fonts| fonts.layout_job(job))
    };
    let output = egui::ScrollArea::both()
        .id_salt("editor-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let editor = egui::TextEdit::multiline(&mut document.text)
                .id(editor_id())
                .code_editor()
                // No border: the editor is the page itself.
                .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(EDITOR_MARGIN.0, EDITOR_MARGIN.1)))
                .lock_focus(true)
                .desired_width(f32::INFINITY)
                .min_size(available);
            if markdown { editor.layouter(&mut layouter).show(ui) } else { editor.show(ui) }
        })
        .inner;
    if output.response.changed() {
        document.on_text_changed();
    }
    // While a file is dragged over the editor, the cursor follows the
    // pointer, so the drop lands where it shows.
    let dragging_files = ui.ctx().input(|input| !input.raw.hovered_files.is_empty());
    if dragging_files
        && let Some(pointer) = ui.ctx().pointer_latest_pos()
        && output.response.response.rect.contains(pointer)
    {
        let cursor = output.galley.cursor_from_pos(pointer - output.galley_pos);
        set_editor_cursor(ui.ctx(), cursor.index.0);
    }
    let range = output.cursor_range?;
    let sorted = range.as_sorted_char_range();
    Some(text_stats(&document.text, range.primary.index.0, sorted.start.0..sorted.end.0))
}

/// A top-level block being edited as Markdown inside the preview.
#[derive(Debug)]
pub struct BlockEdit {
    /// Which top-level block.
    pub index: usize,
    /// Its source range in the document when editing started.
    pub range: std::ops::Range<usize>,
    /// The Markdown being typed.
    pub text: String,
    /// Focus is given to the box once, when editing starts.
    pub focused: bool,
}

/// What the user did in the preview.
#[derive(Clone, Debug, PartialEq)]
pub enum PreviewEvent {
    /// Clicked a block: edit it.
    StartEdit(usize),
    /// Done with the block being edited (Esc, Cmd+Enter, Done, or a click elsewhere).
    FinishEdit,
    /// Copy or Prettify on a code block inside top-level block `index`.
    Code { index: usize, action: CodeAction },
    /// A button on a voice memo.
    Memo(MemoAction),
}

/// What the preview reports after drawing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreviewOutput {
    pub event: Option<PreviewEvent>,
    /// The top-level block under the pointer, if any.
    pub hovered: Option<usize>,
}

/// Opacity of the accent behind a block under the mouse.
const HOVER_TINT: f32 = 0.06;
/// Width of the outline around the block being edited.
const EDIT_OUTLINE: f32 = 1.5;
/// Space between a block and its hover / edit background.
const BLOCK_PAD: i8 = 6;

/// Draw the rendered Markdown, one clickable block at a time; the block in
/// `editing` shows as a Markdown text box instead.
pub fn show_preview(ui: &mut Ui, blocks: &[SourceBlock], mut editing: Option<&mut BlockEdit>) -> PreviewOutput {
    let mut event = None;
    let mut hovered = None;
    egui::ScrollArea::vertical().id_salt("preview-scroll").auto_shrink([false, false]).show(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(PREVIEW_MARGIN.0, PREVIEW_MARGIN.1)).show(ui, |ui| {
            ui.set_max_width(PREVIEW_MAX_WIDTH.min(ui.available_width()));
            for (index, source) in blocks.iter().enumerate() {
                if index > 0 {
                    ui.add_space(top_gap(&source.block) - f32::from(BLOCK_PAD) * 2.0);
                }
                let top = ui.cursor().top();
                let found = match editing.as_deref_mut() {
                    Some(edit) if edit.index == index => edit_box(ui, edit),
                    _ => clickable_block(ui, index, source),
                };
                let span = egui::Rect::from_x_y_ranges(ui.max_rect().x_range(), top..=ui.cursor().top());
                if ui.rect_contains_pointer(span) {
                    hovered = Some(index);
                }
                if event.is_none() {
                    event = found;
                }
            }
        });
    });
    PreviewOutput { event, hovered }
}

/// A rendered block: tinted under the mouse, a click (not a text selection
/// or a link) asks to edit it.
fn clickable_block(ui: &mut Ui, index: usize, source: &SourceBlock) -> Option<PreviewEvent> {
    let background = ui.painter().add(egui::Shape::Noop);
    let response = egui::Frame::new().inner_margin(BLOCK_PAD).show(ui, |ui| {
        ui.set_width(ui.available_width());
        render_top_block(ui, &source.block);
    });
    let rect = response.response.rect;
    let hovered = ui.rect_contains_pointer(rect);
    if hovered {
        let fill = ui.visuals().hyperlink_color.gamma_multiply(HOVER_TINT);
        ui.painter().set(background, egui::Shape::rect_filled(rect, 6, fill));
    }
    // A code block button was clicked: that, not editing the block.
    if let Some(action) = take_code_action(ui.ctx()) {
        return Some(PreviewEvent::Code { index, action });
    }
    if let Some(action) = take_memo_action(ui.ctx()) {
        return Some(PreviewEvent::Memo(action));
    }
    // A click (pointer released without dragging) that did not open a link;
    // drags select text for notes instead.
    let clicked = hovered && ui.input(|input| input.pointer.any_click());
    let opened_link = ui
        .ctx()
        .output(|output| output.commands.iter().any(|command| matches!(command, egui::OutputCommand::OpenUrl(_))));
    if clicked && !opened_link {
        ui.ctx().plugin::<egui::text_selection::LabelSelectionState>().lock().clear_selection();
        return Some(PreviewEvent::StartEdit(index));
    }
    None
}

/// The block being edited: its Markdown in an outlined box.
fn edit_box(ui: &mut Ui, edit: &mut BlockEdit) -> Option<PreviewEvent> {
    let visuals = ui.visuals().clone();
    let mut event = None;
    let frame = egui::Frame::new()
        .fill(visuals.text_edit_bg_color())
        .stroke(egui::Stroke::new(EDIT_OUTLINE, visuals.hyperlink_color))
        .corner_radius(8)
        .inner_margin(BLOCK_PAD + 2);
    let response = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new("Editing this block (Markdown)").small().weak());
        let rows = edit.text.lines().count().max(1);
        let output = egui::TextEdit::multiline(&mut edit.text)
            .id(block_editor_id())
            .code_editor()
            .frame(egui::Frame::NONE)
            .desired_width(f32::INFINITY)
            .desired_rows(rows)
            .show(ui);
        if !edit.focused {
            output.response.request_focus();
            edit.focused = true;
        }
        // egui's text box drops focus on Esc by itself, so look for that too.
        let escaped = output.response.lost_focus() && ui.input(|input| input.key_pressed(Key::Escape));
        let finish_keys = escaped
            || (output.response.has_focus() && ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::Enter)));
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Esc, Cmd+Enter or a click outside to finish").small().weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Done").clicked() || finish_keys {
                    event = Some(PreviewEvent::FinishEdit);
                }
            });
        });
    });
    let clicked_outside =
        ui.input(|input| input.pointer.any_click()) && !ui.rect_contains_pointer(response.response.rect);
    if clicked_outside {
        event = Some(PreviewEvent::FinishEdit);
    }
    event
}

fn block_editor_id() -> Id {
    Id::unique("rubrique-block-editor")
}

/// The editor's selection as sorted character indices, if any text is selected.
pub fn editor_selection(ctx: &Context) -> Option<(usize, usize)> {
    let state = TextEditState::load(ctx, editor_id())?;
    let range = state.cursor.char_range()?.as_sorted_char_range();
    (range.start.0 < range.end.0).then_some((range.start.0, range.end.0))
}

/// The editor's cursor as a character index, when it has one.
pub fn editor_cursor(ctx: &Context) -> Option<usize> {
    let state = TextEditState::load(ctx, editor_id())?;
    Some(state.cursor.char_range()?.primary.index.0)
}

/// Put the editor's cursor at character `index` (no selection).
pub fn set_editor_cursor(ctx: &Context, index: usize) {
    if let Some(mut state) = TextEditState::load(ctx, editor_id()) {
        state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(index))));
        state.store(ctx, editor_id());
    }
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
