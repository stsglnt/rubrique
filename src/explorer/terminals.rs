//! Terminals in the explorer: the Terminals section (docked at the bottom
//! of the panel, foldable and resizable), and the rows of `.tmux` files. Each row shows the terminal icon, the
//! name, then a colored status dot with the agent's state and when the
//! session was opened.

use eframe::egui::text::LayoutJob;
use eframe::egui::{self, Align, Color32, FontSelection, RichText, Ui, Visuals};

use super::section::{SECTION_HEADER_HEIGHT, section_header};
use super::view::{ExplorerAction, HEADER_BUTTON_SIZE, ICON_GAP, NOTE_MARK_SIZE, TreeContext, file_menu, tree_row};
use crate::explorer::tree::FsNode;
use crate::icons::{Icon, paint_icon, small_icon_button};
use crate::terminal::monitor::{TerminalRow, ago_label, opened_label};
use crate::terminal::status::TerminalStatus;
use crate::terminal::tmux::session_name_for_file;

/// Height of the open Terminals section until the user drags its edge.
const TERMINALS_DEFAULT_HEIGHT: f32 = 180.0;
/// The open section keeps room for its header and about two rows.
const TERMINALS_MIN_HEIGHT: f32 = 90.0;
/// Space above the Terminals header, under the panel's dividing line.
const SECTION_GAP: i8 = 4;
/// Status color of an idle agent: the green used for "all good".
const IDLE_COLOR: Color32 = Color32::from_rgb(0x34, 0xa8, 0x53);
/// Spaces left after the name for the status dot to be painted in.
const DOT_ROOM: &str = "     ";
const DOT_RADIUS: f32 = 3.5;

/// The Terminals section docked at the bottom of the explorer: a panel the
/// user can resize by its top edge, or only its header while folded.
/// Nothing when tmux is not installed.
pub(super) fn show_docked_terminals(
    ui: &mut Ui,
    context: &TreeContext,
    collapsed: &mut bool,
    action: &mut Option<ExplorerAction>,
) {
    if context.terminals.is_none() {
        return;
    }
    let frame = egui::Frame::new().inner_margin(egui::Margin { top: SECTION_GAP, ..egui::Margin::ZERO });
    // Folded and open use their own ids, so folding keeps the height the
    // user dragged the open section to.
    let panel = if *collapsed {
        egui::Panel::bottom("terminals-folded").exact_size(SECTION_HEADER_HEIGHT + f32::from(SECTION_GAP))
    } else {
        egui::Panel::bottom("terminals")
            .resizable(true)
            .default_size(TERMINALS_DEFAULT_HEIGHT)
            .size_range(TERMINALS_MIN_HEIGHT..=f32::INFINITY)
    };
    panel.frame(frame).show(ui, |ui| show_terminals_section(ui, context, collapsed, action));
}

/// The "TERMINALS" header (with a count, status dots while folded, and
/// "New terminal"), then one row per tmux session, newest first, in its own
/// scroll area. Nothing when tmux is not installed.
pub(super) fn show_terminals_section(
    ui: &mut Ui,
    context: &TreeContext,
    collapsed: &mut bool,
    action: &mut Option<ExplorerAction>,
) {
    let Some(rows) = context.terminals else {
        return;
    };
    let expanded = !*collapsed;
    let toggled = section_header(
        ui,
        "TERMINALS",
        expanded,
        |ui| header_details(ui, rows, expanded),
        |ui| {
            let tip = "New terminal: a tmux session in the first folder";
            if small_icon_button(ui, Icon::Plus, HEADER_BUTTON_SIZE, "New terminal", tip).clicked() {
                *action = Some(ExplorerAction::NewTerminal);
            }
        },
    );
    if toggled {
        *collapsed = expanded;
    }
    if !expanded {
        return;
    }
    if rows.is_empty() {
        let hint = "No tmux sessions yet. Start one with +, or right-click a folder for a .tmux file.";
        ui.add(egui::Label::new(RichText::new(hint).small().weak()).wrap());
        return;
    }
    egui::ScrollArea::both().id_salt("terminal-list").auto_shrink([false, false]).show(ui, |ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        for row in rows {
            terminal_row(ui, row, context, action);
        }
    });
}

/// One session in the list: a click shows it in a tab.
fn terminal_row(ui: &mut Ui, row: &TerminalRow, context: &TreeContext, action: &mut Option<ExplorerAction>) {
    let selected = context.open_terminal == Some(row.info.name.as_str());
    let label = terminal_label(ui, &row.info.name, Some(row));
    let response = tree_row(ui, selected, label).on_hover_text(terminal_tip(Some(row)));
    paint_terminal_icon(ui, &response);
    paint_status_dot(ui, &response, &row.info.name, Some(&row.status));
    if response.clicked() {
        *action = Some(ExplorerAction::OpenTerminal(row.info.name.clone()));
    }
}

/// Next to the title: how many sessions there are and, while the section
/// is folded, a dot and count for agents that need you or are working, so
/// nothing important hides behind the fold.
fn header_details(ui: &mut Ui, rows: &[TerminalRow], expanded: bool) {
    if rows.is_empty() {
        return;
    }
    ui.label(RichText::new(rows.len().to_string()).small().weak());
    if expanded {
        return;
    }
    let needs_you = rows.iter().filter(|row| matches!(row.status, TerminalStatus::NeedsYou(_))).count();
    let working = rows.iter().filter(|row| matches!(row.status, TerminalStatus::Working(_))).count();
    let visuals = ui.visuals().clone();
    for (count, color, meaning) in
        [(needs_you, visuals.warn_fg_color, "need you"), (working, visuals.hyperlink_color, "working")]
    {
        if count == 0 {
            continue;
        }
        let (rect, response) = ui.allocate_exact_size(egui::vec2(DOT_RADIUS * 2.0 + 4.0, 12.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), DOT_RADIUS, color);
        response.on_hover_text(format!("{count} {meaning}"));
        ui.label(RichText::new(count.to_string()).small().color(color));
    }
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
    paint_terminal_icon(ui, &response);
    paint_status_dot(ui, &response, &file.name, row.map(|row| &row.status));
    if response.double_clicked() {
        *action = Some(ExplorerAction::RenameInTree(file.path.clone()));
    } else if response.clicked() {
        *action = Some(ExplorerAction::OpenFile(file.path.clone()));
    }
    file_menu(&response, &file.path, context, action);
}

/// The name, room for the status dot, then the status and opening time in
/// small text: "build  ●  claude · working · 14:02".
fn terminal_label(ui: &Ui, name: &str, row: Option<&TerminalRow>) -> egui::WidgetText {
    let mut job = LayoutJob::default();
    let style = ui.style();
    RichText::new(format!("{ICON_GAP}{name}{DOT_ROOM}")).append_to(
        &mut job,
        style,
        FontSelection::Default,
        Align::Center,
    );
    let details = match row {
        Some(row) => format!("{} · {}", row.status.label(), opened_label(row.info.created)),
        None => "not running".to_string(),
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

/// Paint the terminal icon in the gap before the name, in one quiet color:
/// the status dot carries the color.
fn paint_terminal_icon(ui: &Ui, response: &egui::Response) {
    let visuals = ui.visuals();
    let center = egui::pos2(
        response.rect.left() + ui.spacing().button_padding.x + NOTE_MARK_SIZE / 2.0,
        response.rect.center().y,
    );
    let behind = if response.hovered() { visuals.widgets.hovered.weak_bg_fill } else { visuals.panel_fill };
    let color = visuals.weak_text_color();
    paint_icon(ui.painter(), Icon::Terminal, center, color, color, behind);
}

/// Paint the status dot in the room left after `name`: filled in the
/// status color, or a hollow gray ring for a terminal that is not running.
fn paint_status_dot(ui: &Ui, response: &egui::Response, name: &str, status: Option<&TerminalStatus>) {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let width_of =
        |text: String| ui.fonts_mut(|fonts| fonts.layout_no_wrap(text, font.clone(), Color32::WHITE).size().x);
    let name_width = width_of(format!("{ICON_GAP}{name}"));
    let room_width = width_of(DOT_ROOM.to_string());
    let x = response.rect.left() + ui.spacing().button_padding.x + name_width + room_width / 2.0;
    let center = egui::pos2(x, response.rect.center().y);
    let visuals = ui.visuals();
    if let Some(status) = status {
        ui.painter().circle_filled(center, DOT_RADIUS, status_color(status, visuals));
    } else {
        let ring = egui::Stroke::new(1.2, visuals.weak_text_color());
        ui.painter().circle_stroke(center, DOT_RADIUS - 0.6, ring);
    }
}

/// Blue while an agent works, orange when it needs you, green when it is
/// idle, gray for shells and other programs.
fn status_color(status: &TerminalStatus, visuals: &Visuals) -> Color32 {
    match status {
        TerminalStatus::Working(_) => visuals.hyperlink_color,
        TerminalStatus::NeedsYou(_) => visuals.warn_fg_color,
        TerminalStatus::Idle(_) => IDLE_COLOR,
        TerminalStatus::Shell | TerminalStatus::Program(_) => visuals.weak_text_color(),
    }
}
