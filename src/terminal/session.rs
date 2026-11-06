//! A terminal open in a tab: a tmux session, mirrored by a background
//! thread. The thread reads the screen a dozen times a second and types the
//! keys the tab sends it. Hiding or closing the tab stops the thread; the
//! tmux session (and whatever runs in it) keeps going.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use eframe::egui::Context;

use super::keys::KeyInput;
use super::screen::Screen;
use super::tmux::{Tmux, session_name_for_file};

/// How often the screen is read while nothing is typed.
const POLL_INTERVAL: Duration = Duration::from_millis(80);
/// A short wait after typing, so the echo is on screen when it is read.
const ECHO_DELAY: Duration = Duration::from_millis(12);

/// What the tab shows.
#[derive(Clone, Debug, Default)]
pub enum Latest {
    /// The first screen has not arrived yet.
    #[default]
    Waiting,
    Screen(Screen),
    /// The session ended (`exit`) or tmux failed; holds tmux's message.
    Gone(String),
}

/// Work for the background thread.
#[derive(Debug)]
enum Request {
    Keys(Vec<KeyInput>),
    Resize(u16, u16),
}

/// The running background thread of a shown terminal.
#[derive(Debug)]
struct Worker {
    requests: Sender<Request>,
}

/// A tmux session shown in a tab.
#[derive(Debug)]
pub struct LiveTerminal {
    tmux: Tmux,
    /// The tmux session name.
    pub name: String,
    /// The `.tmux` file it was opened from; `None` for one opened from the
    /// Terminals list.
    pub file: Option<PathBuf>,
    /// Where it starts again after it ended.
    folder: PathBuf,
    /// Shared with the thread, which writes each new screen here.
    latest: Arc<Mutex<Latest>>,
    worker: Option<Worker>,
    /// The size last asked for, to resize only on change.
    size: Option<(u16, u16)>,
}

impl LiveTerminal {
    /// Show session `name` (already running, or about to be started in `folder`).
    pub fn new(tmux: Tmux, name: String, file: Option<PathBuf>, folder: PathBuf) -> Self {
        LiveTerminal { tmux, name, file, folder, latest: Arc::default(), worker: None, size: None }
    }

    /// The terminal of a `.tmux` file: its session, started in the file's folder.
    ///
    /// # Errors
    ///
    /// tmux could not start the session.
    pub fn open_file(tmux: Tmux, file: &Path) -> std::io::Result<Self> {
        let name = session_name_for_file(file);
        let folder = file.parent().map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
        tmux.start(&name, &folder)?;
        Ok(LiveTerminal::new(tmux, name, Some(file.to_path_buf()), folder))
    }

    /// What to draw now.
    pub fn latest(&self) -> Latest {
        self.latest.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Start mirroring the session, if not already (when the tab shows).
    pub fn ensure_running(&mut self, ctx: &Context) {
        if self.worker.is_some() || matches!(self.latest(), Latest::Gone(_)) {
            return;
        }
        let (requests, receiver) = mpsc::channel();
        let (tmux, name, latest, ctx) = (self.tmux.clone(), self.name.clone(), Arc::clone(&self.latest), ctx.clone());
        std::thread::spawn(move || mirror(&tmux, &name, &receiver, &latest, &ctx));
        self.worker = Some(Worker { requests });
        // A new thread starts at tmux's size; ask for ours again.
        self.size = None;
    }

    /// Stop mirroring (the tab is hidden). The session keeps running.
    pub fn pause(&mut self) {
        // Dropping the sender ends the thread's loop.
        self.worker = None;
    }

    /// Start the ended session again, in its folder. When tmux cannot, the
    /// tab keeps showing the "ended" screen with tmux's reason.
    pub fn restart(&mut self) {
        let latest = match self.tmux.start(&self.name, &self.folder) {
            Ok(()) => Latest::Waiting,
            Err(error) => Latest::Gone(format!("Cannot start it again: {error}")),
        };
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner) = latest;
        self.worker = None;
    }

    /// Type `keys` into the session.
    pub fn send(&self, keys: Vec<KeyInput>) {
        if let Some(worker) = &self.worker
            && !keys.is_empty()
        {
            // A send fails only when the thread already stopped.
            let _ = worker.requests.send(Request::Keys(keys));
        }
    }

    /// Fit the session to `columns` by `rows`, when that changed.
    pub fn resize(&mut self, columns: u16, rows: u16) {
        if self.size == Some((columns, rows)) {
            return;
        }
        if let Some(worker) = &self.worker {
            let _ = worker.requests.send(Request::Resize(columns, rows));
            self.size = Some((columns, rows));
        }
    }

    /// The folder it starts in.
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Follow the session after it was renamed from the Terminals list.
    pub fn follow_session_rename(&mut self, name: &str) {
        self.name = name.to_string();
        // The mirroring thread watches the old name; a new one starts on the next frame.
        self.worker = None;
    }

    /// Follow a rename of the `.tmux` file: its session gets the new name.
    pub fn follow_rename(&mut self, file: &Path) {
        let name = session_name_for_file(file);
        // Best effort: when the session is not running, it starts under the
        // new name next time.
        let _ = self.tmux.rename(&self.name, &name);
        self.name = name;
        self.file = Some(file.to_path_buf());
        self.worker = None;
    }
}

/// The background thread: apply requests, read the screen, store it when it
/// changed and ask for a repaint. Stops when the tab drops its sender, or
/// when the session is gone.
fn mirror(tmux: &Tmux, name: &str, requests: &Receiver<Request>, latest: &Mutex<Latest>, ctx: &Context) {
    let mut last: Option<Screen> = None;
    loop {
        match requests.recv_timeout(POLL_INTERVAL) {
            Ok(request) => {
                apply(tmux, name, request);
                for request in requests.try_iter() {
                    apply(tmux, name, request);
                }
                std::thread::sleep(ECHO_DELAY);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        match tmux.capture(name) {
            Ok(screen) if last.as_ref() != Some(&screen) => {
                *latest.lock().unwrap_or_else(PoisonError::into_inner) = Latest::Screen(screen.clone());
                last = Some(screen);
                ctx.request_repaint();
            }
            Ok(_) => {}
            Err(error) => {
                *latest.lock().unwrap_or_else(PoisonError::into_inner) = Latest::Gone(error.to_string());
                ctx.request_repaint();
                return;
            }
        }
    }
}

/// Send one request to tmux. Errors show up in the next capture.
fn apply(tmux: &Tmux, name: &str, request: Request) {
    let _ = match request {
        Request::Keys(keys) => tmux.send(name, &keys),
        Request::Resize(columns, rows) => tmux.resize(name, columns, rows),
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempFolder;
    use std::time::Instant;

    /// Runs only where tmux is installed; uses its own tmux server.
    #[test]
    fn a_terminal_file_mirrors_its_session_and_takes_keys() {
        let socket = format!("rubrique-session-test-{}", std::process::id());
        let Some(tmux) = Tmux::with_socket(&socket) else {
            return;
        };
        let temp = TempFolder::new();
        let file = temp.write("build.tmux", "");
        let mut terminal = LiveTerminal::open_file(tmux.clone(), &file).unwrap();
        assert_eq!(terminal.name, session_name_for_file(&file));
        let ctx = Context::default();
        terminal.ensure_running(&ctx);
        terminal.resize(60, 10);
        terminal.send(vec![KeyInput::Text("echo mirrored-$((6*7))".to_string()), KeyInput::Key("Enter".to_string())]);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut seen = String::new();
        while Instant::now() < deadline {
            if let Latest::Screen(screen) = terminal.latest() {
                seen = screen.plain_text();
                if seen.contains("mirrored-42") && screen.columns == 60 {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        terminal.pause();
        tmux.kill_server();
        assert!(seen.contains("mirrored-42"), "screen was:\n{seen}");
    }
}
