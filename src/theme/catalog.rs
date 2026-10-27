//! The list of available themes and the user's choice among them.

use std::fs;
use std::path::Path;

use super::palette::{Base, Theme};

/// File extension of theme files in the themes folder.
const THEME_EXTENSION: &str = "theme";
/// What the settings file stores for "follow the system".
const SYSTEM_ID: &str = "system";

/// Which theme the user picked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ThemeChoice {
    /// Light or dark, following the operating system.
    #[default]
    System,
    /// A theme by id (`light`, `dark`, or a file name in the themes folder).
    Named(String),
}

impl ThemeChoice {
    /// Read a `theme=` settings value; empty means "system".
    pub fn parse(value: &str) -> Self {
        match value {
            "" | SYSTEM_ID => ThemeChoice::System,
            id => ThemeChoice::Named(id.to_string()),
        }
    }

    /// The `theme=` settings value.
    pub fn id(&self) -> &str {
        match self {
            ThemeChoice::System => SYSTEM_ID,
            ThemeChoice::Named(id) => id,
        }
    }
}

/// All themes: Light and Dark first, then the user's themes by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Themes {
    pub list: Vec<Theme>,
}

impl Themes {
    /// The built-in themes plus every `*.theme` file in `folder`. A user file
    /// named `light.theme` or `dark.theme` replaces the built-in one.
    pub fn load(folder: Option<&Path>) -> Self {
        let mut themes = Themes::builtin();
        let Some(entries) = folder.and_then(|folder| fs::read_dir(folder).ok()) else {
            return themes;
        };
        let mut user_themes: Vec<Theme> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == THEME_EXTENSION))
            .filter_map(|path| {
                let id = path.file_stem()?.to_str()?.to_string();
                let text = fs::read_to_string(&path).ok()?;
                Some(Theme::parse(&id, &text))
            })
            .collect();
        user_themes.sort_by_key(|theme| theme.name.to_lowercase());
        for theme in user_themes {
            match themes.list.iter_mut().find(|known| known.id == theme.id) {
                Some(known) => *known = theme,
                None => themes.list.push(theme),
            }
        }
        themes
    }

    /// Only the themes shipped with rubrique.
    pub fn builtin() -> Self {
        Themes { list: vec![Theme::builtin(Base::Light), Theme::builtin(Base::Dark)] }
    }

    /// The theme with this id, if there is one.
    pub fn find(&self, id: &str) -> Option<&Theme> {
        self.list.iter().find(|theme| theme.id == id)
    }

    /// The theme used for the system's light or dark mode.
    pub fn for_base(&self, base: Base) -> Theme {
        self.find(base.id()).cloned().unwrap_or_else(|| Theme::builtin(base))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempFolder;
    use crate::theme::palette::Rgb;

    #[test]
    fn choice_round_trip() {
        assert_eq!(ThemeChoice::parse(""), ThemeChoice::System);
        assert_eq!(ThemeChoice::parse("system"), ThemeChoice::System);
        assert_eq!(ThemeChoice::parse("dark"), ThemeChoice::Named("dark".to_string()));
        for choice in [ThemeChoice::System, ThemeChoice::Named("ocean".to_string())] {
            assert_eq!(ThemeChoice::parse(choice.id()), choice);
        }
    }

    #[test]
    fn light_and_dark_are_always_installed() {
        let themes = Themes::load(None);
        let ids: Vec<&str> = themes.list.iter().map(|theme| theme.id.as_str()).collect();
        assert_eq!(ids, ["light", "dark"]);
    }

    #[test]
    fn loads_user_themes_after_the_builtin_ones() {
        let temp = TempFolder::new();
        temp.write("zen.theme", "name=Zen\nbase=light\n");
        temp.write("Night.theme", "name=night owl\nbase=dark\n");
        temp.write("notes.md", "name=Not a theme\n");
        let themes = Themes::load(Some(temp.path()));
        let names: Vec<&str> = themes.list.iter().map(|theme| theme.name.as_str()).collect();
        assert_eq!(names, ["Light", "Dark", "night owl", "Zen"]);
        assert_eq!(themes.find("Night").map(|theme| theme.base), Some(Base::Dark));
    }

    #[test]
    fn a_user_file_replaces_a_builtin_theme() {
        let temp = TempFolder::new();
        temp.write("dark.theme", "name=My Dark\nbase=dark\naccent=#ff0000\n");
        let themes = Themes::load(Some(temp.path()));
        assert_eq!(themes.list.len(), 2);
        let dark = themes.for_base(Base::Dark);
        assert_eq!(dark.name, "My Dark");
        assert_eq!(dark.palette.accent, Rgb(255, 0, 0));
    }

    #[test]
    fn missing_folder_gives_the_builtin_themes() {
        let temp = TempFolder::new();
        assert_eq!(Themes::load(Some(&temp.path().join("nope"))), Themes::builtin());
    }
}
