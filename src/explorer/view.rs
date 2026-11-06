//! Draw the file tree and report clicks and copy requests back to the app.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Key, RichText, Ui};
use egui::text::{CCursor, CCursorRange};

use super::note_marks::NoteMarks;
use super::terminals::{show_terminal_file, show_terminals};
use crate::icons::{Icon, icon_button, paint_icon};
use crate::terminal::is_terminal_path;
use crate::terminal::monitor::TerminalRow;

use super::paths::{absolute_path_text, name_text, relative_path_text};
use super::tree::{Children, FsNode, NodeKind, is_markdown_path};

/// Something the user asked for in the explorer.
#[derive(Debug)]
pub enum ExplorerAction {
    /// The user clicked a file.
    OpenFile(PathBuf),
    /// Put this text on the clipboard.
    Copy(String),
    /// Re-read the folders from disk.
    Refresh,
    /// Take this root folder out of the explorer.
    RemoveRoot(PathBuf),
    /// Ask the system for a folder to add as a root.
    AddFolder,
    /// Ask for a folder on another computer to add as a root.
    AddRemoteFolder,
    /// Create a new scratch file and open it.
    NewScratch,
    /// Ask for a new name for this file in the "Rename" dialog.
    Rename(PathBuf),
    /// Turn this file's row into a name field (a double click).
    RenameInTree(PathBuf),
    /// Ask, then move this file to the Trash.
    Delete(PathBuf),
    /// Show this tmux session in a tab.
    OpenTerminal(String),
    /// Start a new tmux session and show it.
    NewTerminal,
    /// Create a `.tmux` file in this folder and open it.
    NewTerminalFile(PathBuf),
}

/// A file being renamed right in its tree row.
#[derive(Debug)]
pub struct TreeRename {
    pub path: PathBuf,
    /// The name being typed.
    pub name: String,
    /// Why the last attempt failed, shown under the row.
    pub error: Option<String>,
    /// Enter was pressed or the field was left: the app should rename now.
    pub submitted: bool,
    /// The field still needs focus and its name selected (on the first
    /// frame, and again after a failed attempt).
    needs_focus: bool,
}

impl TreeRename {
    pub fn new(path: PathBuf) -> Self {
        let name = name_text(&path);
        TreeRename { path, name, error: None, submitted: false, needs_focus: true }
    }

    /// Show `error` under the field and give the field back its focus.
    pub fn failed(&mut self, error: String) {
        self.error = Some(error);
        self.submitted = false;
        self.needs_focus = true;
    }
}

/// The file row under the pointer, for the hover summary.
#[derive(Clone, Debug, PartialEq)]
pub struct HoveredFile {
    pub path: PathBuf,
    /// Where the row is, to place the summary next to it.
    pub rect: egui::Rect,
}

/// Remember the hovered file row for [`hovered_file`].
fn set_hovered(ui: &Ui, file: HoveredFile) {
    ui.ctx().data_mut(|data| data.insert_temp(hovered_id(), file));
}

/// The file row under the pointer this frame, if any.
pub fn hovered_file(ctx: &egui::Context) -> Option<HoveredFile> {
    ctx.data_mut(|data| {
        let file = data.get_temp::<HoveredFile>(hovered_id());
        data.remove::<HoveredFile>(hovered_id());
        file
    })
}

fn hovered_id() -> egui::Id {
    egui::Id::unique("explorer-hovered-file")
}

/// Read-only facts the tree needs while drawing.
#[derive(Clone, Copy, Debug)]
pub struct TreeContext<'a> {
    /// Root folder of the entry being drawn, for "Copy relative path".
    pub root: &'a Path,
    /// More than one root is shown, so a root may be removed.
    pub can_remove_roots: bool,
    /// Hide files that are not Markdown.
    pub markdown_only: bool,
    /// File shown in the editor, highlighted in the tree.
    pub open_file: Option<&'a Path>,
    /// Files with unsaved changes, in any tab: shown in green with a dot
    /// after the name.
    pub dirty_files: &'a [PathBuf],
    /// Which files have notes, when the marker is turned on.
    pub note_marks: Option<&'a NoteMarks>,
    /// Remote folders are turned on (shows "Add remote folder").
    pub remote_enabled: bool,
    /// The tmux sessions and their status; `None` when tmux is not
    /// installed (no Terminals list then).
    pub terminals: Option<&'a [TerminalRow]>,
    /// The session shown in the active tab, highlighted in the list.
    pub open_terminal: Option<&'a str>,
}

/// Draw the explorer panel: "Add folder…", refresh, the Markdown filter and one tree per root folder.
/// `rename` is the file being renamed in its row, if any; Esc in the field clears it.
pub fn show_explorer(
    ui: &mut Ui,
    roots: &mut [FsNode],
    markdown_only: &mut bool,
    context: &TreeContext,
    rename: &mut Option<TreeRename>,
) -> Option<ExplorerAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        if icon_button(ui, Icon::FolderPlus, "Add folder", "Add a folder to the explorer", false).clicked() {
            action = Some(ExplorerAction::AddFolder);
        }
        if context.remote_enabled
            && icon_button(ui, Icon::ServerPlus, "Add remote folder", "Add a folder on another computer (SSH)", false)
                .clicked()
        {
            action = Some(ExplorerAction::AddRemoteFolder);
        }
        if icon_button(ui, Icon::NewScratch, "New scratch", "New scratch file for quick notes (Cmd+N)", false).clicked()
        {
            action = Some(ExplorerAction::NewScratch);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, Icon::Refresh, "Refresh", "Re-read all folders from disk", false).clicked() {
                action = Some(ExplorerAction::Refresh);
            }
        });
    });
    ui.checkbox(markdown_only, "Markdown only").on_hover_text("Hide files that are not Markdown");
    ui.separator();

    // The checkbox above may have just changed the filter.
    let context = TreeContext { markdown_only: *markdown_only, ..*context };
    // Rows never wrap (a wrapped name would put the folder arrow over its
    // text); long names scroll sideways instead.
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        show_terminals(ui, &context, &mut action);
        for root in roots.iter_mut() {
            let root_path = root.path.clone();
            let context = TreeContext { root: &root_path, ..context };
            show_root(ui, root, &context, rename, &mut action);
        }
    });
    action
}

/// A root folder: a bold row that folds its tree open and closed.
fn show_root(
    ui: &mut Ui,
    root: &mut FsNode,
    context: &TreeContext,
    rename: &mut Option<TreeRename>,
    action: &mut Option<ExplorerAction>,
) {
    let label = RichText::new(format!("{ICON_GAP}{}", root.name)).strong();
    let response = tree_row(ui, false, label).on_hover_text(absolute_path_text(&root.path));
    paint_arrow(ui, &response, root.expanded);
    if response.clicked() {
        root.toggle_expanded();
    }
    root_menu(&response, &root.path, context, action);
    if root.expanded {
        let indent_id = root.path.clone();
        ui.indent(indent_id, |ui| show_children(ui, root, context, rename, action));
    }
}

/// Draw a folder's entries, or why they could not be read.
fn show_children(
    ui: &mut Ui,
    folder: &mut FsNode,
    context: &TreeContext,
    rename: &mut Option<TreeRename>,
    action: &mut Option<ExplorerAction>,
) {
    match &mut folder.children {
        Children::NotLoaded => {}
        Children::Failed(error) => {
            ui.label(RichText::new(format!("Cannot read: {error}")).weak().italics());
        }
        Children::Loaded(children) => {
            for child in children.iter_mut() {
                if child.is_folder() {
                    show_folder(ui, child, context, rename, action);
                } else if !context.markdown_only || is_markdown_path(&child.path) || is_terminal_path(&child.path) {
                    show_file(ui, child, context, rename, action);
                }
            }
        }
    }
}

/// Draw one folder row and, when expanded, its contents indented below.
fn show_folder(
    ui: &mut Ui,
    folder: &mut FsNode,
    context: &TreeContext,
    rename: &mut Option<TreeRename>,
    action: &mut Option<ExplorerAction>,
) {
    let label = format!("{ICON_GAP}{}{}", folder.name, link_marker(folder));
    let response = tree_row(ui, false, label);
    if folder.kind == NodeKind::SymlinkLoop {
        response.clone().on_hover_text("Link to a parent folder; not expanded to avoid a loop");
    } else {
        paint_arrow(ui, &response, folder.expanded);
    }
    if response.clicked() {
        folder.toggle_expanded();
    }
    path_menu(&response, &folder.path, context, action);
    if folder.expanded {
        let indent_id = folder.path.clone();
        ui.indent(indent_id, |ui| show_children(ui, folder, context, rename, action));
    }
}

/// Draw one file row; a click opens the file, a double click renames it
/// in place.
fn show_file(
    ui: &mut Ui,
    file: &FsNode,
    context: &TreeContext,
    rename: &mut Option<TreeRename>,
    action: &mut Option<ExplorerAction>,
) {
    if rename.as_ref().is_some_and(|rename| rename.path == file.path) {
        rename_row(ui, rename);
        return;
    }
    if is_terminal_path(&file.path) {
        show_terminal_file(ui, file, context, action);
        return;
    }
    let is_open = context.open_file == Some(file.path.as_path());
    let dirty = context.dirty_files.iter().any(|path| path == &file.path);
    let dirty_marker = if dirty { " •" } else { "" };
    let label = format!("{ICON_GAP}{}{}{dirty_marker}", file.name, link_marker(file));
    let label = if dirty { egui::RichText::new(label).color(UNSAVED_COLOR) } else { egui::RichText::new(label) };
    let response = tree_row(ui, is_open, label);
    if let Some(marks) = context.note_marks
        && is_markdown_path(&file.path)
        && marks.has_notes(&file.path)
    {
        paint_note_mark(ui, &response);
    }
    if response.double_clicked() {
        *action = Some(ExplorerAction::RenameInTree(file.path.clone()));
    } else if response.clicked() {
        *action = Some(ExplorerAction::OpenFile(file.path.clone()));
    }
    if response.hovered() {
        set_hovered(ui, HoveredFile { path: file.path.clone(), rect: response.rect });
    }
    file_menu(&response, &file.path, context, action);
}

/// The row of the file being renamed: a name field, with the name before
/// the extension selected. Enter or leaving the field renames; Esc cancels.
fn rename_row(ui: &mut Ui, slot: &mut Option<TreeRename>) {
    let Some(rename) = slot.as_mut() else {
        return;
    };
    let id = egui::Id::unique("explorer-rename-field");
    let output = ui
        .horizontal(|ui| {
            // Line the field up with the file names in the other rows.
            ui.add_space(ui.spacing().button_padding.x + NOTE_MARK_SIZE);
            egui::TextEdit::singleline(&mut rename.name).id(id).desired_width(RENAME_FIELD_WIDTH).show(ui)
        })
        .inner;
    if rename.needs_focus {
        rename.needs_focus = false;
        output.response.request_focus();
        let mut state = output.state;
        let stem_chars = name_stem_chars(&rename.name);
        state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(0), CCursor::new(stem_chars))));
        state.store(ui.ctx(), id);
    }
    if let Some(error) = &rename.error {
        ui.label(RichText::new(error).small().color(ui.visuals().error_fg_color));
    }
    if output.response.lost_focus() {
        if ui.input(|input| input.key_pressed(Key::Escape)) {
            *slot = None;
        } else {
            rename.submitted = true;
        }
    }
}

/// How many characters of `name` come before its extension: all of them
/// when there is none, or when the name only starts with a dot (".env").
fn name_stem_chars(name: &str) -> usize {
    match name.rfind('.') {
        Some(dot) if dot > 0 => name[..dot].chars().count(),
        _ => name.chars().count(),
    }
}

/// One explorer row on a single line (the tree scrolls sideways for long names).
pub(super) fn tree_row(ui: &mut Ui, selected: bool, label: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(egui::Button::selectable(selected, label.into()))
}

/// Name color of a file with unsaved changes: a green that reads on every
/// built-in theme.
const UNSAVED_COLOR: egui::Color32 = egui::Color32::from_rgb(0x34, 0xa8, 0x53);
/// Room at the start of each row for the folder arrow, so file names line
/// up with folder names.
pub(super) const ICON_GAP: &str = "     ";
/// Width and height of the folder arrow, in points.
const ARROW_SIZE: f32 = 7.0;
/// Width of the note marker's icon box, in points.
pub(super) const NOTE_MARK_SIZE: f32 = 14.0;
/// Width of the name field of a file being renamed in the tree.
const RENAME_FIELD_WIDTH: f32 = 220.0;

/// Paint the speech bubble in the gap before a file name, where folders
/// have their arrow.
fn paint_note_mark(ui: &Ui, response: &egui::Response) {
    let visuals = ui.visuals();
    let center = egui::pos2(
        response.rect.left() + ui.spacing().button_padding.x + NOTE_MARK_SIZE / 2.0,
        response.rect.center().y,
    );
    let behind = if response.hovered() { visuals.widgets.hovered.weak_bg_fill } else { visuals.panel_fill };
    paint_icon(ui.painter(), Icon::Bubble, center, visuals.weak_text_color(), visuals.hyperlink_color, behind);
}

/// Paint a small triangle (right when collapsed, down when expanded).
/// Painted by hand because the default egui fonts have no arrow glyphs.
fn paint_arrow(ui: &Ui, response: &egui::Response, expanded: bool) {
    let center =
        egui::pos2(response.rect.left() + ui.spacing().button_padding.x + ARROW_SIZE / 2.0, response.rect.center().y);
    let half = ARROW_SIZE / 2.0;
    let points = if expanded {
        vec![
            center + egui::vec2(-half, -half / 2.0),
            center + egui::vec2(half, -half / 2.0),
            center + egui::vec2(0.0, half),
        ]
    } else {
        vec![
            center + egui::vec2(-half / 2.0, -half),
            center + egui::vec2(half, 0.0),
            center + egui::vec2(-half / 2.0, half),
        ]
    };
    let color = ui.style().interact(response).fg_stroke.color;
    ui.painter().add(egui::Shape::convex_polygon(points, color, egui::Stroke::NONE));
}

/// Marker after the name of a symlinked entry.
fn link_marker(node: &FsNode) -> &'static str {
    if node.is_symlink { " ↗" } else { "" }
}

/// Right-click menu of a root: the copy actions, then "Remove from explorer".
fn root_menu(response: &egui::Response, path: &Path, context: &TreeContext, action: &mut Option<ExplorerAction>) {
    response.context_menu(|ui| {
        copy_items(ui, path, context, action);
        ui.separator();
        new_terminal_item(ui, path, context, action);
        let remove = ui
            .add_enabled(context.can_remove_roots, egui::Button::new("Remove from explorer"))
            .on_hover_text("Hide this folder here; nothing on disk changes")
            .on_disabled_hover_text("The explorer needs at least one folder");
        if remove.clicked() {
            *action = Some(ExplorerAction::RemoveRoot(path.to_path_buf()));
            ui.close();
        }
    });
}

/// Right-click menu of a file: the copy actions, then "Rename…" and "Delete…".
pub(super) fn file_menu(
    response: &egui::Response,
    path: &Path,
    context: &TreeContext,
    action: &mut Option<ExplorerAction>,
) {
    response.context_menu(|ui| {
        copy_items(ui, path, context, action);
        ui.separator();
        if ui.button("Rename…").clicked() {
            *action = Some(ExplorerAction::Rename(path.to_path_buf()));
            ui.close();
        }
        if ui.button("Delete…").clicked() {
            *action = Some(ExplorerAction::Delete(path.to_path_buf()));
            ui.close();
        }
    });
}

/// Right-click menu with the three copy actions.
fn path_menu(response: &egui::Response, path: &Path, context: &TreeContext, action: &mut Option<ExplorerAction>) {
    response.context_menu(|ui| {
        copy_items(ui, path, context, action);
        if new_terminal_item(ui, path, context, action) {
            ui.separator();
        }
    });
}

/// "New terminal here" for a local folder, when tmux is installed. Returns
/// true when the item was shown.
fn new_terminal_item(ui: &mut Ui, folder: &Path, context: &TreeContext, action: &mut Option<ExplorerAction>) -> bool {
    if context.terminals.is_none() || crate::remote::is_mirror(folder) {
        return false;
    }
    let tip = "Add a .tmux file here: opening it starts a terminal in this folder";
    if ui.button("New terminal here").on_hover_text(tip).clicked() {
        *action = Some(ExplorerAction::NewTerminalFile(folder.to_path_buf()));
        ui.close();
    }
    true
}

/// "Copy absolute path", "Copy relative path" and "Copy name".
fn copy_items(ui: &mut Ui, path: &Path, context: &TreeContext, action: &mut Option<ExplorerAction>) {
    let choices = [
        ("Copy absolute path", absolute_path_text(path)),
        ("Copy relative path", relative_path_text(path, context.root)),
        ("Copy name", name_text(path)),
    ];
    for (title, text) in choices {
        if ui.button(title).on_hover_text(&text).clicked() {
            *action = Some(ExplorerAction::Copy(text));
            ui.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::name_stem_chars;

    #[test]
    fn selects_the_name_before_the_extension() {
        assert_eq!(name_stem_chars("notes.md"), 5);
        assert_eq!(name_stem_chars("archive.tar.gz"), 11);
        assert_eq!(name_stem_chars("README"), 6);
        assert_eq!(name_stem_chars(".env"), 4);
        assert_eq!(name_stem_chars("café.md"), 4);
    }
}
