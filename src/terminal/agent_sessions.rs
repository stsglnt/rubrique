//! Agent sessions a terminal has run: which Claude Code or Codex
//! conversation is open in it, remembered so it can be resumed later in the
//! same terminal or a new one.
//!
//! The agents keep each conversation in a file named after its id:
//! Claude Code in `~/.claude/projects/<folder>/<id>.jsonl` (the folder's
//! path with every other character turned into `-`), Codex in
//! `~/.codex/sessions/<year>/<month>/<day>/rollout-<time>-<id>.jsonl`, whose
//! first line names the folder it ran in. The conversation open in a
//! terminal is the file in its folder written closest to the terminal's
//! last output.

use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Most sessions remembered per terminal; the oldest go first.
const MAX_PER_TERMINAL: usize = 20;
/// Longest title kept, in characters.
const TITLE_CHARS: usize = 60;
/// How many days of Codex session folders are searched.
const CODEX_DAYS: usize = 7;
/// Most lines read from a Claude session while looking for its first prompt.
const TITLE_LINES: usize = 200;

/// One agent conversation that ran in a terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSession {
    /// The tmux session it ran in.
    pub terminal: String,
    /// `claude` or `codex`.
    pub agent: String,
    /// The conversation id the agent resumes by.
    pub id: String,
    /// The folder it ran in (a resumed conversation starts there).
    pub folder: PathBuf,
    /// When it was last seen running (Unix seconds).
    pub last_used: i64,
    /// Its first prompt, shortened; empty when unknown.
    pub title: String,
}

impl AgentSession {
    /// The shell command that resumes it.
    pub fn resume_command(&self) -> String {
        match self.agent.as_str() {
            "codex" => format!("codex resume {}", self.id),
            _ => format!("claude --resume {}", self.id),
        }
    }

    /// What the list shows: its title, or the start of its id.
    pub fn label(&self) -> String {
        if self.title.is_empty() { self.id.chars().take(8).collect() } else { self.title.clone() }
    }
}

/// The conversation `agent` has open in `folder`, judging by the session
/// file written closest to `last_output`. `None` for other agents, or when
/// there is no file.
pub fn current_session(
    home: &Path,
    agent: &str,
    folder: &Path,
    last_output: i64,
    terminal: &str,
) -> Option<AgentSession> {
    let files = match agent {
        "claude" => claude_files(home, folder),
        "codex" => codex_files(home, folder),
        _ => return None,
    };
    let file = files.into_iter().min_by_key(|(_, modified)| (modified - last_output).abs())?.0;
    let id = session_id(agent, &file)?;
    let title = if agent == "claude" { claude_title(&file) } else { String::new() };
    Some(AgentSession {
        terminal: terminal.to_string(),
        agent: agent.to_string(),
        id,
        folder: folder.to_path_buf(),
        last_used: last_output,
        title,
    })
}

/// Claude Code's folder for the conversations started in `folder`.
fn claude_project_folder(home: &Path, folder: &Path) -> PathBuf {
    let escaped: String =
        folder.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    home.join(".claude/projects").join(escaped)
}

/// Claude Code's session files for `folder`, with when each was written.
fn claude_files(home: &Path, folder: &Path) -> Vec<(PathBuf, i64)> {
    session_files_in(&claude_project_folder(home, folder))
}

/// Codex's session files of the last days that ran in `folder`.
fn codex_files(home: &Path, folder: &Path) -> Vec<(PathBuf, i64)> {
    let root = home.join(".codex/sessions");
    let mut day_folders = Vec::new();
    for year in sorted_folders(&root).into_iter().rev() {
        for month in sorted_folders(&year).into_iter().rev() {
            day_folders.extend(sorted_folders(&month).into_iter().rev());
        }
    }
    let wanted = format!("\"cwd\":\"{}\"", json_escape(&folder.to_string_lossy()));
    day_folders
        .into_iter()
        .take(CODEX_DAYS)
        .flat_map(|day| session_files_in(&day))
        .filter(|(file, _)| first_line(file).is_some_and(|line| line.contains(&wanted)))
        .collect()
}

/// The `.jsonl` files directly in `folder`, with when each was written.
fn session_files_in(folder: &Path) -> Vec<(PathBuf, i64)> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "jsonl"))
        .filter_map(|path| {
            let modified = fs::metadata(&path).ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
            Some((path, i64::try_from(modified.as_secs()).ok()?))
        })
        .collect()
}

/// The folders directly in `folder`, by name.
fn sorted_folders(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut folders: Vec<PathBuf> =
        entries.filter_map(Result::ok).map(|entry| entry.path()).filter(|path| path.is_dir()).collect();
    folders.sort();
    folders
}

/// The conversation id in a session file's name: the whole name for
/// Claude Code, the last 36 characters (a UUID) for Codex.
fn session_id(agent: &str, file: &Path) -> Option<String> {
    let stem = file.file_stem()?.to_string_lossy().to_string();
    if agent == "codex" {
        let start = stem.len().checked_sub(UUID_LENGTH)?;
        return stem.get(start..).map(str::to_string);
    }
    Some(stem)
}

/// Characters in a UUID with its dashes.
const UUID_LENGTH: usize = 36;

fn first_line(file: &Path) -> Option<String> {
    BufReader::new(fs::File::open(file).ok()?).lines().next()?.ok()
}

/// The first prompt typed in a Claude Code session: the first user message
/// whose content is plain text (not a tool result), shortened.
fn claude_title(file: &Path) -> String {
    let Ok(opened) = fs::File::open(file) else {
        return String::new();
    };
    BufReader::new(opened)
        .lines()
        .take(TITLE_LINES)
        .map_while(Result::ok)
        .filter(|line| line.contains("\"type\":\"user\""))
        .find_map(|line| prompt_text(&line))
        .map(|text| shorten(&text))
        .unwrap_or_default()
}

/// The text of `"content":"…"` in a user message line, when it is a plain
/// string (tool results are lists) and not a command or system note.
fn prompt_text(line: &str) -> Option<String> {
    const KEY: &str = "\"content\":\"";
    let start = line.find(KEY)? + KEY.len();
    let text = json_unescape_until_quote(&line[start..]);
    let text = text.trim();
    (!text.is_empty() && !text.starts_with('<')).then(|| text.to_string())
}

/// A JSON string body up to its closing quote, with the common escapes read.
fn json_unescape_until_quote(body: &str) -> String {
    let mut text = String::new();
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => match chars.next() {
                Some('n' | 't' | 'r') => text.push(' '),
                Some(other) => text.push(other),
                None => break,
            },
            other => text.push(other),
        }
    }
    text
}

/// `text` as it appears inside a JSON string (quotes and backslashes escaped).
fn json_escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// `text` on one line, cut to [`TITLE_CHARS`] with "…".
fn shorten(text: &str) -> String {
    let single: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if single.chars().count() <= TITLE_CHARS {
        return single;
    }
    let cut: String = single.chars().take(TITLE_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

/// Every remembered session, newest first per terminal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionHistory {
    sessions: Vec<AgentSession>,
}

impl SessionHistory {
    /// The sessions of `terminal`, most recently used first.
    pub fn of(&self, terminal: &str) -> Vec<&AgentSession> {
        let mut found: Vec<&AgentSession> =
            self.sessions.iter().filter(|session| session.terminal == terminal).collect();
        found.sort_by_key(|session| std::cmp::Reverse(session.last_used));
        found
    }

    /// Remember `session` (or refresh it when known). Returns true when
    /// anything changed.
    pub fn record(&mut self, session: AgentSession) -> bool {
        if let Some(known) =
            self.sessions.iter_mut().find(|known| known.terminal == session.terminal && known.id == session.id)
        {
            let changed = known.last_used != session.last_used || (known.title.is_empty() && !session.title.is_empty());
            known.last_used = known.last_used.max(session.last_used);
            if known.title.is_empty() {
                known.title = session.title;
            }
            return changed;
        }
        let terminal = session.terminal.clone();
        self.sessions.push(session);
        let mut of_terminal: Vec<usize> =
            (0..self.sessions.len()).filter(|&index| self.sessions[index].terminal == terminal).collect();
        if of_terminal.len() > MAX_PER_TERMINAL {
            of_terminal.sort_by_key(|&index| self.sessions[index].last_used);
            let oldest = of_terminal[0];
            self.sessions.remove(oldest);
        }
        true
    }

    /// Move the sessions of terminal `from` to `to` (after a rename).
    pub fn rename_terminal(&mut self, from: &str, to: &str) {
        for session in self.sessions.iter_mut().filter(|session| session.terminal == from) {
            session.terminal = to.to_string();
        }
    }

    /// Read the history file's lines; broken lines are skipped.
    pub fn parse(text: &str) -> Self {
        let sessions = text
            .lines()
            .filter_map(|line| {
                let fields: Vec<&str> = line.splitn(6, '\t').collect();
                let [terminal, agent, id, last_used, folder, title] = fields[..] else {
                    return None;
                };
                Some(AgentSession {
                    terminal: terminal.to_string(),
                    agent: agent.to_string(),
                    id: id.to_string(),
                    last_used: last_used.parse().ok()?,
                    folder: PathBuf::from(folder),
                    title: title.to_string(),
                })
            })
            .collect();
        SessionHistory { sessions }
    }

    /// The history file's text: one tab-separated line per session.
    pub fn to_text(&self) -> String {
        let mut text = String::new();
        for session in &self.sessions {
            let _ = writeln!(
                text,
                "{}\t{}\t{}\t{}\t{}\t{}",
                session.terminal,
                session.agent,
                session.id,
                session.last_used,
                session.folder.display(),
                session.title.replace(['\t', '\n'], " "),
            );
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempFolder;

    fn session(terminal: &str, id: &str, last_used: i64) -> AgentSession {
        AgentSession {
            terminal: terminal.to_string(),
            agent: "claude".to_string(),
            id: id.to_string(),
            folder: PathBuf::from("/work"),
            last_used,
            title: String::new(),
        }
    }

    #[test]
    fn finds_the_claude_session_written_closest_to_the_last_output() {
        let home = TempFolder::new();
        let folder = Path::new("/Users/me/my app");
        let project = claude_project_folder(home.path(), folder);
        assert!(project.ends_with("-Users-me-my-app"));
        let user_line = r#"{"type":"user","message":{"role":"user","content":"Fix the \"login\" bug\nplease"}}"#;
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("abc-123.jsonl"), format!("{user_line}\n")).unwrap();
        let now = std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let found = current_session(home.path(), "claude", folder, i64::try_from(now).unwrap(), "term-1").unwrap();
        assert_eq!(found.id, "abc-123");
        assert_eq!(found.title, "Fix the \"login\" bug please");
        assert_eq!(found.resume_command(), "claude --resume abc-123");
        assert_eq!(current_session(home.path(), "vim", folder, 0, "term-1"), None);
    }

    #[test]
    fn finds_codex_sessions_by_their_folder() {
        let home = TempFolder::new();
        let day = home.make_folder(".codex/sessions/2026/11/06");
        let id = "01a18973-658b-75a0-9c21-321669fa5910";
        let meta = r#"{"type":"session_meta","payload":{"cwd":"/work/api"}}"#;
        fs::write(day.join(format!("rollout-2026-11-06T10-00-00-{id}.jsonl")), format!("{meta}\n")).unwrap();
        fs::write(day.join("rollout-other.jsonl"), "{\"payload\":{\"cwd\":\"/elsewhere\"}}\n").unwrap();
        let found = current_session(home.path(), "codex", Path::new("/work/api"), 0, "qa").unwrap();
        assert_eq!(found.id, id);
        assert_eq!(found.resume_command(), format!("codex resume {id}"));
    }

    #[test]
    fn titles_skip_tool_results_and_commands() {
        assert_eq!(prompt_text(r#"{"type":"user","message":{"content":[{"type":"tool_result"}]}}"#), None);
        assert_eq!(prompt_text(r#"{"type":"user","message":{"content":"<command-name>/clear"}}"#), None);
        assert_eq!(shorten(&"word ".repeat(30)).chars().count(), TITLE_CHARS);
    }

    #[test]
    fn history_records_refreshes_and_round_trips() {
        let mut history = SessionHistory::default();
        assert!(history.record(session("t1", "a", 10)));
        assert!(history.record(session("t1", "b", 20)));
        assert!(!history.record(session("t1", "a", 10)));
        assert!(history.record(session("t1", "a", 30)));
        let ids: Vec<&str> = history.of("t1").iter().map(|found| found.id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
        history.rename_terminal("t1", "t2");
        assert_eq!(history.of("t2").len(), 2);
        assert_eq!(SessionHistory::parse(&history.to_text()), history);
    }

    #[test]
    fn keeps_a_limited_number_per_terminal() {
        let mut history = SessionHistory::default();
        for number in 0..=MAX_PER_TERMINAL {
            history.record(session("t1", &number.to_string(), i64::try_from(number).unwrap()));
        }
        assert_eq!(history.of("t1").len(), MAX_PER_TERMINAL);
        assert!(history.of("t1").iter().all(|found| found.id != "0"));
    }
}
