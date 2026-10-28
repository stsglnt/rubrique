//! AI tools rubrique can run: command-line programs such as `claude` or
//! `codex`, started with a command template from Settings. rubrique embeds no
//! AI and holds no API keys; each tool handles its own login.
//!
//! This module is pure apart from [`is_installed`], which looks at `PATH`.

use std::fmt::Write as _;
use std::path::Path;

/// Placeholder in a command template for the request text.
pub const PROMPT: &str = "{prompt}";

/// The tools rubrique knows about, in the order Settings lists them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AiTool {
    #[default]
    Claude,
    Codex,
    Grok,
    /// A small model on this machine (Ollama or llama.cpp).
    Local,
}

impl AiTool {
    pub const ALL: [AiTool; 4] = [AiTool::Claude, AiTool::Codex, AiTool::Grok, AiTool::Local];

    /// Name shown in Settings.
    pub fn name(self) -> &'static str {
        match self {
            AiTool::Claude => "Claude",
            AiTool::Codex => "Codex",
            AiTool::Grok => "Grok",
            AiTool::Local => "Local model",
        }
    }

    /// Key used in the settings file.
    pub fn id(self) -> &'static str {
        match self {
            AiTool::Claude => "claude",
            AiTool::Codex => "codex",
            AiTool::Grok => "grok",
            AiTool::Local => "local",
        }
    }

    pub fn from_id(id: &str) -> Option<AiTool> {
        AiTool::ALL.into_iter().find(|tool| tool.id() == id)
    }

    /// Command template used until the user changes it.
    pub fn default_command(self) -> &'static str {
        match self {
            AiTool::Claude => "claude -p {prompt}",
            AiTool::Codex => "codex exec {prompt}",
            AiTool::Grok => "grok --prompt {prompt}",
            AiTool::Local => "ollama run qwen2.5:1.5b {prompt}",
        }
    }

    /// One line on what the tool is good for, shown under its name.
    pub fn hint(self) -> &'static str {
        match self {
            AiTool::Claude => "Claude Code. Rewriting, notes, careful edits.",
            AiTool::Codex => "OpenAI Codex. Code blocks, SQL and JSON in notes.",
            AiTool::Grok => "Grok has several command-line tools; set yours here.",
            AiTool::Local => "Ollama or llama.cpp. Summaries and titles, offline.",
        }
    }
}

/// AI settings: which tool to use and each tool's command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiSettings {
    /// Tool for requests (Ask, Resolve notes, folder questions).
    pub default_tool: AiTool,
    /// Tool for summaries and file titles.
    pub summary_tool: AiTool,
    /// Command templates, one per tool in [`AiTool::ALL`] order.
    commands: [String; 4],
}

impl Default for AiSettings {
    fn default() -> Self {
        AiSettings {
            default_tool: AiTool::Claude,
            summary_tool: AiTool::Local,
            commands: AiTool::ALL.map(|tool| tool.default_command().to_string()),
        }
    }
}

impl AiSettings {
    pub fn command(&self, tool: AiTool) -> &str {
        &self.commands[index(tool)]
    }

    pub fn command_mut(&mut self, tool: AiTool) -> &mut String {
        &mut self.commands[index(tool)]
    }

    pub fn set_command(&mut self, tool: AiTool, command: &str) {
        command.trim().clone_into(&mut self.commands[index(tool)]);
    }

    /// The command differs from the built-in one.
    pub fn is_custom(&self, tool: AiTool) -> bool {
        self.command(tool).trim() != tool.default_command()
    }

    pub fn reset_command(&mut self, tool: AiTool) {
        self.set_command(tool, tool.default_command());
    }

    /// Read one settings line; returns false when the key is not an AI key.
    pub fn parse_line(&mut self, key: &str, value: &str) -> bool {
        match key {
            "ai_tool" => self.default_tool = AiTool::from_id(value).unwrap_or(self.default_tool),
            "ai_summary_tool" => self.summary_tool = AiTool::from_id(value).unwrap_or(self.summary_tool),
            _ => {
                let Some(tool) = key.strip_prefix("ai_command_").and_then(AiTool::from_id) else {
                    return false;
                };
                if !value.trim().is_empty() {
                    self.set_command(tool, value);
                }
            }
        }
        true
    }

    /// Settings lines. Commands are written only when changed, so a better
    /// built-in command in a later version reaches everyone else.
    pub fn to_lines(&self) -> String {
        let mut lines = format!("ai_tool={}\nai_summary_tool={}\n", self.default_tool.id(), self.summary_tool.id());
        for tool in AiTool::ALL.into_iter().filter(|tool| self.is_custom(*tool)) {
            let _ = writeln!(lines, "ai_command_{}={}", tool.id(), self.command(tool).trim());
        }
        lines
    }
}

fn index(tool: AiTool) -> usize {
    AiTool::ALL.iter().position(|each| *each == tool).unwrap_or_default()
}

/// What is wrong with a command template, if anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandProblem {
    Empty,
    /// No `{prompt}` word, so the request would not reach the tool.
    NoPrompt,
}

impl CommandProblem {
    pub fn message(self) -> &'static str {
        match self {
            CommandProblem::Empty => "Enter a command.",
            CommandProblem::NoPrompt => "Add {prompt} where the request goes.",
        }
    }
}

/// Check a command template.
pub fn check_command(command: &str) -> Option<CommandProblem> {
    let words = split_command(command);
    if words.is_empty() {
        Some(CommandProblem::Empty)
    } else if !words.iter().any(|word| word == PROMPT) {
        Some(CommandProblem::NoPrompt)
    } else {
        None
    }
}

/// Split a template into words. Double or single quotes keep spaces inside
/// one word. The program later gets these words as separate arguments, never
/// through a shell, so text in a note cannot run commands.
pub fn split_command(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote = None;
    for character in command.chars() {
        match (quote, character) {
            (Some(open), _) if character == open => quote = None,
            (Some(_), _) => word.push(character),
            (None, '"' | '\'') => {
                quote = Some(character);
                in_word = true;
            }
            (None, _) if character.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            (None, _) => {
                word.push(character);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(word);
    }
    words
}

/// The program a template starts (its first word).
pub fn program(command: &str) -> Option<String> {
    split_command(command).into_iter().next()
}

/// Whether the template's program can be found: an existing file for a path,
/// otherwise a file of that name in a `PATH` folder.
pub fn is_installed(command: &str) -> bool {
    let Some(program) = program(command) else {
        return false;
    };
    if program.contains('/') {
        return Path::new(&program).is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|folder| folder.join(&program).is_file()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_have_a_prompt() {
        for tool in AiTool::ALL {
            assert_eq!(check_command(tool.default_command()), None, "{}", tool.name());
            assert_eq!(AiTool::from_id(tool.id()), Some(tool));
        }
    }

    #[test]
    fn splits_words_and_quotes() {
        assert_eq!(split_command("claude -p {prompt}"), ["claude", "-p", "{prompt}"]);
        assert_eq!(
            split_command(r#"llama-cli -m "/my models/q.gguf" -p {prompt}"#),
            ["llama-cli", "-m", "/my models/q.gguf", "-p", "{prompt}"]
        );
        assert_eq!(split_command("a '' b"), ["a", "", "b"]);
        assert_eq!(split_command("   "), Vec::<String>::new());
    }

    #[test]
    fn finds_problems() {
        assert_eq!(check_command(" "), Some(CommandProblem::Empty));
        assert_eq!(check_command("claude -p"), Some(CommandProblem::NoPrompt));
        assert_eq!(check_command("claude -p '{prompt}'"), None);
    }

    #[test]
    fn writes_only_changed_commands() {
        let mut settings = AiSettings::default();
        assert_eq!(settings.to_lines(), "ai_tool=claude\nai_summary_tool=local\n");
        settings.set_command(AiTool::Local, "  ollama run gemma3:1b {prompt} ");
        settings.default_tool = AiTool::Codex;
        let text = settings.to_lines();
        assert_eq!(text, "ai_tool=codex\nai_summary_tool=local\nai_command_local=ollama run gemma3:1b {prompt}\n");

        let mut read = AiSettings::default();
        for line in text.lines() {
            let (key, value) = line.split_once('=').unwrap_or_default();
            assert!(read.parse_line(key, value));
        }
        assert_eq!(read, settings);
        read.reset_command(AiTool::Local);
        assert!(!read.is_custom(AiTool::Local));
    }

    #[test]
    fn ignores_unknown_values() {
        let mut settings = AiSettings::default();
        assert!(settings.parse_line("ai_tool", "gpt"));
        assert!(settings.parse_line("ai_command_local", ""));
        assert!(!settings.parse_line("ai_command_other", "x {prompt}"));
        assert!(!settings.parse_line("theme", "dark"));
        assert_eq!(settings, AiSettings::default());
    }

    #[test]
    fn finds_programs_by_path() {
        assert!(is_installed("/bin/sh -c {prompt}"));
        assert!(is_installed("sh -c {prompt}"));
        assert!(!is_installed("surely-not-a-real-program-xyz {prompt}"));
        assert!(!is_installed(""));
    }
}
