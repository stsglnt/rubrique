//! Names for scratch files from their content: the AI suggests a few words
//! and rubrique turns them into `DATE-a-few-words.md`.

use super::Prompt;

/// Only the start of a note is sent to suggest a name.
const MAX_NAME_INPUT_CHARS: usize = 4_000;
/// Longest suggested name, without date and extension.
const MAX_NAME_CHARS: usize = 40;
/// Most words kept from a suggestion.
const MAX_NAME_WORDS: usize = 5;

/// The prompt asking for a short name for `text`.
pub fn name_prompt(text: &str) -> Prompt {
    let shown = text.char_indices().nth(MAX_NAME_INPUT_CHARS).map_or(text, |(end, _)| &text[..end]);
    Prompt {
        system: "You name note files. Reply with only a short name of 2 to 4 words that says what the note \
                 is about, in the language of the note. No date, no file extension, no quotes, no other text. \
                 Do not use tools or change any files."
            .to_string(),
        user: format!("Name this note:\n<text>\n{shown}\n</text>\n"),
    }
}

/// Turn a reply into a file name part: lowercase words joined by `-`, only
/// letters and digits, at most a few words. `None` when nothing is left.
pub fn name_slug(reply: &str) -> Option<String> {
    let first_line = reply.lines().map(str::trim).find(|line| !line.is_empty())?;
    let mut slug = String::new();
    let words = first_line.split(|c: char| !c.is_alphanumeric()).filter(|word| !word.is_empty());
    for word in words.take(MAX_NAME_WORDS) {
        let word = word.to_lowercase();
        if slug.chars().count() + word.chars().count() + 1 > MAX_NAME_CHARS {
            break;
        }
        if !slug.is_empty() {
            slug.push('-');
        }
        slug.push_str(&word);
    }
    (!slug.is_empty()).then_some(slug)
}

/// The new file name: the old name's date (when it has one), the slug, and
/// the old extension.
pub fn suggested_file_name(old_name: &str, slug: &str) -> String {
    let (stem, extension) = old_name.rsplit_once('.').unwrap_or((old_name, "md"));
    let date = stem.get(..10).filter(|date| looks_like_date(date));
    match date {
        Some(date) => format!("{date}-{slug}.{extension}"),
        None => format!("{slug}.{extension}"),
    }
}

/// `YYYY-MM-DD`.
fn looks_like_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_sends_the_start() {
        let prompt = name_prompt("Beta plan for March.");
        assert!(prompt.user.contains("<text>\nBeta plan for March.\n</text>"));
        assert!(prompt.system.contains("2 to 4 words"));
        let long = name_prompt(&"x".repeat(MAX_NAME_INPUT_CHARS * 2));
        assert!(long.user.len() < MAX_NAME_INPUT_CHARS + 100);
    }

    #[test]
    fn slugs_are_short_and_clean() {
        assert_eq!(name_slug("Beta plan: March!").as_deref(), Some("beta-plan-march"));
        assert_eq!(name_slug("\n  \"Upgrade guide\"\nmore text").as_deref(), Some("upgrade-guide"));
        assert_eq!(name_slug("План бета-версії").as_deref(), Some("план-бета-версії"));
        assert_eq!(name_slug("one two three four five six seven").as_deref(), Some("one-two-three-four-five"));
        assert!(name_slug(&"long".repeat(30)).is_none_or(|slug| slug.chars().count() <= MAX_NAME_CHARS));
        assert_eq!(name_slug(" ... "), None);
        assert_eq!(name_slug(""), None);
    }

    #[test]
    fn keeps_date_and_extension() {
        assert_eq!(suggested_file_name("2026-10-28-brave-otter.md", "beta-plan"), "2026-10-28-beta-plan.md");
        assert_eq!(suggested_file_name("notes.txt", "beta-plan"), "beta-plan.txt");
        assert_eq!(suggested_file_name("README", "beta-plan"), "beta-plan.md");
        assert_eq!(suggested_file_name("2026-1x-28-a.md", "b"), "b.md");
    }
}
