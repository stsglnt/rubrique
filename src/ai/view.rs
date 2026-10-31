//! The Ask bar (Cmd+K), the output panel of a running tool, and the dialog
//! that asks before the first run in a folder.

use std::path::PathBuf;

use eframe::egui::{self, Align, Context, Key, Layout, Pos2, RichText, Ui};

use super::agents::{self, Agent, MENTION_PREFIX};
use super::skills::Skill;
use super::{AiTool, AskKind, QuickAction, format_elapsed};
use crate::icons::{Icon, icon_label, icon_text_button};
use crate::markdown::render::{CodeAction, render_answer, take_code_action};
use crate::settings::view::path_box;

/// Width of the Ask bar.
const ASK_WIDTH: f32 = 460.0;
const ASK_RADIUS: u8 = 8;
/// Width of the tool picker in the Ask bar.
const TOOL_PICKER_WIDTH: f32 = 96.0;
/// Width of the "Compare with" picker.
const COMPARE_PICKER_WIDTH: f32 = 80.0;
const CONSENT_WIDTH: f32 = 500.0;
/// Longest request shown in the output panel's header.
const HEADER_REQUEST_CHARS: usize = 60;
/// Space between the command line, the output and any errors.
const OUTPUT_ITEM_GAP: f32 = 8.0;

/// The open Ask bar.
#[derive(Debug)]
pub struct AskBar {
    pub request: String,
    pub tool: AiTool,
    /// A second tool to run at the same time, to compare the answers.
    pub compare: Option<AiTool>,
    /// What the request is about, for the footer ("2 lines selected").
    pub scope: String,
    /// The agent picked with `@ai-name`, shown as a chip before the field.
    pub agent: Option<Box<Agent>>,
    /// Why the request cannot run yet (an unknown agent, say).
    error: Option<String>,
    /// Give the field focus on the next frame.
    focus: bool,
}

impl AskBar {
    pub fn new(tool: AiTool, scope: String) -> Self {
        AskBar { request: String::new(), tool, compare: None, scope, agent: None, error: None, focus: true }
    }

    /// Use `agent` for this request, and its tool when it has one that is installed.
    fn pick_agent(&mut self, agent: &Agent, installed: [bool; 4]) {
        if let Some(tool) = agent.tool
            && AiTool::ALL.into_iter().zip(installed).any(|(each, found)| each == tool && found)
        {
            self.tool = tool;
        }
        self.agent = Some(Box::new(agent.clone()));
        self.error = None;
        self.focus = true;
    }

    /// The `@word` being typed at the start of the field, while no agent is
    /// picked and no space follows it yet.
    fn typing_mention(&self) -> Option<&str> {
        if self.agent.is_some() {
            return None;
        }
        let (word, rest) = agents::leading_mention(&self.request)?;
        (rest.is_empty() && !self.request.ends_with(char::is_whitespace)).then_some(word)
    }

    /// Turn a finished `@ai-name ` at the start of the field into the
    /// agent chip. An unknown `@ai-` name leaves the text and sets an error.
    fn take_mention(&mut self, agents: &[Agent], installed: [bool; 4]) {
        self.error = None;
        if self.agent.is_some() || self.typing_mention().is_some() {
            return;
        }
        let Some((word, rest)) = agents::leading_mention(&self.request) else {
            return;
        };
        if !word.to_lowercase().starts_with(MENTION_PREFIX) {
            return;
        }
        match agents::find_by_mention(agents, word) {
            Some(agent) => {
                let rest = rest.to_string();
                self.pick_agent(agent, installed);
                self.request = rest;
            }
            None => self.error = Some(format!("No agent called @{word}. See Settings → Agents.")),
        }
    }
}

/// What the user did in the Ask bar.
#[derive(Debug, PartialEq, Eq)]
pub enum AskEvent {
    Run { request: String, kind: AskKind, skill: Option<Box<Skill>> },
    Close,
}

/// Draw the Ask bar with its top-left corner at `at`.
pub fn show_ask_bar(
    ctx: &Context,
    at: Pos2,
    bar: &mut AskBar,
    installed: [bool; 4],
    skills: &[Skill],
    agents: &[Agent],
) -> Option<AskEvent> {
    let mut event = None;
    bar.take_mention(agents, installed);
    let suggestions: Vec<&Agent> = bar.typing_mention().map(|word| agents::matching(agents, word)).unwrap_or_default();
    egui::Area::new(egui::Id::unique("ask-bar")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
        let visuals = ui.visuals().clone();
        egui::Frame::popup(ui.style()).fill(visuals.extreme_bg_color).corner_radius(ASK_RADIUS).inner_margin(10).show(
            ui,
            |ui| {
                ui.set_width(ASK_WIDTH);
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.horizontal(|ui| {
                    icon_label(ui, Icon::Sparkle);
                    if let Some(agent) = &bar.agent {
                        let chip = ui
                            .small_button(format!("{} ×", agent.mention()))
                            .on_hover_text(format!("{}: {}\nClick to remove", agent.name, agent.description));
                        if chip.clicked() {
                            bar.agent = None;
                            bar.focus = true;
                        }
                    }
                    let field_width = ui.available_width() - TOOL_PICKER_WIDTH - ui.spacing().item_spacing.x;
                    let hint = bar.agent.as_ref().map_or_else(
                        || "What should change? Type @ for agents".to_string(),
                        |agent| format!("What should {} do?", agent.name),
                    );
                    let field =
                        ui.add(egui::TextEdit::singleline(&mut bar.request).hint_text(hint).desired_width(field_width));
                    if bar.focus {
                        field.request_focus();
                        bar.focus = false;
                    }
                    let entered = field.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
                    if entered && let Some(first) = suggestions.first() {
                        // Enter while typing @… picks the first agent listed.
                        bar.pick_agent(first, installed);
                        bar.request.clear();
                    } else if entered && (bar.error.is_some() || bar.typing_mention().is_some()) {
                        bar.focus = true;
                    } else if entered && !bar.request.trim().is_empty() {
                        let request = bar.request.trim().to_string();
                        event = Some(AskEvent::Run { request, kind: AskKind::Rewrite, skill: None });
                    } else if entered {
                        bar.focus = true;
                    }
                    tool_picker(ui, &mut bar.tool, installed);
                });
                if let Some(picked) = agent_suggestions(ui, bar.typing_mention(), &suggestions) {
                    bar.pick_agent(picked, installed);
                    bar.request.clear();
                }
                if let Some(error) = &bar.error {
                    ui.label(RichText::new(error).size(11.5).color(ui.visuals().error_fg_color));
                }
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for action in QuickAction::ALL {
                        if ui.small_button(action.label()).clicked() {
                            if let Some(request) = action.request() {
                                event = Some(AskEvent::Run {
                                    request: request.to_string(),
                                    kind: action.kind(),
                                    skill: None,
                                });
                            } else {
                                "Translate to ".clone_into(&mut bar.request);
                                bar.focus = true;
                            }
                        }
                    }
                    if let Some(skill_event) = skills_menu(ui, &bar.request, skills) {
                        event = Some(skill_event);
                    }
                    if let Some(picked) = agents_menu(ui, agents) {
                        bar.pick_agent(picked, installed);
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Compare with").size(11.5).weak());
                    compare_picker(ui, bar, installed);
                    ui.label(RichText::new("· Enter to run · Esc to close").size(11.5).weak());
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

/// The agents matching the `@word` being typed, one per row; a click picks one.
fn agent_suggestions<'a>(ui: &mut Ui, typing: Option<&str>, matches: &[&'a Agent]) -> Option<&'a Agent> {
    let typing = typing?;
    let mut picked = None;
    if matches.is_empty() {
        let text = format!("No agent matches @{typing}. Add agents in Settings → Agents.");
        ui.label(RichText::new(text).size(11.5).weak());
        return None;
    }
    for (index, agent) in matches.iter().enumerate() {
        let row = ui.horizontal(|ui| {
            let mention = RichText::new(agent.mention()).monospace();
            let clicked = ui.selectable_label(index == 0, mention).clicked();
            ui.label(&agent.name);
            ui.add(egui::Label::new(RichText::new(&agent.description).size(11.5).weak()).truncate());
            clicked
        });
        if row.inner {
            picked = Some(*agent);
        }
    }
    ui.label(RichText::new("Enter picks the first one").size(11.5).weak());
    picked
}

/// The Agents… menu: pick an agent for the request.
fn agents_menu<'a>(ui: &mut Ui, agents: &'a [Agent]) -> Option<&'a Agent> {
    let mut picked = None;
    ui.menu_button(RichText::new("Agents…").weak(), |ui| {
        if agents.is_empty() {
            ui.label(RichText::new("No agents yet. Add them in Settings → Agents.").weak());
        }
        for agent in agents {
            let label = format!("{}  {}", agent.mention(), agent.name);
            if ui.button(label).on_hover_text(&agent.description).clicked() {
                picked = Some(agent);
                ui.close();
            }
        }
    });
    picked
}

/// The Skills… menu: run a skill on the text, with the typed request (if
/// any) as extra detail.
fn skills_menu(ui: &mut Ui, typed: &str, skills: &[Skill]) -> Option<AskEvent> {
    let mut event = None;
    ui.menu_button(RichText::new("Skills…").weak(), |ui| {
        if skills.is_empty() {
            ui.label(RichText::new("No skills yet. Add them in Settings → AI.").weak());
        }
        for skill in skills {
            if ui.button(&skill.name).on_hover_text(&skill.description).clicked() {
                let typed = typed.trim();
                let request = if typed.is_empty() { skill.description.clone() } else { typed.to_string() };
                event = Some(AskEvent::Run { request, kind: skill.kind, skill: Some(Box::new(skill.clone())) });
                ui.close();
            }
        }
    });
    event
}

/// "Compare with": none, or a second tool (not the first one).
fn compare_picker(ui: &mut Ui, bar: &mut AskBar, installed: [bool; 4]) {
    if bar.compare == Some(bar.tool) {
        bar.compare = None;
    }
    let label = bar.compare.map_or("None", AiTool::name);
    egui::ComboBox::from_id_salt("ask-compare")
        .selected_text(label)
        .width(COMPARE_PICKER_WIDTH)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut bar.compare, None, "None");
            for (choice, found) in AiTool::ALL.into_iter().zip(installed) {
                if choice == bar.tool {
                    continue;
                }
                ui.add_enabled_ui(found, |ui| ui.selectable_value(&mut bar.compare, Some(choice), choice.name()))
                    .response
                    .on_disabled_hover_text(format!(
                        "{} is not installed. Install it in Settings → AI.",
                        choice.name()
                    ));
            }
        })
        .response
        .on_hover_text("Run a second tool at the same time and compare the answers in the merge screen");
}

/// A small drop-down of the tools; ones that were not found are disabled.
fn tool_picker(ui: &mut Ui, tool: &mut AiTool, installed: [bool; 4]) {
    egui::ComboBox::from_id_salt("ask-tool").selected_text(tool.name()).width(TOOL_PICKER_WIDTH).show_ui(ui, |ui| {
        for (choice, found) in AiTool::ALL.into_iter().zip(installed) {
            ui.add_enabled_ui(found, |ui| ui.selectable_value(tool, choice, choice.name()))
                .response
                .on_disabled_hover_text(format!("{} is not installed. Install it in Settings → AI.", choice.name()));
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

/// One tool's run in the output panel.
#[derive(Debug)]
pub struct RunView {
    pub tool: AiTool,
    /// The command as run, prompt shortened.
    pub command: String,
    pub output: String,
    pub errors: String,
    pub state: OutputState,
    /// The output is an answer to read, drawn as Markdown; otherwise it is
    /// new text for the file, shown as it is.
    pub formatted: bool,
}

/// What the output panel shows: one run, or two when comparing tools.
#[derive(Debug)]
pub struct OutputInfo<'a> {
    pub request: &'a str,
    pub runs: Vec<RunView>,
    /// The answers can go to the merge screen: "Review in merge" for one,
    /// "Compare in merge" for two.
    pub reviewable: usize,
}

/// What the user clicked in the output panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputAction {
    Stop,
    Copy,
    /// Copy one code block of a formatted answer.
    CopyCode(String),
    Review,
    Close,
}

/// The output panel: header with state and buttons, then what each tool printed.
pub fn show_output(ui: &mut Ui, info: &OutputInfo) -> Option<OutputAction> {
    let mut action = None;
    let visuals = ui.visuals().clone();
    let running = info.runs.iter().any(|run| matches!(run.state, OutputState::Running { .. }));
    let tools = info.runs.iter().map(|run| run.tool.name()).collect::<Vec<_>>().join(" and ");
    egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 8)).show(ui, |ui| {
        ui.horizontal(|ui| {
            icon_label(ui, Icon::Sparkle);
            ui.label(RichText::new(tools).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if running {
                    let tip = if info.runs.len() > 1 { "Stop both tools" } else { "Stop the tool" };
                    if icon_text_button(ui, Icon::Stop, "Stop", tip, false).clicked() {
                        action = Some(OutputAction::Stop);
                    }
                    ui.add(egui::Spinner::new().size(12.0));
                } else {
                    if ui.button("×").on_hover_text("Close").clicked() {
                        action = Some(OutputAction::Close);
                    }
                    if info.reviewable > 0 {
                        let label = if info.reviewable > 1 { "Compare in merge" } else { "Review in merge" };
                        let review = egui::Button::new(RichText::new(label).color(visuals.strong_text_color()))
                            .fill(visuals.selection.bg_fill);
                        if ui.add(review).on_hover_text("See each change and decide").clicked() {
                            action = Some(OutputAction::Review);
                        }
                    }
                    if ui.button("Copy").clicked() {
                        action = Some(OutputAction::Copy);
                    }
                }
                for run in info.runs.iter().rev() {
                    let (text, color) = state_text(run, info.runs.len() > 1, &visuals);
                    ui.label(RichText::new(text).color(color));
                }
                // The request fills the space left, cut short with "…" when needed.
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(shorten(info.request)).weak()).truncate());
                });
            });
        });
    });
    ui.separator();
    egui::ScrollArea::vertical().id_salt("ai-output").auto_shrink([false, false]).stick_to_bottom(true).show(
        ui,
        |ui| {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 6)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = OUTPUT_ITEM_GAP;
                if let [only] = info.runs.as_slice() {
                    run_output(ui, only, false);
                } else {
                    ui.columns(info.runs.len(), |columns| {
                        for (column, run) in columns.iter_mut().zip(&info.runs) {
                            column.with_layout(Layout::top_down(Align::LEFT), |ui| run_output(ui, run, true));
                        }
                    });
                }
            });
        },
    );
    if let Some(CodeAction::Copy(code)) = take_code_action(ui.ctx()) {
        action = Some(OutputAction::CopyCode(code));
    }
    action
}

/// "Done in 0:12" and the like; with the tool name when there are two.
fn state_text(run: &RunView, named: bool, visuals: &egui::Visuals) -> (String, egui::Color32) {
    let name = if named { format!("{} ", run.tool.name()) } else { String::new() };
    match run.state {
        OutputState::Running { seconds } => (format!("{name}{}", format_elapsed(seconds)), visuals.weak_text_color()),
        OutputState::Done { seconds } => {
            let done = if named { name } else { "Done in ".to_string() };
            (format!("{done}{}", format_elapsed(seconds)), visuals.weak_text_color())
        }
        OutputState::Stopped { seconds } => {
            (format!("{name}stopped at {}", format_elapsed(seconds)), visuals.weak_text_color())
        }
        OutputState::Failed { code, .. } => (
            code.map_or_else(|| format!("{name}failed"), |code| format!("{name}failed (exit code {code})")),
            visuals.error_fg_color,
        ),
    }
}

/// What one tool printed, under its command.
fn run_output(ui: &mut Ui, run: &RunView, titled: bool) {
    let visuals = ui.visuals().clone();
    if titled {
        ui.label(RichText::new(run.tool.name()).strong());
    }
    ui.label(RichText::new(format!("$ {}", run.command)).monospace().weak());
    if run.formatted && !run.output.trim().is_empty() {
        render_answer(ui, &run.output);
    } else if !run.output.is_empty() {
        ui.add(egui::Label::new(RichText::new(&run.output).monospace()).selectable(true));
    }
    let failed = matches!(run.state, OutputState::Failed { .. });
    if !run.errors.trim().is_empty() && (failed || run.output.trim().is_empty()) {
        let errors = RichText::new(run.errors.trim_end()).monospace().color(visuals.error_fg_color);
        ui.add(egui::Label::new(errors).selectable(true));
    }
    if matches!(run.state, OutputState::Done { .. }) && run.output.trim().is_empty() {
        ui.label(RichText::new("The tool printed nothing.").weak());
    }
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
    /// A second tool that runs at the same time (comparing), with its command.
    pub also: Option<(AiTool, String)>,
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
        let tools = match &consent.also {
            Some((also, _)) => format!("{} and {}", consent.tool.name(), also.name()),
            None => consent.tool.name().to_string(),
        };
        ui.heading(format!("Run {tools} in this folder?"));
        let programs = if consent.also.is_some() { "these programs" } else { "this program" };
        ui.label(RichText::new(format!("rubrique will start {programs} in:")).weak());
        path_box(ui, &consent.folder);
        let commands: Vec<&str> = std::iter::once(consent.command.as_str())
            .chain(consent.also.as_ref().map(|(_, command)| command.as_str()))
            .collect();
        ui.label(RichText::new(if commands.len() > 1 { "Commands" } else { "Command" }).weak());
        for command in commands {
            egui::Frame::new()
                .fill(ui.visuals().code_bg_color)
                .corner_radius(5)
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(command).monospace());
                });
        }
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
