//! Small line icons drawn with the painter (the built-in fonts have no icon
//! glyphs), and a flat button that shows one.

use std::f32::consts::TAU;

use eframe::egui::{self, Color32, Painter, Pos2, Response, Sense, Stroke, Ui, Vec2, WidgetInfo, pos2, vec2};

/// Size of an icon button.
const BUTTON_SIZE: Vec2 = vec2(28.0, 24.0);
/// Line width of every icon: bold like the app icon's monogram, but thin
/// enough to stay sharp at 14 px.
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
    /// Ask AI.
    Sparkle,
    /// Stop a running AI tool.
    Stop,
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
    // A badge cuts a gap into the shape in the color behind the icon.
    let behind = if fill == Color32::TRANSPARENT { visuals.panel_fill } else { fill };
    paint(painter, icon, rect.center(), Colors { line: color, accent: visuals.hyperlink_color, behind });
    response.on_hover_text(tip)
}

/// An icon on its own (not a button), in the text color.
pub fn icon_label(ui: &mut Ui, icon: Icon) {
    let (rect, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
    let visuals = ui.visuals();
    let colors = Colors { line: visuals.text_color(), accent: visuals.hyperlink_color, behind: visuals.panel_fill };
    paint(ui.painter(), icon, rect.center(), colors);
}

/// A small button with an icon before its text (like "Ask" in the status
/// bar). `active` gives it the selection fill.
pub fn icon_text_button(ui: &mut Ui, icon: Icon, text: &str, tip: &str, active: bool) -> Response {
    const GAP: f32 = 6.0;
    const ICON: f32 = 14.0;
    let padding = ui.spacing().button_padding;
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, Color32::PLACEHOLDER);
    let height = ui.spacing().interact_size.y.min(galley.size().y + 2.0 * padding.y).max(ICON + 4.0);
    let size = vec2(padding.x * 2.0 + ICON + GAP + galley.size().x, height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(egui::Role::Button, true, text));
    let visuals = ui.visuals();
    let widget = ui.style().interact(&response);
    let fill = if active { visuals.selection.bg_fill } else { widget.weak_bg_fill };
    let color = if active { visuals.strong_text_color() } else { widget.fg_stroke.color };
    let painter = ui.painter();
    painter.rect_filled(rect, widget.corner_radius, fill);
    let icon_center = pos2(rect.left() + padding.x + ICON / 2.0, rect.center().y);
    paint(painter, icon, icon_center, Colors { line: color, accent: visuals.hyperlink_color, behind: fill });
    let text_pos = pos2(rect.left() + padding.x + ICON + GAP, rect.center().y - galley.size().y / 2.0);
    painter.galley(text_pos, galley, color);
    response.on_hover_text(tip)
}

/// Draw `icon` centered on `center` in rubrique's icon style (see
/// `.claude/skills/rubrique-icons/SKILL.md`): a 14 × 14 box, heavy strokes with
/// round ends and joins, softly rounded corners, and one detail in `accent`.
/// The colors an icon is drawn with.
#[derive(Clone, Copy)]
struct Colors {
    line: Color32,
    /// The one accent detail.
    accent: Color32,
    /// What is behind the icon, for the gap around a badge.
    behind: Color32,
}

fn paint(painter: &Painter, icon: Icon, center: Pos2, colors: Colors) {
    // Whole pixels keep the thin lines sharp.
    let center = center.round();
    let Colors { line: color, accent, behind } = colors;
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
            // Folder with a tab; the plus badge at the bottom right is the accent.
            let outline = [at(-6.5, -4.5), at(-2.0, -4.5), at(-0.5, -3.0), at(6.5, -3.0), at(6.5, 5.5), at(-6.5, 5.5)];
            stroke_path(painter, &round_corners(&outline, CORNER, true), true, line);
            plus_badge(painter, at(6.0, 5.5), detail, behind);
        }
        Icon::NotePlus => {
            // Page with a cut corner; the plus badge at the bottom right is the accent.
            let outline = [at(-4.5, -6.5), at(1.0, -6.5), at(4.5, -3.0), at(4.5, 6.5), at(-4.5, 6.5)];
            stroke_path(painter, &round_corners(&outline, CORNER, true), true, line);
            plus_badge(painter, at(5.0, 5.5), detail, behind);
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
        Icon::Sparkle => {
            // A four-point star with curved sides; a small plus is the accent.
            let tips = [at(-1.5, -4.5), at(3.5, 1.0), at(-1.5, 6.5), at(-6.5, 1.0)];
            let waist = [at(-0.9, 0.4), at(-0.9, 1.6), at(-2.1, 1.6), at(-2.1, 0.4)];
            let mut outline = Vec::new();
            for side in 0..4 {
                outline.extend(quadratic(tips[side], waist[side], tips[(side + 1) % 4]));
            }
            stroke_path(painter, &outline, true, line);
            plus(painter, at(4.6, -4.5), 1.9, detail);
        }
        Icon::Stop => {
            // A circle; the square inside is the accent.
            stroke_path(painter, &circle_points(center, 6.2), true, line);
            let square = egui::Rect::from_center_size(center, vec2(3.8, 3.8));
            painter.rect_filled(square, 1.0, accent);
        }
    }
}

/// Points along a quadratic curve from `start` to `end` bent toward `control`.
fn quadratic(start: Pos2, control: Pos2, end: Pos2) -> Vec<Pos2> {
    (0..=8_u8)
        .map(|step| {
            let t = f32::from(step) / 8.0;
            let a = start + (control - start) * t;
            let b = control + (end - control) * t;
            a + (b - a) * t
        })
        .collect()
}

/// Corner radius of outlines.
const CORNER: f32 = 1.5;
/// Half the length of a badge plus sign's arms.
const PLUS_HALF: f32 = 2.25;
/// Radius of the gap cut around a badge.
const BADGE_GAP: f32 = 3.6;

/// A plus badge: a gap cut into the shape behind it, then the plus.
fn plus_badge(painter: &Painter, center: Pos2, stroke: Stroke, behind: Color32) {
    painter.circle_filled(center, BADGE_GAP, behind);
    plus(painter, center, PLUS_HALF, stroke);
}

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
