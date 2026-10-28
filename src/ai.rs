//! AI tools rubrique can run: command-line programs such as `claude` or
//! `codex`, started with a command template from Settings. rubrique embeds no
//! AI and holds no API keys; each tool handles its own login.
//!
//! This module is pure apart from [`is_installed`], which looks at `PATH`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub mod run;
pub mod view;

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
    /// Folders where the user allowed AI runs ("Don't ask again").
    pub allowed_folders: Vec<PathBuf>,
}

impl Default for AiSettings {
    fn default() -> Self {
        AiSettings {
            default_tool: AiTool::Claude,
            summary_tool: AiTool::Local,
            commands: AiTool::ALL.map(|tool| tool.default_command().to_string()),
            allowed_folders: Vec::new(),
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
            "ai_allowed_folder" => {
                if !value.is_empty() {
                    self.allowed_folders.push(PathBuf::from(value));
                }
            }
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
        for folder in &self.allowed_folders {
            let _ = writeln!(lines, "ai_allowed_folder={}", folder.display());
        }
        lines
    }
}

fn index(tool: AiTool) -> usize {
    AiTool::ALL.iter().position(|each| *each == tool).unwrap_or_default()
}

impl AiSettings {
    /// AI runs in `folder` were allowed before.
    pub fn is_allowed(&self, folder: &Path) -> bool {
        self.allowed_folders.iter().any(|allowed| allowed == folder)
    }

    pub fn allow(&mut self, folder: &Path) {
        if !self.is_allowed(folder) {
            self.allowed_folders.push(folder.to_path_buf());
        }
    }
}

/// What the answer is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AskKind {
    /// New text for the selection, reviewed in the merge screen.
    Rewrite,
    /// An answer to read (Explain); nothing in the file changes.
    Answer,
}

/// The small buttons under the Ask field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuickAction {
    Shorter,
    FixGrammar,
    Table,
    Translate,
    Explain,
}

impl QuickAction {
    pub const ALL: [QuickAction; 5] = [
        QuickAction::Shorter,
        QuickAction::FixGrammar,
        QuickAction::Table,
        QuickAction::Translate,
        QuickAction::Explain,
    ];

    pub fn label(self) -> &'static str {
        match self {
            QuickAction::Shorter => "Shorter",
            QuickAction::FixGrammar => "Fix grammar",
            QuickAction::Table => "Table",
            QuickAction::Translate => "Translate…",
            QuickAction::Explain => "Explain",
        }
    }

    /// The request this button sends. Translate has none: it fills the
    /// field with "Translate to " for the user to finish.
    pub fn request(self) -> Option<&'static str> {
        match self {
            QuickAction::Shorter => Some("Make it shorter. Keep the meaning and the tone."),
            QuickAction::FixGrammar => Some("Fix spelling and grammar. Change nothing else."),
            QuickAction::Table => Some("Turn it into a Markdown table."),
            QuickAction::Translate => None,
            QuickAction::Explain => Some("Explain this in simple words."),
        }
    }

    pub fn kind(self) -> AskKind {
        if self == QuickAction::Explain { AskKind::Answer } else { AskKind::Rewrite }
    }
}

/// The full prompt sent to the tool: the request, how to answer, and the text.
pub fn build_prompt(request: &str, text: &str, kind: AskKind, whole_file: bool) -> String {
    let what = if whole_file { "a Markdown file" } else { "part of a Markdown file" };
    let how = match kind {
        AskKind::Rewrite => {
            "Apply the request to the text below and reply with only the new text: \
             no explanations, no quotes, no code fences around it. \
             Keep the Markdown formatting unless the request is about it."
        }
        AskKind::Answer => "Answer the request about the text below. Reply in Markdown, short and clear.",
    };
    format!("Request: {}\n\n{how}\n\nThe text is {what}:\n<text>\n{text}\n</text>\n", request.trim())
}

/// The tool's reply as new text: surrounding blank lines and a wrapping
/// code fence removed, and the original's final line break kept.
pub fn clean_reply(reply: &str, original: &str) -> String {
    let mut text = reply.trim();
    if let Some(inner) = text.strip_prefix("```") {
        // Drop the fence line (with any language name) and the closing fence.
        if let Some((_, body)) = inner.split_once('\n')
            && let Some(body) = body.trim_end().strip_suffix("```")
        {
            text = body.trim();
        }
    }
    let mut cleaned = text.to_string();
    if original.ends_with('\n') {
        cleaned.push('\n');
    }
    cleaned
}

/// The program and its arguments for `command` with `prompt` filled in.
/// `{prompt}` inside a word is replaced too (`--prompt={prompt}`).
pub fn command_args(command: &str, prompt: &str) -> Option<(String, Vec<String>)> {
    let mut words = split_command(command).into_iter();
    let program = words.next().filter(|program| !program.is_empty())?;
    let args = words.map(|word| word.replace(PROMPT, prompt)).collect();
    Some((program, args))
}

/// The command as shown to the user, with a long prompt cut short.
pub fn display_command(command: &str, request: &str) -> String {
    const SHOWN: usize = 48;
    let short: String = request.chars().take(SHOWN).collect();
    let quoted = if request.chars().count() > SHOWN { format!("\"{short}…\"") } else { format!("\"{short}\"") };
    split_command(command)
        .iter()
        .map(|word| if word.contains(PROMPT) { word.replace(PROMPT, &quoted) } else { word.clone() })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Seconds as `m:ss`.
pub fn format_elapsed(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
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
    fn builds_prompts() {
        let prompt = build_prompt(" shorter ", "Hello there.", AskKind::Rewrite, false);
        assert!(prompt.starts_with("Request: shorter\n"));
        assert!(prompt.contains("only the new text"));
        assert!(prompt.contains("part of a Markdown file"));
        assert!(prompt.ends_with("<text>\nHello there.\n</text>\n"));
        let answer = build_prompt("why?", "x", AskKind::Answer, true);
        assert!(answer.contains("Answer the request"));
        assert!(answer.contains("The text is a Markdown file"));
    }

    #[test]
    fn cleans_replies() {
        assert_eq!(clean_reply("\n  New text.\n\n", "Old text."), "New text.");
        assert_eq!(clean_reply("New text.", "Old text.\n"), "New text.\n");
        assert_eq!(clean_reply("```markdown\n| a | b |\n```\n", "a b"), "| a | b |");
        assert_eq!(clean_reply("```rust\nfn main() {}\nno close", "x"), "```rust\nfn main() {}\nno close");
    }

    #[test]
    fn fills_in_the_prompt() {
        let (program, args) = command_args("claude -p {prompt}", "say \"hi\"; rm -rf /").unwrap_or_default();
        assert_eq!(program, "claude");
        assert_eq!(args, ["-p", "say \"hi\"; rm -rf /"]);
        let (_, args) = command_args("tool --prompt={prompt}", "x").unwrap_or_default();
        assert_eq!(args, ["--prompt=x"]);
        assert_eq!(command_args("  ", "x"), None);
    }

    #[test]
    fn shows_commands_short() {
        assert_eq!(display_command("claude -p {prompt}", "shorter"), "claude -p \"shorter\"");
        let long = "a".repeat(60);
        assert_eq!(display_command("codex exec {prompt}", &long), format!("codex exec \"{}…\"", "a".repeat(48)));
        assert_eq!(format_elapsed(7), "0:07");
        assert_eq!(format_elapsed(125), "2:05");
    }

    #[test]
    fn remembers_allowed_folders() {
        let mut settings = AiSettings::default();
        settings.allow(Path::new("/home/a/notes"));
        settings.allow(Path::new("/home/a/notes"));
        assert!(settings.is_allowed(Path::new("/home/a/notes")));
        assert!(!settings.is_allowed(Path::new("/home/a")));
        let mut read = AiSettings::default();
        for line in settings.to_lines().lines() {
            let (key, value) = line.split_once('=').unwrap_or_default();
            read.parse_line(key, value);
        }
        assert_eq!(read, settings);
    }

    #[test]
    fn finds_programs_by_path() {
        assert!(is_installed("/bin/sh -c {prompt}"));
        assert!(is_installed("sh -c {prompt}"));
        assert!(!is_installed("surely-not-a-real-program-xyz {prompt}"));
        assert!(!is_installed(""));
    }
}
