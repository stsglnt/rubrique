//! Keep the explorer's Terminals list fresh: every couple of seconds a
//! background thread lists the tmux sessions, reads the screen of the ones
//! running an agent, and works out each one's status.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eframe::egui::Context;

use super::status::{TerminalStatus, is_agent, terminal_status};
use super::tmux::{SessionInfo, Tmux};

/// How often the sessions are checked.
const CHECK_INTERVAL: Duration = Duration::from_secs(2);

/// One session with its status, as the explorer shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalRow {
    pub info: SessionInfo,
    pub status: TerminalStatus,
}

/// The running check. Dropping it stops the thread.
#[derive(Debug)]
pub struct TerminalMonitor {
    tmux: Tmux,
    rows: Arc<Mutex<Vec<TerminalRow>>>,
    /// Wakes the thread for a check now; dropping it stops the thread.
    wake: Sender<()>,
}

impl TerminalMonitor {
    /// Start checking the sessions of `tmux`.
    pub fn start(tmux: Tmux, ctx: &Context) -> Self {
        let rows = Arc::default();
        let (wake, woken) = mpsc::channel();
        let (thread_tmux, thread_rows, ctx) = (tmux.clone(), Arc::clone(&rows), ctx.clone());
        std::thread::spawn(move || check_loop(&thread_tmux, &woken, &thread_rows, &ctx));
        TerminalMonitor { tmux, rows, wake }
    }

    /// The tmux program, to start and open sessions.
    pub fn tmux(&self) -> &Tmux {
        &self.tmux
    }

    /// The sessions from the last check, newest first.
    pub fn rows(&self) -> Vec<TerminalRow> {
        self.rows.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Check now (after a session was started).
    pub fn refresh(&self) {
        let _ = self.wake.send(());
    }
}

/// Check, store the rows when they changed, wait; until the monitor is dropped.
fn check_loop(tmux: &Tmux, woken: &Receiver<()>, rows: &Mutex<Vec<TerminalRow>>, ctx: &Context) {
    loop {
        let fresh = check(tmux);
        let mut stored = rows.lock().unwrap_or_else(PoisonError::into_inner);
        if *stored != fresh {
            *stored = fresh;
            ctx.request_repaint();
        }
        drop(stored);
        match woken.recv_timeout(CHECK_INTERVAL) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Every session and its status, newest first.
fn check(tmux: &Tmux) -> Vec<TerminalRow> {
    let now = unix_now();
    let mut rows: Vec<TerminalRow> = tmux
        .list_sessions()
        .into_iter()
        .map(|info| {
            // Only an agent's screen says anything about its state.
            let screen =
                if is_agent(&info.command) { tmux.plain_screen(&info.name).unwrap_or_default() } else { String::new() };
            let status = terminal_status(&info.command, now - info.last_output, &screen);
            TerminalRow { info, status }
        })
        .collect();
    rows.sort_by(|a, b| b.info.created.cmp(&a.info.created).then_with(|| a.info.name.cmp(&b.info.name)));
    rows
}

/// Seconds since 1970, as tmux counts.
/// Now, in Unix seconds.
pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or(0)
}

/// When a terminal was opened: `14:02` today, otherwise `2026-11-03 14:02`.
pub fn opened_label(created: i64) -> String {
    let Some(opened) = crate::date::local_at(created) else {
        return String::new();
    };
    let time = format!("{:02}:{:02}", opened.hour, opened.minute);
    if opened.date == crate::date::today_iso() { time } else { format!("{} {time}", opened.date) }
}

/// How long ago a Unix time was, roughly: "just now", "5 min ago", "3 h ago", "2 days ago".
pub fn ago_label(time: i64) -> String {
    let seconds = (unix_now() - time).max(0);
    match seconds {
        0..60 => "just now".to_string(),
        60..3600 => format!("{} min ago", seconds / 60),
        3600..86_400 => format!("{} h ago", seconds / 3600),
        _ => format!("{} days ago", seconds / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opened_today_shows_only_the_time() {
        let label = opened_label(unix_now());
        assert_eq!(label.len(), 5, "{label}");
        assert!(opened_label(0).contains("19"), "an old date shows the date");
    }

    #[test]
    fn ago_labels() {
        assert_eq!(ago_label(unix_now()), "just now");
        assert_eq!(ago_label(unix_now() - 300), "5 min ago");
        assert_eq!(ago_label(unix_now() - 7300), "2 h ago");
    }
}
