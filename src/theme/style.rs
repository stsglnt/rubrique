//! Turn a theme into egui styles and install it.

use eframe::egui::style::ScrollAnimation;
use eframe::egui::{self, Color32, Context, CornerRadius, FontId, Stroke, Style, TextStyle, ThemePreference, Visuals};

use super::catalog::{ThemeChoice, Themes};
use super::palette::{Base, Palette, Rgb, Theme};
use crate::settings::accessibility::AccessibilitySettings;

/// Corner rounding of buttons, fields and rows.
const WIDGET_RADIUS: u8 = 5;
/// Corner rounding of dialogs and menus.
const WINDOW_RADIUS: u8 = 8;
/// Width of the text cursor.
const CURSOR_WIDTH: f32 = 2.0;
/// Width of the outline around the focused control with strong focus on.
const STRONG_FOCUS_WIDTH: f32 = 2.5;

/// Install the chosen theme. "System" fills egui's light and dark slots with
/// the Light and Dark themes and lets the OS pick; a named theme fills the
/// slot of its base and switches to it. An unknown name falls back to system.
pub fn apply_theme(ctx: &Context, themes: &Themes, choice: &ThemeChoice) {
    for base in [Base::Light, Base::Dark] {
        ctx.set_style_of(egui_theme(base), style(&themes.for_base(base)));
    }
    let chosen = match choice {
        ThemeChoice::System => None,
        ThemeChoice::Named(id) => themes.find(id),
    };
    match chosen {
        Some(theme) => {
            ctx.set_style_of(egui_theme(theme.base), style(theme));
            ctx.set_theme(egui_theme(theme.base));
        }
        None => ctx.set_theme(ThemePreference::System),
    }
}

/// Install the text size and adjust the light and dark styles. Call after
/// [`apply_theme`], which replaces the styles.
pub fn apply_accessibility(ctx: &Context, settings: &AccessibilitySettings) {
    ctx.set_zoom_factor(settings.text_size.scale());
    ctx.all_styles_mut(|style| adjust_style(style, settings));
}

/// The editor font size, no animations, and thicker focus outlines.
fn adjust_style(style: &mut Style, settings: &AccessibilitySettings) {
    style.text_styles.insert(TextStyle::Monospace, FontId::monospace(f32::from(settings.editor_font_size)));
    if settings.reduce_motion {
        style.animation_time = 0.0;
        style.scroll_animation = ScrollAnimation::none();
    }
    if settings.strong_focus {
        let visuals = &mut style.visuals;
        // Focused buttons use the active look; focused text fields the selection stroke.
        visuals.widgets.active.bg_stroke.width = STRONG_FOCUS_WIDTH;
        visuals.selection.stroke.width = STRONG_FOCUS_WIDTH;
    }
}

/// Fill of the page behind the editor and the preview.
///
/// Colors that tell explorer folders (and their tab groups) apart: distinct
/// hues of middle lightness, so they read on light and dark themes alike.
const GROUP_COLORS: [Color32; 6] = [
    Color32::from_rgb(0x4c, 0x8d, 0xf6),
    Color32::from_rgb(0x34, 0xa8, 0x53),
    Color32::from_rgb(0xf2, 0x9d, 0x38),
    Color32::from_rgb(0xa3, 0x71, 0xf7),
    Color32::from_rgb(0x1f, 0xb5, 0xad),
    Color32::from_rgb(0xe5, 0x60, 0x9a),
];

/// The color of explorer folder number `index` (repeating after six).
pub fn group_color(index: usize) -> Color32 {
    GROUP_COLORS[index % GROUP_COLORS.len()]
}

/// The theme's `background` is stored as egui's text field color, so read it
/// from there.
pub fn page_fill(visuals: &Visuals) -> Color32 {
    visuals.text_edit_bg_color()
}

fn egui_theme(base: Base) -> egui::Theme {
    match base {
        Base::Light => egui::Theme::Light,
        Base::Dark => egui::Theme::Dark,
    }
}

fn color(Rgb(red, green, blue): Rgb) -> Color32 {
    Color32::from_rgb(red, green, blue)
}

/// egui's style with the theme's colors and Rubrique's spacing and rounding.
fn style(theme: &Theme) -> Style {
    let mut style = Style { visuals: visuals(theme.base, &theme.palette), ..Style::default() };
    style.spacing.item_spacing = egui::vec2(8.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    style.spacing.menu_margin = egui::Margin::same(6);
    // egui lets a trackpad scroll past the end of the content and spring
    // back. In a page of text that reads as the page jumping; stop firmly.
    style.spacing.scroll.kinetic.rubber_band = false;
    style
}

/// egui's light or dark visuals, recolored with the palette.
fn visuals(base: Base, palette: &Palette) -> Visuals {
    let mut visuals = match base {
        Base::Light => Visuals::light(),
        Base::Dark => Visuals::dark(),
    };
    let background = color(palette.background);
    let border = Stroke::new(1.0, color(palette.border));
    let text = color(palette.text);
    let text_strong = color(palette.text_strong);
    let accent = color(palette.accent);

    visuals.panel_fill = color(palette.sidebar);
    visuals.window_fill = background;
    visuals.window_stroke = border;
    visuals.extreme_bg_color = background;
    visuals.text_edit_bg_color = Some(background);
    visuals.faint_bg_color = color(palette.surface);
    visuals.code_bg_color = color(palette.code_background);
    visuals.hyperlink_color = accent;
    visuals.warn_fg_color = color(palette.warning);
    visuals.error_fg_color = color(palette.error);
    visuals.weak_text_color = Some(color(palette.text_weak));
    visuals.selection.bg_fill = color(palette.selection);
    visuals.selection.stroke = Stroke::new(1.0, text_strong);
    visuals.text_cursor.stroke = Stroke::new(CURSOR_WIDTH, accent);
    visuals.window_corner_radius = CornerRadius::same(WINDOW_RADIUS);
    visuals.menu_corner_radius = CornerRadius::same(WINDOW_RADIUS);

    let widgets = &mut visuals.widgets;
    // Labels, separators and panel edges.
    widgets.noninteractive.bg_fill = background;
    widgets.noninteractive.weak_bg_fill = background;
    widgets.noninteractive.bg_stroke = border;
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, text);
    // Buttons and checkboxes at rest, under the mouse, while pressed, and open menus.
    let states = [
        (&mut widgets.inactive, palette.surface, Stroke::NONE, text),
        (&mut widgets.hovered, palette.hover, border, text_strong),
        (&mut widgets.active, palette.hover, Stroke::new(1.0, accent), text_strong),
        (&mut widgets.open, palette.surface, border, text),
    ];
    for (state, fill, stroke, foreground) in states {
        state.bg_fill = color(fill);
        state.weak_bg_fill = color(fill);
        state.bg_stroke = stroke;
        state.fg_stroke = Stroke::new(1.0, foreground);
    }
    for state in [
        &mut widgets.noninteractive,
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        state.corner_radius = CornerRadius::same(WIDGET_RADIUS);
    }
    visuals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_and_dark_keep_their_egui_mode() {
        let light = Theme::builtin(Base::Light);
        let dark = Theme::builtin(Base::Dark);
        assert!(!style(&light).visuals.dark_mode);
        assert!(style(&dark).visuals.dark_mode);
    }

    #[test]
    fn scrolling_stops_at_the_edges() {
        assert!(!style(&Theme::builtin(Base::Light)).spacing.scroll.kinetic.rubber_band);
    }

    #[test]
    fn accessibility_adjusts_fonts_motion_and_focus() {
        let mut adjusted = style(&Theme::builtin(Base::Light));
        let settings = AccessibilitySettings {
            editor_font_size: 18,
            reduce_motion: true,
            strong_focus: true,
            ..Default::default()
        };
        adjust_style(&mut adjusted, &settings);
        assert_eq!(adjusted.text_styles[&TextStyle::Monospace], FontId::monospace(18.0));
        assert!(adjusted.animation_time.abs() < f32::EPSILON);
        let unchanged = style(&Theme::builtin(Base::Light)).visuals;
        let focus = Stroke::new(STRONG_FOCUS_WIDTH, unchanged.selection.stroke.color);
        assert_eq!(adjusted.visuals.selection.stroke, focus);

        // The defaults change nothing (egui's monospace font is already 13 points).
        let mut plain = style(&Theme::builtin(Base::Light));
        adjust_style(&mut plain, &AccessibilitySettings::default());
        assert_eq!(format!("{plain:?}"), format!("{:?}", style(&Theme::builtin(Base::Light))));
    }

    #[test]
    fn page_fill_is_the_theme_background() {
        let theme = Theme::builtin(Base::Dark);
        assert_eq!(page_fill(&style(&theme).visuals), color(theme.palette.background));
    }
}
