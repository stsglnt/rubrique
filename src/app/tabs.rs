//! Tabs: every file opened from the explorer gets a tab and stays open,
//! unsaved edits included, until its tab is closed.
//!
//! The active tab's file lives in `RubriqueApp::open_file`, so the editor, the
//! preview and the AI code work on it as before. The other tabs wait in
//! `RubriqueApp::tabs`; the active tab's own slot there holds `OpenFile::Nothing`.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, CornerRadius, Sense, Ui, pos2, vec2};

use super::{GuardedAction, OpenFile, RubriqueApp};
use crate::document::preview_selection;
use crate::document::view::{restore_editor_state, take_editor_state};
use crate::explorer::paths::{absolute_path_text, name_text};
use crate::icons::{Icon, small_icon_button};
use crate::media::image_uri;
use crate::theme::style::page_fill;

/// Height of the tab strip.
const TAB_HEIGHT: f32 = 30.0;
/// Space left and right of a tab's name.
const TAB_PADDING: f32 = 12.0;
/// Room for the close cross (or the unsaved dot) after the name.
const CLOSE_SIZE: f32 = 18.0;
/// Widest a tab gets; longer names are cut with "…".
const TAB_MAX_WIDTH: f32 = 220.0;
/// Height of the accent line on top of the active tab.
const ACTIVE_LINE: f32 = 2.0;
/// Radius of the unsaved-changes dot.
const DIRTY_DOT: f32 = 3.5;

/// Something the user did on the tab strip.
#[derive(Clone, Copy, Debug)]
enum TabAction {
    Switch(usize),
    Close(usize),
    CloseOthers(usize),
    CloseToTheRight(usize),
    CopyPath(usize),
}

impl RubriqueApp {
    /// The file in tab `index`.
    fn tab(&self, index: usize) -> &OpenFile {
        if index == self.active_tab { &self.open_file } else { &self.tabs[index] }
    }

    /// The tab showing `path`, if it is open.
    fn find_tab(&self, path: &Path) -> Option<usize> {
        (0..self.tabs.len()).find(|&index| self.tab(index).path() == Some(path))
    }

    /// Every open file, the active one first (the active tab's empty slot
    /// in `tabs` is included, which is harmless for lookups and renames).
    pub(super) fn all_tabs_mut(&mut self) -> impl Iterator<Item = &mut OpenFile> {
        std::iter::once(&mut self.open_file).chain(self.tabs.iter_mut())
    }

    /// Paths of the open tabs, left to right (for the settings file).
    pub(super) fn tab_paths(&self) -> Vec<PathBuf> {
        (0..self.tabs.len()).filter_map(|index| self.tab(index).path().map(Path::to_path_buf)).collect()
    }

    /// The first tab with unsaved changes whose path passes `filter`,
    /// trying the active tab first.
    pub(super) fn dirty_tab(&self, filter: impl Fn(&Path) -> bool) -> Option<usize> {
        let dirty = |index: usize| {
            let tab = self.tab(index);
            tab.is_dirty() && tab.path().is_some_and(&filter)
        };
        if !self.tabs.is_empty() && dirty(self.active_tab) {
            return Some(self.active_tab);
        }
        (0..self.tabs.len()).find(|&index| dirty(index))
    }

    /// Show `path`: switch to its tab when it is open, otherwise open it in
    /// a new tab right after the active one.
    pub(super) fn open_in_tab(&mut self, path: PathBuf) {
        if let Some(index) = self.find_tab(&path) {
            self.switch_to_tab(index);
            return;
        }
        let Some(file) = self.load_file(path) else {
            return;
        };
        if self.tabs.is_empty() {
            self.tabs.push(OpenFile::Nothing);
            self.active_tab = 0;
            self.open_file = file;
            self.enter_tab();
        } else {
            let index = self.active_tab + 1;
            self.tabs.insert(index, file);
            self.switch_to_tab(index);
        }
    }

    /// Make tab `index` the one on screen.
    pub(super) fn switch_to_tab(&mut self, index: usize) {
        if index >= self.tabs.len() || index == self.active_tab {
            return;
        }
        self.leave_tab();
        std::mem::swap(&mut self.open_file, &mut self.tabs[self.active_tab]);
        std::mem::swap(&mut self.open_file, &mut self.tabs[index]);
        self.active_tab = index;
        self.enter_tab();
    }

    /// The tab after (or, with `forward` false, before) the active one, wrapping around.
    fn cycle_tab(&mut self, forward: bool) {
        let count = self.tabs.len();
        if count > 1 {
            let next = if forward { (self.active_tab + 1) % count } else { (self.active_tab + count - 1) % count };
            self.switch_to_tab(next);
        }
    }

    /// Put away what belongs to the tab on screen before another one shows:
    /// a block being edited is applied, the editor's undo history is kept
    /// with the tab, and screens tied to this file (merge, note, Ask bar) close.
    fn leave_tab(&mut self) {
        self.finish_block_edit();
        self.merging = None;
        self.note_draft = None;
        self.stats = None;
        self.ask.close_bar();
        preview_selection::clear(&self.repaint);
        let state = take_editor_state(&self.repaint);
        if let OpenFile::Text { editor, .. } = &mut self.open_file {
            *editor = state;
        }
    }

    /// Set up the tab that just came on screen, and look for changes made
    /// to its file while it was hidden.
    fn enter_tab(&mut self) {
        let state = match &mut self.open_file {
            OpenFile::Text { editor, .. } => editor.take(),
            _ => None,
        };
        restore_editor_state(&self.repaint, state);
        self.settings.last_file = self.open_file.path().map(Path::to_path_buf);
        self.check_disk();
    }

    /// Close the active tab without asking; the next one to the right (or
    /// the last one) takes its place.
    pub(super) fn close_active_tab_now(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.leave_tab();
        let closed = std::mem::replace(&mut self.open_file, OpenFile::Nothing);
        self.forget_media(&closed);
        self.tabs.remove(self.active_tab);
        if self.tabs.is_empty() {
            self.active_tab = 0;
            self.settings.last_file = None;
            return;
        }
        self.active_tab = self.active_tab.min(self.tabs.len() - 1);
        self.open_file = std::mem::replace(&mut self.tabs[self.active_tab], OpenFile::Nothing);
        self.enter_tab();
    }

    /// Close tab `index`, asking first when it has unsaved changes.
    fn request_close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        if index != self.active_tab && !self.tab(index).is_dirty() {
            self.close_hidden_tab(index);
            return;
        }
        self.switch_to_tab(index);
        self.request(GuardedAction::CloseTab);
    }

    /// Close a tab that is not on screen (and has nothing unsaved).
    fn close_hidden_tab(&mut self, index: usize) {
        let closed = self.tabs.remove(index);
        self.forget_media(&closed);
        if index < self.active_tab {
            self.active_tab -= 1;
        }
    }

    /// Close every tab whose file is inside `folder`, without asking.
    pub(super) fn close_tabs_inside(&mut self, folder: &Path) {
        for index in (0..self.tabs.len()).rev() {
            if index != self.active_tab && self.tabs[index].path().is_some_and(|path| path.starts_with(folder)) {
                self.close_hidden_tab(index);
            }
        }
        if self.open_file.path().is_some_and(|path| path.starts_with(folder)) {
            self.close_active_tab_now();
        }
    }

    /// Close the tabs in `indexes` that have no unsaved changes; say how
    /// many were kept open because they do.
    fn close_clean_tabs(&mut self, indexes: Vec<usize>) {
        let mut kept = 0;
        for index in indexes.into_iter().rev() {
            if self.tab(index).is_dirty() {
                kept += 1;
            } else if index == self.active_tab {
                self.close_active_tab_now();
            } else {
                self.close_hidden_tab(index);
            }
        }
        if kept > 0 {
            let noun = if kept == 1 { "tab has" } else { "tabs have" };
            self.show_status(format!("{kept} {noun} unsaved changes and stayed open"));
        }
    }

    /// Free an image's decoded pixels when its tab closes.
    fn forget_media(&self, file: &OpenFile) {
        if let OpenFile::Media { path, .. } = file {
            self.repaint.forget_image(&image_uri(path));
        }
    }

    /// Tab shortcuts: Cmd+W closes, Ctrl+Tab and Ctrl+Shift+Tab move between tabs.
    pub(super) fn handle_tab_shortcuts(&mut self, ctx: &egui::Context) {
        // Shift first: egui lets Ctrl+Shift+Tab also match Ctrl+Tab.
        if ctx.input_mut(|input| input.consume_shortcut(&super::PREVIOUS_TAB)) {
            self.cycle_tab(false);
        }
        if ctx.input_mut(|input| input.consume_shortcut(&super::NEXT_TAB)) {
            self.cycle_tab(true);
        }
        if ctx.input_mut(|input| input.consume_shortcut(&super::CLOSE_TAB)) && !self.tabs.is_empty() {
            self.request_close_tab(self.active_tab);
        }
    }

    /// The row of tabs above the page.
    pub(super) fn tab_bar(&mut self, ui: &mut Ui) {
        let mut action = None;
        let strip_fill = ui.visuals().panel_fill;
        let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), TAB_HEIGHT), Sense::hover());
        ui.painter().rect_filled(strip, 0, strip_fill);
        let line = ui.visuals().widgets.noninteractive.bg_stroke;
        ui.painter().hline(strip.x_range(), strip.bottom() - line.width / 2.0, line);
        ui.scope_builder(egui::UiBuilder::new().max_rect(strip), |ui| {
            egui::ScrollArea::horizontal()
                .id_salt("tab-strip")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for index in 0..self.tabs.len() {
                            let file = self.tab(index);
                            let tab = TabView {
                                name: file.path().map(name_text).unwrap_or_default(),
                                path: file.path().map(absolute_path_text).unwrap_or_default(),
                                dirty: file.is_dirty(),
                                active: index == self.active_tab,
                            };
                            if let Some(found) = show_tab(ui, &tab, index, self.tabs.len()) {
                                action = Some(found);
                            }
                        }
                    });
                });
        });
        match action {
            Some(TabAction::Switch(index)) => self.switch_to_tab(index),
            Some(TabAction::Close(index)) => self.request_close_tab(index),
            Some(TabAction::CloseOthers(index)) => {
                self.switch_to_tab(index);
                self.close_clean_tabs((0..self.tabs.len()).filter(|&other| other != index).collect());
            }
            Some(TabAction::CloseToTheRight(index)) => {
                self.close_clean_tabs((index + 1..self.tabs.len()).collect());
            }
            Some(TabAction::CopyPath(index)) => {
                if let Some(path) = self.tab(index).path() {
                    let text = absolute_path_text(path);
                    self.copy_to_clipboard(ui.ctx(), text);
                }
            }
            None => {}
        }
    }
}

/// What one tab shows.
struct TabView {
    name: String,
    /// Full path, for the hover tip.
    path: String,
    dirty: bool,
    active: bool,
}

/// One tab: the file name, then a close cross (a dot instead while the file
/// has unsaved changes and the pointer is elsewhere). A click switches, a
/// middle click or the cross closes, a right click opens a menu.
fn show_tab(ui: &mut Ui, tab: &TabView, index: usize, count: usize) -> Option<TabAction> {
    let visuals = ui.visuals().clone();
    let font = egui::TextStyle::Body.resolve(ui.style());
    let text_color = if tab.active { visuals.strong_text_color() } else { visuals.weak_text_color() };
    let max_text = TAB_MAX_WIDTH - TAB_PADDING * 2.0 - CLOSE_SIZE;
    let mut job = egui::text::LayoutJob::simple_singleline(tab.name.clone(), font, text_color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_text);
    let galley = ui.painter().layout_job(job);
    let width = TAB_PADDING * 2.0 + galley.size().x + CLOSE_SIZE;
    let (rect, response) = ui.allocate_exact_size(vec2(width, TAB_HEIGHT), Sense::click());
    let hovered = response.hovered();

    let fill = if tab.active {
        page_fill(&visuals)
    } else if hovered {
        visuals.widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, fill);
    if tab.active {
        let top = egui::Rect::from_min_size(rect.min, vec2(rect.width(), ACTIVE_LINE));
        painter.rect_filled(top, CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, visuals.hyperlink_color);
    }
    // Thin line between tabs that are not active.
    if !tab.active {
        let x = rect.right() - 0.5;
        painter.vline(x, rect.y_range().shrink(8.0), visuals.widgets.noninteractive.bg_stroke);
    }
    let text_pos = pos2(rect.left() + TAB_PADDING, rect.center().y - galley.size().y / 2.0);
    painter.galley(text_pos, galley, text_color);

    let close_rect = egui::Rect::from_center_size(
        pos2(rect.right() - TAB_PADDING / 2.0 - CLOSE_SIZE / 2.0, rect.center().y),
        vec2(CLOSE_SIZE, CLOSE_SIZE),
    );
    let mut action = None;
    if tab.dirty && !hovered {
        ui.painter().circle_filled(close_rect.center(), DIRTY_DOT, visuals.text_color());
    } else if hovered || tab.active {
        let close = ui.put(close_rect, |ui: &mut Ui| {
            small_icon_button(ui, Icon::Close, CLOSE_SIZE, "Close tab", "Close (Cmd+W)")
        });
        if close.clicked() {
            action = Some(TabAction::Close(index));
        }
    }

    let tip = if tab.dirty { format!("{}\nUnsaved changes", tab.path) } else { tab.path.clone() };
    let response = response.on_hover_text(tip);
    if action.is_none() && response.clicked() {
        action = Some(TabAction::Switch(index));
    }
    if response.middle_clicked() {
        action = Some(TabAction::Close(index));
    }
    response.context_menu(|ui| {
        if ui.button("Close").clicked() {
            action = Some(TabAction::Close(index));
        }
        if ui.add_enabled(count > 1, egui::Button::new("Close others")).clicked() {
            action = Some(TabAction::CloseOthers(index));
        }
        if ui.add_enabled(index + 1 < count, egui::Button::new("Close to the right")).clicked() {
            action = Some(TabAction::CloseToTheRight(index));
        }
        ui.separator();
        if ui.button("Copy path").clicked() {
            action = Some(TabAction::CopyPath(index));
        }
    });
    action
}
