//! The settings panel and the gear button that opens it.

use std::path::Path;

use eframe::egui::{
    self, Align, Align2, Color32, FontId, Layout, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, Vec2,
    WidgetInfo, pos2, vec2,
};

use crate::theme::catalog::{ThemeChoice, Themes};
use crate::theme::palette::{Base, Palette, Rgb, Theme};

/// Something the user asked for in the settings panel.
#[derive(Debug)]
pub enum SettingsAction {
    PickTheme(ThemeChoice),
    ReloadThemes,
    Close,
}

/// Size of the gear button at the bottom of the explorer.
const GEAR_BUTTON_SIZE: Vec2 = vec2(30.0, 24.0);
const GEAR_RING_RADIUS: f32 = 5.0;
const GEAR_HOLE_RADIUS: f32 = 2.0;
const GEAR_TOOTH_LENGTH: f32 = 2.5;
const GEAR_TEETH: u16 = 8;
const GEAR_STROKE: f32 = 1.6;
/// Space around the panel's content.
const PANEL_MARGIN: i8 = 14;
const TITLE_SIZE: f32 = 16.0;
const SECTION_SIZE: f32 = 11.5;
/// Space between theme cards.
const CARD_GAP: f32 = 10.0;
const CARD_PADDING: f32 = 6.0;
const CARD_RADIUS: u8 = 8;
const PREVIEW_HEIGHT: f32 = 64.0;
const PREVIEW_RADIUS: u8 = 5;
/// Height of the name row under a card's preview.
const NAME_ROW_HEIGHT: f32 = 22.0;
/// Width of the explorer strip in a card's preview, as a share of the preview.
const PREVIEW_SIDEBAR_SHARE: f32 = 0.26;
const PREVIEW_PADDING: f32 = 8.0;
/// Bars standing for text in a card's preview: width share, height, kind.
const PREVIEW_BARS: [(f32, f32, BarKind); 4] =
    [(0.55, 5.0, BarKind::Accent), (0.85, 4.0, BarKind::Text), (0.7, 4.0, BarKind::Text), (0.45, 4.0, BarKind::Weak)];
const PREVIEW_BAR_GAP: f32 = 5.0;
const CHECK_SIZE: f32 = 12.0;

#[derive(Clone, Copy)]
enum BarKind {
    Accent,
    Text,
    Weak,
}

/// Gear button that opens Settings; highlighted while the panel is open.
pub fn gear_button(ui: &mut Ui, open: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(GEAR_BUTTON_SIZE, Sense::click());
    response.widget_info(|| WidgetInfo::selected(egui::Role::Button, true, open, "Settings"));
    let visuals = ui.visuals();
    let fill = if open {
        visuals.selection.bg_fill
    } else if response.hovered() {
        visuals.widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };
    let color = if open { visuals.strong_text_color() } else { ui.style().interact(&response).fg_stroke.color };
    let painter = ui.painter();
    painter.rect_filled(rect, visuals.widgets.inactive.corner_radius, fill);
    paint_gear(painter, rect.center(), color);
    response.on_hover_text("Settings (Cmd+,)")
}

/// A ring with a hole and teeth around it.
fn paint_gear(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    let stroke = Stroke::new(GEAR_STROKE, color);
    painter.circle_stroke(center, GEAR_RING_RADIUS, stroke);
    painter.circle_stroke(center, GEAR_HOLE_RADIUS, stroke);
    for tooth in 0..GEAR_TEETH {
        let angle = f32::from(tooth) * std::f32::consts::TAU / f32::from(GEAR_TEETH);
        let direction = vec2(angle.cos(), angle.sin());
        let inner = center + direction * GEAR_RING_RADIUS;
        painter.line_segment([inner, inner + direction * GEAR_TOOTH_LENGTH], stroke);
    }
}

/// Draw the settings panel's content. Returns what the user clicked.
pub fn show_settings(
    ui: &mut Ui,
    themes: &Themes,
    current: &ThemeChoice,
    themes_folder: Option<&Path>,
) -> Option<SettingsAction> {
    let mut action = None;
    egui::Frame::new().inner_margin(egui::Margin::symmetric(PANEL_MARGIN, 10)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Settings").size(TITLE_SIZE).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("×").on_hover_text("Close (Esc)").clicked() {
                    action = Some(SettingsAction::Close);
                }
            });
        });
    });
    ui.separator();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Frame::new().inner_margin(PANEL_MARGIN).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.label(RichText::new("APPEARANCE").size(SECTION_SIZE).weak());
            ui.label(RichText::new("Theme").strong());
            if let Some(choice) = theme_cards(ui, themes, current) {
                action = Some(SettingsAction::PickTheme(choice));
            }
            ui.add_space(6.0);
            ui.separator();
            ui.label(RichText::new("Your themes").strong());
            ui.label(RichText::new("Add .theme files to this folder, then reload.").weak());
            if let Some(folder) = themes_folder {
                let path = home_relative(folder);
                egui::Frame::new()
                    .fill(ui.visuals().text_edit_bg_color())
                    .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                    .corner_radius(PREVIEW_RADIUS)
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(path).monospace().size(12.0));
                    });
            }
            if ui.button("Reload themes").clicked() {
                action = Some(SettingsAction::ReloadThemes);
            }
        });
    });
    action
}

/// One card for "System" and one per theme, two per row. Returns the clicked choice.
fn theme_cards(ui: &mut Ui, themes: &Themes, current: &ThemeChoice) -> Option<ThemeChoice> {
    let light = themes.for_base(Base::Light);
    let dark = themes.for_base(Base::Dark);
    let mut cards = vec![(ThemeChoice::System, "System".to_string(), CardLook::Split(&light, &dark))];
    cards.extend(
        themes
            .list
            .iter()
            .map(|theme| (ThemeChoice::Named(theme.id.clone()), theme.name.clone(), CardLook::One(theme))),
    );
    // A saved theme that no longer exists shows as System, like the app uses it.
    let selected = match current {
        ThemeChoice::Named(id) if themes.find(id).is_some() => current.clone(),
        _ => ThemeChoice::System,
    };
    let card_width = ((ui.available_width() - CARD_GAP) / 2.0).floor();
    let mut picked = None;
    for row in cards.chunks(2) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = CARD_GAP;
            for (choice, name, look) in row {
                if theme_card(ui, card_width, name, look, *choice == selected).clicked() {
                    picked = Some(choice.clone());
                }
            }
        });
    }
    picked
}

/// What a card's preview shows.
enum CardLook<'a> {
    One(&'a Theme),
    /// Light on the left, dark on the right.
    Split(&'a Theme, &'a Theme),
}

/// A clickable card: a small picture of the theme and its name.
fn theme_card(ui: &mut Ui, width: f32, name: &str, look: &CardLook, selected: bool) -> Response {
    let size = vec2(width, CARD_PADDING * 3.0 + PREVIEW_HEIGHT + NAME_ROW_HEIGHT - CARD_PADDING);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::selected(egui::Role::RadioButton, true, selected, name));
    let visuals = ui.visuals();
    let accent = visuals.hyperlink_color;
    let ring = if selected {
        Stroke::new(2.0, accent)
    } else if response.hovered() {
        Stroke::new(1.0, visuals.weak_text_color())
    } else {
        visuals.widgets.noninteractive.bg_stroke
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CARD_RADIUS, visuals.text_edit_bg_color());
    painter.rect_stroke(rect, CARD_RADIUS, ring, StrokeKind::Inside);

    let preview = Rect::from_min_size(
        rect.min + vec2(CARD_PADDING, CARD_PADDING),
        vec2(width - 2.0 * CARD_PADDING, PREVIEW_HEIGHT),
    );
    match look {
        CardLook::One(theme) => paint_preview(painter, preview, &theme.palette, true),
        CardLook::Split(left, right) => {
            let (left_rect, right_rect) = preview.split_left_right_at_fraction(0.5);
            paint_preview(painter, left_rect, &left.palette, true);
            paint_preview(painter, right_rect, &right.palette, false);
        }
    }
    painter.rect_stroke(preview, 0, visuals.widgets.noninteractive.bg_stroke, StrokeKind::Inside);

    let name_y = preview.bottom() + NAME_ROW_HEIGHT / 2.0 + 2.0;
    let name_color = if selected { visuals.strong_text_color() } else { visuals.text_color() };
    painter.text(pos2(preview.left() + 2.0, name_y), Align2::LEFT_CENTER, name, FontId::proportional(14.0), name_color);
    if selected {
        paint_check(painter, pos2(preview.right() - CHECK_SIZE / 2.0 - 2.0, name_y), accent);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A tiny window: an explorer strip (when `sidebar`) and a page with text bars.
fn paint_preview(painter: &egui::Painter, rect: Rect, palette: &Palette, sidebar: bool) {
    let page = if sidebar {
        let (strip, page) = rect.split_left_right_at_fraction(PREVIEW_SIDEBAR_SHARE);
        painter.rect_filled(strip, 0, color(palette.sidebar));
        page
    } else {
        rect
    };
    painter.rect_filled(page, 0, color(palette.background));
    let mut y = page.top() + PREVIEW_PADDING;
    let inner_width = page.width() - 2.0 * PREVIEW_PADDING;
    for (share, height, kind) in PREVIEW_BARS {
        let bar_color = match kind {
            BarKind::Accent => palette.accent,
            BarKind::Text => palette.text,
            BarKind::Weak => palette.text_weak,
        };
        let bar = Rect::from_min_size(pos2(page.left() + PREVIEW_PADDING, y), vec2(inner_width * share, height));
        painter.rect_filled(bar, height / 2.0, color(bar_color));
        y += height + PREVIEW_BAR_GAP;
    }
}

/// A check mark centered on `center`.
fn paint_check(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    let box_rect = Rect::from_center_size(center, vec2(CHECK_SIZE, CHECK_SIZE));
    let points = [(0.16, 0.52), (0.4, 0.76), (0.86, 0.26)].map(|(x, y)| box_rect.lerp_inside(vec2(x, y)));
    painter.line(points.to_vec(), Stroke::new(1.7, color));
}

/// `folder` with the home folder shown as `~`, to keep it short.
fn home_relative(folder: &Path) -> String {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    match home.as_deref().and_then(|home| folder.strip_prefix(home).ok()) {
        Some(rest) => Path::new("~").join(rest).display().to_string(),
        None => folder.display().to_string(),
    }
}

fn color(Rgb(red, green, blue): Rgb) -> Color32 {
    Color32::from_rgb(red, green, blue)
}
