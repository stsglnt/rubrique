//! Terminals in the explorer: the Terminals list at the top of the panel,
//! and the rows of `.tmux` files. Both show the terminal icon in the color
//! of their status, the agent's state, and when the session was opened.

use eframe::egui::text::LayoutJob;
use eframe::egui::{self, Align, Color32, FontSelection, RichText, Ui, Visuals};

use super::view::{ExplorerAction, ICON_GAP, NOTE_MARK_SIZE, TreeContext, file_menu, tree_row};
use crate::explorer::tree::FsNode;
use crate::icons::{Icon, paint_icon, small_icon_button};
use crate::terminal::monitor::{TerminalRow, ago_label, opened_label};
use crate::terminal::status::TerminalStatus;
use crate::terminal::tmux::session_name_for_file;

/// Size of the "New terminal" button in the list's header.
const NEW_BUTTON_SIZE: f32 = 20.0;
/// Status color of an idle agent: the green used for "all good".
const IDLE_COLOR: Color32 = Color32::from_rgb(0x34, 0xa8, 0x53);

/// The Terminals list: a header with "New terminal", then one row per tmux
/// session, newest first. Nothing when tmux is not installed.
pub(super) fn show_terminals(ui: &mut Ui, context: &TreeContext, action: &mut Option<ExplorerAction>) {
    let Some(rows) = context.terminals else {
        return;
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new("Terminals").strong());
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            let tip = "Start a new tmux terminal in the first folder";
            if small_icon_button(ui, Icon::Terminal, NEW_BUTTON_SIZE, "New terminal", tip).clicked() {
                *action = Some(ExplorerAction::NewTerminal);
            }
        });
    });
    if rows.is_empty() {
        ui.label(RichText::new("No tmux sessions yet").small().weak());
    }
    for row in rows {
        let selected = context.open_terminal == Some(row.info.name.as_str());
        let label = terminal_label(ui, &row.info.name, Some(row));
        let response = tree_row(ui, selected, label).on_hover_text(terminal_tip(Some(row)));
        paint_terminal_icon(ui, &response, Some(&row.status));
        if response.clicked() {
            *action = Some(ExplorerAction::OpenTerminal(row.info.name.clone()));
        }
    }
    ui.separator();
}

/// A `.tmux` file's row: the terminal icon, its name and its session's
/// status. A click opens it, a double click renames it.
pub(super) fn show_terminal_file(
    ui: &mut Ui,
    file: &FsNode,
    context: &TreeContext,
    action: &mut Option<ExplorerAction>,
) {
    let session = session_name_for_file(&file.path);
    let row = context.terminals.and_then(|rows| rows.iter().find(|row| row.info.name == session));
    let is_open = context.open_file == Some(file.path.as_path());
    let label = terminal_label(ui, &file.name, row);
    let response = tree_row(ui, is_open, label).on_hover_text(terminal_tip(row));
    paint_terminal_icon(ui, &response, row.map(|row| &row.status));
    if response.double_clicked() {
        *action = Some(ExplorerAction::RenameInTree(file.path.clone()));
    } else if response.clicked() {
        *action = Some(ExplorerAction::OpenFile(file.path.clone()));
    }
    file_menu(&response, &file.path, context, action);
}

/// The name, then the status and opening time in small text:
/// "build   claude · working · 14:02".
fn terminal_label(ui: &Ui, name: &str, row: Option<&TerminalRow>) -> egui::WidgetText {
    let mut job = LayoutJob::default();
    let style = ui.style();
    RichText::new(format!("{ICON_GAP}{name}")).append_to(&mut job, style, FontSelection::Default, Align::Center);
    let details = match row {
        Some(row) => format!("   {} · {}", row.status.label(), opened_label(row.info.created)),
        None => "   not running".to_string(),
    };
    RichText::new(details).small().weak().append_to(&mut job, style, FontSelection::Default, Align::Center);
    job.into()
}

/// The hover text: when it was opened, its last output, and its folder.
fn terminal_tip(row: Option<&TerminalRow>) -> String {
    let Some(row) = row else {
        return "Not running: opening it starts a tmux session in this folder".to_string();
    };
    format!(
        "Session {}\nOpened {} ({})\nLast output {}\nRunning {} in {}",
        row.info.name,
        opened_label(row.info.created),
        ago_label(row.info.created),
        ago_label(row.info.last_output),
        row.info.command,
        row.info.folder.display(),
    )
}

/// Paint the terminal icon in the gap before the name, its prompt in the
/// status color.
fn paint_terminal_icon(ui: &Ui, response: &egui::Response, status: Option<&TerminalStatus>) {
    let visuals = ui.visuals();
    let center = egui::pos2(
        response.rect.left() + ui.spacing().button_padding.x + NOTE_MARK_SIZE / 2.0,
        response.rect.center().y,
    );
    let behind = if response.hovered() { visuals.widgets.hovered.weak_bg_fill } else { visuals.panel_fill };
    let accent = status_color(status, visuals);
    paint_icon(ui.painter(), Icon::Terminal, center, visuals.weak_text_color(), accent, behind);
}

/// Blue while an agent works, orange when it needs you, green when it is
/// idle, gray for shells and programs (and terminals not running).
fn status_color(status: Option<&TerminalStatus>, visuals: &Visuals) -> Color32 {
    match status {
        Some(TerminalStatus::Working(_)) => visuals.hyperlink_color,
        Some(TerminalStatus::NeedsYou(_)) => visuals.warn_fg_color,
        Some(TerminalStatus::Idle(_)) => IDLE_COLOR,
        Some(TerminalStatus::Shell | TerminalStatus::Program(_)) | None => visuals.weak_text_color(),
    }
}
