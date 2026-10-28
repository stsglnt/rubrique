//! User settings stored as a plain `key=value` file in the config folder.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::ai::AiSettings;
use crate::theme::catalog::ThemeChoice;
use crate::voice::VoiceSettings;

pub mod view;

/// How a Markdown file is shown: as editable text or as the rendered preview.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MarkdownView {
    #[default]
    Edit,
    Preview,
}

impl MarkdownView {
    /// The other mode (what Cmd+E switches to).
    pub fn toggled(self) -> Self {
        match self {
            MarkdownView::Edit => MarkdownView::Preview,
            MarkdownView::Preview => MarkdownView::Edit,
        }
    }
}

/// Everything rubrique remembers between runs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settings {
    /// Explorer root folders of the last session, in order.
    pub folders: Vec<PathBuf>,
    /// File open at the end of the last session (the active tab).
    pub last_file: Option<PathBuf>,
    /// Every tab open at the end of the last session, left to right.
    pub open_tabs: Vec<PathBuf>,
    /// "Markdown only" checkbox in the explorer.
    pub markdown_only: bool,
    /// Last used mode for Markdown files.
    pub markdown_view: MarkdownView,
    /// Inner window size (width, height) in points.
    pub window_size: Option<(f32, f32)>,
    /// Color theme, or "system" to follow the OS.
    pub theme: ThemeChoice,
    /// Where new scratch files go; `None` means the default folder.
    pub scratch_folder: Option<PathBuf>,
    /// AI tools and their commands.
    pub ai: AiSettings,
    /// Voice memos: folder, dropped files, transcription.
    pub voice: VoiceSettings,
}

impl Settings {
    /// Read settings from disk; missing or broken files give defaults.
    pub fn load() -> Self {
        config_folder()
            .map(|folder| folder.join("settings"))
            .and_then(|path| fs::read_to_string(path).ok())
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Write settings to the config folder, creating it when needed.
    ///
    /// # Errors
    ///
    /// Fails when there is no home folder or the file cannot be written.
    pub fn save(&self) -> io::Result<()> {
        let Some(path) = config_folder().map(|folder| folder.join("settings")) else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "no home folder"));
        };
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder)?;
        }
        fs::write(path, self.to_text())
    }

    /// Parse `key=value` lines. Unknown keys and bad values are ignored.
    pub fn parse(text: &str) -> Self {
        let mut settings = Settings::default();
        let mut width = None;
        let mut height = None;
        // Files from before multiple folders have one `last_folder` line.
        let mut legacy_folder = None;
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            if settings.ai.parse_line(key.trim(), value) || settings.voice.parse_line(key.trim(), value) {
                continue;
            }
            match key.trim() {
                "folder" => settings.folders.extend(non_empty_path(value)),
                "last_folder" => legacy_folder = non_empty_path(value),
                "last_file" => settings.last_file = non_empty_path(value),
                "tab" => settings.open_tabs.extend(non_empty_path(value)),
                "markdown_only" => settings.markdown_only = value == "true",
                // Stored as `preview_mode=true|false` so existing settings files keep working.
                "preview_mode" => settings.markdown_view = preview_mode_view(value),
                "window_width" => width = value.parse::<f32>().ok(),
                "window_height" => height = value.parse::<f32>().ok(),
                "theme" => settings.theme = ThemeChoice::parse(value),
                "scratch_folder" => settings.scratch_folder = non_empty_path(value),
                _ => {}
            }
        }
        if settings.folders.is_empty() {
            settings.folders.extend(legacy_folder);
        }
        if let (Some(width), Some(height)) = (width, height) {
            settings.window_size = Some((width, height));
        }
        settings
    }

    /// Serialize to the `key=value` format that [`Settings::parse`] reads.
    pub fn to_text(&self) -> String {
        let path_text =
            |path: &Option<PathBuf>| path.as_ref().map(|path| path.display().to_string()).unwrap_or_default();
        let window_lines = match self.window_size {
            Some((width, height)) => format!("window_width={width}\nwindow_height={height}\n"),
            None => String::new(),
        };
        let path_lines = |key: &str, paths: &[PathBuf]| {
            paths.iter().fold(String::new(), |mut lines, path| {
                let _ = writeln!(lines, "{key}={}", path.display());
                lines
            })
        };
        let folder_lines = path_lines("folder", &self.folders);
        let tab_lines = path_lines("tab", &self.open_tabs);
        format!(
            "{folder_lines}{tab_lines}last_file={}\nmarkdown_only={}\npreview_mode={}\ntheme={}\nscratch_folder={}\n{window_lines}{}{}",
            path_text(&self.last_file),
            self.markdown_only,
            self.markdown_view == MarkdownView::Preview,
            self.theme.id(),
            path_text(&self.scratch_folder),
            self.ai.to_lines(),
            self.voice.to_lines(),
        )
    }
}

/// `preview_mode=true` means the preview; anything else means edit.
fn preview_mode_view(value: &str) -> MarkdownView {
    if value == "true" { MarkdownView::Preview } else { MarkdownView::Edit }
}

/// An empty value means "not set".
fn non_empty_path(value: &str) -> Option<PathBuf> {
    (!value.is_empty()).then(|| PathBuf::from(value))
}

impl Settings {
    /// The folder for new scratch files: the chosen one, else the default.
    pub fn scratch_folder_or_default(&self) -> Option<PathBuf> {
        self.scratch_folder.clone().or_else(default_scratch_folder)
    }
}

/// Default folder for scratch files (`scratches` in the config folder).
pub fn default_scratch_folder() -> Option<PathBuf> {
    config_folder().map(|folder| folder.join("scratches"))
}

/// Folder for cached file summaries (`cache/summaries` in the config folder).
pub fn summary_cache_folder() -> Option<PathBuf> {
    config_folder().map(|folder| folder.join("cache").join("summaries"))
}

/// Default folder for skills (`skills` in the config folder).
pub fn default_skills_folder() -> Option<PathBuf> {
    config_folder().map(|folder| folder.join("skills"))
}

/// Folder for the user's own `*.theme` files (`themes` in the config folder).
pub fn themes_folder() -> Option<PathBuf> {
    config_folder().map(|folder| folder.join("themes"))
}

/// macOS: `~/Library/Application Support/rubrique`.
/// Linux: `$XDG_CONFIG_HOME/rubrique`, fallback `~/.config/rubrique`.
fn config_folder() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        home?.join("Library/Application Support")
    } else {
        match std::env::var_os("XDG_CONFIG_HOME") {
            Some(folder) if Path::new(&folder).is_absolute() => PathBuf::from(folder),
            _ => home?.join(".config"),
        }
    };
    Some(base.join("rubrique"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let settings = Settings {
            folders: vec![PathBuf::from("/home/a/my notes"), PathBuf::from("/work/docs")],
            last_file: Some(PathBuf::from("/home/a/my notes/x=y.md")),
            open_tabs: vec![PathBuf::from("/home/a/my notes/x=y.md"), PathBuf::from("/work/docs/photo.png")],
            markdown_only: true,
            markdown_view: MarkdownView::Preview,
            window_size: Some((1200.0, 800.5)),
            theme: ThemeChoice::Named("dark".to_string()),
            scratch_folder: Some(PathBuf::from("/home/a/scratch")),
            ai: {
                let mut ai = AiSettings::default();
                ai.default_tool = crate::ai::AiTool::Grok;
                ai.set_command(crate::ai::AiTool::Grok, "grok-cli --headless {prompt}");
                ai
            },
            voice: crate::voice::VoiceSettings {
                folder: "audio".to_string(),
                copy_dropped: false,
                auto_transcribe: false,
                command: "yap transcribe {audio}".to_string(),
            },
        };
        assert_eq!(Settings::parse(&settings.to_text()), settings);
    }

    #[test]
    fn defaults_for_garbage_and_unknown_keys() {
        let settings = Settings::parse("nonsense\ncolor=blue\nwindow_width=wide\nmarkdown_only=yes\n");
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn round_trip_without_window_size() {
        let settings = Settings { markdown_view: MarkdownView::Edit, ..Settings::default() };
        assert_eq!(Settings::parse(&settings.to_text()), settings);
    }

    #[test]
    fn reads_the_preview_mode_flag() {
        assert_eq!(Settings::parse("preview_mode=true\n").markdown_view, MarkdownView::Preview);
        assert_eq!(Settings::parse("preview_mode=false\n").markdown_view, MarkdownView::Edit);
    }

    #[test]
    fn old_last_folder_line_becomes_the_only_folder() {
        let settings = Settings::parse("last_folder=/home/a/notes\n");
        assert_eq!(settings.folders, [PathBuf::from("/home/a/notes")]);
        let both = Settings::parse("last_folder=/old\nfolder=/new\n");
        assert_eq!(both.folders, [PathBuf::from("/new")]);
    }

    #[test]
    fn scratch_folder_falls_back_to_the_default() {
        let custom = Settings { scratch_folder: Some(PathBuf::from("/tmp/s")), ..Settings::default() };
        assert_eq!(custom.scratch_folder_or_default(), Some(PathBuf::from("/tmp/s")));
        assert_eq!(Settings::default().scratch_folder_or_default(), default_scratch_folder());
        assert_eq!(Settings::parse("scratch_folder=\n").scratch_folder, None);
    }

    #[test]
    fn theme_defaults_to_system() {
        assert_eq!(Settings::parse("").theme, ThemeChoice::System);
        assert_eq!(Settings::parse("theme=light\n").theme, ThemeChoice::Named("light".to_string()));
    }

    #[test]
    fn toggling_the_view_switches_between_modes() {
        assert_eq!(MarkdownView::Edit.toggled(), MarkdownView::Preview);
        assert_eq!(MarkdownView::Preview.toggled(), MarkdownView::Edit);
    }
}
