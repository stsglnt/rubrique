//! Skills: reusable instructions for AI requests, in the same format as
//! Claude Code skills, so one skill works in both. Each skill is a folder
//! with a `SKILL.md` file:
//!
//! ```text
//! ---
//! name: action-items
//! description: Turn meeting notes into a list of action items.
//! ---
//! Make a Markdown task list ...
//! ```
//!
//! rubrique adds the instructions to what it sends to any tool. An optional
//! `output: answer` line makes the result an answer to read instead of new
//! text for the selection.

use std::fs;
use std::path::{Path, PathBuf};

use super::AskKind;

/// The file that makes a folder a skill.
pub const SKILL_FILE: &str = "SKILL.md";

/// One skill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    /// The body of SKILL.md: what the tool should do.
    pub instructions: String,
    /// New text for the selection, or an answer to read.
    pub kind: AskKind,
    /// The SKILL.md file.
    pub path: PathBuf,
}

/// Read a SKILL.md. The name falls back to the folder name.
pub fn parse_skill(text: &str, folder_name: &str, path: PathBuf) -> Option<Skill> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let (front, body) = split_front_matter(text);
    let mut name = None;
    let mut description = None;
    let mut kind = AskKind::Rewrite;
    for line in front.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = unquote(value.trim());
        match key.trim() {
            "name" if !value.is_empty() => name = Some(value),
            "description" if !value.is_empty() => description = Some(value),
            "output" if value == "answer" => kind = AskKind::Answer,
            _ => {}
        }
    }
    let instructions = body.trim().to_string();
    if instructions.is_empty() {
        return None;
    }
    let name = name.unwrap_or_else(|| folder_name.to_string());
    let description = description.unwrap_or_else(|| format!("Use the {name} skill."));
    Some(Skill { name, description, instructions, kind, path })
}

/// Front matter between `---` lines at the top, and the rest.
fn split_front_matter(text: &str) -> (&str, &str) {
    let Some(rest) = text.strip_prefix("---\n").or_else(|| text.strip_prefix("---\r\n")) else {
        return ("", text);
    };
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            return (&rest[..offset], &rest[offset + line.len()..]);
        }
        offset += line.len();
    }
    ("", text)
}

fn unquote(value: &str) -> String {
    let quoted = value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"')) || (value.starts_with('\'') && value.ends_with('\'')));
    if quoted { value[1..value.len() - 1].to_string() } else { value.to_string() }
}

/// All skills in `folders` (each skill is a sub-folder with SKILL.md), by
/// name. When two folders have a skill with the same name, the first wins.
pub fn load_skills(folders: &[PathBuf]) -> Vec<Skill> {
    let mut skills: Vec<Skill> = Vec::new();
    for folder in folders {
        let Ok(entries) = fs::read_dir(folder) else {
            continue;
        };
        let mut found: Vec<Skill> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path().join(SKILL_FILE);
                let text = fs::read_to_string(&path).ok()?;
                parse_skill(&text, &entry.file_name().to_string_lossy(), path)
            })
            .collect();
        found.retain(|skill| !skills.iter().any(|known| known.name == skill.name));
        skills.extend(found);
    }
    skills.sort_by_key(|skill| skill.name.to_lowercase());
    skills
}

/// Claude Code's own skills folder, `~/.claude/skills`.
pub fn claude_skills_folder() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| Path::new(&home).join(".claude/skills"))
}

/// Name of the example skill.
pub const EXAMPLE_NAME: &str = "action-items";

/// Write an example skill into `folder`. Returns its SKILL.md path.
///
/// # Errors
///
/// Fails when the folder cannot be created or the file written. An
/// existing example is left as it is.
pub fn create_example(folder: &Path) -> std::io::Result<PathBuf> {
    let skill_folder = folder.join(EXAMPLE_NAME);
    fs::create_dir_all(&skill_folder)?;
    let path = skill_folder.join(SKILL_FILE);
    if !path.exists() {
        fs::write(&path, EXAMPLE_SKILL)?;
    }
    Ok(path)
}

const EXAMPLE_SKILL: &str = "---
name: action-items
description: Turn meeting notes into a list of action items with owners and dates.
---
Turn the text into a Markdown task list of action items.

- One item per line: `- [ ] Owner: what to do (by when)`.
- Use the names and dates from the text; write `?` when they are missing.
- Keep the language of the text.
- Put open questions last, under a `**Questions**` line.
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_front_matter() {
        let text = "---\nname: tidy\ndescription: \"Clean up: spacing and lists\"\noutput: answer\n---\n\nDo it.\n";
        let skill = parse_skill(text, "folder", PathBuf::from("/s/tidy/SKILL.md")).unwrap_or_else(|| panic!("parsed"));
        assert_eq!(skill.name, "tidy");
        assert_eq!(skill.description, "Clean up: spacing and lists");
        assert_eq!(skill.instructions, "Do it.");
        assert_eq!(skill.kind, AskKind::Answer);
    }

    #[test]
    fn falls_back_to_the_folder_name() {
        let skill = parse_skill("Just instructions.", "short", PathBuf::new()).unwrap_or_else(|| panic!("parsed"));
        assert_eq!(skill.name, "short");
        assert_eq!(skill.description, "Use the short skill.");
        assert_eq!(skill.kind, AskKind::Rewrite);
        assert_eq!(parse_skill("---\nname: empty\n---\n  \n", "x", PathBuf::new()), None);
        // An unclosed front matter block is just text.
        let open = parse_skill("---\nname: x\nbody", "f", PathBuf::new()).unwrap_or_else(|| panic!("parsed"));
        assert_eq!(open.name, "f");
    }

    #[test]
    fn loads_folders_in_order() {
        let root = std::env::temp_dir().join(format!("rubrique-skills-{}", std::process::id()));
        let first = root.join("first");
        let second = root.join("second");
        let write = |folder: &Path, name: &str, body: &str| {
            fs::create_dir_all(folder.join(name)).unwrap_or_default();
            fs::write(folder.join(name).join(SKILL_FILE), body).unwrap_or_default();
        };
        write(&first, "b", "---\nname: Beta\n---\nfirst beta");
        write(&second, "b", "---\nname: Beta\n---\nsecond beta");
        write(&second, "a", "alpha");
        fs::create_dir_all(second.join("not-a-skill")).unwrap_or_default();
        let skills = load_skills(&[first, second.clone(), root.join("missing")]);
        let names: Vec<&str> = skills.iter().map(|skill| skill.name.as_str()).collect();
        assert_eq!(names, ["a", "Beta"]);
        assert_eq!(skills[1].instructions, "first beta");

        let path = create_example(&second).unwrap_or_default();
        assert!(path.ends_with("action-items/SKILL.md"));
        assert!(load_skills(&[second]).iter().any(|skill| skill.name == EXAMPLE_NAME));
        fs::remove_dir_all(&root).unwrap_or_default();
    }
}
