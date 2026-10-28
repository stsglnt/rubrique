//! Small line icons drawn with the painter (the built-in fonts have no icon
//! glyphs), and a flat button that shows one.

use std::f32::consts::TAU;

use eframe::egui::{self, Color32, Painter, Pos2, Response, Sense, Stroke, Ui, Vec2, WidgetInfo, pos2, vec2};

/// Size of an icon button.
const BUTTON_SIZE: Vec2 = vec2(28.0, 24.0);
/// Line width of every icon.
const LINE: f32 = 1.5;

/// The icons rubrique draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// Settings.
    Gear,
    /// Add a folder to the explorer.
    FolderPlus,
    /// New scratch file.
    NotePlus,
    /// Re-read from disk.
    Refresh,
}

/// A flat button with `icon` and a hover tip. `active` keeps it highlighted
/// (for a panel that is open). `label` is what screen readers announce.
pub fn icon_button(ui: &mut Ui, icon: Icon, label: &str, tip: &str, active: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(BUTTON_SIZE, Sense::click());
    response.widget_info(|| WidgetInfo::selected(egui::Role::Button, true, active, label));
    let visuals = ui.visuals();
    let fill = if active {
        visuals.selection.bg_fill
    } else if response.hovered() {
        visuals.widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };
    let color = if active { visuals.strong_text_color() } else { ui.style().interact(&response).fg_stroke.color };
    let painter = ui.painter();
    painter.rect_filled(rect, visuals.widgets.inactive.corner_radius, fill);
    paint(painter, icon, rect.center(), color);
    response.on_hover_text(tip)
}

/// Draw `icon` centered on `center`, about 14 points wide.
fn paint(painter: &Painter, icon: Icon, center: Pos2, color: Color32) {
    let stroke = Stroke::new(LINE, color);
    let at = |x: f32, y: f32| center + vec2(x, y);
    match icon {
        Icon::Gear => {
            painter.circle_stroke(center, 5.0, stroke);
            painter.circle_stroke(center, 2.0, stroke);
            for tooth in 0..8_u8 {
                let direction = Vec2::angled(f32::from(tooth) * TAU / 8.0);
                painter.line_segment([center + direction * 5.0, center + direction * 7.5], stroke);
            }
        }
        Icon::FolderPlus => {
            // Folder with a tab on the top left, a plus inside.
            let outline =
                vec![at(-7.0, -5.0), at(-2.5, -5.0), at(-1.0, -3.0), at(7.0, -3.0), at(7.0, 5.5), at(-7.0, 5.5)];
            painter.add(egui::Shape::closed_line(outline, stroke));
            plus(painter, at(0.0, 1.3), 2.6, stroke);
        }
        Icon::NotePlus => {
            // Page with a folded corner, a plus inside.
            let outline = vec![at(-5.5, -7.0), at(2.0, -7.0), at(5.5, -3.5), at(5.5, 7.0), at(-5.5, 7.0)];
            painter.add(egui::Shape::closed_line(outline, stroke));
            painter.add(egui::Shape::line(vec![at(2.0, -7.0), at(2.0, -3.5), at(5.5, -3.5)], stroke));
            plus(painter, at(0.0, 2.0), 2.6, stroke);
        }
        Icon::Refresh => {
            // Almost-full circle with an arrowhead at its end.
            let radius = 5.5;
            let points: Vec<Pos2> = (0..=20)
                .map(|step: u8| {
                    let angle = -TAU * 0.2 + f32::from(step) / 20.0 * TAU * 0.8;
                    center + Vec2::angled(angle) * radius
                })
                .collect();
            let end = points[points.len() - 1];
            painter.add(egui::Shape::line(points, stroke));
            let tip = end + vec2(0.0, -0.5);
            painter.add(egui::Shape::line(vec![tip + vec2(-3.5, -1.0), tip, tip + vec2(-0.5, -3.8)], stroke));
        }
    }
}

/// A plus sign of half-size `half` centered on `center`.
fn plus(painter: &Painter, center: Pos2, half: f32, stroke: Stroke) {
    painter.line_segment([pos2(center.x - half, center.y), pos2(center.x + half, center.y)], stroke);
    painter.line_segment([pos2(center.x, center.y - half), pos2(center.x, center.y + half)], stroke);
}
