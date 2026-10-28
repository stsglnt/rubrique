//! Short summaries of files, shown when hovering a file in the explorer.
//! Pure apart from the small disk cache at the end.

use std::fmt::Write as _;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::{AiTool, Prompt, split_command};

/// Only the start of a long file is summarized.
pub const MAX_SUMMARY_INPUT_CHARS: usize = 12_000;
/// Longest summary kept, in characters.
const MAX_SUMMARY_CHARS: usize = 280;

/// The prompt asking for a two-sentence summary of `text` from file `name`.
pub fn summary_prompt(name: &str, text: &str) -> Prompt {
    let (shown, cut) = match text.char_indices().nth(MAX_SUMMARY_INPUT_CHARS) {
        Some((end, _)) => (&text[..end], true),
        None => (text, false),
    };
    let note = if cut { " It is long; this is its start." } else { "" };
    Prompt {
        system: "You write very short summaries of notes for a file list. \
                 Reply with at most two short sentences, under 30 words, in the language of the note. \
                 Plain text: no Markdown, no title, no lead-in like \"This note\". \
                 Do not use tools or change any files."
            .to_string(),
        user: format!("Summarize the file {name}.{note}\n<text>\n{shown}\n</text>\n"),
    }
}

/// The reply as a summary: one paragraph, no label, not too long.
pub fn clean_summary(reply: &str) -> String {
    let flat = reply.split_whitespace().collect::<Vec<_>>().join(" ");
    let flat = flat.trim_matches('"');
    let flat = ["Summary:", "summary:", "SUMMARY:"]
        .iter()
        .fold(flat, |text, label| text.strip_prefix(label).map_or(text, str::trim_start));
    if flat.chars().count() > MAX_SUMMARY_CHARS {
        let mut short: String = flat.chars().take(MAX_SUMMARY_CHARS).collect();
        short.push('…');
        short
    } else {
        flat.to_string()
    }
}

/// Cache key for a summary of `text` made with `command`: a different
/// command (another model) or a changed file gives a new key.
pub fn cache_key(command: &str, text: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    command.trim().hash(&mut hasher);
    text.hash(&mut hasher);
    let mut key = String::new();
    let _ = write!(key, "{:016x}", hasher.finish());
    key
}

/// Short name of the model or tool for the card's footer:
/// "Local · qwen2.5:1.5b" for Ollama, the model file for llama.cpp.
pub fn model_label(tool: AiTool, command: &str) -> String {
    let words = split_command(command);
    let after = |flag: &str| words.iter().position(|word| word == flag).and_then(|at| words.get(at + 1));
    let model = if tool == AiTool::Local {
        after("run").cloned().or_else(|| after("-m").or_else(|| after("--model")).map(|path| file_stem(path)))
    } else {
        after("--model").or_else(|| after("-m")).cloned()
    };
    let short = if tool == AiTool::Local { "Local" } else { tool.name() };
    match model {
        Some(model) => format!("{short} · {model}"),
        None => short.to_string(),
    }
}

fn file_stem(path: &str) -> String {
    Path::new(path).file_stem().map_or_else(|| path.to_string(), |stem| stem.to_string_lossy().into_owned())
}

/// "edited 2 days ago" for a file changed at `modified`.
pub fn edited_ago(modified: SystemTime, now: SystemTime) -> String {
    let seconds = now.duration_since(modified).unwrap_or(Duration::ZERO).as_secs();
    let (count, unit) = match seconds {
        0..60 => return "edited just now".to_string(),
        60..3_600 => (seconds / 60, "minute"),
        3_600..86_400 => (seconds / 3_600, "hour"),
        86_400..2_592_000 => (seconds / 86_400, "day"),
        2_592_000..31_536_000 => (seconds / 2_592_000, "month"),
        _ => (seconds / 31_536_000, "year"),
    };
    let plural = if count == 1 { "" } else { "s" };
    format!("edited {count} {unit}{plural} ago")
}

/// A cached summary, if there is one.
pub fn read_cached(folder: &Path, key: &str) -> Option<String> {
    fs::read_to_string(cache_path(folder, key)).ok().filter(|text| !text.trim().is_empty())
}

/// Keep a summary for next time. Failing to write only costs a new run later.
pub fn write_cached(folder: &Path, key: &str, summary: &str) {
    if fs::create_dir_all(folder).is_ok() {
        let _ = fs::write(cache_path(folder, key), summary);
    }
}

fn cache_path(folder: &Path, key: &str) -> PathBuf {
    folder.join(format!("{key}.txt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_cut_long_files() {
        let short = summary_prompt("ideas.md", "Tabs next.");
        assert!(short.user.starts_with("Summarize the file ideas.md.\n"));
        assert!(short.system.contains("at most two short sentences"));
        let long = summary_prompt("big.md", &"a".repeat(MAX_SUMMARY_INPUT_CHARS + 50));
        assert!(long.user.contains("this is its start"));
        assert!(long.user.len() < MAX_SUMMARY_INPUT_CHARS + 200);
    }

    #[test]
    fn cleans_summaries() {
        assert_eq!(clean_summary("  Summary: Tabs are\n the most asked for.  \n"), "Tabs are the most asked for.");
        assert_eq!(clean_summary("\"Plans for beta.\""), "Plans for beta.");
        let long = clean_summary(&"word ".repeat(100));
        assert!(long.ends_with('…'));
        assert_eq!(long.chars().count(), MAX_SUMMARY_CHARS + 1);
    }

    #[test]
    fn keys_change_with_text_and_command() {
        let key = cache_key("ollama run a {prompt}", "text");
        assert_eq!(key.len(), 16);
        assert_eq!(key, cache_key(" ollama run a {prompt} ", "text"));
        assert_ne!(key, cache_key("ollama run b {prompt}", "text"));
        assert_ne!(key, cache_key("ollama run a {prompt}", "text!"));
    }

    #[test]
    fn labels_name_the_model() {
        assert_eq!(model_label(AiTool::Local, "ollama run qwen2.5:1.5b {prompt}"), "Local · qwen2.5:1.5b");
        assert_eq!(
            model_label(AiTool::Local, "llama-cli -m /models/gemma-3-1b.gguf -p {prompt}"),
            "Local · gemma-3-1b"
        );
        assert_eq!(model_label(AiTool::Claude, "claude -p {prompt}"), "Claude");
        assert_eq!(model_label(AiTool::Claude, "claude -p --model haiku {prompt}"), "Claude · haiku");
    }

    #[test]
    fn says_when_a_file_was_edited() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        let ago = |seconds| edited_ago(now - Duration::from_secs(seconds), now);
        assert_eq!(ago(5), "edited just now");
        assert_eq!(ago(60), "edited 1 minute ago");
        assert_eq!(ago(7_200), "edited 2 hours ago");
        assert_eq!(ago(2 * 86_400), "edited 2 days ago");
        assert_eq!(ago(400 * 86_400), "edited 1 year ago");
        assert_eq!(edited_ago(now + Duration::from_secs(9), now), "edited just now");
    }

    #[test]
    fn caches_on_disk() {
        let folder = std::env::temp_dir().join(format!("rubrique-summaries-{}", std::process::id()));
        assert_eq!(read_cached(&folder, "abc"), None);
        write_cached(&folder, "abc", "Short.");
        assert_eq!(read_cached(&folder, "abc").as_deref(), Some("Short."));
        fs::remove_dir_all(&folder).unwrap_or_default();
    }
}
