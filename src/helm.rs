//! Helm charts: find the chart a file belongs to, and build the `helm`
//! commands that render (`helm template`) or check (`helm lint`) it.
//!
//! Pure apart from [`chart_root`] and [`find_helm`], which look at the disk.

pub mod highlight;

use std::path::{Path, PathBuf};

use crate::ai::find_on_path;

/// The file that makes a folder a Helm chart.
pub const CHART_FILE: &str = "Chart.yaml";

/// Where to get `helm`.
pub const INSTALL_PAGE: &str = "https://helm.sh/docs/intro/install/";

/// Where Homebrew puts programs. Apps opened from the Finder often do not
/// have these folders on `PATH`.
const BREW_FOLDERS: [&str; 3] = ["/opt/homebrew/bin", "/usr/local/bin", "/home/linuxbrew/.linuxbrew/bin"];

/// The chart folder `path` is in: the nearest folder above it with a
/// `Chart.yaml`. `None` for remote files, whose chart is on the server.
pub fn chart_root(path: &Path) -> Option<PathBuf> {
    if crate::remote::is_mirror(path) {
        return None;
    }
    path.ancestors().skip(1).find(|folder| folder.join(CHART_FILE).is_file()).map(Path::to_path_buf)
}

/// Where `helm` is: on `PATH`, or in a Homebrew folder.
pub fn find_helm() -> Option<PathBuf> {
    find_on_path("helm")
        .or_else(|| BREW_FOLDERS.iter().map(|folder| Path::new(folder).join("helm")).find(|path| path.is_file()))
}

/// What to do with a chart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelmAction {
    /// `helm template`: the Kubernetes files the chart makes.
    Render,
    /// `helm lint`: problems in the chart.
    Lint,
}

impl HelmAction {
    pub fn name(self) -> &'static str {
        match self {
            HelmAction::Render => "Render",
            HelmAction::Lint => "Lint",
        }
    }

    fn subcommand(self) -> &'static str {
        match self {
            HelmAction::Render => "template",
            HelmAction::Lint => "lint",
        }
    }

    /// The arguments for `helm`, run inside `chart`. When `file` is an extra
    /// values file of the chart (`values-prod.yaml`), it is used too.
    pub fn args(self, chart: &Path, file: &Path) -> Vec<String> {
        let mut args = vec![self.subcommand().to_string(), ".".to_string()];
        if let Some(values) = extra_values_file(chart, file) {
            args.push("--values".to_string());
            args.push(values);
        }
        args
    }
}

/// The name of `file` when it is a values file in the chart folder other
/// than the default `values.yaml`, like `values-prod.yaml`.
fn extra_values_file(chart: &Path, file: &Path) -> Option<String> {
    if file.parent() != Some(chart) {
        return None;
    }
    let name = file.file_name()?.to_str()?;
    let (stem, extension) = name.rsplit_once('.')?;
    let is_yaml = extension.eq_ignore_ascii_case("yaml") || extension.eq_ignore_ascii_case("yml");
    let stem = stem.to_ascii_lowercase();
    let extra = is_yaml && stem.starts_with("values") && stem != "values";
    extra.then(|| name.to_string())
}

/// True for files that are YAML or Helm templates, for coloring: `.yaml`,
/// `.yml`, `.tpl`, `Chart.lock`, and a chart's `templates/NOTES.txt`.
pub fn is_yaml_path(path: &Path) -> bool {
    let name = path.file_name().map(|name| name.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if name == "chart.lock" {
        return true;
    }
    if name == "notes.txt" {
        return path.parent().and_then(Path::file_name).is_some_and(|folder| folder == "templates");
    }
    let extension = path.extension().map(|extension| extension.to_string_lossy().to_ascii_lowercase());
    matches!(extension.as_deref(), Some("yaml" | "yml" | "tpl"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempFolder;

    #[test]
    fn finds_the_nearest_chart() {
        let dir = TempFolder::new();
        let chart = dir.write("web/Chart.yaml", "name: web\n").parent().map(Path::to_path_buf).unwrap_or_default();
        assert_eq!(chart_root(&chart.join("templates/deployment.yaml")), Some(chart.clone()));
        assert_eq!(chart_root(&chart.join("values.yaml")), Some(chart.clone()));
        assert_eq!(chart_root(&dir.path().join("notes.md")), None);
    }

    #[test]
    fn commands_use_extra_values_files() {
        let chart = Path::new("/c/web");
        let render = HelmAction::Render.args(chart, &chart.join("templates/a.yaml"));
        assert_eq!(render, ["template", "."]);
        let lint = HelmAction::Lint.args(chart, &chart.join("values-prod.yaml"));
        assert_eq!(lint, ["lint", ".", "--values", "values-prod.yaml"]);
        assert_eq!(HelmAction::Render.args(chart, &chart.join("values.yaml")), ["template", "."]);
        // A values file in a subfolder is not the chart's.
        assert_eq!(HelmAction::Render.args(chart, &chart.join("ci/values-x.yaml")), ["template", "."]);
    }

    #[test]
    fn yaml_paths() {
        assert!(is_yaml_path(Path::new("a/values.yaml")));
        assert!(is_yaml_path(Path::new("a/ci.YML")));
        assert!(is_yaml_path(Path::new("c/templates/_helpers.tpl")));
        assert!(is_yaml_path(Path::new("c/templates/NOTES.txt")));
        assert!(is_yaml_path(Path::new("c/Chart.lock")));
        assert!(!is_yaml_path(Path::new("c/NOTES.txt")));
        assert!(!is_yaml_path(Path::new("notes.md")));
    }
}
