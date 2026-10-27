//! Draw the file tree and report clicks and copy requests back to the app.

use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText, Ui};

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
}

/// Read-only facts the tree needs while drawing.
#[derive(Clone, Copy, Debug)]
pub struct TreeContext<'a> {
    /// Explorer root, for "Copy relative path".
    pub root: &'a Path,
    /// Hide files that are not Markdown.
    pub markdown_only: bool,
    /// File shown in the editor, highlighted in the tree.
    pub open_file: Option<&'a Path>,
    /// The open file has unsaved changes (shows a dot after its name).
    pub open_file_dirty: bool,
}

/// Draw the explorer panel: filter checkbox, refresh button and the tree.
pub fn show_explorer(
    ui: &mut Ui,
    root: &mut FsNode,
    markdown_only: &mut bool,
    context: &TreeContext,
) -> Option<ExplorerAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.checkbox(markdown_only, "Markdown only").on_hover_text("Hide files that are not Markdown");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Refresh").clicked() {
                action = Some(ExplorerAction::Refresh);
            }
        });
    });
    ui.separator();

    // The checkbox above may have just changed the filter.
    let context = TreeContext { markdown_only: *markdown_only, ..*context };
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
        let root_label = ui.label(RichText::new(&root.name).strong());
        path_menu(&root_label, &root.path, &context, &mut action);
        show_children(ui, root, &context, &mut action);
    });
    action
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
    let response = ui.selectable_label(false, label);
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
    let dirty_marker = if is_open && context.open_file_dirty { " ●" } else { "" };
    let label = format!("{ICON_GAP}{}{}{dirty_marker}", file.name, link_marker(file));
    let response = ui.selectable_label(is_open, label);
    if response.clicked() {
        *action = Some(ExplorerAction::OpenFile(file.path.clone()));
    }
    path_menu(&response, &file.path, context, action);
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

/// Right-click menu with the three copy actions.
fn path_menu(response: &egui::Response, path: &Path, context: &TreeContext, action: &mut Option<ExplorerAction>) {
    response.context_menu(|ui| {
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
    });
}
