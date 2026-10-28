//! Small line icons drawn with the painter (the built-in fonts have no icon
//! glyphs), and a flat button that shows one.

use std::f32::consts::TAU;

use eframe::egui::{self, Color32, Painter, Pos2, Response, Sense, Stroke, Ui, Vec2, WidgetInfo, pos2, vec2};

/// Size of an icon button.
const BUTTON_SIZE: Vec2 = vec2(28.0, 24.0);
/// Line width of every icon (heavy, like the app icon's monogram).
const LINE: f32 = 1.8;

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
    paint(painter, icon, rect.center(), color, visuals.hyperlink_color);
    response.on_hover_text(tip)
}

/// Draw `icon` centered on `center` in rubrique's icon style (see
/// `.claude/skills/rubrique-icons/SKILL.md`): a 14 × 14 box, heavy strokes with
/// round ends and joins, softly rounded corners, and one detail in `accent`.
fn paint(painter: &Painter, icon: Icon, center: Pos2, color: Color32, accent: Color32) {
    let line = Stroke::new(LINE, color);
    let detail = Stroke::new(LINE, accent);
    let at = |x: f32, y: f32| center + vec2(x, y);
    match icon {
        Icon::Gear => {
            // Ring with six rounded teeth; the hub is the accent.
            stroke_path(painter, &circle_points(center, 4.6), true, line);
            for tooth in 0..6_u8 {
                let direction = Vec2::angled(f32::from(tooth) * TAU / 6.0 + TAU / 12.0);
                stroke_path(painter, &[center + direction * 4.6, center + direction * 6.6], false, line);
            }
            painter.circle_filled(center, 1.6, accent);
        }
        Icon::FolderPlus => {
            // Folder with a soft tab; the plus is the accent.
            let outline = [at(-7.0, -5.5), at(-2.6, -5.5), at(-1.0, -3.8), at(7.0, -3.8), at(7.0, 6.0), at(-7.0, 6.0)];
            stroke_path(painter, &round_corners(&outline, CORNER, true), true, line);
            plus(painter, at(0.0, 1.2), PLUS_HALF, detail);
        }
        Icon::NotePlus => {
            // Page with a folded corner, the same height as the folder; the plus is the accent.
            let outline = [at(-5.5, -5.5), at(2.0, -5.5), at(5.5, -2.0), at(5.5, 6.0), at(-5.5, 6.0)];
            stroke_path(painter, &round_corners(&outline, CORNER, true), true, line);
            stroke_path(painter, &[at(2.0, -5.5), at(2.0, -2.0), at(5.5, -2.0)], false, line);
            plus(painter, at(0.0, 2.0), PLUS_HALF, detail);
        }
        Icon::Refresh => {
            // Most of a circle; the arrowhead at its end is the accent.
            let radius = 5.6;
            let points: Vec<Pos2> = (0..=24_u8)
                .map(|step| center + Vec2::angled(-TAU * 0.22 + f32::from(step) / 24.0 * TAU * 0.78) * radius)
                .collect();
            let end_angle = -TAU * 0.22 + TAU * 0.78;
            let end = points[points.len() - 1];
            stroke_path(painter, &points, false, line);
            // Arrowhead along the circle's direction at its end (y points down,
            // so a growing angle turns clockwise on screen).
            let ahead = Vec2::angled(end_angle + TAU / 4.0);
            let outward = Vec2::angled(end_angle);
            let tip = end + ahead * 1.2;
            let wings = [tip - ahead * 3.0 + outward * 2.6, tip, tip - ahead * 3.0 - outward * 2.6];
            stroke_path(painter, &wings, false, detail);
        }
    }
}

/// Corner radius of outlines.
const CORNER: f32 = 2.0;
/// Half the length of a plus sign's arms.
const PLUS_HALF: f32 = 2.6;

/// A path with round joins and round ends: the line, plus a dot of the
/// line's width at every point (egui draws plain lines with square ends).
fn stroke_path(painter: &Painter, points: &[Pos2], closed: bool, stroke: Stroke) {
    let shape = if closed {
        egui::Shape::closed_line(points.to_vec(), stroke)
    } else {
        egui::Shape::line(points.to_vec(), stroke)
    };
    painter.add(shape);
    for point in points {
        painter.circle_filled(*point, stroke.width / 2.0, stroke.color);
    }
}

/// `outline` with every corner replaced by a short curve of about `radius`.
fn round_corners(outline: &[Pos2], radius: f32, closed: bool) -> Vec<Pos2> {
    let count = outline.len();
    let mut points = Vec::with_capacity(count * 5);
    for index in 0..count {
        let corner = outline[index];
        let is_end = !closed && (index == 0 || index == count - 1);
        if is_end {
            points.push(corner);
            continue;
        }
        let previous = outline[(index + count - 1) % count];
        let next = outline[(index + 1) % count];
        let into = (corner - previous).normalized();
        let out_of = (next - corner).normalized();
        let reach = radius.min((corner - previous).length() / 2.0).min((next - corner).length() / 2.0);
        let start = corner - into * reach;
        let end = corner + out_of * reach;
        // Quadratic curve from `start` to `end` with the corner as control point.
        for step in 0..=4_u8 {
            let t = f32::from(step) / 4.0;
            let a = start + (corner - start) * t;
            let b = corner + (end - corner) * t;
            points.push(a + (b - a) * t);
        }
    }
    points
}

/// Points around a circle, for paths that need round joins.
fn circle_points(center: Pos2, radius: f32) -> Vec<Pos2> {
    (0..32_u8).map(|step| center + Vec2::angled(f32::from(step) / 32.0 * TAU) * radius).collect()
}

/// A plus sign with round ends.
fn plus(painter: &Painter, center: Pos2, half: f32, stroke: Stroke) {
    stroke_path(painter, &[pos2(center.x - half, center.y), pos2(center.x + half, center.y)], false, stroke);
    stroke_path(painter, &[pos2(center.x, center.y - half), pos2(center.x, center.y + half)], false, stroke);
}
