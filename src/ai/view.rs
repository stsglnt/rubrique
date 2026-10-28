//! The Ask bar (Cmd+K), the output panel of a running tool, and the dialog
//! that asks before the first run in a folder.

use std::path::PathBuf;

use eframe::egui::{self, Align, Context, Key, Layout, Pos2, RichText, Ui};

use super::{AiTool, AskKind, QuickAction, format_elapsed};
use crate::icons::{Icon, icon_label, icon_text_button};
use crate::settings::view::path_box;

/// Width of the Ask bar.
const ASK_WIDTH: f32 = 460.0;
const ASK_RADIUS: u8 = 8;
/// Width of the tool picker in the Ask bar.
const TOOL_PICKER_WIDTH: f32 = 96.0;
const CONSENT_WIDTH: f32 = 500.0;
/// Longest request shown in the output panel's header.
const HEADER_REQUEST_CHARS: usize = 60;

/// The open Ask bar.
#[derive(Debug)]
pub struct AskBar {
    pub request: String,
    pub tool: AiTool,
    /// What the request is about, for the footer ("2 lines selected").
    pub scope: String,
    /// Give the field focus on the next frame.
    focus: bool,
}

impl AskBar {
    pub fn new(tool: AiTool, scope: String) -> Self {
        AskBar { request: String::new(), tool, scope, focus: true }
    }
}

/// What the user did in the Ask bar.
#[derive(Debug, PartialEq, Eq)]
pub enum AskEvent {
    Run { request: String, kind: AskKind },
    Close,
}

/// Draw the Ask bar with its top-left corner at `at`.
pub fn show_ask_bar(ctx: &Context, at: Pos2, bar: &mut AskBar, installed: [bool; 4]) -> Option<AskEvent> {
    let mut event = None;
    egui::Area::new(egui::Id::unique("ask-bar")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
        let visuals = ui.visuals().clone();
        egui::Frame::popup(ui.style()).fill(visuals.extreme_bg_color).corner_radius(ASK_RADIUS).inner_margin(10).show(
            ui,
            |ui| {
                ui.set_width(ASK_WIDTH);
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.horizontal(|ui| {
                    icon_label(ui, Icon::Sparkle);
                    let field_width = ui.available_width() - TOOL_PICKER_WIDTH - ui.spacing().item_spacing.x;
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut bar.request)
                            .hint_text("What should change?")
                            .desired_width(field_width),
                    );
                    if bar.focus {
                        field.request_focus();
                        bar.focus = false;
                    }
                    let entered = field.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
                    if entered && !bar.request.trim().is_empty() {
                        event = Some(AskEvent::Run { request: bar.request.trim().to_string(), kind: AskKind::Rewrite });
                    }
                    tool_picker(ui, &mut bar.tool, installed);
                });
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for action in QuickAction::ALL {
                        if ui.small_button(action.label()).clicked() {
                            if let Some(request) = action.request() {
                                event = Some(AskEvent::Run { request: request.to_string(), kind: action.kind() });
                            } else {
                                "Translate to ".clone_into(&mut bar.request);
                                bar.focus = true;
                            }
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Enter to run · Esc to close").size(11.5).weak());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(&bar.scope).size(11.5).weak());
                    });
                });
            },
        );
    });
    if ctx.input(|input| input.key_pressed(Key::Escape)) {
        event = Some(AskEvent::Close);
    }
    event
}

/// A small drop-down of the tools; ones that were not found are disabled.
fn tool_picker(ui: &mut Ui, tool: &mut AiTool, installed: [bool; 4]) {
    egui::ComboBox::from_id_salt("ask-tool").selected_text(tool.name()).width(TOOL_PICKER_WIDTH).show_ui(ui, |ui| {
        for (choice, found) in AiTool::ALL.into_iter().zip(installed) {
            ui.add_enabled_ui(found, |ui| ui.selectable_value(tool, choice, choice.name()))
                .response
                .on_disabled_hover_text(format!("{} was not found. Set its command in Settings.", choice.name()));
        }
    });
}

/// Where a run stands, for the output panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputState {
    Running { seconds: u64 },
    Done { seconds: u64 },
    Failed { seconds: u64, code: Option<i32> },
    Stopped { seconds: u64 },
}

/// What the output panel shows.
#[derive(Debug)]
pub struct OutputInfo<'a> {
    pub tool: AiTool,
    pub request: &'a str,
    /// The command as run, prompt shortened.
    pub command: &'a str,
    pub output: &'a str,
    pub errors: &'a str,
    pub state: OutputState,
    /// The answer can go to the merge screen (a rewrite that finished well).
    pub can_review: bool,
}

/// What the user clicked in the output panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputAction {
    Stop,
    Copy,
    Review,
    Close,
}

/// The output panel: header with state and buttons, then what the tool printed.
pub fn show_output(ui: &mut Ui, info: &OutputInfo) -> Option<OutputAction> {
    let mut action = None;
    let visuals = ui.visuals().clone();
    egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 8)).show(ui, |ui| {
        ui.horizontal(|ui| {
            icon_label(ui, Icon::Sparkle);
            ui.label(RichText::new(info.tool.name()).strong());
            ui.label(RichText::new(format!("\u{201c}{}\u{201d}", shorten(info.request))).weak());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| match &info.state {
                OutputState::Running { seconds } => {
                    if icon_text_button(ui, Icon::Stop, "Stop", "Stop the tool", false).clicked() {
                        action = Some(OutputAction::Stop);
                    }
                    ui.add(egui::Spinner::new().size(12.0));
                    ui.label(RichText::new(format_elapsed(*seconds)).monospace().weak());
                }
                finished => {
                    if ui.button("×").on_hover_text("Close").clicked() {
                        action = Some(OutputAction::Close);
                    }
                    if info.can_review {
                        let review =
                            egui::Button::new(RichText::new("Review in merge").color(visuals.strong_text_color()))
                                .fill(visuals.selection.bg_fill);
                        if ui.add(review).on_hover_text("See each change and decide").clicked() {
                            action = Some(OutputAction::Review);
                        }
                    }
                    if ui.button("Copy").clicked() {
                        action = Some(OutputAction::Copy);
                    }
                    let (text, color) = match finished {
                        OutputState::Done { seconds } => {
                            (format!("Done in {}", format_elapsed(*seconds)), visuals.weak_text_color())
                        }
                        OutputState::Stopped { seconds } => {
                            (format!("Stopped at {}", format_elapsed(*seconds)), visuals.weak_text_color())
                        }
                        OutputState::Failed { code, .. } => (
                            code.map_or_else(|| "Failed".to_string(), |code| format!("Failed (exit code {code})")),
                            visuals.error_fg_color,
                        ),
                        OutputState::Running { .. } => (String::new(), visuals.weak_text_color()),
                    };
                    ui.label(RichText::new(text).color(color));
                }
            });
        });
    });
    ui.separator();
    egui::ScrollArea::vertical().id_salt("ai-output").auto_shrink([false, false]).stick_to_bottom(true).show(
        ui,
        |ui| {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 6)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(format!("$ {}", info.command)).monospace().weak());
                if !info.output.is_empty() {
                    ui.add(egui::Label::new(RichText::new(info.output).monospace()).selectable(true));
                }
                let failed = matches!(info.state, OutputState::Failed { .. });
                if !info.errors.trim().is_empty() && (failed || info.output.trim().is_empty()) {
                    ui.add(
                        egui::Label::new(
                            RichText::new(info.errors.trim_end()).monospace().color(visuals.error_fg_color),
                        )
                        .selectable(true),
                    );
                }
                if matches!(info.state, OutputState::Done { .. }) && info.output.trim().is_empty() {
                    ui.label(RichText::new("The tool printed nothing.").weak());
                }
            });
        },
    );
    action
}

/// `request` cut to fit the header.
fn shorten(request: &str) -> String {
    let flat = request.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > HEADER_REQUEST_CHARS {
        format!("{}…", flat.chars().take(HEADER_REQUEST_CHARS).collect::<String>())
    } else {
        flat
    }
}

/// The question before the first run in a folder.
#[derive(Debug)]
pub struct Consent {
    pub tool: AiTool,
    pub folder: PathBuf,
    /// The command template, shown as it will run.
    pub command: String,
    /// "Don't ask again for this folder".
    pub remember: bool,
}

/// The user's answer in the consent dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsentChoice {
    Run,
    Cancel,
}

/// Ask whether to run `consent.tool` in its folder.
pub fn show_consent(ctx: &Context, consent: &mut Consent) -> Option<ConsentChoice> {
    let modal = egui::Modal::new(egui::Id::unique("ai-consent")).show(ctx, |ui| {
        ui.set_width(CONSENT_WIDTH);
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.heading(format!("Run {} in this folder?", consent.tool.name()));
        ui.label(RichText::new("rubrique will start this program in:").weak());
        path_box(ui, &consent.folder);
        ui.label(RichText::new("Command").weak());
        egui::Frame::new()
            .fill(ui.visuals().code_bg_color)
            .corner_radius(5)
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(&consent.command).monospace());
            });
        ui.label(
            "It gets your request and the selected text, and sends back new text. \
             Your file changes only when you press Apply in the merge screen.",
        );
        ui.checkbox(&mut consent.remember, "Don't ask again for this folder");
        ui.add_space(4.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Run").clicked() {
                Some(ConsentChoice::Run)
            } else if ui.button("Cancel").clicked() {
                Some(ConsentChoice::Cancel)
            } else {
                None
            }
        })
        .inner
    });
    if modal.should_close() { Some(ConsentChoice::Cancel) } else { modal.inner }
}
