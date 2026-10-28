//! `RubriqueApp`: top-level state, panel layout, global shortcuts and dialogs.

use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui::{self, Context, Key, KeyboardShortcut, Modifiers, RichText, Ui, ViewportCommand};

use crate::document::merge::MergeSession;
use crate::document::merge_view::{MergeAction, show_merge};
use crate::document::text_file::{DiskState, Document, Opened, Unsupported};
use crate::document::text_stats::TextStats;
use crate::document::view::{focus_editor, handle_editor_shortcuts, show_editor, show_preview};
use crate::explorer::paths::{absolute_path_text, name_text};
use crate::explorer::tree::{FsNode, is_markdown_path};
use crate::explorer::view::{ExplorerAction, TreeContext, show_explorer};
use crate::icons::{Icon, icon_button};
use crate::markdown::blocks::{Block, parse_blocks};
use crate::markdown::html::{file_url, html_page, is_html_path, with_base};
use crate::scratch::{create_scratch, rename_target};
use crate::settings::view::{ScratchInfo, SettingsAction, show_settings};
use crate::settings::{MarkdownView, Settings, default_scratch_folder, themes_folder};
use crate::theme::catalog::Themes;
use crate::theme::style::{apply_theme, page_fill};

/// How long a status message ("Copied ...") stays visible.
const STATUS_MESSAGE_SECONDS: f64 = 4.0;
const EXPLORER_DEFAULT_WIDTH: f32 = 260.0;
/// Width of the settings panel.
const SETTINGS_PANEL_WIDTH: f32 = 320.0;
/// Space around the "cannot display this file" message.
const UNSUPPORTED_MARGIN: f32 = 16.0;
/// How often the open file is checked for changes made by other programs.
const DISK_CHECK_SECONDS: f64 = 2.0;
/// Width of the "Rename" dialog.
const RENAME_DIALOG_WIDTH: f32 = 320.0;
/// Inner margin (x, y) of the status bar.
const BAR_MARGIN: (i8, i8) = (10, 6);
/// Opacity of the warning color behind the "changed on disk" bar.
const NOTICE_TINT: f32 = 0.15;
/// Shown before the file name while it has unsaved changes.
const DIRTY_MARKER: &str = "• ";

const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
const TOGGLE_PREVIEW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::E);
const OPEN_IN_BROWSER: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::B);
const NEW_SCRATCH: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
const TOGGLE_SETTINGS: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Comma);
const COPY_OPEN_PATH: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::C);

/// What the right panel shows.
#[derive(Debug)]
enum OpenFile {
    Nothing,
    Text {
        document: Document,
        /// Parsed preview, rebuilt lazily after the text changes.
        preview: Option<Vec<Block>>,
        /// Shown when the file changed or vanished on disk.
        disk_notice: Option<DiskState>,
    },
    Unsupported {
        path: PathBuf,
        reason: Unsupported,
    },
}

impl OpenFile {
    /// Path of the shown file, if any.
    fn path(&self) -> Option<&Path> {
        match self {
            OpenFile::Nothing => None,
            OpenFile::Text { document, .. } => Some(&document.path),
            OpenFile::Unsupported { path, .. } => Some(path),
        }
    }

    /// True when a text file with unsaved changes is open.
    fn is_dirty(&self) -> bool {
        matches!(self, OpenFile::Text { document, .. } if document.is_dirty())
    }
}

/// An action that replaces or closes the open document, so it may need the
/// "unsaved changes" question first.
#[derive(Debug)]
enum GuardedAction {
    OpenFile(PathBuf),
    /// Take a root out of the explorer (closes the open file if it is inside).
    RemoveRoot(PathBuf),
    Quit,
}

/// Answer to the "unsaved changes" dialog.
#[derive(Clone, Copy, Debug)]
enum UnsavedChoice {
    Save,
    Discard,
    Cancel,
}

/// The whole application: owns all state and draws every panel each frame.
#[derive(Debug)]
pub struct RubriqueApp {
    settings: Settings,
    /// Explorer root folders, in the order they were added. Never empty.
    roots: Vec<FsNode>,
    open_file: OpenFile,
    /// Action waiting for the user to answer the unsaved-changes dialog.
    pending: Option<GuardedAction>,
    /// Set once the user agreed to quit, so the next close is not blocked.
    quit_confirmed: bool,
    /// Message for the status bar and the egui time (seconds) it was first
    /// shown; `NaN` until the next frame fills it in.
    status_message: Option<(String, f64)>,
    /// Cursor and count stats from the last editor frame.
    stats: Option<TextStats>,
    /// Last title sent to the window, to avoid resending it every frame.
    window_title: String,
    /// Built-in and user themes, read at start and on "Reload themes".
    themes: Themes,
    /// The settings panel is shown on the right.
    settings_open: bool,
    /// The "Rename" dialog, while it is open.
    renaming: Option<Renaming>,
    /// The merge screen for the open file, while it is open.
    merging: Option<MergeSession>,
    /// egui time of the last check for outside changes.
    last_disk_check: f64,
}

/// State of the "Rename" dialog.
#[derive(Debug)]
struct Renaming {
    path: PathBuf,
    /// The name being typed.
    name: String,
    /// Why the last attempt failed, shown under the field.
    error: Option<String>,
}

impl RubriqueApp {
    /// Start with `roots` in the explorer (at least one) and, optionally,
    /// `file` open, and install the saved theme.
    pub fn new(ctx: &Context, settings: Settings, roots: Vec<PathBuf>, file: Option<PathBuf>) -> Self {
        let themes = Themes::load(themes_folder().as_deref());
        apply_theme(ctx, &themes, &settings.theme);
        let mut app = RubriqueApp {
            settings,
            roots: roots.into_iter().map(FsNode::new_root).collect(),
            open_file: OpenFile::Nothing,
            pending: None,
            quit_confirmed: false,
            status_message: None,
            stats: None,
            window_title: String::new(),
            themes,
            settings_open: false,
            renaming: None,
            merging: None,
            last_disk_check: 0.0,
        };
        app.remember_roots();
        if let Some(file) = file {
            app.open_file_now(file);
        }
        app
    }

    // ---------- actions ----------

    /// Run an action now, or ask about unsaved changes first.
    fn request(&mut self, action: GuardedAction) {
        if self.open_file.is_dirty() {
            self.pending = Some(action);
        } else {
            self.perform(action);
        }
    }

    /// Run an action without asking (the user already answered, or nothing is unsaved).
    fn perform(&mut self, action: GuardedAction) {
        match action {
            GuardedAction::OpenFile(path) => self.open_file_now(path),
            GuardedAction::RemoveRoot(path) => self.remove_root_now(&path),
            GuardedAction::Quit => self.quit_confirmed = true,
        }
    }

    /// Show `path` in the right panel, or a status message when it cannot be read.
    fn open_file_now(&mut self, path: PathBuf) {
        self.stats = None;
        self.merging = None;
        match Document::open(&path) {
            Ok(Opened::Text(document)) => {
                self.open_file = OpenFile::Text { document, preview: None, disk_notice: None };
                self.settings.last_file = Some(path);
            }
            Ok(Opened::Unsupported(reason)) => {
                self.open_file = OpenFile::Unsupported { path, reason };
            }
            Err(error) => self.show_status(format!("Cannot open {}: {error}", path.display())),
        }
    }

    /// Add `path` as one more explorer root, unless it is already one.
    fn add_folder(&mut self, path: PathBuf) {
        if self.roots.iter().any(|root| root.path == path) {
            self.show_status(format!("Already in the explorer: {}", path.display()));
            return;
        }
        self.roots.push(FsNode::new_root(path));
        self.remember_roots();
    }

    /// Ask to remove a root; only an unsaved file inside it needs the question.
    fn request_remove_root(&mut self, path: PathBuf) {
        let file_inside = self.open_file.path().is_some_and(|file| file.starts_with(&path));
        if file_inside {
            self.request(GuardedAction::RemoveRoot(path));
        } else {
            self.remove_root_now(&path);
        }
    }

    /// Take a root out of the explorer, keeping at least one.
    fn remove_root_now(&mut self, path: &Path) {
        if self.roots.len() <= 1 {
            return;
        }
        self.roots.retain(|root| root.path != path);
        if self.open_file.path().is_some_and(|file| file.starts_with(path)) {
            self.close_file();
        }
        self.remember_roots();
    }

    /// Show nothing in the right panel.
    fn close_file(&mut self) {
        self.open_file = OpenFile::Nothing;
        self.stats = None;
        self.settings.last_file = None;
    }

    /// Keep the settings' folder list in step with the explorer.
    fn remember_roots(&mut self) {
        self.settings.folders = self.roots.iter().map(|root| root.path.clone()).collect();
    }

    /// Save the open document; returns false when saving failed.
    fn save(&mut self) -> bool {
        let OpenFile::Text { document, disk_notice, .. } = &mut self.open_file else {
            return true;
        };
        match document.save() {
            Ok(()) => {
                *disk_notice = None;
                let message = format!("Saved {}", document.path.display());
                self.show_status(message);
                true
            }
            Err(error) => {
                let message = format!("Save failed: {error}");
                self.show_status(message);
                false
            }
        }
    }

    /// Show a short-lived message in the status bar.
    fn show_status(&mut self, message: String) {
        // The time is filled in on the next frame (see `status_bar`).
        self.status_message = Some((message, f64::NAN));
    }

    /// Put `text` on the clipboard and confirm it in the status bar.
    fn copy_to_clipboard(&mut self, ctx: &Context, text: String) {
        self.show_status(format!("Copied {text}"));
        ctx.copy_text(text);
    }

    /// Switch a Markdown file between editor and preview (no-op for other files).
    fn toggle_preview(&mut self) {
        let is_markdown = self.open_file.path().is_some_and(is_markdown_path);
        if is_markdown {
            self.settings.markdown_view = self.settings.markdown_view.toggled();
        }
    }

    /// True when the right panel shows the rendered preview.
    fn showing_preview(&self) -> bool {
        self.settings.markdown_view == MarkdownView::Preview && self.open_file.path().is_some_and(is_markdown_path)
    }

    /// When the window regains focus, look for changes made by other programs.
    fn check_disk(&mut self) {
        let OpenFile::Text { document, disk_notice, .. } = &mut self.open_file else {
            return;
        };
        match document.disk_state() {
            DiskState::Unchanged => {}
            DiskState::Deleted => *disk_notice = Some(DiskState::Deleted),
            DiskState::Modified => {
                let Ok(disk_text) = document.read_disk_text() else {
                    *disk_notice = Some(DiskState::Modified);
                    return;
                };
                // Only the time changed, or the disk already holds my text: nothing to tell.
                if disk_text == document.base_text() || disk_text == document.text {
                    document.accept_disk_text(disk_text);
                    *disk_notice = None;
                } else if document.is_dirty() {
                    *disk_notice = Some(DiskState::Modified);
                } else {
                    // Nothing of ours to lose: pick up the new version and say so.
                    let path = document.path.clone();
                    self.open_file_now(path.clone());
                    self.show_status(format!("Reloaded {}: it was changed by another program", name_text(&path)));
                }
            }
        }
    }

    /// Check for outside changes every few seconds while a file is open.
    fn poll_disk(&mut self, ctx: &Context) {
        if !matches!(self.open_file, OpenFile::Text { .. }) || self.merging.is_some() {
            return;
        }
        let now = ctx.input(|input| input.time);
        if now - self.last_disk_check >= DISK_CHECK_SECONDS {
            self.last_disk_check = now;
            self.check_disk();
        }
        ctx.request_repaint_after(Duration::from_secs_f64(DISK_CHECK_SECONDS));
    }

    // ---------- per-frame input ----------

    /// React to focus changes and to the window's close button.
    fn handle_window_events(&mut self, ctx: &Context) {
        let regained_focus =
            ctx.input(|input| input.events.iter().any(|event| matches!(event, egui::Event::WindowFocused(true))));
        if regained_focus {
            self.check_disk();
        }

        if ctx.input(|input| input.viewport().close_requested()) {
            if self.quit_confirmed || !self.open_file.is_dirty() {
                self.save_settings(ctx);
            } else {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.pending = Some(GuardedAction::Quit);
            }
        }
    }

    /// Global shortcuts; they run before the panels so widgets never see them.
    fn handle_shortcuts(&mut self, ctx: &Context) {
        if ctx.input_mut(|input| input.consume_shortcut(&COPY_OPEN_PATH))
            && let Some(path) = self.open_file.path()
        {
            let text = absolute_path_text(path);
            self.copy_to_clipboard(ctx, text);
        }
        if ctx.input_mut(|input| input.consume_shortcut(&TOGGLE_SETTINGS)) {
            self.settings_open = !self.settings_open;
        }
        if ctx.input_mut(|input| input.consume_shortcut(&OPEN_IN_BROWSER)) {
            self.open_in_browser(ctx);
        }
        if ctx.input_mut(|input| input.consume_shortcut(&NEW_SCRATCH)) {
            self.new_scratch(ctx);
        }
        if self.settings_open
            && self.pending.is_none()
            && self.renaming.is_none()
            && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.settings_open = false;
        }
        if ctx.input_mut(|input| input.consume_shortcut(&SAVE)) {
            self.save();
        }
        if ctx.input_mut(|input| input.consume_shortcut(&TOGGLE_PREVIEW)) {
            self.toggle_preview();
            if !self.showing_preview() {
                focus_editor(ctx);
            }
        }
        if let OpenFile::Text { document, preview, .. } = &mut self.open_file
            && handle_editor_shortcuts(ctx, document)
        {
            *preview = None;
        }
    }

    /// Remember the window size, then write the settings file.
    fn save_settings(&mut self, ctx: &Context) {
        if let Some(rect) = ctx.input(|input| input.viewport().inner_rect) {
            self.settings.window_size = Some((rect.width(), rect.height()));
        }
        self.write_settings();
    }

    /// Write the settings file. A failure is only reported on stderr: the
    /// window is closing and there is nowhere else to show it.
    fn write_settings(&self) {
        if let Err(error) = self.settings.save() {
            eprintln!("rubrique: could not save settings: {error}");
        }
    }

    /// "name - rubrique", with the dirty marker; sent only when it changes.
    fn update_title(&mut self, ctx: &Context) {
        let name = self.open_file.path().map(name_text);
        let dirty = if self.open_file.is_dirty() { DIRTY_MARKER } else { "" };
        let title = match name {
            Some(name) => format!("{dirty}{name} - rubrique"),
            None => "rubrique".to_string(),
        };
        if title != self.window_title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
    }

    // ---------- panels ----------

    /// Right panel: settings. Theme picks apply at once.
    fn settings_panel(&mut self, ui: &mut Ui) {
        let folder = themes_folder();
        let scratch_folder = self.settings.scratch_folder_or_default();
        let scratch = ScratchInfo {
            folder: scratch_folder.as_deref(),
            is_default: self.settings.scratch_folder.is_none() || scratch_folder == default_scratch_folder(),
        };
        let action = show_settings(ui, &self.themes, &self.settings.theme, folder.as_deref(), scratch);
        let Some(action) = action else {
            return;
        };
        match action {
            SettingsAction::PickTheme(choice) => {
                apply_theme(ui.ctx(), &self.themes, &choice);
                self.settings.theme = choice;
            }
            SettingsAction::ReloadThemes => {
                self.themes = Themes::load(folder.as_deref());
                apply_theme(ui.ctx(), &self.themes, &self.settings.theme);
                let count = self.themes.list.len();
                self.show_status(format!("Loaded {count} themes"));
            }
            SettingsAction::ChooseScratchFolder => {
                let mut dialog = rfd::FileDialog::new().set_title("Folder for scratch files");
                if let Some(current) = &scratch_folder {
                    dialog = dialog.set_directory(current);
                }
                if let Some(chosen) = dialog.pick_folder() {
                    self.settings.scratch_folder = Some(chosen);
                }
            }
            SettingsAction::ResetScratchFolder => self.settings.scratch_folder = None,
            SettingsAction::Close => self.settings_open = false,
        }
    }

    /// Show the open Markdown (rendered, in the current theme's colors) or
    /// HTML file in the system browser, from a temporary copy so unsaved
    /// changes show too.
    fn open_in_browser(&mut self, ctx: &Context) {
        let OpenFile::Text { document, .. } = &self.open_file else {
            return;
        };
        let path = document.path.clone();
        let folder = path.parent().map(|folder| format!("{}/", file_url(folder))).unwrap_or_default();
        let page = if is_markdown_path(&path) {
            let palette = self.current_palette(ctx);
            html_page(&name_text(&path), &parse_blocks(&document.text), &palette, Some(&folder))
        } else if is_html_path(&path) {
            with_base(&document.text, &folder)
        } else {
            self.show_status("Open in browser works for Markdown and HTML files".to_string());
            return;
        };
        match write_browser_copy(&path, &page) {
            Ok(copy) => {
                ctx.open_url(egui::OpenUrl::new_tab(file_url(&copy)));
                self.show_status(format!("Opened {} in the browser", name_text(&path)));
            }
            Err(error) => self.show_status(format!("Cannot write the browser copy: {error}")),
        }
    }

    /// Colors of the theme on screen now (for System, the light or dark one in use).
    fn current_palette(&self, ctx: &Context) -> crate::theme::palette::Palette {
        if let crate::theme::catalog::ThemeChoice::Named(id) = &self.settings.theme
            && let Some(theme) = self.themes.find(id)
        {
            return theme.palette;
        }
        let base = match ctx.theme() {
            egui::Theme::Dark => crate::theme::palette::Base::Dark,
            egui::Theme::Light => crate::theme::palette::Base::Light,
        };
        self.themes.for_base(base).palette
    }

    /// Create a scratch file, show its folder in the explorer, and open it for typing.
    fn new_scratch(&mut self, ctx: &Context) {
        let Some(folder) = self.settings.scratch_folder_or_default() else {
            self.show_status("No folder for scratch files: set one in Settings".to_string());
            return;
        };
        let path = match create_scratch(&folder, &crate::date::today_iso()) {
            Ok(path) => path,
            Err(error) => {
                self.show_status(format!("Cannot create a scratch in {}: {error}", folder.display()));
                return;
            }
        };
        if self.roots.iter().any(|root| folder.starts_with(&root.path)) {
            self.roots.iter_mut().for_each(FsNode::refresh);
        } else {
            self.add_folder(folder);
        }
        self.settings.markdown_view = MarkdownView::Edit;
        self.request(GuardedAction::OpenFile(path));
        focus_editor(ctx);
    }

    /// Open the "Rename" dialog for `path`.
    fn start_rename(&mut self, path: PathBuf) {
        let name = name_text(&path);
        self.renaming = Some(Renaming { path, name, error: None });
    }

    /// Rename the file on disk and follow it if it is the open one.
    /// Returns an error message to show in the dialog.
    fn finish_rename(&mut self, path: &Path, new_name: &str) -> Result<(), String> {
        let target = rename_target(path, new_name)?;
        if target == path {
            return Ok(());
        }
        std::fs::rename(path, &target).map_err(|error| format!("Cannot rename: {error}"))?;
        match &mut self.open_file {
            OpenFile::Text { document, .. } if document.path == path => document.path.clone_from(&target),
            OpenFile::Unsupported { path: open, .. } if open == path => open.clone_from(&target),
            _ => {}
        }
        if self.settings.last_file.as_deref() == Some(path) {
            self.settings.last_file = Some(target.clone());
        }
        self.roots.iter_mut().for_each(FsNode::refresh);
        self.show_status(format!("Renamed to {}", name_text(&target)));
        Ok(())
    }

    /// Draw the "Rename" dialog while it is open.
    fn rename_dialog(&mut self, ctx: &Context) {
        let Some(renaming) = &mut self.renaming else {
            return;
        };
        let modal = egui::Modal::new(egui::Id::unique("rename-file")).show(ctx, |ui| {
            ui.set_width(RENAME_DIALOG_WIDTH);
            ui.heading("Rename");
            ui.add_space(4.0);
            let field = ui.add(egui::TextEdit::singleline(&mut renaming.name).desired_width(f32::INFINITY));
            if !field.has_focus() && renaming.error.is_none() && !field.lost_focus() {
                field.request_focus();
            }
            let entered = field.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
            if let Some(error) = &renaming.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let rename = ui.button("Rename").clicked() || entered;
                let cancel = ui.button("Cancel").clicked();
                (rename, cancel)
            })
            .inner
        });
        let (rename, cancel) = modal.inner;
        if cancel || modal.should_close() {
            self.renaming = None;
            return;
        }
        if rename {
            let path = renaming.path.clone();
            let name = renaming.name.clone();
            match self.finish_rename(&path, &name) {
                Ok(()) => self.renaming = None,
                Err(error) => {
                    if let Some(renaming) = &mut self.renaming {
                        renaming.error = Some(error);
                    }
                }
            }
        }
    }

    /// Let the user pick a folder in the system dialog and add it as a root.
    fn choose_folder_to_add(&mut self) {
        let start = self.roots.first().and_then(|root| root.path.parent().map(Path::to_path_buf));
        let mut dialog = rfd::FileDialog::new().set_title("Add folder to rubrique");
        if let Some(start) = start {
            dialog = dialog.set_directory(start);
        }
        if let Some(path) = dialog.pick_folder() {
            self.add_folder(path);
        }
    }

    /// Left panel: one file tree per root folder.
    fn explorer(&mut self, ui: &mut Ui) {
        let open_path = self.open_file.path().map(Path::to_path_buf);
        let context = TreeContext {
            root: Path::new(""),
            can_remove_roots: self.roots.len() > 1,
            markdown_only: self.settings.markdown_only,
            open_file: open_path.as_deref(),
            open_file_dirty: self.open_file.is_dirty(),
        };
        let action = show_explorer(ui, &mut self.roots, &mut self.settings.markdown_only, &context);
        match action {
            Some(ExplorerAction::OpenFile(path)) => {
                // Clicking the file that is already open does nothing.
                if self.open_file.path() != Some(path.as_path()) {
                    self.request(GuardedAction::OpenFile(path));
                }
            }
            Some(ExplorerAction::Copy(text)) => self.copy_to_clipboard(ui.ctx(), text),
            Some(ExplorerAction::Refresh) => self.roots.iter_mut().for_each(FsNode::refresh),
            Some(ExplorerAction::RemoveRoot(path)) => self.request_remove_root(path),
            Some(ExplorerAction::AddFolder) => self.choose_folder_to_add(),
            Some(ExplorerAction::NewScratch) => self.new_scratch(ui.ctx()),
            Some(ExplorerAction::Rename(path)) => self.start_rename(path),
            None => {}
        }
    }

    /// Right panel: editor, preview, or why the file cannot be shown.
    fn document_area(&mut self, ui: &mut Ui) {
        let showing_preview = self.showing_preview();
        let mut notice_choice = None;
        let mut merge_action = None;
        match &mut self.open_file {
            OpenFile::Nothing => {
                ui.centered_and_justified(|ui| ui.label(RichText::new("Pick a file in the explorer").weak()));
            }
            OpenFile::Unsupported { path, reason } => {
                egui::Frame::new().inner_margin(UNSUPPORTED_MARGIN).show(ui, |ui| {
                    ui.heading(name_text(path));
                    ui.label(format!("Cannot display this file. {}", reason.describe()));
                });
            }
            OpenFile::Text { document, preview, disk_notice } => {
                if let Some(session) = &mut self.merging {
                    merge_action = show_merge(ui, session, &name_text(&document.path));
                } else if let Some(state) = disk_notice {
                    notice_choice = disk_notice_bar(ui, state);
                }
                if self.merging.is_some() {
                    // The merge screen fills the page.
                } else if showing_preview {
                    let blocks = preview.get_or_insert_with(|| parse_blocks(&document.text));
                    show_preview(ui, blocks);
                } else {
                    let was_dirty = document.is_dirty();
                    let markdown = is_markdown_path(&document.path);
                    if let Some(stats) = show_editor(ui, document, markdown) {
                        self.stats = Some(stats);
                    }
                    // Any edit invalidates the cached preview.
                    if was_dirty || document.is_dirty() {
                        *preview = None;
                    }
                }
            }
        }
        if let Some(choice) = notice_choice {
            self.apply_notice_choice(choice);
        }
        if let Some(action) = merge_action {
            self.finish_merge(action);
        }
    }

    /// Act on a button of the "changed on disk" bar.
    fn apply_notice_choice(&mut self, choice: NoticeChoice) {
        match choice {
            NoticeChoice::Reload => {
                if let Some(path) = self.open_file.path().map(Path::to_path_buf) {
                    self.open_file_now(path);
                }
            }
            NoticeChoice::Merge => self.start_merge(),
            NoticeChoice::KeepMine => {
                if let OpenFile::Text { document, disk_notice, .. } = &mut self.open_file {
                    document.acknowledge_disk_change();
                    *disk_notice = None;
                }
            }
        }
    }

    /// Open the merge screen: the file on disk against my buffer, from the
    /// text both started from.
    fn start_merge(&mut self) {
        let OpenFile::Text { document, .. } = &self.open_file else {
            return;
        };
        match document.read_disk_text() {
            Ok(disk_text) => self.merging = Some(MergeSession::new(document.base_text(), &document.text, disk_text)),
            Err(error) => self.show_status(format!("Cannot read the file on disk: {error}")),
        }
    }

    /// Apply or drop the merge result.
    fn finish_merge(&mut self, action: MergeAction) {
        let Some(session) = self.merging.take() else {
            return;
        };
        if action == MergeAction::Cancel {
            return;
        }
        let Some(result) = session.result() else {
            self.merging = Some(session);
            return;
        };
        if let OpenFile::Text { document, preview, disk_notice } = &mut self.open_file {
            document.apply_merge(result, session.disk_text);
            *preview = None;
            *disk_notice = None;
            let message = if document.is_dirty() {
                "Merged. Save to write the result to disk."
            } else {
                "Merged: the result matches the file on disk."
            };
            self.show_status(message.to_string());
        }
    }

    /// Bottom bar: a recent status message, or the open file's path, mode and stats.
    fn status_bar(&mut self, ui: &mut Ui) {
        let now = ui.input(|input| input.time);
        ui.horizontal(|ui| {
            if let Some((message, remaining_seconds)) = self.visible_status_message(now) {
                ui.label(message);
                ui.ctx().request_repaint_after(Duration::from_secs_f64(remaining_seconds));
            } else {
                self.document_status(ui);
            }
        });
    }

    /// The status message while it is still on screen, with its remaining
    /// seconds. Clears the message once it has expired.
    fn visible_status_message(&mut self, now: f64) -> Option<(&str, f64)> {
        let (_, shown_at) = self.status_message.as_mut()?;
        if shown_at.is_nan() {
            *shown_at = now;
        }
        let remaining_seconds = STATUS_MESSAGE_SECONDS - (now - *shown_at);
        if remaining_seconds <= 0.0 {
            self.status_message = None;
            return None;
        }
        self.status_message.as_ref().map(|(message, _)| (message.as_str(), remaining_seconds))
    }

    /// Path of the open file on the left; mode and editor stats on the right.
    fn document_status(&mut self, ui: &mut Ui) {
        let Some(path) = self.open_file.path() else {
            return;
        };
        let dirty = if self.open_file.is_dirty() { DIRTY_MARKER } else { "" };
        let path_text = format!("{dirty}{}", path.display());
        let is_text = matches!(self.open_file, OpenFile::Text { .. });
        let showing_preview = self.showing_preview();
        let is_markdown = self.open_file.path().is_some_and(is_markdown_path);
        let is_html = self.open_file.path().is_some_and(is_html_path);
        // Right side first, so a long path is shortened instead of overlapping it.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if is_text && (is_markdown || is_html) {
                let browser =
                    ui.small_button("Open in browser").on_hover_text("Show it in your web browser (Cmd+Shift+B)");
                if browser.clicked() {
                    self.open_in_browser(ui.ctx());
                }
            }
            if is_text {
                let mode = if showing_preview { "Preview" } else { "Edit" };
                // Markdown files switch mode with a click; other files only edit.
                if is_markdown {
                    let toggle = ui.small_button(mode).on_hover_text("Switch edit / preview (Cmd+E)");
                    if toggle.clicked() {
                        self.toggle_preview();
                        if !self.showing_preview() {
                            focus_editor(ui.ctx());
                        }
                    }
                } else {
                    ui.label(mode);
                }
                if !showing_preview && let Some(stats) = &self.stats {
                    ui.label(format!("{}   ", stats.summary()));
                }
            }
            let left = egui::Layout::left_to_right(egui::Align::Center);
            ui.allocate_ui_with_layout(ui.available_size(), left, |ui| {
                ui.add(egui::Label::new(path_text.as_str()).truncate()).on_hover_text(path_text.as_str());
            });
        });
    }

    /// Ask about unsaved changes while an action is pending, then run it or drop it.
    fn unsaved_dialog(&mut self, ctx: &Context) {
        if self.pending.is_none() {
            return;
        }
        let name = self.open_file.path().map(name_text).unwrap_or_default();
        let Some(choice) = ask_about_unsaved_changes(ctx, &name) else {
            return;
        };
        // The question is answered: whatever happens next, stop asking.
        let Some(action) = self.pending.take() else {
            return;
        };
        let proceed = match choice {
            UnsavedChoice::Save => self.save(),
            UnsavedChoice::Discard => true,
            UnsavedChoice::Cancel => false,
        };
        if !proceed {
            return;
        }
        let is_quit = matches!(action, GuardedAction::Quit);
        self.perform(action);
        if is_quit {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }
}

/// Write `page` to a temporary file for the browser, one per source file, and
/// return its path.
fn write_browser_copy(source: &Path, page: &str) -> std::io::Result<PathBuf> {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let folder = std::env::temp_dir().join("rubrique-browser");
    std::fs::create_dir_all(&folder)?;
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    let stem = source.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    let copy = folder.join(format!("{stem}-{:08x}.html", hasher.finish() & 0xffff_ffff));
    std::fs::write(&copy, page)?;
    Ok(copy)
}

/// Buttons on the "changed on disk" bar.
#[derive(Clone, Copy, Debug)]
enum NoticeChoice {
    /// Open the merge screen.
    Merge,
    /// Load the version on disk, dropping my changes.
    Reload,
    /// Keep my version; the next save overwrites the one on disk.
    KeepMine,
}

/// Draw the "changed on disk" bar; returns the button the user clicked.
fn disk_notice_bar(ui: &mut Ui, state: &DiskState) -> Option<NoticeChoice> {
    let tint = ui.visuals().warn_fg_color.gamma_multiply(NOTICE_TINT);
    egui::Frame::new()
        .fill(tint)
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| disk_notice_row(ui, state)).inner
        })
        .inner
}

/// Message and buttons inside the "changed on disk" bar.
fn disk_notice_row(ui: &mut Ui, state: &DiskState) -> Option<NoticeChoice> {
    let mut choice = None;
    if *state == DiskState::Deleted {
        ui.label("This file was deleted on disk. Saving will recreate it.");
        if ui.button("Keep mine").clicked() {
            choice = Some(NoticeChoice::KeepMine);
        }
        return choice;
    }
    ui.label("Changed on disk by another program.").on_hover_text("You can keep working and decide later.");
    if ui.button("Compare and merge…").on_hover_text("See both versions side by side and combine them").clicked() {
        choice = Some(NoticeChoice::Merge);
    }
    if ui.button("Reload theirs").on_hover_text("Load the version on disk and drop your unsaved changes").clicked() {
        choice = Some(NoticeChoice::Reload);
    }
    if ui.button("Keep mine").on_hover_text("Keep your version; saving overwrites the one on disk").clicked() {
        choice = Some(NoticeChoice::KeepMine);
    }
    choice
}

/// Draw the "unsaved changes" modal for the file `name`. Returns the
/// answer once the user gives one; closing the modal counts as Cancel.
fn ask_about_unsaved_changes(ctx: &Context, name: &str) -> Option<UnsavedChoice> {
    let modal = egui::Modal::new(egui::Id::unique("unsaved-changes")).show(ctx, |ui| {
        ui.heading("Unsaved changes");
        ui.label(format!("Save changes to {name}?"));
        ui.add_space(8.0);
        ui.horizontal(unsaved_choice_buttons).inner
    });
    let closed = modal.should_close();
    modal.inner.or(closed.then_some(UnsavedChoice::Cancel))
}

/// The Save / Discard / Cancel buttons; returns the one clicked this frame.
fn unsaved_choice_buttons(ui: &mut Ui) -> Option<UnsavedChoice> {
    let mut choice = None;
    for (title, option) in
        [("Save", UnsavedChoice::Save), ("Discard", UnsavedChoice::Discard), ("Cancel", UnsavedChoice::Cancel)]
    {
        if ui.button(title).clicked() {
            choice = Some(option);
        }
    }
    choice
}

impl eframe::App for RubriqueApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_window_events(&ctx);
        self.handle_shortcuts(&ctx);
        self.poll_disk(&ctx);

        let style = ctx.global_style();
        let bar_frame =
            egui::Frame::side_top_panel(&style).inner_margin(egui::Margin::symmetric(BAR_MARGIN.0, BAR_MARGIN.1));
        // The page fills the whole central area; editor and preview add their own margins.
        let page_frame = egui::Frame::central_panel(&style).fill(page_fill(&style.visuals)).inner_margin(0);

        egui::Panel::bottom("status-bar").frame(bar_frame).show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("explorer").resizable(true).default_size(EXPLORER_DEFAULT_WIDTH).show(ui, |ui| {
            // Settings sit at the bottom of the explorer, out of the way.
            egui::Panel::bottom("explorer-footer").show_separator_line(false).show(ui, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let gear = icon_button(ui, Icon::Gear, "Settings", "Settings (Cmd+,)", self.settings_open);
                    if gear.clicked() {
                        self.settings_open = !self.settings_open;
                    }
                });
                ui.add_space(2.0);
            });
            self.explorer(ui);
        });
        if self.settings_open {
            let panel_frame = egui::Frame::side_top_panel(&style).inner_margin(0);
            egui::Panel::right("settings")
                .resizable(false)
                .exact_size(SETTINGS_PANEL_WIDTH)
                .frame(panel_frame)
                .show(ui, |ui| self.settings_panel(ui));
        }
        egui::CentralPanel::default().frame(page_frame).show(ui, |ui| self.document_area(ui));

        self.unsaved_dialog(&ctx);
        self.rename_dialog(&ctx);
        self.update_title(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.write_settings();
    }
}
