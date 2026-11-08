//! Section headers of the explorer ("FILES", "TERMINALS"): a fold arrow and
//! a small uppercase title on the left, the section's buttons on the right.
//! Both sections share this look so the panel reads as one list of parts.

use eframe::egui::{self, Color32, Painter, Pos2, Sense, Ui, vec2};

/// Height of a section header row.
pub const SECTION_HEADER_HEIGHT: f32 = 28.0;
/// Size of the title text: smaller than the file names, like a label, but
/// not so small next to 14 point names that it drops out.
const TITLE_SIZE: f32 = 12.0;
/// Room before the title for the fold arrow.
const ARROW_ROOM: f32 = 18.0;
/// Width and height of a fold arrow, in points.
const ARROW_SIZE: f32 = 7.0;

/// One section header. `title` is drawn as given (callers pass it in
/// capitals). `after_title` adds small details next to the title (a count,
/// status dots); `buttons` adds the section's buttons, right-aligned.
/// Returns true when the title was clicked (the caller folds or unfolds
/// the section).
pub fn section_header(
    ui: &mut Ui,
    title: &str,
    expanded: bool,
    after_title: impl FnOnce(&mut Ui),
    buttons: impl FnOnce(&mut Ui),
) -> bool {
    let layout = egui::Layout::left_to_right(egui::Align::Center);
    let size = vec2(ui.available_width(), SECTION_HEADER_HEIGHT);
    ui.allocate_ui_with_layout(size, layout, |ui| {
        let clicked = section_title(ui, title, expanded);
        after_title(ui);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), buttons);
        clicked
    })
    .inner
}

/// The fold arrow and the title, as one click target.
fn section_title(ui: &mut Ui, title: &str, expanded: bool) -> bool {
    let font = egui::FontId::proportional(TITLE_SIZE);
    let galley = ui.painter().layout_no_wrap(title.to_string(), font, Color32::PLACEHOLDER);
    let size = vec2(ARROW_ROOM + galley.size().x + 6.0, SECTION_HEADER_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let visuals = ui.visuals();
    let color = if response.hovered() { visuals.strong_text_color() } else { visuals.weak_text_color() };
    let arrow_center = egui::pos2(rect.left() + ARROW_ROOM / 2.0, rect.center().y);
    paint_fold_arrow(ui.painter(), arrow_center, expanded, color);
    let text_top = rect.center().y - galley.size().y / 2.0;
    ui.painter().galley_with_override_text_color(egui::pos2(rect.left() + ARROW_ROOM, text_top), galley, color);
    let state = if expanded { "Collapse" } else { "Expand" };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
    response.on_hover_text(format!("{state} {}", title.to_lowercase())).clicked()
}

/// A small triangle: pointing right when folded, down when open. Painted by
/// hand because the default egui fonts have no arrow glyphs.
pub fn paint_fold_arrow(painter: &Painter, center: Pos2, expanded: bool, color: Color32) {
    let half = ARROW_SIZE / 2.0;
    let points = if expanded {
        vec![center + vec2(-half, -half / 2.0), center + vec2(half, -half / 2.0), center + vec2(0.0, half)]
    } else {
        vec![center + vec2(-half / 2.0, -half), center + vec2(half, 0.0), center + vec2(-half / 2.0, half)]
    };
    painter.add(egui::Shape::convex_polygon(points, color, egui::Stroke::NONE));
}
