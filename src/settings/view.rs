//! The settings panel and the gear button that opens it.

use std::path::{Path, PathBuf};

use eframe::egui::{
    self, Align, Align2, Color32, FontId, Layout, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, WidgetInfo,
    pos2, vec2,
};

use crate::ai::skills::Skill;
use crate::ai::{AiSettings, AiTool, CommandProblem, check_command};
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
    /// Pick another skills folder.
    ChooseSkillsFolder,
    /// Go back to the default skills folder.
    ResetSkillsFolder,
    /// Read the skills folders again.
    ReloadSkills,
    /// Write the example skill into the skills folder.
    CreateExampleSkill,
    /// Ask again before the next run in every folder.
    ForgetAllowedFolders,
    Close,
}

/// AI settings to edit, and which tools were found (in [`AiTool::ALL`] order).
#[derive(Debug)]
pub struct AiInfo<'a> {
    pub settings: &'a mut AiSettings,
    /// Where each tool's program was found, in [`AiTool::ALL`] order.
    pub found: &'a [Option<PathBuf>; 4],
    /// Skills found in the skills folders.
    pub skills: &'a [Skill],
    /// rubrique's skills folder (the chosen one or the default).
    pub skills_folder: Option<&'a Path>,
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
            if let Some(ai_action) = ai_section(ui, ai) {
                action = Some(ai_action);
            }
        });
    });
    action
}

/// Width of the "Summaries use" drop-down.
const SUMMARY_PICKER_WIDTH: f32 = 170.0;
/// Size of help lines under a command.
const SMALL_SIZE: f32 = 11.5;
/// Size of the Installed / Not found text.
const STATUS_SIZE: f32 = 12.0;

/// The AI section: default tool, each tool's command, summaries, skills and safety.
fn ai_section(ui: &mut Ui, ai: AiInfo) -> Option<SettingsAction> {
    let AiInfo { settings, found, skills, skills_folder } = ai;
    let is_found = |tool: AiTool| found[tool_index(tool)].is_some();
    let mut action = None;
    ui.label(RichText::new("AI").size(SECTION_SIZE).weak());

    ui.label(RichText::new("Default tool").strong());
    ui.label(RichText::new("Used by Ask (Cmd+K) and Resolve notes.").weak());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for tool in AiTool::ALL {
            let short = if tool == AiTool::Local { "Local" } else { tool.name() };
            ui.add_enabled_ui(is_found(tool), |ui| ui.selectable_value(&mut settings.default_tool, tool, short))
                .response
                .on_disabled_hover_text(format!("{} was not found. Set its command below.", tool.name()));
        }
    });

    ui.add_space(6.0);
    ui.separator();
    ui.label(RichText::new("Commands").strong());
    ui.label(RichText::new("rubrique starts these programs. {prompt} becomes your request.").weak());
    for tool in AiTool::ALL {
        ui.add_space(2.0);
        if command_row(ui, settings, tool, found[tool_index(tool)].as_deref()) {
            action = Some(SettingsAction::AiCommandChanged);
        }
    }

    ui.add_space(6.0);
    ui.separator();
    ui.label(RichText::new("Summaries use").strong());
    ui.label(RichText::new("Hover summaries in the explorer and scratch names.").weak());
    summary_picker(ui, &mut settings.summary_tool, &is_found);

    ui.add_space(6.0);
    ui.separator();
    if let Some(skills_action) = skills_section(ui, settings, skills, skills_folder) {
        action = Some(skills_action);
    }

    ui.add_space(6.0);
    ui.separator();
    ui.label(RichText::new("Safety").strong());
    ui.label(RichText::new("You start every run. The first run in a folder shows the command and asks once.").weak());
    ui.horizontal(|ui| {
        let count = settings.allowed_folders.len();
        if ui.add_enabled(count > 0, egui::Button::new("Forget allowed folders")).clicked() {
            action = Some(SettingsAction::ForgetAllowedFolders);
        }
        let text = match count {
            0 => "None yet".to_string(),
            1 => "1 folder".to_string(),
            count => format!("{count} folders"),
        };
        ui.label(RichText::new(text).weak());
    });
    action
}

fn tool_index(tool: AiTool) -> usize {
    AiTool::ALL.iter().position(|each| *each == tool).unwrap_or_default()
}

/// Longest list of skill names shown before "and N more".
const SKILLS_LISTED: usize = 6;

/// Skills: the folder, Claude's skills, and what was found.
fn skills_section(
    ui: &mut Ui,
    settings: &mut AiSettings,
    skills: &[Skill],
    folder: Option<&Path>,
) -> Option<SettingsAction> {
    let mut action = None;
    ui.label(RichText::new("Skills").strong());
    ui.label(
        RichText::new(
            "Reusable instructions for Ask, one folder per skill with a SKILL.md file \
             (the same format as Claude Code).",
        )
        .weak(),
    );
    if let Some(folder) = folder {
        path_box(ui, folder);
    }
    ui.horizontal(|ui| {
        if ui.button("Change…").clicked() {
            action = Some(SettingsAction::ChooseSkillsFolder);
        }
        if ui.add_enabled(settings.skills_folder.is_some(), egui::Button::new("Use default")).clicked() {
            action = Some(SettingsAction::ResetSkillsFolder);
        }
    });
    if ui.checkbox(&mut settings.use_claude_skills, "Also use Claude's skills (~/.claude/skills)").changed() {
        action = Some(SettingsAction::ReloadSkills);
    }
    let found = match skills.len() {
        0 => "No skills found yet.".to_string(),
        count => {
            let mut names: Vec<&str> = skills.iter().take(SKILLS_LISTED).map(|skill| skill.name.as_str()).collect();
            let more = count.saturating_sub(SKILLS_LISTED);
            let more = if more > 0 { format!(" and {more} more") } else { String::new() };
            let noun = if count == 1 { "skill" } else { "skills" };
            names.sort_unstable();
            format!("{count} {noun}: {}{more}", names.join(", "))
        }
    };
    ui.label(RichText::new(found).weak());
    ui.horizontal(|ui| {
        if ui.button("Reload").clicked() {
            action = Some(SettingsAction::ReloadSkills);
        }
        if ui.button("Create example skill").on_hover_text("Adds an \"action-items\" skill to the folder").clicked() {
            action = Some(SettingsAction::CreateExampleSkill);
        }
    });
    action
}

/// The "Summaries use" drop-down: a tool, or Off.
fn summary_picker(ui: &mut Ui, choice: &mut Option<AiTool>, is_found: &dyn Fn(AiTool) -> bool) {
    let label = |choice: Option<AiTool>| choice.map_or("Off", AiTool::name);
    egui::ComboBox::from_id_salt("ai-summary-tool").selected_text(label(*choice)).width(SUMMARY_PICKER_WIDTH).show_ui(
        ui,
        |ui| {
            for tool in [AiTool::Local, AiTool::Claude, AiTool::Codex, AiTool::Grok] {
                ui.add_enabled_ui(is_found(tool), |ui| ui.selectable_value(choice, Some(tool), tool.name()))
                    .response
                    .on_disabled_hover_text(format!("{} was not found", tool.name()));
            }
            ui.separator();
            ui.selectable_value(choice, None, "Off");
        },
    );
}

/// One tool: name, Reset (when changed), Installed or Not found, the
/// command field, and a help line when one is needed.
fn command_row(ui: &mut Ui, settings: &mut AiSettings, tool: AiTool, found: Option<&Path>) -> bool {
    let mut changed = false;
    let visuals = ui.visuals().clone();
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.horizontal(|ui| {
            ui.label(tool.full_name());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(path) = found {
                    let status = ui.label(RichText::new("Installed").size(STATUS_SIZE).weak());
                    let (check, _) = ui.allocate_exact_size(vec2(CHECK_SIZE, CHECK_SIZE), Sense::hover());
                    paint_check(ui.painter(), check.center(), visuals.hyperlink_color);
                    status.on_hover_text(RichText::new(path.display().to_string()).monospace());
                } else {
                    ui.label(RichText::new("Not found").size(STATUS_SIZE).color(visuals.warn_fg_color));
                }
                if settings.is_custom(tool) && ui.small_button("Reset").on_hover_text(tool.default_command()).clicked()
                {
                    settings.reset_command(tool);
                    changed = true;
                }
            });
        });
        let problem = check_command(settings.command(tool));
        let border = if problem.is_some() {
            Stroke::new(1.0, visuals.warn_fg_color)
        } else {
            visuals.widgets.noninteractive.bg_stroke
        };
        egui::Frame::new()
            .fill(visuals.text_edit_bg_color())
            .stroke(border)
            .corner_radius(PREVIEW_RADIUS)
            .inner_margin(egui::Margin::symmetric(7, 4))
            .show(ui, |ui| {
                let edit = egui::TextEdit::singleline(settings.command_mut(tool))
                    .font(FontId::monospace(12.0))
                    .frame(egui::Frame::NONE)
                    .hint_text(format!("{} command with {{prompt}}", tool.id()))
                    .desired_width(f32::INFINITY);
                changed |= ui.add(edit).changed();
            });
        if let Some(help) = command_help(tool, problem, found.is_some()) {
            let color = if problem.is_some() { visuals.warn_fg_color } else { visuals.weak_text_color() };
            ui.label(RichText::new(help).size(SMALL_SIZE).color(color));
        }
    });
    changed
}

/// The line under a command, only when there is something to say.
fn command_help(tool: AiTool, problem: Option<CommandProblem>, found: bool) -> Option<String> {
    let text = match (problem, found, tool) {
        (Some(CommandProblem::NoPrompt), ..) => "Add {prompt} so rubrique can pass your request.".to_string(),
        (Some(CommandProblem::Empty), ..) => "Enter a command.".to_string(),
        (None, true, AiTool::Local) => "Ollama or llama.cpp (llama-cli -m model.gguf -p {prompt}).".to_string(),
        (None, false, AiTool::Local) => {
            "Ollama is not on PATH. Install Ollama or llama.cpp, or put the full path in the command.".to_string()
        }
        (None, false, AiTool::Grok) => "No Grok tool on PATH. Install one, then set its command.".to_string(),
        (None, false, tool) => {
            format!("{} is not on PATH. Install it, or put the full path in the command.", tool.full_name())
        }
        (None, true, _) => return None,
    };
    Some(text)
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
