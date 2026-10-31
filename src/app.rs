//! `RubriqueApp`: top-level state, panel layout, global shortcuts and dialogs.

use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui::{self, Context, Key, KeyboardShortcut, Modifiers, RichText, Ui, ViewportCommand};

mod ask;
mod drafts;
mod images;
mod install;
mod naming;
mod summary;
mod tabs;
mod voice;

use crate::ai::skills::{self, Skill};
use crate::ai::{self, AiTool};
use crate::document::media_view::{MediaEvent, image_pixels, show_image, show_video};
use crate::document::merge::MergeSession;
use crate::document::merge_view::{MergeAction, show_merge};
use crate::document::preview_selection;
use crate::document::text_file::{DiskState, Document, Opened, Unsupported};
use crate::document::text_stats::TextStats;
use crate::document::view::{
    BlockEdit, EditorState, PreviewEvent, editor_selection, focus_editor, handle_editor_shortcuts, set_editor_cursor,
    show_editor, show_preview,
};
use crate::explorer::paths::{absolute_path_text, name_text};
use crate::explorer::tree::{FsNode, is_markdown_path};
use crate::explorer::view::{ExplorerAction, TreeContext, show_explorer};
use crate::format::{Language, format};
use crate::icons::icon_text_button;
use crate::icons::{Icon, icon_button};
use crate::markdown::blocks::{SourceBlock, parse_blocks, parse_source_blocks};
use crate::markdown::html::{file_url, html_page, is_html_path, with_base};
use crate::markdown::notes::{self, Note, find_notes, insert_note, remove_note, rewrite_note};
use crate::markdown::render::CodeAction;
use crate::media::{MediaKind, format_name, image_uri, media_kind, open_with_system, size_text};
use crate::scratch::{create_scratch, rename_target};
use crate::settings::accessibility::TextSize;
use crate::settings::view::{AiInfo, ScratchInfo, SettingsAction, SettingsGroup, VoiceInfo, show_settings};
use crate::settings::{MarkdownView, Settings, default_scratch_folder, default_skills_folder, themes_folder};
use crate::theme::catalog::Themes;
use crate::theme::style::{apply_accessibility, apply_theme, page_fill};
use crate::voice::view::set_memo_states;

/// How long a status message ("Copied ...") stays visible.
const STATUS_MESSAGE_SECONDS: f64 = 4.0;
const EXPLORER_DEFAULT_WIDTH: f32 = 260.0;
/// Width of the settings panel.
/// Starting height of the AI output panel.
const AI_OUTPUT_HEIGHT: f32 = 230.0;
/// Space around the "cannot display this file" message.
const UNSUPPORTED_MARGIN: f32 = 16.0;
/// How often the open file is checked for changes made by other programs.
const DISK_CHECK_SECONDS: f64 = 2.0;
/// Width of the note dialog.
const NOTE_DIALOG_WIDTH: f32 = 360.0;
/// Quoted text longer than this is shortened in the note dialog and panel.
const NOTE_QUOTE_CHARS: usize = 80;
/// Default width of the notes panel.
const NOTES_PANEL_WIDTH: f32 = 280.0;
/// Opacity of the warning color behind a note card.
const NOTE_CARD_TINT: f32 = 0.18;
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
const ADD_NOTE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::M);
const FORMAT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::F);
const OPEN_IN_BROWSER: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::B);
const NEW_SCRATCH: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
const TOGGLE_SETTINGS: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Comma);
const ASK: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::K);
const RECORD: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::R);
const COPY_OPEN_PATH: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::C);
const CLOSE_TAB: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::W);
const NEXT_TAB: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Tab);
const PREVIOUS_TAB: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL.plus(Modifiers::SHIFT), Key::Tab);
const TEXT_LARGER: [KeyboardShortcut; 2] =
    [KeyboardShortcut::new(Modifiers::COMMAND, Key::Plus), KeyboardShortcut::new(Modifiers::COMMAND, Key::Equals)];
const TEXT_SMALLER: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Minus);
const TEXT_DEFAULT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Num0);

/// What a tab shows (and, for the active tab, the right panel).
#[derive(Debug)]
enum OpenFile {
    Nothing,
    Text {
        document: Document,
        /// Parsed preview, rebuilt lazily after the text changes.
        preview: Option<Vec<SourceBlock>>,
        /// Shown when the file changed or vanished on disk.
        disk_notice: Option<DiskState>,
        /// Cursor and undo history while the tab is hidden.
        editor: Option<EditorState>,
    },
    /// An image (shown in the page) or a video (shown as a card).
    Media {
        path: PathBuf,
        kind: MediaKind,
        /// File size in bytes.
        bytes: u64,
        /// Modification time when read, to notice a new version.
        modified: Option<std::time::SystemTime>,
        /// An image at its own size (with scroll bars) instead of fitted to the page.
        actual_size: bool,
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
            OpenFile::Media { path, .. } | OpenFile::Unsupported { path, .. } => Some(path),
        }
    }

    /// Follow a rename of the file on disk.
    fn set_path(&mut self, target: &Path) {
        match self {
            OpenFile::Nothing => {}
            OpenFile::Text { document, .. } => document.path = target.to_path_buf(),
            OpenFile::Media { path, .. } | OpenFile::Unsupported { path, .. } => *path = target.to_path_buf(),
        }
    }

    /// True when a text file with unsaved changes is open.
    fn is_dirty(&self) -> bool {
        matches!(self, OpenFile::Text { document, .. } if document.is_dirty())
    }
}

/// An action that closes files, so it may need the "unsaved changes"
/// question first (once for each file with unsaved changes).
#[derive(Debug)]
enum GuardedAction {
    /// Close the active tab.
    CloseTab,
    /// Take a root out of the explorer (closes the tabs inside it).
    RemoveRoot(PathBuf),
    /// Quit; unsaved edits are kept as drafts, so this only asks when
    /// a draft cannot be written.
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
    /// The active tab's file (see `tabs.rs`).
    open_file: OpenFile,
    /// Every open tab, left to right; the active one's slot holds
    /// `OpenFile::Nothing` while its file is in `open_file`.
    tabs: Vec<OpenFile>,
    /// Index of the active tab in `tabs`.
    active_tab: usize,
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
    /// The group open in the settings window.
    settings_group: SettingsGroup,
    /// Ask bar, AI run and its output panel.
    ask: ask::AskState,
    /// Skills from the skills folders, read when Settings or Ask opens.
    skills: Vec<Skill>,
    /// Hover summaries in the explorer.
    summaries: summary::SummaryState,
    /// Name suggestions for scratch files.
    naming: naming::NamingState,
    /// Asks the window to redraw, from background threads.
    repaint: Context,
    /// Where each AI tool's program was found, in [`AiTool::ALL`] order.
    /// Checked when Settings or Ask opens and when a command changes.
    ai_found: [Option<PathBuf>; 4],
    /// The "Rename" dialog, while it is open.
    renaming: Option<Renaming>,
    /// The merge screen for the open file, while it is open.
    merging: Option<MergeSession>,
    /// egui time of the last check for outside changes.
    last_disk_check: f64,
    /// The preview block being edited as Markdown.
    block_edit: Option<BlockEdit>,
    /// The note being written or edited.
    note_draft: Option<NoteDraft>,
    /// Voice memos: recording, playback, transcription.
    voice: voice::VoiceState,
    /// Installing missing tools.
    install: install::InstallState,
    /// Unsaved edits kept between runs.
    drafts: drafts::DraftState,
}

/// A note in the making (new, or an existing one being edited).
#[derive(Debug)]
struct NoteDraft {
    /// The text the note is about, as shown.
    quote: String,
    /// Where that text is in the source, when known (selection in Edit mode).
    range: Option<std::ops::Range<usize>>,
    /// What the note says.
    text: String,
    /// The note being changed, for Edit.
    existing: Option<Note>,
    /// Why saving failed, shown in the dialog.
    error: Option<String>,
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
        apply_accessibility(ctx, &settings.accessibility);
        // rubrique steps through its own text sizes instead (see `handle_text_size_keys`).
        ctx.options_mut(|options| options.zoom_with_keyboard = false);
        let mut app = RubriqueApp {
            settings,
            roots: roots.into_iter().map(FsNode::new_root).collect(),
            open_file: OpenFile::Nothing,
            tabs: Vec::new(),
            active_tab: 0,
            pending: None,
            quit_confirmed: false,
            status_message: None,
            stats: None,
            window_title: String::new(),
            themes,
            settings_open: false,
            settings_group: SettingsGroup::default(),
            ask: ask::AskState::default(),
            skills: Vec::new(),
            summaries: summary::SummaryState::default(),
            naming: naming::NamingState::default(),
            repaint: ctx.clone(),
            ai_found: [None, None, None, None],
            renaming: None,
            merging: None,
            last_disk_check: 0.0,
            block_edit: None,
            note_draft: None,
            voice: voice::VoiceState::default(),
            install: install::InstallState::default(),
            drafts: drafts::DraftState::default(),
        };
        preview_selection::install(ctx);
        egui_extras::install_image_loaders(ctx);
        app.remember_roots();
        // Reopen last session's tabs (with their unsaved edits), then the file asked for.
        let saved_tabs = std::mem::take(&mut app.settings.open_tabs);
        app.reopen_tabs(saved_tabs);
        if let Some(file) = file {
            app.open_in_tab(file);
        }
        app
    }

    // ---------- actions ----------

    /// Run an action, asking about unsaved changes first where it closes them.
    fn request(&mut self, action: GuardedAction) {
        self.finish_block_edit();
        self.step(action);
    }

    /// Run `action`, or, while a file it would close has unsaved changes,
    /// show that file and ask about it. The answer runs `step` again, so
    /// each unsaved file is asked about in turn.
    fn step(&mut self, action: GuardedAction) {
        match action {
            GuardedAction::CloseTab if self.open_file.is_dirty() => self.pending = Some(action),
            GuardedAction::CloseTab => self.close_active_tab_now(),
            GuardedAction::RemoveRoot(folder) => match self.dirty_tab(|path| path.starts_with(&folder)) {
                Some(index) => {
                    self.switch_to_tab(index);
                    self.pending = Some(GuardedAction::RemoveRoot(folder));
                }
                None => self.remove_root_now(&folder),
            },
            GuardedAction::Quit => {
                // Asking is the fallback for when the edits cannot be kept.
                if !self.keep_drafts()
                    && let Some(index) = self.dirty_tab(|_| true)
                {
                    self.switch_to_tab(index);
                    self.pending = Some(GuardedAction::Quit);
                } else {
                    self.quit_confirmed = true;
                    self.repaint.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        }
    }

    /// Read `path` for a tab: an image or video, a text file, or one that
    /// cannot be shown. `None` (with a status message) when it cannot be read.
    fn load_file(&mut self, path: PathBuf) -> Option<OpenFile> {
        if let Some(kind) = media_kind(&path) {
            return match std::fs::metadata(&path) {
                Ok(metadata) => Some(OpenFile::Media {
                    path,
                    kind,
                    bytes: metadata.len(),
                    modified: metadata.modified().ok(),
                    actual_size: false,
                }),
                Err(error) => {
                    self.show_status(format!("Cannot open {}: {error}", path.display()));
                    None
                }
            };
        }
        match Document::open(&path) {
            Ok(Opened::Text(document)) => {
                Some(OpenFile::Text { document, preview: None, disk_notice: None, editor: None })
            }
            Ok(Opened::Unsupported(reason)) => Some(OpenFile::Unsupported { path, reason }),
            Err(error) => {
                self.show_status(format!("Cannot open {}: {error}", path.display()));
                None
            }
        }
    }

    /// Read `path` again into the active tab (dropping unsaved changes), or
    /// open it in a tab when none is open.
    fn open_file_now(&mut self, path: PathBuf) {
        if self.tabs.is_empty() {
            self.open_in_tab(path);
            return;
        }
        self.stats = None;
        self.merging = None;
        self.block_edit = None;
        self.note_draft = None;
        self.voice.stop_playback();
        crate::document::view::restore_editor_state(&self.repaint, None);
        if let Some(file) = self.load_file(path) {
            self.open_file = file;
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

    /// Ask to remove a root; unsaved files inside it need the question.
    fn request_remove_root(&mut self, path: PathBuf) {
        if self.roots.len() > 1 {
            self.request(GuardedAction::RemoveRoot(path));
        }
    }

    /// Take a root out of the explorer, keeping at least one, and close the
    /// tabs inside it.
    fn remove_root_now(&mut self, path: &Path) {
        if self.roots.len() <= 1 {
            return;
        }
        self.roots.retain(|root| root.path != path);
        self.close_tabs_inside(path);
        self.remember_roots();
    }

    /// Keep the settings' folder list in step with the explorer.
    fn remember_roots(&mut self) {
        self.settings.folders = self.roots.iter().map(|root| root.path.clone()).collect();
    }

    /// Save the open document; returns false when saving failed.
    fn save(&mut self) -> bool {
        self.finish_block_edit();
        let OpenFile::Text { document, disk_notice, .. } = &mut self.open_file else {
            return true;
        };
        match document.save() {
            Ok(()) => {
                *disk_notice = None;
                let message = format!("Saved {}", document.path.display());
                self.show_status(message);
                self.maybe_suggest_scratch_name();
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
        self.finish_block_edit();
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
        if let OpenFile::Media { path, kind, bytes, modified, .. } = &mut self.open_file {
            // A new version of an image is shown at once; nothing is ever unsaved here.
            let metadata = std::fs::metadata(&*path).ok();
            let now = metadata.as_ref().and_then(|metadata| metadata.modified().ok());
            if now.is_some() && now != *modified {
                *modified = now;
                *bytes = metadata.map_or(*bytes, |metadata| metadata.len());
                if *kind == MediaKind::Image {
                    self.repaint.forget_image(&image_uri(path));
                }
            }
            return;
        }
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
        if !matches!(self.open_file, OpenFile::Text { .. } | OpenFile::Media { .. }) || self.merging.is_some() {
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
            if self.quit_confirmed || self.keep_drafts() || self.dirty_tab(|_| true).is_none() {
                self.pending = None;
                self.save_settings(ctx);
            } else {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                if self.pending.is_none() {
                    self.request(GuardedAction::Quit);
                }
            }
        }
    }

    /// Global shortcuts; they run before the panels so widgets never see them.
    fn handle_shortcuts(&mut self, ctx: &Context) {
        self.handle_tab_shortcuts(ctx);
        self.handle_text_size_keys(ctx);
        if ctx.input_mut(|input| input.consume_shortcut(&COPY_OPEN_PATH))
            && let Some(path) = self.open_file.path()
        {
            let text = absolute_path_text(path);
            self.copy_to_clipboard(ctx, text);
        }
        if ctx.input_mut(|input| input.consume_shortcut(&TOGGLE_SETTINGS)) {
            self.toggle_settings();
        }
        if ctx.input_mut(|input| input.consume_shortcut(&ASK)) {
            self.check_ai_tools();
            self.load_skills();
            self.open_ask(ctx);
        }
        if ctx.input_mut(|input| input.consume_shortcut(&ADD_NOTE)) {
            self.start_note(ctx);
        }
        if ctx.input_mut(|input| input.consume_shortcut(&FORMAT)) {
            self.format_text(ctx);
        }
        if ctx.input_mut(|input| input.consume_shortcut(&OPEN_IN_BROWSER)) {
            self.open_in_browser(ctx);
        }
        if ctx.input_mut(|input| input.consume_shortcut(&RECORD)) {
            self.toggle_recording(ctx);
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

    /// Cmd/Ctrl + plus, minus and 0: the next text size, the one before, the default.
    fn handle_text_size_keys(&mut self, ctx: &Context) {
        let current = self.settings.accessibility.text_size;
        let size = if ctx.input_mut(|input| input.consume_shortcut(&TEXT_DEFAULT)) {
            TextSize::Default
        } else if ctx.input_mut(|input| TEXT_LARGER.iter().any(|keys| input.consume_shortcut(keys))) {
            current.larger()
        } else if ctx.input_mut(|input| input.consume_shortcut(&TEXT_SMALLER)) {
            current.smaller()
        } else {
            return;
        };
        if size != current {
            self.settings.accessibility.text_size = size;
            apply_accessibility(ctx, &self.settings.accessibility);
            self.show_status(format!("Text size: {} ({}%)", size.name(), (size.scale() * 100.0).round()));
        }
    }

    /// The theme, then the accessibility settings on top of it.
    fn apply_look(&self, ctx: &Context) {
        apply_theme(ctx, &self.themes, &self.settings.theme);
        apply_accessibility(ctx, &self.settings.accessibility);
    }

    /// Remember the window size and the open tabs, then write the settings file.
    fn save_settings(&mut self, ctx: &Context) {
        if let Some(rect) = ctx.input(|input| input.viewport().inner_rect) {
            // In points of the native scale, as the window was opened with, not zoomed ones.
            let zoom = ctx.zoom_factor();
            self.settings.window_size = Some((rect.width() * zoom, rect.height() * zoom));
        }
        self.remember_tabs();
        self.write_settings();
    }

    /// Keep the settings' tab list and active file in step with the tabs.
    fn remember_tabs(&mut self) {
        self.settings.open_tabs = self.tab_paths();
        self.settings.last_file = self.open_file.path().map(Path::to_path_buf);
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

    fn toggle_settings(&mut self) {
        self.settings_open = !self.settings_open;
        if self.settings_open {
            self.check_ai_tools();
            self.load_skills();
            self.check_voice_tool();
        }
    }

    /// Read the skills from the skills folders.
    fn load_skills(&mut self) {
        self.skills = skills::load_skills(&self.settings.ai.skill_folders(default_skills_folder()));
    }

    /// Look for each AI tool's program on `PATH`.
    fn check_ai_tools(&mut self) {
        self.ai_found = AiTool::ALL.map(|tool| ai::find_program(self.settings.ai.command(tool)));
        self.find_ai_recipes();
    }

    /// The settings window over the dimmed app. A click outside closes it.
    fn settings_window(&mut self, ctx: &Context) {
        if !self.settings_open {
            return;
        }
        let visuals = ctx.global_style().visuals.clone();
        let frame = egui::Frame::new()
            .fill(visuals.window_fill)
            .corner_radius(12)
            .shadow(visuals.window_shadow)
            .stroke(visuals.window_stroke);
        let response = egui::Modal::new(egui::Id::unique("settings-window")).frame(frame).show(ctx, |ui| {
            self.settings_panel(ui);
        });
        if response.should_close() {
            self.settings_open = false;
        }
    }

    /// The settings window's content. Theme picks apply at once.
    fn settings_panel(&mut self, ui: &mut Ui) {
        let folder = themes_folder();
        let scratch_folder = self.settings.scratch_folder_or_default();
        let scratch = ScratchInfo {
            folder: scratch_folder.as_deref(),
            is_default: self.settings.scratch_folder.is_none() || scratch_folder == default_scratch_folder(),
        };
        let skills_folder = self.settings.ai.skills_folder.clone().or_else(default_skills_folder);
        let install = self.install_status();
        let ai = AiInfo {
            settings: &mut self.settings.ai,
            found: &self.ai_found,
            skills: &self.skills,
            skills_folder: skills_folder.as_deref(),
            recipes: &self.install.ai,
            local_model: self.install.local_model.as_ref(),
            install: &install,
        };
        let voice = VoiceInfo {
            settings: &mut self.settings.voice,
            found: self.voice.program_found.as_deref(),
            recipe: self.install.voice.as_ref(),
            model_recipe: self.install.voice_model.as_ref(),
            install: &install,
        };
        let action = show_settings(
            ui,
            &mut self.settings_group,
            &self.themes,
            &self.settings.theme,
            folder.as_deref(),
            &mut self.settings.accessibility,
            scratch,
            ai,
            voice,
        );
        let Some(action) = action else {
            return;
        };
        match action {
            SettingsAction::PickTheme(choice) => {
                self.settings.theme = choice;
                self.apply_look(ui.ctx());
            }
            SettingsAction::AccessibilityChanged => self.apply_look(ui.ctx()),
            SettingsAction::ReloadThemes => {
                self.themes = Themes::load(folder.as_deref());
                self.apply_look(ui.ctx());
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
            SettingsAction::AiCommandChanged => self.check_ai_tools(),
            SettingsAction::ForgetAllowedFolders => {
                self.settings.ai.allowed_folders.clear();
                self.show_status("rubrique will ask again before the next AI run in each folder".to_string());
            }
            SettingsAction::ChooseSkillsFolder => {
                let mut dialog = rfd::FileDialog::new().set_title("Folder for skills");
                if let Some(current) = &skills_folder {
                    dialog = dialog.set_directory(current);
                }
                if let Some(chosen) = dialog.pick_folder() {
                    self.settings.ai.skills_folder = Some(chosen);
                    self.load_skills();
                }
            }
            SettingsAction::ResetSkillsFolder => {
                self.settings.ai.skills_folder = None;
                self.load_skills();
            }
            SettingsAction::ReloadSkills => {
                self.load_skills();
                let count = self.skills.len();
                self.show_status(format!("Found {count} skills"));
            }
            SettingsAction::CreateExampleSkill => match skills_folder.as_deref().map(skills::create_example) {
                Some(Ok(path)) => {
                    self.load_skills();
                    self.show_status(format!("Example skill: {}", path.display()));
                }
                Some(Err(error)) => self.show_status(format!("Cannot create the example skill: {error}")),
                None => {}
            },
            SettingsAction::VoiceCommandChanged => self.check_voice_tool(),
            SettingsAction::Copy(text) => {
                ui.ctx().copy_text(text);
                self.show_status("Copied the command; paste it into a terminal".to_string());
            }
            SettingsAction::Install(action) => self.install_action(ui.ctx(), action),
            SettingsAction::Close => self.settings_open = false,
        }
    }

    /// Show the open Markdown (rendered, in the current theme's colors) or
    /// HTML file in the system browser, from a temporary copy so unsaved
    /// changes show too.
    fn open_in_browser(&mut self, ctx: &Context) {
        self.finish_block_edit();
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

    /// Pretty-print the selection (its language guessed from the text unless
    /// the file says), or the whole JSON / SQL / XML file.
    fn format_text(&mut self, ctx: &Context) {
        self.finish_block_edit();
        let selection = if self.showing_preview() { None } else { editor_selection(ctx) };
        let OpenFile::Text { document, preview, .. } = &mut self.open_file else {
            return;
        };
        let file_language = Language::from_path(&document.path);
        let (start, end) = match selection {
            Some((first, last)) => (char_to_byte(&document.text, first), char_to_byte(&document.text, last)),
            None if file_language.is_some() => (0, document.text.len()),
            None => {
                self.show_status(
                    "Select JSON, SQL or XML text to format, or open a .json, .sql or .xml file".to_string(),
                );
                return;
            }
        };
        let original = &document.text[start..end];
        let language = file_language.unwrap_or_else(|| Language::guess(original));
        let mut formatted = match format(language, original) {
            Ok(formatted) => formatted,
            Err(error) => {
                let place = if selection.is_some() { " (in the selection)" } else { "" };
                let message = format!("Cannot format as {}{place}: {error}", language.name());
                self.show_status(message);
                return;
            }
        };
        // A selection keeps its own ending: no new line is added after it.
        if selection.is_some() && !original.ends_with('\n') {
            formatted.truncate(formatted.trim_end_matches('\n').len());
        }
        if formatted == original {
            self.show_status(format!("Already formatted as {}", language.name()));
            return;
        }
        document.text.replace_range(start..end, &formatted);
        document.on_text_changed();
        *preview = None;
        let cursor = document.text[..start].chars().count();
        set_editor_cursor(ctx, cursor);
        self.show_status(format!("Formatted as {}", language.name()));
    }

    /// Copy or Prettify on a code block in the preview. Prettify rewrites the
    /// code inside block `index` in the file.
    fn code_block_action(&mut self, ctx: &Context, index: usize, action: CodeAction) {
        let (info, code) = match action {
            CodeAction::Copy(code) => {
                let lines = code.lines().count();
                ctx.copy_text(code);
                let noun = if lines == 1 { "line" } else { "lines" };
                self.show_status(format!("Copied the code ({lines} {noun})"));
                return;
            }
            CodeAction::Prettify { info, code } => (info, code),
        };
        let Some(language) = Language::from_code_info(&info) else {
            return;
        };
        let formatted = match format(language, &code) {
            Ok(formatted) => formatted.trim_end_matches('\n').to_string(),
            Err(error) => {
                self.show_status(format!("Cannot prettify this {}: {error}", language.name()));
                return;
            }
        };
        if formatted == code {
            self.show_status(format!("This {} is already tidy", language.name()));
            return;
        }
        let OpenFile::Text { document, preview, .. } = &mut self.open_file else {
            return;
        };
        let blocks = parse_source_blocks(&document.text);
        // The code as written between the fences; indented code (inside a
        // list) does not match line for line, so it is left to Format.
        let found = blocks.get(index).and_then(|block| {
            let start = block.range.start + document.text[block.range.clone()].find(code.as_str())?;
            Some(start..start + code.len())
        });
        let Some(range) = found else {
            self.show_status(
                "Prettify works on code blocks that are not indented; use Format in Edit mode".to_string(),
            );
            return;
        };
        document.text.replace_range(range, &formatted);
        document.on_text_changed();
        *preview = None;
        self.show_status(format!("Prettified the {} (save to keep it)", language.name()));
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
        self.open_in_tab(path);
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
        let uri = image_uri(path);
        for tab in self.all_tabs_mut() {
            if tab.path() == Some(path) {
                tab.set_path(&target);
            }
        }
        self.repaint.forget_image(&uri);
        if self.settings.last_file.as_deref() == Some(path) {
            self.settings.last_file = Some(target.clone());
        }
        self.roots.iter_mut().for_each(FsNode::refresh);
        self.show_status(format!("Renamed to {}", name_text(&target)));
        Ok(())
    }

    /// Draw the "Rename" dialog while it is open.
    fn rename_dialog(&mut self, ctx: &Context) {
        let waiting = self.naming.dialog_waiting();
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
                ui.separator();
                let suggest = if waiting {
                    ui.add(egui::Spinner::new().size(12.0));
                    ui.label(RichText::new("Thinking of a name…").weak());
                    false
                } else {
                    let tip = "Ask the Summaries use tool (Settings → AI) for a name from the content";
                    icon_text_button(ui, Icon::Sparkle, "Suggest", tip, false).clicked()
                };
                (rename, cancel, suggest)
            })
            .inner
        });
        let (rename, cancel, suggest) = modal.inner;
        if suggest {
            let path = renaming.path.clone();
            let open_text = match &self.open_file {
                OpenFile::Text { document, .. } if document.path == path => Some(document.text.clone()),
                _ => None,
            };
            let text = open_text.or_else(|| std::fs::read_to_string(&path).ok()).unwrap_or_default();
            if let Err(error) = self.start_naming(path, &text, true)
                && let Some(renaming) = &mut self.renaming
            {
                renaming.error = Some(error);
            }
            return;
        }
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
            // Opens a tab, or switches to the file's tab when it is open.
            Some(ExplorerAction::OpenFile(path)) => self.open_in_tab(path),
            Some(ExplorerAction::Copy(text)) => self.copy_to_clipboard(ui.ctx(), text),
            Some(ExplorerAction::Refresh) => self.roots.iter_mut().for_each(FsNode::refresh),
            Some(ExplorerAction::RemoveRoot(path)) => self.request_remove_root(path),
            Some(ExplorerAction::AddFolder) => self.choose_folder_to_add(),
            Some(ExplorerAction::NewScratch) => self.new_scratch(ui.ctx()),
            Some(ExplorerAction::Rename(path)) => self.start_rename(path),
            None => {}
        }
    }

    /// Right panel: the tabs, then the editor, preview, image, video, or
    /// why the file cannot be shown.
    fn document_area(&mut self, ui: &mut Ui) {
        if !self.tabs.is_empty() {
            self.tab_bar(ui);
        }
        self.ask.page_rect = Some(ui.available_rect_before_wrap());
        let page = ui.available_rect_before_wrap();
        if self.merging.is_none() {
            self.recording_bar(ui);
        }
        let showing_preview = self.showing_preview();
        let mut notice_choice = None;
        let mut name_choice = None;
        let mut merge_action = None;
        let mut preview_event = None;
        let mut media_event = None;
        match &mut self.open_file {
            OpenFile::Nothing => {
                ui.centered_and_justified(|ui| ui.label(RichText::new("Pick a file in the explorer").weak()));
            }
            OpenFile::Media { path, kind: MediaKind::Image, actual_size, .. } => {
                media_event = show_image(ui, path, *actual_size);
            }
            OpenFile::Media { path, kind: MediaKind::Video, bytes, .. } => {
                media_event = show_video(ui, path, *bytes);
            }
            OpenFile::Unsupported { path, reason } => {
                egui::Frame::new().inner_margin(UNSUPPORTED_MARGIN).show(ui, |ui| {
                    ui.heading(name_text(path));
                    ui.label(format!("Cannot display this file. {}", reason.describe()));
                });
            }
            OpenFile::Text { document, preview, disk_notice, .. } => {
                if let Some(session) = &mut self.merging {
                    merge_action = show_merge(ui, session, &name_text(&document.path));
                } else if let Some(state) = disk_notice {
                    notice_choice = disk_notice_bar(ui, state);
                }
                let offer = self.naming.offer.as_ref().filter(|(path, _)| *path == document.path);
                if self.merging.is_none()
                    && let Some((_, name)) = offer
                {
                    name_choice = naming::name_offer_bar(ui, name);
                }
                if self.merging.is_some() {
                    // The merge screen fills the page.
                } else if showing_preview {
                    let blocks = preview.get_or_insert_with(|| parse_source_blocks(&document.text));
                    let folder = document.path.parent().unwrap_or(Path::new("/"));
                    set_memo_states(ui.ctx(), self.voice.memo_states(folder, blocks));
                    let output = show_preview(ui, &document.path, blocks, self.block_edit.as_mut());
                    preview_event = output.event;
                    self.voice.preview_hover = output.hovered;
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
        if let Some(choice) = name_choice {
            self.apply_name_choice(choice);
        }
        if let Some(action) = merge_action {
            self.finish_merge(action);
        }
        match preview_event {
            Some(PreviewEvent::StartEdit(index)) => self.start_block_edit(index),
            Some(PreviewEvent::FinishEdit) => self.finish_block_edit(),
            Some(PreviewEvent::Code { index, action }) => self.code_block_action(ui.ctx(), index, action),
            Some(PreviewEvent::Memo(action)) => self.memo_action(ui.ctx(), action),
            Some(PreviewEvent::PlayVideo(path)) => self.play_video(&path),
            None => {}
        }
        if let Some(event) = media_event {
            self.media_action(ui.ctx(), event);
        }
        self.drop_hint(ui.ctx(), page);
        if showing_preview && self.block_edit.is_none() && self.note_draft.is_none() {
            self.add_note_button(ui.ctx());
        }
    }

    // ---------- images and videos ----------

    /// Act on a click on an image or video page.
    fn media_action(&mut self, ctx: &Context, event: MediaEvent) {
        let OpenFile::Media { path, actual_size, .. } = &mut self.open_file else {
            return;
        };
        match event {
            MediaEvent::ToggleSize => *actual_size = !*actual_size,
            MediaEvent::PlayInPlayer => {
                let path = path.clone();
                self.play_video(&path);
            }
            MediaEvent::PlayInBrowser => {
                let path = path.clone();
                self.play_video_in_browser(ctx, &path);
            }
        }
    }

    /// Open a video in the system's video player.
    fn play_video(&mut self, path: &Path) {
        match open_with_system(path) {
            Ok(()) => self.show_status(format!("Opened {} in your video player", name_text(path))),
            Err(error) => self.show_status(error),
        }
    }

    /// Open a video in the browser, from a small page that holds it.
    fn play_video_in_browser(&mut self, ctx: &Context, path: &Path) {
        let page = video_page(&name_text(path), &file_url(path));
        match write_browser_copy(path, &page) {
            Ok(copy) => {
                ctx.open_url(egui::OpenUrl::new_tab(file_url(&copy)));
                self.show_status(format!("Opened {} in the browser", name_text(path)));
            }
            Err(error) => self.show_status(format!("Cannot write the browser page: {error}")),
        }
    }

    /// Status bar for an image or video: size, format, and Fit / Actual size.
    fn media_status(&mut self, ui: &mut Ui) {
        let OpenFile::Media { path, kind, bytes, actual_size, .. } = &mut self.open_file else {
            return;
        };
        match kind {
            MediaKind::Image => {
                let label = if *actual_size { "Actual size" } else { "Fit" };
                let tip = if *actual_size { "Fit the image to the page" } else { "Show the image at its own size" };
                if ui.small_button(label).on_hover_text(tip).clicked() {
                    *actual_size = !*actual_size;
                }
                let pixels = image_pixels(ui.ctx(), path)
                    .map(|[width, height]| format!("{width} × {height} px   "))
                    .unwrap_or_default();
                ui.label(format!("{pixels}{}   {}   ", format_name(path), size_text(*bytes)));
            }
            MediaKind::Video => {
                if ui.small_button("Play").on_hover_text("Open in your video player").clicked() {
                    let path = path.clone();
                    self.play_video(&path);
                    return;
                }
                ui.label(format!("{} video   {}   ", format_name(path), size_text(*bytes)));
            }
        }
    }

    // ---------- editing in the preview ----------

    /// Edit top-level block `index` of the preview as Markdown, after
    /// applying any block being edited (whose length change shifts the rest).
    fn start_block_edit(&mut self, index: usize) {
        let target = match &self.open_file {
            OpenFile::Text { preview: Some(blocks), .. } => blocks.get(index).map(|block| block.range.clone()),
            _ => None,
        };
        let Some(mut range) = target else {
            return;
        };
        if let Some((edited_at, delta)) = self.finish_block_edit_with_shift()
            && range.start > edited_at
        {
            range = shift(range.start, delta)..shift(range.end, delta);
        }
        let OpenFile::Text { document, .. } = &self.open_file else {
            return;
        };
        let Some(text) = document.text.get(range.clone()) else {
            return;
        };
        self.block_edit = Some(BlockEdit { index, range, text: text.to_string(), focused: false });
    }

    /// Put the edited block's Markdown into the document.
    fn finish_block_edit(&mut self) {
        self.finish_block_edit_with_shift();
    }

    /// Like [`Self::finish_block_edit`]; returns where the text changed and
    /// by how many bytes, so ranges after it can be moved.
    fn finish_block_edit_with_shift(&mut self) -> Option<(usize, isize)> {
        let edit = self.block_edit.take()?;
        let OpenFile::Text { document, preview, .. } = &mut self.open_file else {
            return None;
        };
        if document.text.get(edit.range.clone()) == Some(edit.text.as_str()) {
            return None;
        }
        let old_length = isize::try_from(edit.range.len()).unwrap_or(0);
        let new_length = isize::try_from(edit.text.len()).unwrap_or(0);
        document.text.replace_range(edit.range.clone(), &edit.text);
        document.on_text_changed();
        *preview = None;
        Some((edit.range.start, new_length - old_length))
    }

    // ---------- notes ----------

    /// Floating "+ Add note" next to text selected in the preview.
    fn add_note_button(&mut self, ctx: &Context) {
        let Some(selection) = preview_selection::current(ctx) else {
            return;
        };
        let area = egui::Area::new(egui::Id::unique("add-note-button"))
            .order(egui::Order::Foreground)
            .fixed_pos(selection.pointer + egui::vec2(10.0, 14.0));
        let shown = area.show(ctx, |ui| {
            ui.button("+ Add note").on_hover_text("Add a note about the selected text (Cmd+Shift+M)").clicked()
        });
        preview_selection::set_button_rect(ctx, shown.response.rect);
        if shown.inner {
            self.start_note(ctx);
        }
    }

    /// Open the note dialog for the selection (preview text, or editor text in Edit mode).
    fn start_note(&mut self, ctx: &Context) {
        self.finish_block_edit();
        let OpenFile::Text { document, .. } = &self.open_file else {
            return;
        };
        let draft = if self.showing_preview() {
            preview_selection::current(ctx).map(|selection| NoteDraft {
                quote: normalize_space(&selection.text),
                range: None,
                text: String::new(),
                existing: None,
                error: None,
            })
        } else {
            editor_selection(ctx).map(|(first, last)| {
                let range = char_to_byte(&document.text, first)..char_to_byte(&document.text, last);
                let shown: String = document.text[range.clone()].chars().filter(|c| !"*_~`".contains(*c)).collect();
                NoteDraft {
                    quote: normalize_space(&shown),
                    range: Some(range),
                    text: String::new(),
                    existing: None,
                    error: None,
                }
            })
        };
        match draft {
            Some(draft) => self.note_draft = Some(draft),
            None => self.show_status("Select some text first, then add a note".to_string()),
        }
    }

    /// Save the note in the draft into the document. Returns an error to show.
    fn save_note(&mut self, draft: &NoteDraft) -> Result<(), String> {
        let OpenFile::Text { document, preview, .. } = &mut self.open_file else {
            return Ok(());
        };
        if draft.text.trim().is_empty() {
            return Err("Write something first".to_string());
        }
        if let Some(existing) = &draft.existing {
            rewrite_note(&mut document.text, existing, &draft.text);
        } else {
            let range = draft
                .range
                .clone()
                .or_else(|| notes::locate(&document.text, &draft.quote))
                .ok_or("Cannot find this text in the file. Select text inside one paragraph.")?;
            insert_note(&mut document.text, &range, &draft.quote, &draft.text);
        }
        document.on_text_changed();
        *preview = None;
        Ok(())
    }

    /// The dialog for writing a note.
    fn note_dialog(&mut self, ctx: &Context) {
        let Some(draft) = &mut self.note_draft else {
            return;
        };
        let modal = egui::Modal::new(egui::Id::unique("note-dialog")).show(ctx, |ui| {
            ui.set_width(NOTE_DIALOG_WIDTH);
            ui.heading(if draft.existing.is_some() { "Edit note" } else { "Add note" });
            ui.label(
                RichText::new(format!("On \u{201c}{}\u{201d}", shorten(&draft.quote, NOTE_QUOTE_CHARS)))
                    .weak()
                    .italics(),
            );
            ui.add_space(4.0);
            let field = ui.add(
                egui::TextEdit::multiline(&mut draft.text)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .hint_text("Your note"),
            );
            if !field.has_focus() && draft.text.is_empty() && draft.error.is_none() {
                field.request_focus();
            }
            if let Some(error) = &draft.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            let save_keys = ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::Enter));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let save = ui.button("Save note").on_hover_text("Cmd+Enter").clicked() || save_keys;
                let cancel = ui.button("Cancel").clicked();
                (save, cancel)
            })
            .inner
        });
        let (save, cancel) = modal.inner;
        if cancel || modal.should_close() {
            self.note_draft = None;
            return;
        }
        if save && let Some(draft) = self.note_draft.take() {
            match self.save_note(&draft) {
                Ok(()) => {
                    preview_selection::clear(ctx);
                    self.show_status(
                        "Note saved in the file as a hidden comment (save the file to keep it)".to_string(),
                    );
                }
                Err(error) => self.note_draft = Some(NoteDraft { error: Some(error), ..draft }),
            }
        }
    }

    /// Right panel in the preview: every note in the file, with Edit and Delete.
    fn notes_panel(&mut self, ui: &mut Ui) {
        let OpenFile::Text { document, .. } = &self.open_file else {
            return;
        };
        let notes = find_notes(&document.text);
        let mut edit = None;
        let mut delete = None;
        let mut resolve = None;
        let mut stop = false;
        let busy = self.ai_busy();
        let all_state = self.resolving_state(&ask::Resolving::All);
        let note_states: Vec<_> =
            notes.iter().map(|note| self.resolving_state(&ask::Resolving::One(note.quote.clone()))).collect();
        egui::Frame::new().inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Notes").strong().size(16.0));
                ui.label(RichText::new(format!("{} in this file", notes.len())).weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(!busy, |ui| {
                        let tip = "Ask AI to resolve every note; the answer opens in merge";
                        if icon_text_button(ui, Icon::Sparkle, "Resolve all", tip, false).clicked() {
                            resolve = Some(ask::Resolving::All);
                        }
                    });
                });
            });
            ui.add_space(6.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for (index, note) in notes.iter().enumerate() {
                    let tint = ui.visuals().warn_fg_color.gamma_multiply(NOTE_CARD_TINT);
                    egui::Frame::new().fill(tint).corner_radius(8).inner_margin(10).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new(format!("\u{201c}{}\u{201d}", shorten(&note.quote, NOTE_QUOTE_CHARS)))
                                .weak()
                                .size(11.5),
                        );
                        ui.label(&note.text);
                        ui.horizontal(|ui| {
                            if let Some((tool, seconds)) = note_states[index].or(all_state) {
                                ui.add(egui::Spinner::new().size(11.0));
                                let working = format!("{} is working · {}", tool.name(), ai::format_elapsed(seconds));
                                ui.label(RichText::new(working).weak().size(11.5));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if icon_text_button(ui, Icon::Stop, "Stop", "Stop the tool", false).clicked() {
                                        stop = true;
                                    }
                                });
                                return;
                            }
                            ui.add_enabled_ui(!busy, |ui| {
                                let tip = "Ask AI to do what the note says; the answer opens in merge";
                                if icon_text_button(ui, Icon::Sparkle, "Resolve", tip, false).clicked() {
                                    resolve = Some(ask::Resolving::One(note.quote.clone()));
                                }
                            });
                            if ui.small_button("Edit").clicked() {
                                edit = Some(index);
                            }
                            if ui.small_button("Delete").clicked() {
                                delete = Some(index);
                            }
                        });
                    });
                    ui.add_space(8.0);
                }
                let tool = self.settings.ai.default_tool.name();
                ui.label(
                    RichText::new(format!(
                        "Select text in the preview, then Add note (Cmd+Shift+M). \
                         Resolve sends the file and the note to {tool}; the answer opens in merge."
                    ))
                    .weak()
                    .size(11.5),
                );
            });
        });
        if stop {
            self.stop_ai();
        }
        if let Some(which) = resolve {
            self.resolve_notes(ui.ctx(), which);
        }
        if let Some(index) = edit {
            let note = notes[index].clone();
            self.note_draft = Some(NoteDraft {
                quote: note.quote.clone(),
                range: None,
                text: note.text.clone(),
                existing: Some(note),
                error: None,
            });
        }
        if let Some(index) = delete
            && let OpenFile::Text { document, preview, .. } = &mut self.open_file
        {
            remove_note(&mut document.text, &notes[index]);
            document.on_text_changed();
            *preview = None;
            self.show_status("Note deleted (save the file to keep the change)".to_string());
        }
    }

    /// True when the preview is shown and the file has notes.
    fn shows_notes_panel(&self) -> bool {
        self.merging.is_none()
            && self.showing_preview()
            && matches!(&self.open_file, OpenFile::Text { document, .. } if document.text.contains("<!-- note on \""))
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
        self.finish_block_edit();
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
        if let Some(title) = &session.review {
            let transcript = title.starts_with("Transcript");
            if let OpenFile::Text { document, preview, .. } = &mut self.open_file {
                let changed = document.text != result;
                document.text = result;
                document.on_text_changed();
                *preview = None;
                self.ask.job = None;
                let message = match (changed, transcript) {
                    (true, true) => "Transcript added. Save to keep it.",
                    (true, false) => "AI changes applied. Save to keep them.",
                    (false, _) => "Kept the original",
                };
                self.show_status(message.to_string());
            }
            return;
        }
        if let OpenFile::Text { document, preview, disk_notice, .. } = &mut self.open_file {
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
            self.media_status(ui);
            let formattable = self.open_file.path().and_then(Language::from_path);
            if is_text
                && let Some(language) = formattable
                && ui
                    .small_button("Format")
                    .on_hover_text(format!("Pretty-print this {} (Cmd+Shift+F)", language.name()))
                    .clicked()
            {
                self.format_text(ui.ctx());
            }
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
                let ask = icon_text_button(
                    ui,
                    Icon::Sparkle,
                    "Ask",
                    "Ask AI about the selection, or the whole file (Cmd+K)",
                    self.ask.is_open(),
                );
                if ask.clicked() {
                    self.check_ai_tools();
                    self.load_skills();
                    self.open_ask(ui.ctx());
                }
                if is_markdown || is_html {
                    self.voice_buttons(ui);
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
            UnsavedChoice::Discard => {
                // Back to the file, so the next step sees nothing unsaved here
                // (and the draft goes with the edits).
                if let OpenFile::Text { document, preview, .. } = &mut self.open_file {
                    document.discard_changes();
                    *preview = None;
                }
                true
            }
            UnsavedChoice::Cancel => false,
        };
        if proceed {
            self.step(action);
        }
    }
}

/// `value` moved by `delta` bytes.
fn shift(value: usize, delta: isize) -> usize {
    value.saturating_add_signed(delta)
}

/// Runs of whitespace as single spaces.
fn normalize_space(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// At most `limit` characters, with "…" when cut.
fn shorten(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut short: String = text.chars().take(limit).collect();
    short.push('…');
    short
}

/// Byte index of character `index` in `text` (the end when past it).
fn char_to_byte(text: &str, index: usize) -> usize {
    text.char_indices().nth(index).map_or(text.len(), |(offset, _)| offset)
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

/// A page that plays one video, for the browser.
fn video_page(title: &str, video_url: &str) -> String {
    use crate::markdown::html::escape;
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>{}</title>\n\
         <style>html,body{{margin:0;height:100%;background:#111;}}\
         video{{display:block;width:100%;height:100%;object-fit:contain;}}</style></head>\n\
         <body><video controls autoplay src=\"{}\"></video></body></html>\n",
        escape(title),
        escape(video_url)
    )
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
        self.poll_drafts(&ctx);
        self.poll_naming();
        self.poll_voice(&ctx);
        self.poll_install(&ctx);

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
                        self.toggle_settings();
                    }
                });
                ui.add_space(2.0);
            });
            self.explorer(ui);
        });
        if self.shows_notes_panel() {
            egui::Panel::right("notes")
                .resizable(true)
                .default_size(NOTES_PANEL_WIDTH)
                .frame(egui::Frame::side_top_panel(&style).inner_margin(0))
                .show(ui, |ui| self.notes_panel(ui));
        }
        if self.ask.job.is_some() && self.merging.is_none() {
            egui::Panel::bottom("ai-output")
                .resizable(true)
                .default_size(AI_OUTPUT_HEIGHT)
                .frame(egui::Frame::side_top_panel(&style).inner_margin(0))
                .show(ui, |ui| self.ai_output_panel(ui));
        }
        egui::CentralPanel::default().frame(page_frame).show(ui, |ui| self.document_area(ui));
        self.handle_dropped_files(&ctx);

        self.ask_dialogs(&ctx);
        self.install_dialog(&ctx);
        self.hover_summary(&ctx);
        self.settings_window(&ctx);
        self.unsaved_dialog(&ctx);
        self.note_dialog(&ctx);
        self.rename_dialog(&ctx);
        self.update_title(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Usually done when the window asked to close; this covers other exits.
        self.keep_drafts();
        self.remember_tabs();
        self.write_settings();
    }
}
