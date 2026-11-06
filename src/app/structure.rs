//! Structure (Cmd+Shift+O): the open Markdown file's headings in the right
//! panel, where the notes are. A click on a heading jumps to it in the
//! preview or the editor; the section on screen is highlighted.

use eframe::egui::cache::{ComputerMut, FrameCache};
use eframe::egui::{self, Context, Key, KeyboardShortcut, Modifiers, RichText, Ui};

use super::{NOTES_PANEL_WIDTH, OpenFile, RubriqueApp, char_to_byte};
use crate::document::view::{editor_cursor, jump_editor_to, preview_top_offset, scroll_preview_to_block};
use crate::explorer::tree::is_markdown_path;
use crate::find::char_index;
use crate::markdown::blocks::parse_source_blocks;
use crate::markdown::outline::{Heading, current_heading, headings, top_level};

const TOGGLE_STRUCTURE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::O);
/// Indent per heading level below the top one.
const LEVEL_INDENT: f32 = 14.0;
const SMALL_TEXT: f32 = 11.5;

impl RubriqueApp {
    /// Show or hide the Structure panel on Cmd+Shift+O.
    pub(super) fn structure_shortcut(&mut self, ctx: &Context) {
        if ctx.input_mut(|input| input.consume_shortcut(&TOGGLE_STRUCTURE)) {
            self.structure_open = !self.structure_open;
        }
    }

    /// Status-bar toggle for the Structure panel, for Markdown files only.
    pub(super) fn structure_button(&mut self, ui: &mut Ui) {
        if !self.open_file.path().is_some_and(is_markdown_path) {
            return;
        }
        let toggle = ui
            .selectable_label(self.structure_open, "Structure")
            .on_hover_text("Headings of this file, click one to jump to it (Cmd+Shift+O)");
        if toggle.clicked() {
            self.structure_open = !self.structure_open;
        }
    }

    /// The right panel: Structure when it is open for a Markdown file,
    /// otherwise the notes when the preview has any. Both share one panel
    /// (and its width), so the page does not get squeezed twice.
    pub(super) fn right_panel(&mut self, ui: &mut Ui) {
        let shows_structure = self.shows_structure_panel();
        if !shows_structure && !self.shows_notes_panel() {
            return;
        }
        let frame = egui::Frame::side_top_panel(&ui.ctx().global_style()).inner_margin(0);
        egui::Panel::right("notes").resizable(true).default_size(NOTES_PANEL_WIDTH).frame(frame).show(ui, |ui| {
            if shows_structure {
                self.structure_panel(ui);
            } else {
                self.notes_panel(ui);
            }
        });
    }

    /// True when Structure is open and a Markdown file is shown (not merging).
    fn shows_structure_panel(&self) -> bool {
        self.structure_open
            && self.merging.is_none()
            && matches!(&self.open_file, OpenFile::Text { document, .. } if is_markdown_path(&document.path))
    }

    /// The headings, indented by level, the current section highlighted.
    fn structure_panel(&mut self, ui: &mut Ui) {
        let OpenFile::Text { document, .. } = &self.open_file else {
            return;
        };
        let found = cached_headings(ui.ctx(), &document.text);
        let current = self.current_section(ui.ctx(), &found);
        let notes_waiting = self.shows_notes_panel();
        let mut clicked = None;
        let mut hide = false;
        egui::Frame::new().inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Structure").strong().size(16.0));
                ui.label(RichText::new(heading_count(found.len())).weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Closing Structure brings the notes back, so say so.
                    let (label, tip) = if notes_waiting {
                        ("Notes", "Show the notes instead (Cmd+Shift+O)")
                    } else {
                        ("Hide", "Hide the structure (Cmd+Shift+O)")
                    };
                    hide = ui.small_button(label).on_hover_text(tip).clicked();
                });
            });
            ui.add_space(6.0);
            if found.is_empty() {
                let hint = "No headings yet. Lines that start with # become headings.";
                ui.label(RichText::new(hint).weak().size(SMALL_TEXT));
                return;
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                clicked = heading_list(ui, &found, current);
            });
        });
        if hide {
            self.structure_open = false;
        }
        if let Some(ordinal) = clicked {
            self.jump_to_heading(ui.ctx(), ordinal);
        }
    }

    /// Index of the heading whose section is on screen: the first block
    /// shown in the preview, or the cursor's line in the editor.
    fn current_section(&self, ctx: &Context, found: &[Heading]) -> Option<usize> {
        let OpenFile::Text { document, .. } = &self.open_file else {
            return None;
        };
        let offset = if self.showing_preview() {
            preview_top_offset(ctx, &document.path)?
        } else {
            char_to_byte(&document.text, editor_cursor(ctx)?)
        };
        current_heading(found, offset)
    }

    /// Jump to heading number `ordinal`: scroll the preview to its block,
    /// or put the editor's cursor on its line.
    fn jump_to_heading(&mut self, ctx: &Context, ordinal: usize) {
        // A block being edited changes the text (and maybe the block count)
        // when it is put back, so finish it first and look the heading up again.
        self.finish_block_edit();
        let showing_preview = self.showing_preview();
        let OpenFile::Text { document, .. } = &self.open_file else {
            return;
        };
        let Some(heading) = headings(&parse_source_blocks(&document.text)).into_iter().nth(ordinal) else {
            return;
        };
        if showing_preview {
            scroll_preview_to_block(ctx, heading.block_index);
        } else {
            jump_editor_to(ctx, char_index(&document.text, heading.start));
        }
    }
}

/// One clickable row per heading; returns the index of the one clicked.
fn heading_list(ui: &mut Ui, found: &[Heading], current: Option<usize>) -> Option<usize> {
    let top = top_level(found);
    let mut clicked = None;
    for (ordinal, heading) in found.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.add_space(LEVEL_INDENT * f32::from(heading.level.saturating_sub(top)));
            let mut text = RichText::new(&heading.text);
            if heading.level == top {
                text = text.strong();
            }
            let row = ui.add(egui::Button::selectable(current == Some(ordinal), text).truncate());
            if row.on_hover_text(&heading.text).clicked() {
                clicked = Some(ordinal);
            }
        });
    }
    clicked
}

/// "1 heading", "3 headings".
fn heading_count(count: usize) -> String {
    let noun = if count == 1 { "heading" } else { "headings" };
    format!("{count} {noun}")
}

/// The headings of `text`, parsed again only when the text changes: the
/// panel is drawn every frame, and the editor drops the preview's parsed
/// blocks on every edit.
fn cached_headings(ctx: &Context, text: &str) -> Vec<Heading> {
    ctx.memory_mut(|memory| memory.caches.cache::<HeadingCache>().get(text).clone())
}

type HeadingCache = FrameCache<Vec<Heading>, HeadingFinder>;

/// Finds headings for the cache.
#[derive(Default)]
struct HeadingFinder;

impl ComputerMut<&str, Vec<Heading>> for HeadingFinder {
    fn compute(&mut self, text: &str) -> Vec<Heading> {
        headings(&parse_source_blocks(text))
    }
}
