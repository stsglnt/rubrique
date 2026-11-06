//! Structure: the open Markdown file's headings, one of the two views of
//! the side panel. A click on a heading jumps to it in the preview or the
//! editor; the section on screen is highlighted.

use eframe::egui::cache::{ComputerMut, FrameCache};
use eframe::egui::{self, Context, RichText, Ui};

use super::{OpenFile, RubriqueApp, char_to_byte};
use crate::document::view::{editor_cursor, jump_editor_to, preview_top_offset, scroll_preview_to_block};
use crate::find::char_index;
use crate::markdown::blocks::parse_source_blocks;
use crate::markdown::outline::{Heading, current_heading, headings, top_level};

/// Indent per heading level below the top one.
const LEVEL_INDENT: f32 = 14.0;
const SMALL_TEXT: f32 = 11.5;

impl RubriqueApp {
    /// The headings, indented by level, the current section highlighted.
    pub(super) fn structure_panel(&mut self, ui: &mut Ui) {
        let OpenFile::Text { document, .. } = &self.open_file else {
            return;
        };
        let found = cached_headings(ui.ctx(), &document.text);
        let current = self.current_section(ui.ctx(), &found);
        let mut clicked = None;
        egui::Frame::new().inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            if found.is_empty() {
                let hint = "No headings yet. Lines that start with # become headings.";
                ui.label(RichText::new(hint).weak().size(SMALL_TEXT));
                return;
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                clicked = heading_list(ui, &found, current);
            });
        });
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
