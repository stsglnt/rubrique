//! Ask (Cmd+K): pick the text, run the AI tool, show its output, and review
//! the answer in the merge screen.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui::{self, Context, Ui, pos2};

use super::{RubriqueApp, OpenFile, char_to_byte};
use crate::ai::run::{AiRun, RunEnd};
use crate::ai::skills::Skill;
use crate::ai::view::{
    AskBar, AskEvent, Consent, ConsentChoice, OutputAction, OutputInfo, OutputState, show_ask_bar, show_consent,
    show_output,
};
use crate::ai::{self, AiTool, AskKind};
use crate::document::merge::MergeSession;
use crate::document::preview_selection;
use crate::document::view::editor_selection;
use crate::markdown::notes::locate;

/// Space between the top of the page and the Ask bar.
const ASK_TOP: f32 = 56.0;
/// Width the Ask bar takes, to center it on the page.
const ASK_OUTER_WIDTH: f32 = 480.0;
/// Longest request shown in the merge column title.
const TITLE_REQUEST_CHARS: usize = 28;

/// The text a request is about.
#[derive(Clone, Debug)]
pub(super) struct AskTarget {
    path: PathBuf,
    /// Byte range in the document when the request was made.
    range: Range<usize>,
    text: String,
    whole_file: bool,
}

/// A request ready to run.
#[derive(Debug)]
pub(super) struct PendingRun {
    target: AskTarget,
    request: String,
    kind: AskKind,
    tool: AiTool,
    skill: Option<Box<Skill>>,
}

/// A run and what it was for, while its output panel is open.
#[derive(Debug)]
pub(super) struct AiJob {
    run: AiRun,
    pending: PendingRun,
    /// The command as shown in the panel.
    command: String,
}

/// The Ask bar, the text it is about, and a run waiting for permission.
#[derive(Debug, Default)]
pub(super) struct AskState {
    bar: Option<(AskBar, AskTarget)>,
    consent: Option<(Consent, PendingRun)>,
    pub(super) job: Option<AiJob>,
    /// Where the page (editor or preview) was drawn, to place the bar.
    pub(super) page_rect: Option<egui::Rect>,
}

impl AskState {
    pub(super) fn is_open(&self) -> bool {
        self.bar.is_some()
    }
}

impl RubriqueApp {
    /// Open the Ask bar for the selection, or the whole file when nothing is selected.
    pub(super) fn open_ask(&mut self, ctx: &Context) {
        self.finish_block_edit();
        if self.merging.is_some() {
            return;
        }
        if self.ask.job.as_ref().is_some_and(|job| job.run.is_running()) {
            self.show_status("Wait for the AI tool to finish, or press Stop".to_string());
            return;
        }
        let OpenFile::Text { document, .. } = &self.open_file else {
            self.show_status("Open a file first, then ask".to_string());
            return;
        };
        let text = &document.text;
        let range = if self.showing_preview() {
            match preview_selection::current(ctx) {
                Some(selection) => {
                    let Some(range) = locate(text, &selection.text) else {
                        self.show_status("Cannot find the selection in the file; try in Edit mode".to_string());
                        return;
                    };
                    Some(range)
                }
                None => None,
            }
        } else {
            editor_selection(ctx).map(|(first, last)| char_to_byte(text, first)..char_to_byte(text, last))
        };
        let (range, whole_file) = match range {
            Some(range) if !range.is_empty() => (range, false),
            _ => (0..text.len(), true),
        };
        let scope = if whole_file {
            "Whole file".to_string()
        } else {
            match text[range.clone()].lines().count() {
                0 | 1 => "1 line selected".to_string(),
                lines => format!("{lines} lines selected"),
            }
        };
        let target =
            AskTarget { path: document.path.clone(), text: text[range.clone()].to_string(), range, whole_file };
        self.ask.bar = Some((AskBar::new(self.settings.ai.default_tool, scope), target));
    }

    /// Draw the Ask bar and the consent dialog when they are open.
    pub(super) fn ask_dialogs(&mut self, ctx: &Context) {
        if let Some((bar, target)) = &mut self.ask.bar {
            let page = self.ask.page_rect.unwrap_or_else(|| ctx.content_rect());
            let left = page.left() + ((page.width() - ASK_OUTER_WIDTH) / 2.0).max(8.0);
            let event = show_ask_bar(
                ctx,
                pos2(left, page.top() + ASK_TOP),
                bar,
                self.ai_found.each_ref().map(Option::is_some),
                &self.skills,
            );
            match event {
                Some(AskEvent::Run { request, kind, skill }) => {
                    let pending = PendingRun { target: target.clone(), request, kind, tool: bar.tool, skill };
                    self.ask.bar = None;
                    self.begin_run(ctx, pending);
                }
                Some(AskEvent::Close) => self.ask.bar = None,
                None => {}
            }
        }
        if let Some((consent, _)) = &mut self.ask.consent
            && let Some(choice) = show_consent(ctx, consent)
            && let Some((consent, pending)) = self.ask.consent.take()
            && choice == ConsentChoice::Run
        {
            if consent.remember {
                self.settings.ai.allow(&consent.folder);
                self.write_settings();
            }
            self.start_run(ctx, pending);
        }
    }

    /// Ask for permission the first time in a folder, then run.
    fn begin_run(&mut self, ctx: &Context, pending: PendingRun) {
        let folder = run_folder(&pending.target.path);
        if self.settings.ai.is_allowed(&folder) {
            self.start_run(ctx, pending);
        } else {
            let command = self.settings.ai.command(pending.tool).to_string();
            let consent = Consent { tool: pending.tool, folder, command, remember: true };
            self.ask.consent = Some((consent, pending));
        }
    }

    fn start_run(&mut self, ctx: &Context, pending: PendingRun) {
        let command = self.settings.ai.command(pending.tool).to_string();
        if let Some(problem) = ai::check_command(&command) {
            self.show_status(format!("{} command: {} (Settings → AI)", pending.tool.name(), problem.message()));
            return;
        }
        let target = &pending.target;
        let mut prompt = ai::build_prompt(&pending.request, &target.text, pending.kind, target.whole_file);
        if let Some(skill) = &pending.skill {
            prompt = prompt.with_skill(skill);
        }
        if prompt.is_too_long() {
            self.show_status("The text is too long to send; select a smaller part".to_string());
            return;
        }
        let Some((program, args)) = ai::command_args(&command, &prompt) else {
            return;
        };
        let repaint = ctx.clone();
        match AiRun::start(&program, &args, &run_folder(&target.path), move || repaint.request_repaint()) {
            Ok(run) => {
                let shown = ai::display_command(&command, &pending.request);
                self.ask.job = Some(AiJob { run, pending, command: shown });
            }
            Err(error) => self.show_status(format!("Cannot start {program}: {error}. Check Settings → AI.")),
        }
    }

    /// The output panel of the current run.
    pub(super) fn ai_output_panel(&mut self, ui: &mut Ui) {
        let Some(job) = &mut self.ask.job else {
            return;
        };
        let seconds = job.run.elapsed_seconds();
        let state = match job.run.poll() {
            None => {
                ui.ctx().request_repaint_after(Duration::from_secs(1));
                OutputState::Running { seconds }
            }
            Some(RunEnd::Stopped) => OutputState::Stopped { seconds },
            Some(RunEnd::Exited { success: true, .. }) => OutputState::Done { seconds },
            Some(RunEnd::Exited { code, .. }) => OutputState::Failed { seconds, code: *code },
        };
        let output = job.run.output_text();
        let can_review = matches!(state, OutputState::Done { .. })
            && job.pending.kind == AskKind::Rewrite
            && !output.trim().is_empty();
        let request = match &job.pending.skill {
            Some(skill) => format!("{} · {}", skill.name, job.pending.request),
            None => job.pending.request.clone(),
        };
        let info = OutputInfo {
            tool: job.pending.tool,
            request: &request,
            command: &job.command,
            output: &output,
            errors: &job.run.error_text(),
            state,
            can_review,
        };
        match show_output(ui, &info) {
            Some(OutputAction::Stop) => job.run.stop(),
            Some(OutputAction::Copy) => self.copy_to_clipboard(ui.ctx(), output.trim().to_string()),
            Some(OutputAction::Close) => self.ask.job = None,
            Some(OutputAction::Review) => self.review_answer(),
            None => {}
        }
    }

    /// Open the merge screen with the answer in place of the target text.
    fn review_answer(&mut self) {
        let Some(job) = &self.ask.job else {
            return;
        };
        let target = &job.pending.target;
        let OpenFile::Text { document, .. } = &self.open_file else {
            self.show_status("Open the file again to review the answer".to_string());
            return;
        };
        if document.path != target.path {
            let name = target.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            self.show_status(format!("Open {name} to review the answer"));
            return;
        }
        if document.text.get(target.range.clone()) != Some(target.text.as_str()) {
            self.show_status("The text changed while the tool ran; ask again".to_string());
            return;
        }
        let answer = ai::clean_reply(&job.run.output_text(), &target.text);
        let mut suggestion = document.text.clone();
        suggestion.replace_range(target.range.clone(), &answer);
        if suggestion == document.text {
            self.show_status("The tool suggested no changes".to_string());
            return;
        }
        let title = format!("{} · \u{201c}{}\u{201d}", job.pending.tool.name(), short(&job.pending.request));
        self.merging = Some(MergeSession::review(&document.text, &suggestion, title));
    }
}

/// Tools run in the file's folder, so they can read nearby notes.
fn run_folder(path: &Path) -> PathBuf {
    path.parent().map_or_else(|| PathBuf::from("/"), Path::to_path_buf)
}

fn short(request: &str) -> String {
    let flat = request.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > TITLE_REQUEST_CHARS {
        format!("{}…", flat.chars().take(TITLE_REQUEST_CHARS).collect::<String>())
    } else {
        flat
    }
}
