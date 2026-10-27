//! rubrique: a fast, lightweight Markdown editor for Linux and macOS.

mod app;
mod date;
mod document;
mod explorer;
mod markdown;
mod settings;
#[cfg(test)]
mod test_support;
mod theme;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use eframe::egui;

use settings::Settings;

/// Window size (width, height) on the first run, before one is saved.
const DEFAULT_WINDOW_SIZE: (f32, f32) = (1100.0, 720.0);

const USAGE: &str = "Usage: rubrique [FOLDER | FILE]

Opens FOLDER in the explorer, or FILE together with its folder.
Without an argument, reopens the last folder (or the current one).";

/// Handle `--help` / `--version`, pick the starting folder, run the window.
fn main() -> ExitCode {
    let argument = std::env::args_os().nth(1).map(PathBuf::from);
    match argument.as_deref().and_then(Path::to_str) {
        Some("-h" | "--help") => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Some("-V" | "--version") => {
            println!("rubrique {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        _ => {}
    }

    let settings = Settings::load();
    let (root, file) = match starting_location(argument.as_deref(), &settings) {
        Ok(location) => location,
        Err(message) => {
            eprintln!("rubrique: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let (width, height) = settings.window_size.unwrap_or(DEFAULT_WINDOW_SIZE);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("rubrique").with_inner_size([width, height]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "rubrique",
        options,
        Box::new(move |creation_context| {
            Ok(Box::new(app::RubriqueApp::new(&creation_context.egui_ctx, settings, root, file)))
        }),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rubrique: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Pick the explorer root and an optional file to open (spec 01, "Root folder").
///
/// Errors are messages for the terminal: a missing path, or no readable
/// current folder.
fn starting_location(argument: Option<&Path>, settings: &Settings) -> Result<(PathBuf, Option<PathBuf>), String> {
    if let Some(argument) = argument {
        let path = std::path::absolute(argument).map_err(|error| format!("{}: {error}", argument.display()))?;
        if path.is_dir() {
            return Ok((path, None));
        }
        if path.is_file() {
            let folder = path.parent().map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
            return Ok((folder, Some(path)));
        }
        return Err(format!("no such file or folder: {}", path.display()));
    }

    if let Some(folder) = settings.last_folder.as_ref().filter(|folder| folder.is_dir()) {
        let file = settings.last_file.clone().filter(|file| file.is_file() && file.starts_with(folder));
        return Ok((folder.clone(), file));
    }
    let current = std::env::current_dir().map_err(|error| format!("current folder: {error}"))?;
    Ok((current, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempFolder;

    #[test]
    fn folder_argument_opens_that_folder() {
        let temp = TempFolder::new();
        let location = starting_location(Some(temp.path()), &Settings::default()).unwrap();
        assert_eq!(location, (temp.path().to_path_buf(), None));
    }

    #[test]
    fn file_argument_opens_the_file_and_its_folder() {
        let temp = TempFolder::new();
        let file = temp.write("docs/todo.md", "");
        let location = starting_location(Some(&file), &Settings::default()).unwrap();
        assert_eq!(location, (temp.path().join("docs"), Some(file)));
    }

    #[test]
    fn missing_argument_is_an_error() {
        let temp = TempFolder::new();
        let missing = temp.path().join("nope.md");
        let error = starting_location(Some(&missing), &Settings::default()).unwrap_err();
        assert!(error.starts_with("no such file or folder"), "{error}");
    }

    #[test]
    fn no_argument_reopens_last_folder_and_file_inside_it() {
        let temp = TempFolder::new();
        let file = temp.write("todo.md", "");
        let settings = Settings {
            last_folder: Some(temp.path().to_path_buf()),
            last_file: Some(file.clone()),
            ..Settings::default()
        };
        assert_eq!(starting_location(None, &settings).unwrap(), (temp.path().to_path_buf(), Some(file)));
    }

    #[test]
    fn last_file_outside_last_folder_is_not_reopened() {
        let temp = TempFolder::new();
        let folder = temp.make_folder("notes");
        let outside = temp.write("other.md", "");
        let settings = Settings { last_folder: Some(folder.clone()), last_file: Some(outside), ..Settings::default() };
        assert_eq!(starting_location(None, &settings).unwrap(), (folder, None));
    }

    #[test]
    fn no_argument_and_no_saved_folder_uses_the_current_folder() {
        let settings = Settings { last_folder: Some(PathBuf::from("/definitely/not/here")), ..Settings::default() };
        let current = std::env::current_dir().unwrap();
        assert_eq!(starting_location(None, &settings).unwrap(), (current, None));
    }
}
