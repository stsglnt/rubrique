//! The merge screen: the file on disk, the result, and my version side by
//! side (sides can be swapped), with a choice for each conflict.

use eframe::egui::{self, Color32, RichText, Ui};

use super::merge::{Chunk, ChunkKind, MergeSession};

/// What the user asked for on the merge screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeAction {
    /// Put the result into the editor.
    Apply,
    /// Leave the merge; nothing changes.
    Cancel,
}

/// Opacity of the tint behind changed and conflicting chunks.
const TINT: f32 = 0.14;
/// Unchanged stretches longer than this are folded in the middle.
const FOLD_OVER_LINES: usize = 8;
/// Lines kept at each end of a folded stretch.
const FOLD_KEEP: usize = 3;
const CHUNK_GAP: f32 = 6.0;

/// Which version a side column shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Version {
    Theirs,
    Mine,
}

impl Version {
    fn title(self) -> &'static str {
        match self {
            Version::Theirs => "On disk (changed elsewhere)",
            Version::Mine => "Mine (in the editor)",
        }
    }

    fn lines(self, chunk: &Chunk) -> &[String] {
        match self {
            Version::Theirs => &chunk.theirs,
            Version::Mine => &chunk.mine,
        }
    }

    /// The chunk kind that means "only this version changed it".
    fn own_kind(self) -> ChunkKind {
        match self {
            Version::Theirs => ChunkKind::Theirs,
            Version::Mine => ChunkKind::Mine,
        }
    }
}

/// Draw the merge screen for `file_name`. Returns Apply or Cancel when clicked.
pub fn show_merge(ui: &mut Ui, session: &mut MergeSession, file_name: &str) -> Option<MergeAction> {
    let mut action = None;
    let (left, right) =
        if session.swapped { (Version::Mine, Version::Theirs) } else { (Version::Theirs, Version::Mine) };

    egui::Frame::new().fill(ui.visuals().panel_fill).inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Merge changes · {file_name}")).strong().size(16.0));
            let left_count = session.conflicts_left();
            let status = match left_count {
                0 => "No conflicts left".to_string(),
                1 => "1 conflict left".to_string(),
                count => format!("{count} conflicts left"),
            };
            ui.label(RichText::new(status).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let apply = ui
                    .add_enabled(left_count == 0, egui::Button::new("Apply"))
                    .on_hover_text("Put the result in the editor; save when you are ready")
                    .on_disabled_hover_text("Decide every conflict first");
                if apply.clicked() {
                    action = Some(MergeAction::Apply);
                }
                if ui.button("Cancel").clicked() {
                    action = Some(MergeAction::Cancel);
                }
                ui.separator();
                if ui.button("Swap sides").on_hover_text("Show mine on the left and theirs on the right").clicked() {
                    session.swapped = !session.swapped;
                }
                if left_count > 0 {
                    if ui.button("All from right").clicked() {
                        session.take_all(right == Version::Theirs);
                    }
                    if ui.button("All from left").clicked() {
                        session.take_all(left == Version::Theirs);
                    }
                }
            });
        });
    });
    ui.separator();

    ui.columns(3, |columns| {
        columns[0].label(RichText::new(left.title()).strong());
        columns[1].label(RichText::new("Result").strong());
        columns[2].label(RichText::new(right.title()).strong());
    });

    egui::ScrollArea::vertical().id_salt("merge-scroll").auto_shrink([false, false]).show(ui, |ui| {
        for index in 0..session.chunks.len() {
            ui.add_space(CHUNK_GAP);
            show_chunk(ui, session, index, left, right);
        }
        ui.add_space(CHUNK_GAP * 4.0);
    });
    action
}

/// One chunk as a row of three columns.
fn show_chunk(ui: &mut Ui, session: &mut MergeSession, index: usize, left: Version, right: Version) {
    let chunk = session.chunks[index].clone();
    let visuals = ui.visuals().clone();
    let change_tint = visuals.hyperlink_color.gamma_multiply(TINT);
    let conflict_tint = visuals.warn_fg_color.gamma_multiply(TINT);
    let side_tint = |version: Version| match chunk.kind {
        ChunkKind::Conflict => conflict_tint,
        kind if kind == version.own_kind() || kind == ChunkKind::Both => change_tint,
        _ => Color32::TRANSPARENT,
    };
    let middle_tint = match chunk.kind {
        ChunkKind::Same => Color32::TRANSPARENT,
        ChunkKind::Conflict => conflict_tint,
        _ => change_tint,
    };

    ui.columns(3, |columns| {
        tinted(&mut columns[0], side_tint(left), |ui| {
            side_lines(ui, &chunk, left);
            if chunk.kind == ChunkKind::Conflict && ui.small_button("Use left").clicked() {
                session.resolutions[index] = Some(left.lines(&chunk).concat());
            }
        });
        tinted(&mut columns[2], side_tint(right), |ui| {
            side_lines(ui, &chunk, right);
            if chunk.kind == ChunkKind::Conflict && ui.small_button("Use right").clicked() {
                session.resolutions[index] = Some(right.lines(&chunk).concat());
            }
        });
        tinted(&mut columns[1], middle_tint, |ui| match chunk.auto_text() {
            Some(text) if chunk.kind == ChunkKind::Same => folded_lines(ui, &super::merge::lines(&text), false),
            Some(text) => code_lines(ui, &text, false),
            None => conflict_result(ui, session, index, &chunk, left, right),
        });
    });
}

/// The middle of a conflict: buttons until decided, then an editable result.
fn conflict_result(
    ui: &mut Ui,
    session: &mut MergeSession,
    index: usize,
    chunk: &Chunk,
    left: Version,
    right: Version,
) {
    if let Some(text) = &mut session.resolutions[index] {
        let rows = text.lines().count().max(1);
        ui.add(egui::TextEdit::multiline(text).code_editor().desired_width(f32::INFINITY).desired_rows(rows));
        if ui.small_button("Undo choice").clicked() {
            session.resolutions[index] = None;
        }
    } else {
        ui.label(RichText::new("Conflict: both changed these lines. Pick a side, or").italics().weak());
        if ui.small_button("Both (left, then right)").clicked() {
            let both = [left.lines(chunk).concat(), right.lines(chunk).concat()].concat();
            session.resolutions[index] = Some(both);
        }
    }
}

/// A side column: its lines (folded when long and unchanged).
fn side_lines(ui: &mut Ui, chunk: &Chunk, version: Version) {
    let lines = version.lines(chunk);
    if chunk.kind == ChunkKind::Same {
        folded_lines(ui, lines, false);
    } else if lines.is_empty() {
        ui.label(RichText::new("(no lines)").italics().weak());
    } else {
        code_lines(ui, &lines.concat(), false);
    }
}

/// Unchanged lines, with the middle of long stretches replaced by a count.
fn folded_lines(ui: &mut Ui, lines: &[String], strong: bool) {
    if lines.len() <= FOLD_OVER_LINES {
        code_lines(ui, &lines.concat(), strong);
        return;
    }
    code_lines(ui, &lines[..FOLD_KEEP].concat(), strong);
    let hidden = lines.len() - 2 * FOLD_KEEP;
    ui.label(RichText::new(format!("⋯ {hidden} unchanged lines")).weak().small());
    code_lines(ui, &lines[lines.len() - FOLD_KEEP..].concat(), strong);
}

/// Monospace text without its final line break.
fn code_lines(ui: &mut Ui, text: &str, strong: bool) {
    let shown = text.strip_suffix('\n').unwrap_or(text);
    let mut rich = RichText::new(shown).monospace();
    if strong {
        rich = rich.strong();
    }
    ui.label(rich);
}

/// Content on a rounded, tinted background that fills the column width.
fn tinted(ui: &mut Ui, fill: Color32, add_contents: impl FnOnce(&mut Ui)) {
    egui::Frame::new().fill(fill).corner_radius(4).inner_margin(6).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add_contents(ui);
    });
}
