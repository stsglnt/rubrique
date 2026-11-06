//! Turn what you type in a terminal tab into what tmux's `send-keys` takes.
//!
//! Cmd shortcuts (macOS) stay with rubrique. Everything else typed while the
//! terminal has focus goes to tmux: text as it is, other keys by their tmux
//! names (`Enter`, `BSpace`, `C-c`).

use eframe::egui::{Event, Key, Modifiers};

/// One thing to type into tmux.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyInput {
    /// Text typed or pasted, sent as it is.
    Text(String),
    /// A key by its tmux name: `Enter`, `Up`, `C-c`, `M-Left`.
    Key(String),
}

/// What `event` types into the terminal, if anything.
pub fn key_input(event: &Event) -> Option<KeyInput> {
    match event {
        Event::Text(text) | Event::Paste(text) => Some(KeyInput::Text(text.clone())),
        // On Linux, Ctrl+C and Ctrl+X arrive as copy and cut (Ctrl is the
        // command key there); in a terminal they mean interrupt and Ctrl+X.
        Event::Copy if !cfg!(target_os = "macos") => Some(KeyInput::Key("C-c".to_string())),
        Event::Cut if !cfg!(target_os = "macos") => Some(KeyInput::Key("C-x".to_string())),
        Event::Key { key, pressed: true, modifiers, .. } if !modifiers.mac_cmd => named_key(*key, *modifiers),
        _ => None,
    }
}

/// The tmux name of `key` with `modifiers`; `None` for keys that arrive as
/// text instead (plain letters, digits, punctuation).
fn named_key(key: Key, modifiers: Modifiers) -> Option<KeyInput> {
    let base = match key {
        Key::Enter => "Enter",
        Key::Tab if modifiers.shift => "BTab",
        Key::Tab => "Tab",
        Key::Backspace => "BSpace",
        Key::Escape => "Escape",
        Key::ArrowUp => "Up",
        Key::ArrowDown => "Down",
        Key::ArrowLeft => "Left",
        Key::ArrowRight => "Right",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PPage",
        Key::PageDown => "NPage",
        Key::Delete => "DC",
        Key::Insert => "IC",
        _ => return control_letter(key, modifiers).or_else(|| function_key(key)),
    };
    let mut name = String::new();
    if modifiers.ctrl {
        name.push_str("C-");
    }
    if modifiers.alt {
        name.push_str("M-");
    }
    name.push_str(base);
    Some(KeyInput::Key(name))
}

/// Ctrl plus a letter: `C-a` to `C-z`. Plain letters come as text.
fn control_letter(key: Key, modifiers: Modifiers) -> Option<KeyInput> {
    if !modifiers.ctrl {
        return None;
    }
    let name = key.name();
    let mut letters = name.chars();
    match (letters.next(), letters.next()) {
        (Some(letter), None) if letter.is_ascii_alphabetic() => {
            Some(KeyInput::Key(format!("C-{}", letter.to_ascii_lowercase())))
        }
        _ => None,
    }
}

/// F1 to F12 keep their names.
fn function_key(key: Key) -> Option<KeyInput> {
    let name = key.name();
    let number: u8 = name.strip_prefix('F')?.parse().ok()?;
    (1..=12).contains(&number).then(|| KeyInput::Key(name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: Key, modifiers: Modifiers) -> Event {
        Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    fn named(name: &str) -> KeyInput {
        KeyInput::Key(name.to_string())
    }

    #[test]
    fn text_and_paste_go_as_they_are() {
        assert_eq!(key_input(&Event::Text("ls".to_string())), Some(KeyInput::Text("ls".to_string())));
        assert_eq!(key_input(&Event::Paste("a\nb".to_string())), Some(KeyInput::Text("a\nb".to_string())));
    }

    #[test]
    fn special_keys_get_tmux_names() {
        assert_eq!(key_input(&press(Key::Enter, Modifiers::NONE)), Some(named("Enter")));
        assert_eq!(key_input(&press(Key::Backspace, Modifiers::NONE)), Some(named("BSpace")));
        assert_eq!(key_input(&press(Key::Tab, Modifiers::SHIFT)), Some(named("BTab")));
        assert_eq!(key_input(&press(Key::ArrowLeft, Modifiers::ALT)), Some(named("M-Left")));
        assert_eq!(key_input(&press(Key::F5, Modifiers::NONE)), Some(named("F5")));
    }

    #[test]
    fn ctrl_letters_and_plain_letters() {
        assert_eq!(key_input(&press(Key::C, Modifiers::CTRL)), Some(named("C-c")));
        // A plain letter also arrives as a Text event, so the key is skipped.
        assert_eq!(key_input(&press(Key::C, Modifiers::NONE)), None);
    }

    #[test]
    fn mac_cmd_shortcuts_stay_with_the_app() {
        let cmd = Modifiers { mac_cmd: true, command: true, ..Modifiers::NONE };
        assert_eq!(key_input(&press(Key::K, cmd)), None);
        assert_eq!(key_input(&press(Key::Enter, Modifiers::NONE)), Some(named("Enter")));
    }
}
