//! Draw the editor and the preview, and apply editor keyboard shortcuts.

use std::ops::Range;
use std::path::{Path, PathBuf};

use eframe::egui::text::{CCursor, CCursorRange};
use eframe::egui::text_edit::{TextEditOutput, TextEditState};
use eframe::egui::{self, Align, Align2, Color32, Context, FontId, Id, Key, Modifiers, TextBuffer, Ui, pos2};

use super::highlighting::{HighlightStyle, Syntax, highlighted};
use super::line_ops::{Direction, Selection, duplicate_lines, insert_text, move_lines};
use super::text_file::Document;
use super::text_stats::{TextStats, text_stats};
use crate::ai::agents::Agent;
use crate::ai::mention_list;
use crate::markdown::blocks::SourceBlock;
use crate::markdown::render::{
    CodeAction, render_top_block, set_image_folder, take_code_action, take_play_request, top_gap,
};
use crate::voice::view::{MemoAction, take_memo_action};

/// Widest the preview text gets, for comfortable reading.
const PREVIEW_MAX_WIDTH: f32 = 760.0;
/// Space (x, y) between the page edge and the editor text.
const EDITOR_MARGIN: (i8, i8) = (16, 12);
/// Space (x, y) between the page edge and the preview text.
const PREVIEW_MARGIN: (i8, i8) = (32, 24);
/// Space between the line numbers and the editor text.
const GUTTER_GAP: f32 = 12.0;
/// Widest the line-number gutter gets (an egui margin is at most 127).
const GUTTER_MAX_WIDTH: f32 = 100.0;
/// Opacity of the warning color behind a find match, and of the accent
/// behind the current one.
const MATCH_TINT: f32 = 0.35;
const CURRENT_MATCH_TINT: f32 = 0.55;

/// Find matches to show in the editor.
#[derive(Clone, Copy, Debug)]
pub struct FindHighlight<'a> {
    /// Byte ranges, in order.
    pub matches: &'a [Range<usize>],
    /// Index into `matches` of the one being looked at.
    pub current: Option<usize>,
    /// Scroll the current match into view this frame.
    pub scroll_to_current: bool,
}

/// The editor widget's id, shared by drawing, focus and the line shortcuts.
fn editor_id() -> Id {
    Id::unique("rubrique-editor")
}

/// The editor's cursor, selection and undo history for a tab that is not
/// shown. There is one editor widget, so each tab keeps its own copy.
#[derive(Clone)]
pub struct EditorState(Box<TextEditState>);

impl std::fmt::Debug for EditorState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("EditorState")
    }
}

/// Take the editor's state out of egui, for the tab being left.
pub fn take_editor_state(ctx: &Context) -> Option<EditorState> {
    let state = TextEditState::load(ctx, editor_id())?;
    ctx.data_mut(|data| data.remove::<TextEditState>(editor_id()));
    Some(EditorState(Box::new(state)))
}

/// Give the editor the state of the tab being shown; `None` starts fresh
/// (so Undo never reaches into another file).
pub fn restore_editor_state(ctx: &Context, state: Option<EditorState>) {
    match state {
        Some(EditorState(state)) => state.store(ctx, editor_id()),
        None => ctx.data_mut(|data| data.remove::<TextEditState>(editor_id())),
    }
}

/// Draw the text editor, coloring `syntax` when there is one, numbering
/// the lines when `line_numbers` is set, and tinting the `find` matches.
/// Typing `@ai-` suggests `agents`. Returns status-bar stats for the cursor.
pub fn show_editor(
    ui: &mut Ui,
    document: &mut Document,
    syntax: Option<Syntax>,
    line_numbers: bool,
    find: Option<FindHighlight<'_>>,
    agents: &[Agent],
) -> Option<TextStats> {
    let available = ui.available_size();
    let style = HighlightStyle::from_ui(ui);
    let gutter = line_numbers.then(|| Gutter::measure(ui, &document.text));
    let margin = egui::Margin {
        left: EDITOR_MARGIN.0 + gutter.as_ref().map_or(0, |gutter| gutter.width),
        right: EDITOR_MARGIN.0,
        top: EDITOR_MARGIN.1,
        bottom: EDITOR_MARGIN.1,
    };
    let list_key = mention_list::take_key(ui.ctx(), editor_id());
    let output = egui::ScrollArea::both()
        // One scroll position per file, so each tab keeps its place.
        .id_salt(("editor-scroll", &document.path))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let editor = egui::TextEdit::multiline(&mut document.text)
                .id(editor_id())
                .code_editor()
                // No border: the editor is the page itself.
                .frame(egui::Frame::NONE.inner_margin(margin))
                .lock_focus(true)
                .desired_width(f32::INFINITY)
                .min_size(available);
            let output = match syntax {
                Some(syntax) => {
                    let mut layouter = |ui: &Ui, buffer: &dyn TextBuffer, wrap_width: f32| {
                        let mut job = highlighted(ui.ctx(), buffer.as_str(), &style, syntax);
                        job.wrap.max_width = wrap_width;
                        ui.ctx().fonts_mut(|fonts| fonts.layout_job(job))
                    };
                    editor.layouter(&mut layouter).show(ui)
                }
                None => editor.show(ui),
            };
            if let Some(gutter) = &gutter {
                gutter.paint(ui, &output);
            }
            if let Some(find) = find {
                paint_matches(ui, &output, &document.text, find);
            }
            // A jump from the Structure panel: its line at the top of the page.
            if let Some(index) = take_editor_scroll(ui.ctx()) {
                let line = output.galley.pos_from_cursor(CCursor::new(index)).translate(output.galley_pos.to_vec2());
                ui.scroll_to_rect(line, Some(Align::TOP));
            }
            output
        })
        .inner;
    let mention_added = mention_list::show(ui, editor_id(), &output, &mut document.text, agents, list_key);
    if output.response.changed() || mention_added {
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

/// Tint the find matches behind the text, the current one in the accent,
/// and scroll to the current one when asked.
fn paint_matches(ui: &Ui, output: &TextEditOutput, text: &str, find: FindHighlight<'_>) {
    let visuals = ui.visuals();
    let tint = visuals.warn_fg_color.gamma_multiply(MATCH_TINT);
    let current_tint = visuals.hyperlink_color.gamma_multiply(CURRENT_MATCH_TINT);
    let clip = ui.clip_rect();
    // Byte to character conversion, walking forward through the sorted matches.
    let mut walked = (0_usize, 0_usize);
    let mut to_char = |byte: usize| {
        let (from_byte, from_char) = walked;
        let chars = from_char + text.get(from_byte..byte).map_or(0, |between| between.chars().count());
        walked = (byte, chars);
        chars
    };
    for (index, range) in find.matches.iter().enumerate() {
        let start = to_char(range.start);
        let end = to_char(range.end);
        let is_current = find.current == Some(index);
        let mut current_rect: Option<egui::Rect> = None;
        for rect in match_rects(&output.galley, start, end) {
            let rect = rect.translate(output.galley_pos.to_vec2());
            if is_current {
                current_rect = Some(current_rect.map_or(rect, |whole| whole.union(rect)));
            }
            if rect.intersects(clip) {
                ui.painter().rect_filled(rect, 2, if is_current { current_tint } else { tint });
            }
        }
        if is_current
            && find.scroll_to_current
            && let Some(rect) = current_rect
        {
            ui.scroll_to_rect(rect.expand(24.0), Some(Align::Center));
        }
    }
}

/// The rectangles a match covers, one per row it touches, in galley space.
fn match_rects(galley: &egui::Galley, start: usize, end: usize) -> Vec<egui::Rect> {
    let from = galley.pos_from_cursor(CCursor::new(start));
    let to = galley.pos_from_cursor(CCursor::new(end));
    if (from.min.y - to.min.y).abs() < 0.5 {
        return vec![egui::Rect::from_min_max(from.min, egui::pos2(to.max.x.max(from.min.x + 2.0), from.max.y))];
    }
    let mut rects = Vec::new();
    for row in &galley.rows {
        let rect = row.rect();
        if rect.bottom() <= from.min.y + 0.5 || rect.top() >= to.max.y - 0.5 {
            continue;
        }
        let left = if rect.top() <= from.min.y + 0.5 { from.min.x } else { rect.left() };
        let right = if rect.bottom() >= to.max.y - 0.5 { to.max.x } else { rect.right() };
        rects.push(egui::Rect::from_min_max(egui::pos2(left, rect.top()), egui::pos2(right, rect.bottom())));
    }
    rects
}

/// Line numbers to the left of the editor text, one per line of the file.
/// A wrapped line is numbered once, on its first row.
struct Gutter {
    font: FontId,
    color: Color32,
    /// Extra left margin for the numbers and the gap after them.
    width: i8,
}

impl Gutter {
    fn measure(ui: &Ui, text: &str) -> Gutter {
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        let digit = ui.ctx().fonts_mut(|fonts| fonts.glyph_width(&font, '0'));
        let width = gutter_width(digit, line_count(text));
        // Clamped to 0..=100 first, so the cast cannot truncate.
        #[expect(clippy::cast_possible_truncation, reason = "clamped to the range")]
        let width = width.clamp(0.0, GUTTER_MAX_WIDTH) as i8;
        Gutter { font, color: ui.visuals().weak_text_color(), width }
    }

    /// Paint the numbers next to the rows that start a line.
    fn paint(&self, ui: &Ui, output: &TextEditOutput) {
        let right = output.galley_pos.x - GUTTER_GAP;
        let clip = ui.clip_rect();
        let painter = ui.painter();
        let mut number = 1_usize;
        let mut starts_line = true;
        for row in &output.galley.rows {
            let top = output.galley_pos.y + row.pos.y;
            if starts_line && top <= clip.bottom() && top + row.row.size.y >= clip.top() {
                painter.text(pos2(right, top), Align2::RIGHT_TOP, number, self.font.clone(), self.color);
            }
            starts_line = row.ends_with_newline;
            if row.ends_with_newline {
                number += 1;
            }
        }
    }
}

/// Lines in `text`: one more than its line breaks.
fn line_count(text: &str) -> usize {
    text.bytes().filter(|byte| *byte == b'\n').count() + 1
}

/// Width of the numbers (at least two digits) and the gap after them.
fn gutter_width(digit_width: f32, lines: usize) -> f32 {
    let digits = lines.to_string().len().max(2);
    #[expect(clippy::cast_precision_loss, reason = "a handful of digits")]
    let numbers = digit_width * digits as f32;
    (numbers + GUTTER_GAP).ceil()
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
    /// Clicked the empty space under the last block: type a new block there.
    StartNewBlock,
    /// Done with the block being edited (Esc, Cmd+Enter, Done, or a click elsewhere).
    FinishEdit,
    /// Copy or Prettify on a code block inside top-level block `index`.
    Code { index: usize, action: CodeAction },
    /// Play on a video in the preview.
    PlayVideo(PathBuf),
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
/// Least height of the clickable space under the last block.
const NEW_BLOCK_SPACE: f32 = 48.0;
/// Width of the outline around the block being edited.
const EDIT_OUTLINE: f32 = 1.5;
/// Space between a block and its hover / edit background.
const BLOCK_PAD: i8 = 6;

/// Draw the rendered Markdown, one clickable block at a time; the block in
/// `editing` shows as a Markdown text box instead, where typing `@ai-`
/// suggests `agents`.
pub fn show_preview(
    ui: &mut Ui,
    path: &Path,
    blocks: &[SourceBlock],
    mut editing: Option<&mut BlockEdit>,
    agents: &[Agent],
) -> PreviewOutput {
    let mut event = None;
    let mut hovered = None;
    let scroll_target = take_preview_scroll(ui.ctx());
    // Source offset of the first block still on screen, for the Structure panel.
    let mut top_offset = None;
    // Relative image paths are next to the file.
    set_image_folder(ui.ctx(), path.parent().map(Path::to_path_buf));
    egui::ScrollArea::vertical().id_salt(("preview-scroll", path)).auto_shrink([false, false]).show(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(PREVIEW_MARGIN.0, PREVIEW_MARGIN.1)).show(ui, |ui| {
            ui.set_max_width(PREVIEW_MAX_WIDTH.min(ui.available_width()));
            for (index, source) in blocks.iter().enumerate() {
                if index > 0 {
                    ui.add_space(top_gap(&source.block) - f32::from(BLOCK_PAD) * 2.0);
                }
                let top = ui.cursor().top();
                let found = match editing.as_deref_mut() {
                    Some(edit) if edit.index == index => edit_box(ui, edit, agents),
                    _ => clickable_block(ui, index, source),
                };
                let span = egui::Rect::from_x_y_ranges(ui.max_rect().x_range(), top..=ui.cursor().top());
                if ui.rect_contains_pointer(span) {
                    hovered = Some(index);
                }
                if scroll_target == Some(index) {
                    ui.scroll_to_rect(span, Some(Align::TOP));
                }
                if top_offset.is_none() && span.bottom() > ui.clip_rect().top() {
                    top_offset = Some(source.range.start);
                }
                if event.is_none() {
                    event = found;
                }
            }
            // Under the last block: the new block being typed, or empty
            // space that starts one when clicked.
            let found = match editing {
                Some(edit) if edit.index >= blocks.len() => edit_box(ui, edit, agents),
                Some(_) => None,
                None => new_block_space(ui),
            };
            if event.is_none() {
                event = found;
            }
        });
    });
    ui.ctx().data_mut(|data| match top_offset {
        Some(offset) => {
            data.insert_temp(preview_top_id(path), offset);
        }
        None => data.remove::<usize>(preview_top_id(path)),
    });
    PreviewOutput { event, hovered }
}

/// Where a jump from the Structure panel waits for the next preview frame.
fn preview_scroll_id() -> Id {
    Id::unique("rubrique-preview-scroll-to-block")
}

/// Where the preview of `path` keeps the source offset of its first block on screen.
fn preview_top_id(path: &Path) -> Id {
    Id::unique(("rubrique-preview-top", path))
}

/// Scroll the preview so top-level block `index` is at the top, on the next frame.
pub fn scroll_preview_to_block(ctx: &Context, index: usize) {
    ctx.data_mut(|data| data.insert_temp(preview_scroll_id(), index));
}

/// The block the preview was asked to scroll to, if any (taken once).
fn take_preview_scroll(ctx: &Context) -> Option<usize> {
    ctx.data_mut(|data| {
        let index = data.get_temp::<usize>(preview_scroll_id());
        data.remove::<usize>(preview_scroll_id());
        index
    })
}

/// Byte offset in the source of the first block on screen in the last
/// drawn preview of `path`, so the Structure panel can mark that section.
pub fn preview_top_offset(ctx: &Context, path: &Path) -> Option<usize> {
    ctx.data(|data| data.get_temp::<usize>(preview_top_id(path)))
}

/// The rest of the page under the last block. A click there starts a new
/// block at the end of the file.
fn new_block_space(ui: &mut Ui) -> Option<PreviewEvent> {
    let size = egui::vec2(ui.available_width(), ui.available_height().max(NEW_BLOCK_SPACE));
    let (_, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::Text);
    response.clicked().then_some(PreviewEvent::StartNewBlock)
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
    if let Some(path) = take_play_request(ui.ctx()) {
        return Some(PreviewEvent::PlayVideo(path));
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
fn edit_box(ui: &mut Ui, edit: &mut BlockEdit, agents: &[Agent]) -> Option<PreviewEvent> {
    let visuals = ui.visuals().clone();
    let mut event = None;
    let frame = egui::Frame::new()
        .fill(visuals.text_edit_bg_color())
        .stroke(egui::Stroke::new(EDIT_OUTLINE, visuals.hyperlink_color))
        .corner_radius(8)
        .inner_margin(BLOCK_PAD + 2);
    let response = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        let title = if edit.range.is_empty() { "New block (Markdown)" } else { "Editing this block (Markdown)" };
        ui.label(egui::RichText::new(title).small().weak());
        let rows = edit.text.lines().count().max(1);
        let list_key = mention_list::take_key(ui.ctx(), block_editor_id());
        let output = egui::TextEdit::multiline(&mut edit.text)
            .id(block_editor_id())
            .code_editor()
            .frame(egui::Frame::NONE)
            .desired_width(f32::INFINITY)
            .desired_rows(rows)
            .show(ui);
        mention_list::show(ui, block_editor_id(), &output, &mut edit.text, agents, list_key);
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

/// Put the editor's cursor at character `index`, focus the editor, and
/// scroll that line to the top of the page on the next frame. Works before
/// the editor has drawn this file, unlike [`set_editor_cursor`].
pub fn jump_editor_to(ctx: &Context, index: usize) {
    let mut state = TextEditState::load(ctx, editor_id()).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(index))));
    state.store(ctx, editor_id());
    ctx.data_mut(|data| data.insert_temp(editor_scroll_id(), index));
    focus_editor(ctx);
}

/// Where a jump waits for the editor's next frame.
fn editor_scroll_id() -> Id {
    Id::unique("rubrique-editor-scroll-to-char")
}

/// The character the editor was asked to scroll to, if any (taken once).
fn take_editor_scroll(ctx: &Context) -> Option<usize> {
    ctx.data_mut(|data| {
        let index = data.get_temp::<usize>(editor_scroll_id());
        data.remove::<usize>(editor_scroll_id());
        index
    })
}

/// Select characters `start..end` in the editor, the cursor at the end.
pub fn select_editor_range(ctx: &Context, start: usize, end: usize) {
    if let Some(mut state) = TextEditState::load(ctx, editor_id()) {
        state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(start), CCursor::new(end))));
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
