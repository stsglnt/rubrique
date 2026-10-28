//! Draw the file tree and report clicks and copy requests back to the app.

use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText, Ui};

use crate::icons::{Icon, icon_button};

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
    /// Create a new scratch file and open it.
    NewScratch,
    /// Ask for a new name for this file.
    Rename(PathBuf),
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
    /// The open file has unsaved changes (shows a dot after its name).
    pub open_file_dirty: bool,
}

/// Draw the explorer panel: "Add folder…", refresh, the Markdown filter and one tree per root folder.
pub fn show_explorer(
    ui: &mut Ui,
    roots: &mut [FsNode],
    markdown_only: &mut bool,
    context: &TreeContext,
) -> Option<ExplorerAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        if icon_button(ui, Icon::FolderPlus, "Add folder", "Add a folder to the explorer", false).clicked() {
            action = Some(ExplorerAction::AddFolder);
        }
        if icon_button(ui, Icon::NotePlus, "New scratch", "New scratch file for quick notes (Cmd+N)", false).clicked() {
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
        for root in roots.iter_mut() {
            let root_path = root.path.clone();
            let context = TreeContext { root: &root_path, ..context };
            show_root(ui, root, &context, &mut action);
        }
    });
    action
}

/// A root folder: a bold row that folds its tree open and closed.
fn show_root(ui: &mut Ui, root: &mut FsNode, context: &TreeContext, action: &mut Option<ExplorerAction>) {
    let label = RichText::new(format!("{ICON_GAP}{}", root.name)).strong();
    let response = tree_row(ui, false, label).on_hover_text(absolute_path_text(&root.path));
    paint_arrow(ui, &response, root.expanded);
    if response.clicked() {
        root.toggle_expanded();
    }
    root_menu(&response, &root.path, context, action);
    if root.expanded {
        let indent_id = root.path.clone();
        ui.indent(indent_id, |ui| show_children(ui, root, context, action));
    }
}

/// Draw a folder's entries, or why they could not be read.
fn show_children(ui: &mut Ui, folder: &mut FsNode, context: &TreeContext, action: &mut Option<ExplorerAction>) {
    match &mut folder.children {
        Children::NotLoaded => {}
        Children::Failed(error) => {
            ui.label(RichText::new(format!("Cannot read: {error}")).weak().italics());
        }
        Children::Loaded(children) => {
            for child in children.iter_mut() {
                if child.is_folder() {
                    show_folder(ui, child, context, action);
                } else if !context.markdown_only || is_markdown_path(&child.path) {
                    show_file(ui, child, context, action);
                }
            }
        }
    }
}

/// Draw one folder row and, when expanded, its contents indented below.
fn show_folder(ui: &mut Ui, folder: &mut FsNode, context: &TreeContext, action: &mut Option<ExplorerAction>) {
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
        ui.indent(indent_id, |ui| show_children(ui, folder, context, action));
    }
}

/// Draw one file row; clicking it asks the app to open the file.
fn show_file(ui: &mut Ui, file: &FsNode, context: &TreeContext, action: &mut Option<ExplorerAction>) {
    let is_open = context.open_file == Some(file.path.as_path());
    let dirty_marker = if is_open && context.open_file_dirty { " •" } else { "" };
    let label = format!("{ICON_GAP}{}{}{dirty_marker}", file.name, link_marker(file));
    let response = tree_row(ui, is_open, label);
    if response.clicked() {
        *action = Some(ExplorerAction::OpenFile(file.path.clone()));
    }
    if response.hovered() {
        set_hovered(ui, HoveredFile { path: file.path.clone(), rect: response.rect });
    }
    file_menu(&response, &file.path, context, action);
}

/// One explorer row on a single line (the tree scrolls sideways for long names).
fn tree_row(ui: &mut Ui, selected: bool, label: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(egui::Button::selectable(selected, label.into()))
}

/// Room at the start of each row for the folder arrow, so file names line
/// up with folder names.
const ICON_GAP: &str = "     ";
/// Width and height of the folder arrow, in points.
const ARROW_SIZE: f32 = 7.0;

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

/// Right-click menu of a file: the copy actions, then "Rename…".
fn file_menu(response: &egui::Response, path: &Path, context: &TreeContext, action: &mut Option<ExplorerAction>) {
    response.context_menu(|ui| {
        copy_items(ui, path, context, action);
        ui.separator();
        if ui.button("Rename…").clicked() {
            *action = Some(ExplorerAction::Rename(path.to_path_buf()));
            ui.close();
        }
    });
}

/// Right-click menu with the three copy actions.
fn path_menu(response: &egui::Response, path: &Path, context: &TreeContext, action: &mut Option<ExplorerAction>) {
    response.context_menu(|ui| copy_items(ui, path, context, action));
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
