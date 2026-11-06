//! Terminals in the app: open a tmux session in a tab (from the Terminals
//! list or a `.tmux` file), start new ones, and create `.tmux` files.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::{OpenFile, RubriqueApp};
use crate::explorer::tree::FsNode;
use crate::remote;
use crate::terminal::session::LiveTerminal;
use crate::terminal::tmux::new_session_name;
use crate::terminal::{TERMINAL_EXTENSION, TERMINAL_FILE_TEXT};

/// Most `terminal-N.tmux` names tried in one folder.
const MAX_FILE_NUMBER: u32 = 999;

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

    /// Start a new session in the first local folder and show it.
    pub(super) fn new_terminal(&mut self) {
        let Some(monitor) = &self.terminals else {
            return;
        };
        let name = new_session_name(&monitor.rows().into_iter().map(|row| row.info).collect::<Vec<_>>());
        let folder = self.new_terminal_folder();
        if let Err(error) = monitor.tmux().start(&name, &folder) {
            self.show_status(format!("Cannot start a terminal: {error}"));
            return;
        }
        monitor.refresh();
        self.open_terminal_session(&name);
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

    /// Create `terminal.tmux` (or `terminal-2.tmux`, …) in `folder` and open it.
    pub(super) fn new_terminal_file(&mut self, folder: &Path) {
        match create_terminal_file(folder) {
            Ok(path) => {
                self.roots.iter_mut().for_each(FsNode::refresh);
                self.open_in_tab(path);
            }
            Err(error) => self.show_status(format!("Cannot create a terminal file: {error}")),
        }
    }
}

/// Write a new `.tmux` file in `folder` under the first free name. Never
/// replaces a file: `create_new` fails when the name is taken.
fn create_terminal_file(folder: &Path) -> std::io::Result<PathBuf> {
    for number in 1..=MAX_FILE_NUMBER {
        let name = if number == 1 {
            format!("terminal.{TERMINAL_EXTENSION}")
        } else {
            format!("terminal-{number}.{TERMINAL_EXTENSION}")
        };
        let path = folder.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(TERMINAL_FILE_TEXT.as_bytes())?;
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::other("too many terminal files in this folder"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempFolder;

    #[test]
    fn creates_numbered_terminal_files_without_replacing() {
        let temp = TempFolder::new();
        let first = create_terminal_file(temp.path()).unwrap();
        let second = create_terminal_file(temp.path()).unwrap();
        assert_eq!(first.file_name().unwrap(), "terminal.tmux");
        assert_eq!(second.file_name().unwrap(), "terminal-2.tmux");
        assert_eq!(std::fs::read_to_string(&first).unwrap(), TERMINAL_FILE_TEXT);
    }
}
