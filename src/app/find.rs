//! Find (Cmd+F): a bar at the top right of the page that searches the open
//! file, or every text file under the explorer's folders.

use std::ops::Range;
use std::path::PathBuf;

use eframe::egui::{self, Context, Key, Modifiers, RichText, Ui, pos2};

use super::{OpenFile, RubriqueApp};
use crate::document::view::{FindHighlight, focus_editor, select_editor_range};
use crate::explorer::paths::name_text;
use crate::find::{FileHit, HitKind, SearchResults, char_index, find_all, search_files};

const FIND_WIDTH: f32 = 440.0;
const FIND_RADIUS: u8 = 8;
/// Space between the bar and the page's top and right edges.
const FIND_MARGIN: f32 = 12.0;
/// Width of the match count ("12 of 40").
const COUNT_WIDTH: f32 = 72.0;
/// Tallest the all-files results list gets before it scrolls.
const RESULTS_HEIGHT: f32 = 320.0;
/// Hits an all-files search stops at.
const RESULT_LIMIT: usize = 300;
const SMALL_TEXT: f32 = 11.5;

/// The open find bar.
#[derive(Debug, Default)]
pub(super) struct FindBar {
    pub(super) query: String,
    /// Search every file under the explorer's folders instead of the open one.
    all_files: bool,
    /// Give the field focus on the next frame.
    focus: bool,
    /// Matches in the open file, in order.
    matches: Vec<Range<usize>>,
    /// Index into `matches` of the one being looked at.
    current: Option<usize>,
    /// The query, file and text revision `matches` were computed for.
    computed_for: Option<(String, PathBuf, u64)>,
    /// Scroll the editor to the current match this frame.
    scroll: bool,
    /// After opening a file from the results: make this match the current one.
    pending: Option<Range<usize>>,
    /// All-files results, and the query they are for.
    results: Option<(String, SearchResults)>,
}

impl RubriqueApp {
    /// Open the bar (or put the pointer back in its field). Find works on the
    /// source, so a Markdown preview switches to it.
    pub(super) fn open_find(&mut self) {
        self.finish_block_edit();
        if self.showing_preview() {
            self.toggle_preview();
        }
        let bar = self.find.get_or_insert_with(FindBar::default);
        bar.focus = true;
    }

    pub(super) fn close_find(&mut self, ctx: &Context) {
        if self.find.take().is_some() && matches!(self.open_file, OpenFile::Text { .. }) {
            focus_editor(ctx);
        }
    }

    /// Cmd+G / Cmd+Shift+G while the bar is open.
    pub(super) fn find_shortcuts(&mut self, ctx: &Context) {
        if self.find.is_none() {
            return;
        }
        if ctx.input_mut(|input| input.consume_shortcut(&super::FIND_PREVIOUS)) {
            self.step_match(ctx, -1);
        } else if ctx.input_mut(|input| input.consume_shortcut(&super::FIND_NEXT)) {
            self.step_match(ctx, 1);
        }
    }

    /// Bring the open file's matches up to date with its text and the query.
    /// Runs before the editor is drawn, so the highlight is current.
    pub(super) fn prepare_find(&mut self, ctx: &Context) {
        let Some(bar) = &mut self.find else {
            return;
        };
        let OpenFile::Text { document, .. } = &self.open_file else {
            bar.matches.clear();
            bar.current = None;
            bar.computed_for = None;
            return;
        };
        let key = (bar.query.clone(), document.path.clone(), document.revision());
        if bar.computed_for.as_ref() != Some(&key) {
            bar.matches = find_all(&document.text, &bar.query);
            bar.computed_for = Some(key);
            bar.current = match (bar.pending.take(), bar.current) {
                (Some(range), _) => bar.matches.iter().position(|found| *found == range),
                (None, Some(current)) => Some(current.min(bar.matches.len().saturating_sub(1))),
                (None, None) => None,
            }
            .filter(|_| !bar.matches.is_empty())
            .or_else(|| (!bar.matches.is_empty()).then_some(0));
            if bar.current.is_some() {
                bar.scroll = true;
                self.select_current_match(ctx);
            }
        }
    }

    fn step_match(&mut self, ctx: &Context, step: isize) {
        let Some(bar) = &mut self.find else {
            return;
        };
        if bar.matches.is_empty() {
            return;
        }
        let count = bar.matches.len();
        let current = bar.current.unwrap_or(0);
        bar.current = Some((current + count).wrapping_add_signed(step) % count);
        bar.scroll = true;
        self.select_current_match(ctx);
    }

    /// Select the current match in the editor, so closing the bar leaves the
    /// cursor on it.
    fn select_current_match(&self, ctx: &Context) {
        let (Some(bar), OpenFile::Text { document, .. }) = (&self.find, &self.open_file) else {
            return;
        };
        let Some(range) = bar.current.and_then(|index| bar.matches.get(index)) else {
            return;
        };
        select_editor_range(ctx, char_index(&document.text, range.start), char_index(&document.text, range.end));
    }

    /// Open a hit's file in a tab, on its source, and go to the match.
    fn open_hit(&mut self, hit: &FileHit) {
        self.open_in_tab(hit.path.clone());
        if let OpenFile::Text { source, .. } = &mut self.open_file {
            *source = true;
        }
        if let Some(bar) = &mut self.find {
            bar.all_files = false;
            bar.computed_for = None;
            bar.pending = match &hit.kind {
                HitKind::Text { range, .. } => Some(range.clone()),
                HitKind::Name => None,
            };
        }
    }

    /// Draw the bar over the top right of the page.
    pub(super) fn find_bar(&mut self, ctx: &Context) {
        let Some(page) = self.ask.page_rect else {
            return;
        };
        if self.find.is_none() {
            return;
        }
        let at = pos2(page.right() - FIND_WIDTH - FIND_MARGIN, page.top() + FIND_MARGIN);
        let mut event = None;
        egui::Area::new(egui::Id::unique("find-bar")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
            let fill = ui.visuals().extreme_bg_color;
            egui::Frame::popup(ui.style()).fill(fill).corner_radius(FIND_RADIUS).inner_margin(10).show(ui, |ui| {
                ui.set_width(FIND_WIDTH);
                ui.spacing_mut().item_spacing.y = 6.0;
                let Some(bar) = &mut self.find else {
                    return;
                };
                event = find_row(ui, bar);
                if bar.all_files {
                    let roots: Vec<PathBuf> = self.roots.iter().map(|root| root.path.clone()).collect();
                    if let Some(hit) = results_list(ui, bar, &roots) {
                        event = Some(FindEvent::Open(hit));
                    }
                }
            });
        });
        match event {
            Some(FindEvent::Next) => self.step_match(ctx, 1),
            Some(FindEvent::Previous) => self.step_match(ctx, -1),
            Some(FindEvent::Close) => self.close_find(ctx),
            Some(FindEvent::Open(hit)) => self.open_hit(&hit),
            None => {}
        }
    }
}

/// What the editor should tint this frame. A free function over the field,
/// so the app can borrow its open file at the same time.
pub(super) fn highlight(find: &mut Option<FindBar>) -> Option<FindHighlight<'_>> {
    let bar = find.as_mut()?;
    if bar.all_files || bar.matches.is_empty() {
        return None;
    }
    let scroll_to_current = std::mem::take(&mut bar.scroll);
    Some(FindHighlight { matches: &bar.matches, current: bar.current, scroll_to_current })
}

/// What the bar asked for this frame.
enum FindEvent {
    Next,
    Previous,
    Close,
    Open(FileHit),
}

/// The field, the count, Previous / Next, the All files switch and the cross.
fn find_row(ui: &mut Ui, bar: &mut FindBar) -> Option<FindEvent> {
    let mut event = None;
    ui.horizontal(|ui| {
        let hint = if bar.all_files { "Find in all files" } else { "Find in this file" };
        let field_width = ui.available_width() - COUNT_WIDTH - 2.0 * 64.0 - 24.0 - 5.0 * ui.spacing().item_spacing.x;
        let field = ui.add(egui::TextEdit::singleline(&mut bar.query).hint_text(hint).desired_width(field_width));
        if bar.focus {
            field.request_focus();
            bar.focus = false;
        }
        if field.lost_focus() && ui.input(|input| input.key_pressed(Key::Escape)) {
            event = Some(FindEvent::Close);
        } else if field.has_focus() && ui.input_mut(|input| input.consume_key(Modifiers::SHIFT, Key::Enter)) {
            event = Some(FindEvent::Previous);
        } else if field.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
            event = Some(FindEvent::Next);
            bar.focus = true;
        }
        let count = if bar.all_files {
            String::new()
        } else {
            match (bar.current, bar.matches.len()) {
                (_, 0) if bar.query.is_empty() => String::new(),
                (_, 0) => "No matches".to_string(),
                (Some(current), total) => format!("{} of {total}", current + 1),
                (None, total) => format!("{total}"),
            }
        };
        ui.add_sized([COUNT_WIDTH, ui.spacing().interact_size.y], egui::Label::new(RichText::new(count).weak()));
        let has_matches = !bar.all_files && !bar.matches.is_empty();
        if ui
            .add_enabled(has_matches, egui::Button::new("Previous"))
            .on_hover_text("Shift+Enter, Cmd+Shift+G")
            .clicked()
        {
            event = Some(FindEvent::Previous);
        }
        if ui.add_enabled(has_matches, egui::Button::new("Next")).on_hover_text("Enter, Cmd+G").clicked() {
            event = Some(FindEvent::Next);
        }
        if ui.small_button("×").on_hover_text("Close (Esc)").clicked() {
            event = Some(FindEvent::Close);
        }
    });
    ui.horizontal(|ui| {
        if ui
            .checkbox(&mut bar.all_files, "All files")
            .on_hover_text("Search every file in the explorer's folders")
            .changed()
        {
            bar.focus = true;
        }
    });
    event
}

/// The all-files results under the field; a click opens the hit.
fn results_list(ui: &mut Ui, bar: &mut FindBar, roots: &[PathBuf]) -> Option<FileHit> {
    if bar.results.as_ref().is_none_or(|(query, _)| *query != bar.query) {
        let results = if bar.query.trim().is_empty() {
            SearchResults::default()
        } else {
            search_files(roots, &bar.query, RESULT_LIMIT)
        };
        bar.results = Some((bar.query.clone(), results));
    }
    let Some((_, results)) = &bar.results else {
        return None;
    };
    if bar.query.trim().is_empty() {
        return None;
    }
    ui.separator();
    let summary = if results.hits.is_empty() {
        format!("No matches in {} files", results.files_searched)
    } else {
        format!(
            "{}{} matches in {} files",
            if results.truncated { "First " } else { "" },
            results.hits.len(),
            results.file_count()
        )
    };
    ui.label(RichText::new(summary).size(SMALL_TEXT).weak());
    let mut clicked = None;
    egui::ScrollArea::vertical().id_salt("find-results").max_height(RESULTS_HEIGHT).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for hit in &results.hits {
            if result_row(ui, hit).clicked() {
                clicked = Some(hit.clone());
            }
        }
    });
    clicked
}

/// One hit: the file's path under its folder, then the line and its text.
fn result_row(ui: &mut Ui, hit: &FileHit) -> egui::Response {
    let relative = hit.path.strip_prefix(&hit.root).unwrap_or(&hit.path);
    let place = match &hit.kind {
        HitKind::Name => format!("{}  (file name)", relative.display()),
        HitKind::Text { line, .. } => format!("{}:{line}", relative.display()),
    };
    let text = match &hit.kind {
        HitKind::Name => name_text(&hit.path),
        HitKind::Text { excerpt, .. } => excerpt.clone(),
    };
    let label = format!("{place}\n{text}");
    ui.add(egui::Button::new(RichText::new(label).size(SMALL_TEXT)).frame(false).wrap())
        .on_hover_text(hit.path.display().to_string())
}
