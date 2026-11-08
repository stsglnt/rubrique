//! Helm charts: the Helm button renders (`helm template`) or lints the
//! chart the open file is in, and shows what `helm` printed in a panel.

use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui::{self, RichText, Ui};

use super::{OpenFile, RubriqueApp};
use crate::ai::run::{AiRun, RunEnd};
use crate::document::highlighting::{HighlightStyle, Syntax, highlighted};
use crate::explorer::paths::name_text;
use crate::helm::{HelmAction, INSTALL_PAGE, chart_root, find_helm};

/// The chart of the open file, and the last `helm` run.
#[derive(Debug, Default)]
pub(super) struct HelmState {
    /// The open file and its chart folder, looked up once per file.
    chart_of: Option<(PathBuf, Option<PathBuf>)>,
    /// What the panel shows; `None` hides it.
    pub(super) panel: Option<HelmPanel>,
}

/// One `helm` run, or why it could not start.
#[derive(Debug)]
pub(super) struct HelmPanel {
    action: HelmAction,
    chart: PathBuf,
    /// The file it was started from (for an extra values file).
    file: PathBuf,
    /// The command as shown: `helm template .`.
    command: String,
    outcome: Result<AiRun, String>,
}

/// A click in the panel.
enum PanelAction {
    Run(HelmAction),
    Stop,
    Copy,
    Close,
}

impl RubriqueApp {
    /// The chart folder the open text file is in, if any.
    pub(super) fn open_chart(&mut self) -> Option<PathBuf> {
        let OpenFile::Text { document, .. } = &self.open_file else {
            return None;
        };
        match &self.helm.chart_of {
            Some((path, chart)) if *path == document.path => chart.clone(),
            _ => {
                let chart = chart_root(&document.path);
                self.helm.chart_of = Some((document.path.clone(), chart.clone()));
                chart
            }
        }
    }

    /// The Helm button in the status bar, for files inside a chart.
    pub(super) fn helm_button(&mut self, ui: &mut Ui) {
        if self.open_chart().is_some()
            && ui.small_button("Helm").on_hover_text("Render this chart with helm template, or lint it").clicked()
        {
            self.run_helm(HelmAction::Render);
        }
    }

    /// Run `helm` on the open file's chart and show the panel.
    pub(super) fn run_helm(&mut self, action: HelmAction) {
        let open_chart = self.open_chart();
        let (chart, file) = match (open_chart, self.open_file.path(), &self.helm.panel) {
            (Some(chart), Some(file), _) => (chart, file.to_path_buf()),
            // The panel can run again for its chart while another file is open.
            (_, _, Some(panel)) => (panel.chart.clone(), panel.file.clone()),
            _ => return,
        };
        if let Some(HelmPanel { outcome: Ok(run), .. }) = &mut self.helm.panel {
            run.stop();
        }
        let args = action.args(&chart, &file);
        let command = format!("helm {}", args.join(" "));
        let outcome = match find_helm() {
            Some(program) => {
                let repaint = self.repaint.clone();
                AiRun::start(&program.display().to_string(), &args, &chart, move || repaint.request_repaint())
                    .map_err(|error| format!("Cannot start helm: {error}"))
            }
            None => Err("helm is not installed, or not on PATH.".to_string()),
        };
        self.helm.panel = Some(HelmPanel { action, chart, file, command, outcome });
    }

    /// The panel with what `helm` printed.
    pub(super) fn helm_panel(&mut self, ui: &mut Ui) {
        // helm reads the files on disk, so unsaved changes are left out.
        let unsaved = self.tabs_with_unsaved_changes_in_chart();
        let Some(panel) = &mut self.helm.panel else {
            return;
        };
        let action = show_panel(ui, panel, unsaved);
        match action {
            Some(PanelAction::Run(action)) => self.run_helm(action),
            Some(PanelAction::Stop) => {
                if let Ok(run) = &mut panel.outcome {
                    run.stop();
                }
            }
            Some(PanelAction::Copy) => {
                if let Ok(run) = &panel.outcome {
                    let text = run.output_text();
                    self.copy_to_clipboard(ui.ctx(), text);
                }
            }
            Some(PanelAction::Close) => {
                if let Some(HelmPanel { outcome: Ok(run), .. }) = &mut self.helm.panel {
                    run.stop();
                }
                self.helm.panel = None;
            }
            None => {}
        }
    }

    /// How many open files in the panel's chart have unsaved changes.
    fn tabs_with_unsaved_changes_in_chart(&self) -> usize {
        let Some(panel) = &self.helm.panel else {
            return 0;
        };
        let in_chart =
            |file: &OpenFile| file.is_dirty() && file.path().is_some_and(|path: &Path| path.starts_with(&panel.chart));
        // The active tab's slot is empty while its file is in `open_file`.
        usize::from(in_chart(&self.open_file)) + self.tabs.iter().filter(|tab| in_chart(tab)).count()
    }
}

/// Draw the panel: a header with the buttons, then the output.
fn show_panel(ui: &mut Ui, panel: &mut HelmPanel, unsaved: usize) -> Option<PanelAction> {
    let mut action = None;
    let running = panel.outcome.as_mut().is_ok_and(|run| run.poll().is_none());
    if running {
        ui.ctx().request_repaint_after(Duration::from_secs(1));
    }
    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.strong(format!("Helm: {}", name_text(&panel.chart)));
            ui.label(RichText::new(&panel.command).monospace().weak());
            ui.label(state_text(&panel.outcome));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Close").clicked() {
                    action = Some(PanelAction::Close);
                }
                if running && ui.small_button("Stop").clicked() {
                    action = Some(PanelAction::Stop);
                }
                if panel.outcome.is_ok()
                    && !running
                    && ui.small_button("Copy").on_hover_text("Copy the output").clicked()
                {
                    action = Some(PanelAction::Copy);
                }
                for choice in [HelmAction::Lint, HelmAction::Render] {
                    let tip = match choice {
                        HelmAction::Render => "Show the Kubernetes files the chart makes (helm template)",
                        HelmAction::Lint => "Check the chart for problems (helm lint)",
                    };
                    if ui.small_button(choice.name()).on_hover_text(tip).clicked() {
                        action = Some(PanelAction::Run(choice));
                    }
                }
            });
        });
        if unsaved > 0 {
            let noun = if unsaved == 1 { "file has" } else { "files have" };
            ui.label(RichText::new(format!("{unsaved} {noun} unsaved changes; helm uses the saved files.")).weak());
        }
        ui.separator();
        match &panel.outcome {
            Err(error) => {
                ui.colored_label(ui.visuals().error_fg_color, error);
                ui.hyperlink_to("How to install helm", INSTALL_PAGE);
            }
            Ok(run) => show_output(ui, panel.action, run),
        }
    });
    action
}

/// "Running… 3 s", "Done", "Failed (exit 1)".
fn state_text(outcome: &Result<AiRun, String>) -> String {
    let Ok(run) = outcome else {
        return String::new();
    };
    match run.end() {
        None => format!("Running… {} s", run.elapsed_seconds()),
        Some(RunEnd::Stopped) => "Stopped".to_string(),
        Some(RunEnd::Exited { success: true, .. }) => "Done".to_string(),
        Some(RunEnd::Exited { code: Some(code), .. }) => format!("Failed (exit {code})"),
        Some(RunEnd::Exited { code: None, .. }) => "Failed".to_string(),
    }
}

/// The errors in the error color, then the output: colored YAML for a
/// render, plain text for lint.
fn show_output(ui: &mut Ui, action: HelmAction, run: &AiRun) {
    let errors = run.error_text();
    let output = run.output_text();
    egui::ScrollArea::both().id_salt("helm-output").auto_shrink([false, false]).show(ui, |ui| {
        if !errors.trim().is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, RichText::new(errors.trim_end()).monospace());
        }
        if output.is_empty() {
            return;
        }
        if action == HelmAction::Render {
            let style = HighlightStyle::from_ui(ui);
            let mut job = highlighted(ui.ctx(), &output, &style, Some(Syntax::Yaml));
            // Rendered files read best unwrapped; the panel scrolls sideways.
            job.wrap.max_width = f32::INFINITY;
            ui.add(egui::Label::new(job).selectable(true).extend());
        } else {
            ui.add(egui::Label::new(RichText::new(output.trim_end()).monospace()).selectable(true).extend());
        }
    });
}
