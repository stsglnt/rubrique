//! What a terminal is doing, for the explorer: which agent runs in it, and
//! whether that agent is working, waiting for you, or idle.
//!
//! It is read from outside, the way you would by looking: the program tmux
//! says is running, how long ago the screen last changed, and the last lines
//! on screen (agents print "esc to interrupt" while they work and a numbered
//! choice when they ask for permission).

/// Programs counted as coding agents, by the name tmux reports.
const AGENTS: [&str; 7] = ["claude", "codex", "grok", "gemini", "aider", "opencode", "ollama"];
/// Programs counted as a plain shell prompt.
const SHELLS: [&str; 6] = ["zsh", "bash", "fish", "sh", "dash", "nu"];
/// Output this recent means the agent is still working.
const WORKING_SECONDS: i64 = 4;
/// How many of the last screen lines are read for prompts.
const TAIL_LINES: usize = 15;

/// Lines on screen that mean an agent waits for an answer from you.
const QUESTION_MARKS: [&str; 7] =
    ["do you want to", "❯ 1.", "› 1.", "(y/n)", "[y/n]", "allow this", "press enter to continue"];
/// Lines on screen that mean an agent is busy.
const WORKING_MARKS: [&str; 2] = ["esc to interrupt", "ctrl+c to interrupt"];

/// The state shown next to a terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalStatus {
    /// A shell prompt, nothing running.
    Shell,
    /// Some other program (a dev server, vim).
    Program(String),
    /// An agent is busy.
    Working(String),
    /// An agent asks you something.
    NeedsYou(String),
    /// An agent waits at its prompt.
    Idle(String),
}

impl TerminalStatus {
    /// A short label: "claude · working".
    pub fn label(&self) -> String {
        match self {
            TerminalStatus::Shell => "shell".to_string(),
            TerminalStatus::Program(program) => program.clone(),
            TerminalStatus::Working(agent) => format!("{agent} · working"),
            TerminalStatus::NeedsYou(agent) => format!("{agent} · needs you"),
            TerminalStatus::Idle(agent) => format!("{agent} · idle"),
        }
    }

    /// The agent running, whatever it is doing; `None` for shells and other programs.
    pub fn agent(&self) -> Option<&str> {
        match self {
            TerminalStatus::Working(agent) | TerminalStatus::NeedsYou(agent) | TerminalStatus::Idle(agent) => {
                Some(agent)
            }
            TerminalStatus::Shell | TerminalStatus::Program(_) => None,
        }
    }
}

/// True when `command` is an agent, so its screen is worth reading.
pub fn is_agent(command: &str) -> bool {
    agent_name(command).is_some()
}

/// The agent's name when `command` is a known agent.
fn agent_name(command: &str) -> Option<&'static str> {
    // Claude Code's installer links `claude` to a file named after its
    // version, so tmux reports the program as `2.1.318`.
    if is_version_number(command) {
        return Some("claude");
    }
    let command = command.to_lowercase();
    AGENTS.iter().copied().find(|agent| command == *agent || command.starts_with(&format!("{agent}-")))
}

/// `2.1.318`: digits with at least one dot between them.
fn is_version_number(command: &str) -> bool {
    command.contains('.') && command.split('.').all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

/// What the terminal is doing. `seconds_since_output` is how long ago its
/// screen last changed; `screen` is its plain text (only read for agents).
pub fn terminal_status(command: &str, seconds_since_output: i64, screen: &str) -> TerminalStatus {
    let Some(agent) = agent_name(command) else {
        let program = command.trim_start_matches('-');
        return if program.is_empty() || SHELLS.contains(&program) {
            TerminalStatus::Shell
        } else {
            TerminalStatus::Program(program.to_string())
        };
    };
    let agent = agent.to_string();
    let tail = last_lines(screen).to_lowercase();
    if QUESTION_MARKS.iter().any(|mark| tail.contains(mark)) {
        TerminalStatus::NeedsYou(agent)
    } else if WORKING_MARKS.iter().any(|mark| tail.contains(mark)) || seconds_since_output < WORKING_SECONDS {
        TerminalStatus::Working(agent)
    } else {
        TerminalStatus::Idle(agent)
    }
}

/// The last non-empty lines of `screen`, where agents show their state.
fn last_lines(screen: &str) -> String {
    let lines: Vec<&str> = screen.lines().filter(|line| !line.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shells_and_other_programs() {
        assert_eq!(terminal_status("zsh", 100, ""), TerminalStatus::Shell);
        assert_eq!(terminal_status("-bash", 0, ""), TerminalStatus::Shell);
        assert_eq!(terminal_status("vim", 0, ""), TerminalStatus::Program("vim".to_string()));
    }

    #[test]
    fn an_agent_asking_needs_you() {
        let screen = "Edit file?\n Do you want to make this edit to main.rs?\n ❯ 1. Yes\n   2. No";
        assert_eq!(terminal_status("claude", 0, screen), TerminalStatus::NeedsYou("claude".to_string()));
    }

    #[test]
    fn working_from_the_screen_or_recent_output() {
        let busy = "✻ Thinking… (12s · esc to interrupt)";
        assert_eq!(terminal_status("claude", 60, busy), TerminalStatus::Working("claude".to_string()));
        assert_eq!(terminal_status("codex", 1, "> "), TerminalStatus::Working("codex".to_string()));
    }

    #[test]
    fn claude_shows_up_as_its_version() {
        assert!(is_agent("2.1.318"));
        assert!(!is_agent("1."));
        assert!(!is_agent("node"));
    }

    #[test]
    fn a_quiet_agent_is_idle() {
        assert_eq!(terminal_status("claude", 60, "> \n? for shortcuts"), TerminalStatus::Idle("claude".to_string()));
        assert_eq!(TerminalStatus::Idle("claude".to_string()).label(), "claude · idle");
    }

    #[test]
    fn only_the_last_lines_count() {
        let old_question = format!("Do you want to proceed?\n{}> ", "output\n".repeat(30));
        assert_eq!(terminal_status("claude", 60, &old_question), TerminalStatus::Idle("claude".to_string()));
    }
}
