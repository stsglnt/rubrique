//! The settings panel and the gear button that opens it.

use std::path::Path;

use eframe::egui::{
    self, Align, Align2, Color32, FontId, Layout, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, WidgetInfo,
    pos2, vec2,
};

use crate::ai::{AiSettings, AiTool, check_command};
use crate::theme::catalog::{ThemeChoice, Themes};
use crate::theme::palette::{Base, Palette, Rgb, Theme};

/// Something the user asked for in the settings panel.
#[derive(Debug)]
pub enum SettingsAction {
    PickTheme(ThemeChoice),
    ReloadThemes,
    /// Pick another folder for scratch files.
    ChooseScratchFolder,
    /// Go back to the default scratch folder.
    ResetScratchFolder,
    /// An AI command changed, so which tools are installed may have too.
    AiCommandChanged,
    Close,
}

/// AI settings to edit, and which tools were found (in [`AiTool::ALL`] order).
#[derive(Debug)]
pub struct AiInfo<'a> {
    pub settings: &'a mut AiSettings,
    pub installed: [bool; 4],
}

/// Where scratch files go, for the Scratches section.
#[derive(Clone, Copy, Debug)]
pub struct ScratchInfo<'a> {
    pub folder: Option<&'a Path>,
    /// The folder is the default one (so "Use default" has nothing to do).
    pub is_default: bool,
}

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

/// Draw the settings panel's content. Returns what the user clicked.
pub fn show_settings(
    ui: &mut Ui,
    themes: &Themes,
    current: &ThemeChoice,
    themes_folder: Option<&Path>,
    scratch: ScratchInfo,
    ai: AiInfo,
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
                path_box(ui, folder);
            }
            if ui.button("Reload themes").clicked() {
                action = Some(SettingsAction::ReloadThemes);
            }

            ui.add_space(10.0);
            ui.label(RichText::new("FILES").size(SECTION_SIZE).weak());
            ui.label(RichText::new("Scratches").strong());
            ui.label(RichText::new("New scratch files (Cmd+N) go to this folder.").weak());
            if let Some(folder) = scratch.folder {
                path_box(ui, folder);
            }
            ui.horizontal(|ui| {
                if ui.button("Change…").clicked() {
                    action = Some(SettingsAction::ChooseScratchFolder);
                }
                if ui.add_enabled(!scratch.is_default, egui::Button::new("Use default")).clicked() {
                    action = Some(SettingsAction::ResetScratchFolder);
                }
            });

            ui.add_space(10.0);
            if ai_section(ui, ai) {
                action = Some(SettingsAction::AiCommandChanged);
            }
        });
    });
    action
}

/// Size of the dot that shows whether a tool was found.
const STATUS_DOT: f32 = 7.0;

/// The AI section: which tool to use, and each tool's command. Returns true
/// when a command changed.
fn ai_section(ui: &mut Ui, ai: AiInfo) -> bool {
    let AiInfo { settings, installed } = ai;
    let found = |tool: AiTool| installed[AiTool::ALL.iter().position(|each| *each == tool).unwrap_or_default()];
    ui.label(RichText::new("AI").size(SECTION_SIZE).weak());

    ui.label(RichText::new("Ask with").strong());
    ui.label(RichText::new("For Ask (Cmd+K), resolving notes and folder questions.").weak());
    tool_picker(ui, "ai-default-tool", &mut settings.default_tool, &found);

    ui.label(RichText::new("Summaries with").strong());
    ui.label(RichText::new("For summaries and file titles. A local model keeps your notes on this computer.").weak());
    tool_picker(ui, "ai-summary-tool", &mut settings.summary_tool, &found);

    ui.add_space(6.0);
    ui.separator();
    ui.label(RichText::new("Commands").strong());
    ui.label(
        RichText::new(
            "rubrique runs these programs only when you ask. {prompt} becomes your request, \
             passed as one argument, never through a shell.",
        )
        .weak(),
    );
    let mut changed = false;
    for tool in AiTool::ALL {
        ui.add_space(4.0);
        changed |= command_row(ui, settings, tool, found(tool));
    }
    changed
}

/// A drop-down of the AI tools; tools that were not found say so.
fn tool_picker(ui: &mut Ui, id: &str, choice: &mut AiTool, found: &dyn Fn(AiTool) -> bool) {
    let label = |tool: AiTool| {
        if found(tool) { tool.name().to_string() } else { format!("{} (not found)", tool.name()) }
    };
    egui::ComboBox::from_id_salt(id).selected_text(label(*choice)).width(ui.available_width()).show_ui(ui, |ui| {
        for tool in AiTool::ALL {
            ui.selectable_value(choice, tool, label(tool));
        }
    });
}

/// One tool: name, found or not, its command, and a hint or a problem.
fn command_row(ui: &mut Ui, settings: &mut AiSettings, tool: AiTool, found: bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(tool.name()).strong());
        let (dot, _) = ui.allocate_exact_size(vec2(STATUS_DOT, STATUS_DOT), Sense::hover());
        let visuals = ui.visuals();
        if found {
            ui.painter().circle_filled(dot.center(), STATUS_DOT / 2.0, visuals.hyperlink_color);
            ui.label(RichText::new("found").weak());
        } else {
            ui.painter().circle_stroke(
                dot.center(),
                STATUS_DOT / 2.0 - 0.5,
                Stroke::new(1.0, visuals.weak_text_color()),
            );
            ui.label(RichText::new("not found").weak());
        }
        if settings.is_custom(tool) {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("Reset").on_hover_text(tool.default_command()).clicked() {
                    settings.reset_command(tool);
                    changed = true;
                }
            });
        }
    });
    let edit = egui::TextEdit::singleline(settings.command_mut(tool))
        .font(egui::TextStyle::Monospace)
        .desired_width(f32::INFINITY);
    changed |= ui.add(edit).changed();
    match check_command(settings.command(tool)) {
        Some(problem) => ui.label(RichText::new(problem.message()).color(ui.visuals().error_fg_color)),
        None => ui.label(RichText::new(tool.hint()).weak()),
    };
    changed
}

/// A folder path in a small outlined box, home folder shown as `~`.
pub fn path_box(ui: &mut Ui, folder: &Path) {
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
pub fn home_relative(folder: &Path) -> String {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    match home.as_deref().and_then(|home| folder.strip_prefix(home).ok()) {
        Some(rest) => Path::new("~").join(rest).display().to_string(),
        None => folder.display().to_string(),
    }
}

fn color(Rgb(red, green, blue): Rgb) -> Color32 {
    Color32::from_rgb(red, green, blue)
}
