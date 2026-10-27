//! Turn a theme into egui styles and install it.

use eframe::egui::{self, Color32, Context, CornerRadius, Stroke, Style, ThemePreference, Visuals};

use super::catalog::{ThemeChoice, Themes};
use super::palette::{Base, Palette, Rgb, Theme};

/// Corner rounding of buttons, fields and rows.
const WIDGET_RADIUS: u8 = 5;
/// Corner rounding of dialogs and menus.
const WINDOW_RADIUS: u8 = 8;
/// Width of the text cursor.
const CURSOR_WIDTH: f32 = 2.0;

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

/// Fill of the page behind the editor and the preview.
///
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

/// egui's style with the theme's colors and rubrique's spacing and rounding.
fn style(theme: &Theme) -> Style {
    let mut style = Style { visuals: visuals(theme.base, &theme.palette), ..Style::default() };
    style.spacing.item_spacing = egui::vec2(8.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    style.spacing.menu_margin = egui::Margin::same(6);
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
    fn page_fill_is_the_theme_background() {
        let theme = Theme::builtin(Base::Dark);
        assert_eq!(page_fill(&style(&theme).visuals), color(theme.palette.background));
    }
}
