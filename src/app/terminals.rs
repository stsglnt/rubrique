//! Terminals in the app: open a tmux session in a tab (from the Terminals
//! list or a `.tmux` file), and the "New terminal" dialog that names a new
//! session or a new `.tmux` file.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use eframe::egui::{self, Context, Key, RichText};

use super::{OpenFile, RubriqueApp};
use crate::explorer::tree::FsNode;
use crate::remote;
use crate::terminal::session::LiveTerminal;
use crate::terminal::tmux::new_session_name;
use crate::terminal::{TERMINAL_EXTENSION, TERMINAL_FILE_TEXT};

/// Width of the "New terminal" dialog.
const DIALOG_WIDTH: f32 = 340.0;
/// Most `terminal-N` names tried when suggesting a file name.
const MAX_FILE_NUMBER: u32 = 999;

/// The "New terminal" dialog, while it is open.
#[derive(Debug)]
pub(super) struct NewTerminalDialog {
    /// The name being typed.
    name: String,
    /// `Some`: create a `.tmux` file in this folder. `None`: start a tmux
    /// session (from the Terminals list).
    folder: Option<PathBuf>,
    /// Why the last attempt failed, shown under the field.
    error: Option<String>,
    /// The field has been given focus once.
    focused: bool,
}

impl RubriqueApp {
    /// A `.tmux` file as a tab: its session, started in the file's folder.
    /// `None` (with a status message) when it cannot be opened.
    pub(super) fn load_terminal(&mut self, path: &Path) -> Option<OpenFile> {
        if remote::is_mirror(path) {
            self.show_status("Terminals work in local folders only for now".to_string());
            return None;
        }
        let Some(monitor) = &self.terminals else {
            self.show_status("Install tmux to open terminals (macOS: brew install tmux)".to_string());
            return None;
        };
        match LiveTerminal::open_file(monitor.tmux().clone(), path) {
            Ok(terminal) => {
                monitor.refresh();
                Some(OpenFile::Terminal { terminal: Box::new(terminal) })
            }
            Err(error) => {
                self.show_status(format!("Cannot start the terminal: {error}"));
                None
            }
        }
    }

    /// Show session `name`: switch to its tab, or open one.
    pub(super) fn open_terminal_session(&mut self, name: &str) {
        let shows_session = |file: &OpenFile| matches!(file, OpenFile::Terminal { terminal } if terminal.name == name);
        if let Some(index) = (0..self.tabs.len()).find(|&index| shows_session(self.tab(index))) {
            self.switch_to_tab(index);
            return;
        }
        let Some(monitor) = &self.terminals else {
            return;
        };
        let folder = monitor
            .rows()
            .into_iter()
            .find(|row| row.info.name == name)
            .map_or_else(|| self.new_terminal_folder(), |row| row.info.folder);
        let terminal = LiveTerminal::new(monitor.tmux().clone(), name.to_string(), None, folder);
        self.add_tab(OpenFile::Terminal { terminal: Box::new(terminal) });
    }

    /// Ask for the name of a new tmux session (from the Terminals list).
    pub(super) fn request_new_terminal(&mut self) {
        let Some(monitor) = &self.terminals else {
            return;
        };
        let sessions: Vec<_> = monitor.rows().into_iter().map(|row| row.info).collect();
        let name = new_session_name(&sessions);
        self.new_terminal = Some(NewTerminalDialog { name, folder: None, error: None, focused: false });
    }

    /// Ask for the name of a new `.tmux` file in `folder`.
    pub(super) fn request_new_terminal_file(&mut self, folder: PathBuf) {
        let name = free_file_name(&folder);
        self.new_terminal = Some(NewTerminalDialog { name, folder: Some(folder), error: None, focused: false });
    }

    /// Draw the "New terminal" dialog while it is open.
    pub(super) fn new_terminal_dialog(&mut self, ctx: &Context) {
        let start_folder = self.new_terminal_folder();
        let Some(dialog) = &mut self.new_terminal else {
            return;
        };
        let modal = egui::Modal::new(egui::Id::unique("new-terminal")).show(ctx, |ui| {
            ui.set_width(DIALOG_WIDTH);
            ui.heading("New terminal");
            ui.add_space(4.0);
            let field = ui.add(egui::TextEdit::singleline(&mut dialog.name).desired_width(f32::INFINITY));
            if !dialog.focused {
                field.request_focus();
                dialog.focused = true;
            }
            let entered = field.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
            let place = match &dialog.folder {
                Some(folder) => format!("Adds {}.{TERMINAL_EXTENSION} in {}", dialog.name.trim(), folder.display()),
                None => format!("A tmux session that starts in {}", start_folder.display()),
            };
            ui.label(RichText::new(place).small().weak());
            if let Some(error) = &dialog.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let create = ui.button("Create").clicked() || entered;
                let cancel = ui.button("Cancel").clicked();
                (create, cancel)
            })
            .inner
        });
        let (create, cancel) = modal.inner;
        if cancel || modal.should_close() {
            self.new_terminal = None;
        } else if create {
            self.create_terminal();
        }
    }

    /// Create what the dialog asks for and open it; on failure the dialog
    /// stays open with the reason.
    fn create_terminal(&mut self) {
        let Some(dialog) = &self.new_terminal else {
            return;
        };
        let result = match clean_terminal_name(&dialog.name) {
            Ok(name) => match dialog.folder.clone() {
                Some(folder) => self.create_terminal_file(&folder, &name),
                None => self.start_named_session(&name),
            },
            Err(error) => Err(error),
        };
        match result {
            Ok(()) => self.new_terminal = None,
            Err(error) => {
                if let Some(dialog) = &mut self.new_terminal {
                    dialog.error = Some(error);
                    dialog.focused = false;
                }
            }
        }
    }

    /// Start session `name` in the first local folder and show it.
    fn start_named_session(&mut self, name: &str) -> Result<(), String> {
        let Some(monitor) = &self.terminals else {
            return Err("tmux is not installed".to_string());
        };
        if monitor.tmux().has_session(name) {
            return Err(format!("A terminal called \u{201c}{name}\u{201d} is already running"));
        }
        let folder = self.new_terminal_folder();
        monitor.tmux().start(name, &folder).map_err(|error| format!("Cannot start it: {error}"))?;
        monitor.refresh();
        self.open_terminal_session(name);
        Ok(())
    }

    /// Create `<name>.tmux` in `folder` and open it.
    fn create_terminal_file(&mut self, folder: &Path, name: &str) -> Result<(), String> {
        let path = write_terminal_file(folder, name).map_err(|error| match error.kind() {
            std::io::ErrorKind::AlreadyExists => format!("{name}.{TERMINAL_EXTENSION} already exists here"),
            _ => format!("Cannot create it: {error}"),
        })?;
        self.roots.iter_mut().for_each(FsNode::refresh);
        self.open_in_tab(path);
        Ok(())
    }

    /// Where a new terminal starts: the first local explorer folder, or home.
    fn new_terminal_folder(&self) -> PathBuf {
        self.roots
            .iter()
            .map(|root| root.path.clone())
            .find(|path| !remote::is_mirror(path))
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/"))
    }
}

/// The typed name, trimmed and without a `.tmux` the user may have added.
/// Letters, digits, spaces, `-` and `_` only: the name becomes a tmux
/// session or a file name, and tmux reserves `.` and `:`.
fn clean_terminal_name(typed: &str) -> Result<String, String> {
    let name = typed.trim();
    let name = name.strip_suffix(&format!(".{TERMINAL_EXTENSION}")).unwrap_or(name).trim();
    if name.is_empty() {
        return Err("Type a name".to_string());
    }
    if !name.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_')) {
        return Err("Use letters, digits, spaces, - and _".to_string());
    }
    Ok(name.to_string())
}

/// A name for a new `.tmux` file that is free in `folder`: `terminal`,
/// then `terminal-2`, and so on.
fn free_file_name(folder: &Path) -> String {
    (1..=MAX_FILE_NUMBER)
        .map(|number| if number == 1 { "terminal".to_string() } else { format!("terminal-{number}") })
        .find(|name| !folder.join(format!("{name}.{TERMINAL_EXTENSION}")).exists())
        .unwrap_or_else(|| "terminal".to_string())
}

/// Write `<name>.tmux` in `folder`. Never replaces a file: `create_new`
/// fails with `AlreadyExists` when the name is taken.
fn write_terminal_file(folder: &Path, name: &str) -> std::io::Result<PathBuf> {
    let path = folder.join(format!("{name}.{TERMINAL_EXTENSION}"));
    let mut file = OpenOptions::new().write(true).create_new(true).open(&path)?;
    file.write_all(TERMINAL_FILE_TEXT.as_bytes())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempFolder;

    #[test]
    fn suggests_free_names_and_never_replaces_a_file() {
        let temp = TempFolder::new();
        assert_eq!(free_file_name(temp.path()), "terminal");
        let path = write_terminal_file(temp.path(), "terminal").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), TERMINAL_FILE_TEXT);
        assert_eq!(free_file_name(temp.path()), "terminal-2");
        let taken = write_terminal_file(temp.path(), "terminal").unwrap_err();
        assert_eq!(taken.kind(), std::io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn cleans_typed_names() {
        assert_eq!(clean_terminal_name("  build "), Ok("build".to_string()));
        assert_eq!(clean_terminal_name("api server.tmux"), Ok("api server".to_string()));
        assert!(clean_terminal_name("   ").is_err());
        assert!(clean_terminal_name("a/b").is_err());
        assert!(clean_terminal_name("v1.2").is_err());
    }
}
