//! Remote folders in the app: the "Add remote folder" dialog, checking the
//! open remote file for changes in the background, and downloading the
//! images a remote note shows.

use std::io;
use std::path::{Component, Path, PathBuf};

use eframe::egui::{self, Context, Key, RichText};

use super::{OpenFile, RubriqueApp};
use crate::document::text_file::DiskState;
use crate::media::{MediaKind, image_uri, media_kind_of_url};
use crate::remote::task::{Task, spawn_with_repaint};
use crate::remote::{self, commands};
use crate::vfs::{self, Stamp};

/// Largest image downloaded for a remote note's preview.
const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
/// Most images downloaded for one note.
const MAX_IMAGES: usize = 60;
/// Largest image or video opened from a remote folder.
pub(super) const MAX_MEDIA_BYTES: u64 = 200 * 1024 * 1024;
/// Width of the "Add remote folder" dialog.
const DIALOG_WIDTH: f32 = 380.0;

/// Background work for remote folders.
#[derive(Debug, Default)]
pub(super) struct RemoteState {
    /// Checking the open remote file for outside changes.
    check: Option<(PathBuf, Task<io::Result<Option<Stamp>>>)>,
    /// Image downloads for remote notes; each gives the mirror paths it wrote.
    images: Vec<Task<Vec<PathBuf>>>,
    /// The "Add remote folder" dialog, while it is open.
    dialog: Option<AddRemote>,
}

/// State of the "Add remote folder" dialog.
#[derive(Debug, Default)]
struct AddRemote {
    host: String,
    folder: String,
    /// Hosts from `~/.ssh/config`, offered as buttons.
    known_hosts: Vec<String>,
    /// Why the last attempt failed.
    error: Option<String>,
    /// Connecting and finding the folder.
    connecting: Option<Task<io::Result<PathBuf>>>,
}

impl RubriqueApp {
    /// Each frame: pick up finished background work for remote files.
    pub(super) fn poll_remote(&mut self) {
        let mut fetched = Vec::new();
        self.remote.images.retain(|task| match task.take() {
            Some(paths) => {
                fetched.extend(paths);
                false
            }
            None => true,
        });
        for path in fetched {
            self.repaint.forget_image(&image_uri(&path));
        }

        let finished = self.remote.check.as_ref().and_then(|(path, task)| Some((path.clone(), task.take()?)));
        if let Some((path, result)) = finished {
            self.remote.check = None;
            let state = match &self.open_file {
                OpenFile::Text { document, .. } if document.path == path && self.merging.is_none() => {
                    Some(document.disk_state_from(result))
                }
                _ => None,
            };
            if let Some(state) = state {
                self.apply_disk_state(&state);
            }
        }
    }

    /// Ask the host in the background whether the open file changed.
    pub(super) fn start_remote_check(&mut self, path: &Path) {
        if self.remote.check.is_some() {
            return;
        }
        let task_path = path.to_path_buf();
        let task = spawn_with_repaint(&self.repaint, move || vfs::current_stamp(&task_path));
        self.remote.check = Some((path.to_path_buf(), task));
    }

    /// Download the images a remote note links to, so its preview shows them.
    pub(super) fn fetch_remote_images(&mut self, note: &Path, text: &str) {
        let Some(folder) = note.parent().filter(|_| remote::is_mirror(note)) else {
            return;
        };
        let mut wanted: Vec<PathBuf> = Vec::new();
        for url in image_links(text) {
            for path in image_paths(folder, &url) {
                if remote::is_mirror(&path) && !wanted.contains(&path) {
                    wanted.push(path);
                }
            }
        }
        wanted.truncate(MAX_IMAGES);
        if wanted.is_empty() {
            return;
        }
        let task = spawn_with_repaint(&self.repaint, move || download_images(&wanted));
        self.remote.images.push(task);
    }

    /// True (with a message) when the open note is remote: adding images,
    /// audio or voice memos to remote notes is not supported yet.
    pub(super) fn refuse_remote_note(&mut self, what: &str) -> bool {
        let is_remote = self.open_file.path().is_some_and(remote::is_mirror);
        if is_remote {
            self.show_status(format!("Adding {what} to notes in remote folders is not supported yet"));
        }
        is_remote
    }

    /// Open the "Add remote folder" dialog.
    pub(super) fn open_add_remote(&mut self) {
        let known_hosts = remote::ssh_config::user_host_names();
        let host = known_hosts.first().cloned().unwrap_or_default();
        self.remote.dialog = Some(AddRemote { host, folder: "~".to_string(), known_hosts, ..AddRemote::default() });
    }

    /// Draw the "Add remote folder" dialog while it is open.
    pub(super) fn add_remote_dialog(&mut self, ctx: &Context) {
        let Some(dialog) = &mut self.remote.dialog else {
            return;
        };
        let finished = dialog.connecting.as_ref().and_then(Task::take);
        if let Some(result) = finished {
            dialog.connecting = None;
            match result.map(|path| remote::to_mirror(dialog.host.trim(), &path)) {
                Ok(Some(mirror)) => {
                    self.remote.dialog = None;
                    self.add_folder(mirror.clone());
                    self.show_status(format!("Added {}", remote::display(&mirror)));
                    return;
                }
                Ok(None) => dialog.error = Some("That folder cannot be used".to_string()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    dialog.error = Some(format!("No folder \"{}\" on {}", dialog.folder.trim(), dialog.host.trim()));
                }
                Err(error) => dialog.error = Some(error.to_string()),
            }
        }

        let connecting = dialog.connecting.is_some();
        let modal = egui::Modal::new(egui::Id::unique("add-remote-folder")).show(ctx, |ui| {
            ui.set_width(DIALOG_WIDTH);
            ui.heading("Add remote folder");
            ui.label(RichText::new("A folder on another computer, opened over SSH.").weak());
            ui.add_space(8.0);
            ui.label("Computer");
            let host_field = ui.add_enabled(
                !connecting,
                egui::TextEdit::singleline(&mut dialog.host)
                    .hint_text("server or user@server")
                    .desired_width(f32::INFINITY),
            );
            if !dialog.known_hosts.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("From ~/.ssh/config:").weak());
                    for name in &dialog.known_hosts {
                        if ui.add_enabled(!connecting, egui::Button::selectable(dialog.host == *name, name)).clicked() {
                            dialog.host.clone_from(name);
                        }
                    }
                });
            }
            ui.add_space(4.0);
            ui.label("Folder");
            let folder_field = ui.add_enabled(
                !connecting,
                egui::TextEdit::singleline(&mut dialog.folder).hint_text("~/notes").desired_width(f32::INFINITY),
            );
            if let Some(error) = &dialog.error {
                ui.add_space(4.0);
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.add_space(8.0);
            let entered = (host_field.lost_focus() || folder_field.lost_focus())
                && ui.input(|input| input.key_pressed(Key::Enter));
            let host_ok = remote::mirror::is_valid_host(dialog.host.trim());
            let ready = host_ok && !dialog.folder.trim().is_empty() && !connecting;
            ui.horizontal(|ui| {
                let add = ui.add_enabled(ready, egui::Button::new("Add")).clicked() || (entered && ready);
                let cancel = ui.button("Cancel").clicked();
                if connecting {
                    ui.add(egui::Spinner::new().size(12.0));
                    ui.label(RichText::new(format!("Connecting to {}…", dialog.host.trim())).weak());
                } else if !host_ok && !dialog.host.trim().is_empty() {
                    ui.label(RichText::new("Use a host name, or user@host").weak());
                }
                (add, cancel)
            })
            .inner
        });
        let (add, cancel) = modal.inner;
        if cancel || (modal.should_close() && !connecting) {
            self.remote.dialog = None;
            return;
        }
        if add {
            let host = dialog.host.trim().to_string();
            let folder = dialog.folder.trim().to_string();
            dialog.error = None;
            // The user asked to try now, even if the host failed a moment ago.
            remote::connection::forget_failure(&host);
            dialog.connecting = Some(spawn_with_repaint(ctx, move || commands::resolve_folder(&host, &folder)));
        }
    }

    /// Keep the settings' folder list in step with the explorer. While remote
    /// folders are off, the remote ones in the settings are kept for later.
    pub(super) fn remember_roots(&mut self) {
        let mut folders: Vec<PathBuf> = self.roots.iter().map(|root| root.path.clone()).collect();
        if !self.settings.remote.enabled {
            let hidden =
                self.settings.folders.iter().filter(|folder| remote::is_mirror(folder) && !folders.contains(folder));
            folders.extend(hidden.cloned().collect::<Vec<_>>());
        }
        self.settings.folders = folders;
    }

    /// Settings → Files changed the remote folder settings.
    pub(super) fn remote_settings_changed(&mut self) {
        remote::set_ssh_program(&self.settings.remote.ssh_program);
        if self.settings.remote.enabled {
            // Bring back the remote folders kept while it was off.
            let missing: Vec<PathBuf> = self
                .settings
                .folders
                .iter()
                .filter(|folder| remote::is_mirror(folder) && !self.roots.iter().any(|root| root.path == **folder))
                .cloned()
                .collect();
            for folder in missing {
                self.add_folder(folder);
            }
        } else {
            self.remember_roots();
            self.show_status("Remote folders stay in the explorer until rubrique restarts".to_string());
        }
    }
}

/// Fetch images from their hosts into the mirror. Returns the mirror paths
/// that were written. Images that cannot be read are skipped.
fn download_images(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut hosts: Vec<String> = Vec::new();
    for path in paths {
        if let Some(found) = remote::from_mirror(path)
            && !hosts.contains(&found.host)
        {
            hosts.push(found.host);
        }
    }
    let mut written = Vec::new();
    for host in hosts {
        let mirrors: Vec<&PathBuf> =
            paths.iter().filter(|path| remote::from_mirror(path).is_some_and(|found| found.host == host)).collect();
        let remote_paths: Vec<PathBuf> =
            mirrors.iter().filter_map(|path| remote::from_mirror(path).map(|found| found.path)).collect();
        let Ok(files) = commands::read_many(&host, &remote_paths, MAX_IMAGE_BYTES) else {
            continue;
        };
        for (mirror, bytes) in mirrors.into_iter().zip(files) {
            if let Some(bytes) = bytes {
                vfs::write_mirror(mirror, &bytes);
                written.push(mirror.clone());
            }
        }
    }
    written
}

/// The targets of `![alt](url)` images and `<img src="url">` tags.
fn image_links(text: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("![") {
        rest = &rest[start + 2..];
        let Some(close) = rest.find("](") else {
            break;
        };
        let after = &rest[close + 2..];
        let target = after[..after.find(')').unwrap_or(after.len())].trim();
        let url = match target.strip_prefix('<').and_then(|inner| inner.split_once('>')) {
            Some((url, _)) => url,
            // A title may follow the URL: ![a](cat.png "Cat").
            None => target.split_whitespace().next().unwrap_or(""),
        };
        links.push(url.to_string());
        rest = after;
    }
    for (start, _) in text.match_indices("<img") {
        let tag = &text[start..start + text[start..].find('>').unwrap_or(text.len() - start)];
        let Some(at) = tag.find("src=") else {
            continue;
        };
        let value = &tag[at + 4..];
        let Some(quote) = value.chars().next().filter(|quote| matches!(quote, '"' | '\'')) else {
            continue;
        };
        if let Some(end) = value[1..].find(quote) {
            links.push(value[1..=end].to_string());
        }
    }
    links
}

/// Where a relative image link points, under `folder`: as written and,
/// when it has `%20`-style escapes, decoded too. Web links, absolute paths
/// (which mean this computer to the preview) and non-images give nothing.
fn image_paths(folder: &Path, url: &str) -> Vec<PathBuf> {
    let url = url.split(['?', '#']).next().unwrap_or(url);
    if url.is_empty() || url.contains(':') || url.starts_with('/') || media_kind_of_url(url) != Some(MediaKind::Image) {
        return Vec::new();
    }
    let mut paths = vec![normalize(&folder.join(url))];
    let decoded = crate::voice::decode_url(url);
    if decoded != url {
        paths.push(normalize(&folder.join(decoded)));
    }
    paths
}

/// `a/b/../c` as `a/c`, without looking at the disk.
fn normalize(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normal.pop();
            }
            Component::CurDir => {}
            other => normal.push(other),
        }
    }
    normal
}

impl DiskState {
    /// True when the file changed or vanished.
    #[cfg(test)]
    fn is_news(&self) -> bool {
        !matches!(self, DiskState::Unchanged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::text_file::{Document, Opened};
    use crate::remote::testing::mirror_of;
    use crate::test_support::TempFolder;

    #[test]
    fn finds_image_links() {
        let text = "![cat](images/cat.png) and ![dog](<my dog.jpg> \"Dog\")\n\
            ![t](pics/a.gif \"title\") ![web](https://x.org/a.png)\n<img alt=\"x\" src='html.webp'>";
        assert_eq!(
            image_links(text),
            ["images/cat.png", "my dog.jpg", "pics/a.gif", "https://x.org/a.png", "html.webp"]
        );
    }

    #[test]
    fn resolves_relative_image_paths_only() {
        let folder = Path::new("/m/host/notes");
        assert_eq!(image_paths(folder, "images/cat.png"), [PathBuf::from("/m/host/notes/images/cat.png")]);
        assert_eq!(image_paths(folder, "../shared/a.png?v=2"), [PathBuf::from("/m/host/shared/a.png")]);
        assert_eq!(
            image_paths(folder, "my%20cat.png"),
            [PathBuf::from("/m/host/notes/my%20cat.png"), PathBuf::from("/m/host/notes/my cat.png")]
        );
        assert_eq!(image_paths(folder, "https://x.org/a.png"), Vec::<PathBuf>::new());
        assert_eq!(image_paths(folder, "/etc/a.png"), Vec::<PathBuf>::new());
        assert_eq!(image_paths(folder, "notes.md"), Vec::<PathBuf>::new());
    }

    #[test]
    fn downloads_images_into_the_mirror() {
        let temp = TempFolder::new();
        temp.write("notes/images/cat.png", "meow");
        let note = mirror_of("img-host", &temp.path().join("notes/a.md"));
        let folder = note.parent().unwrap();
        let wanted: Vec<PathBuf> =
            ["images/cat.png", "images/missing.png"].iter().flat_map(|url| image_paths(folder, url)).collect();
        let written = download_images(&wanted);
        assert_eq!(written, [folder.join("images/cat.png")]);
        assert_eq!(std::fs::read(folder.join("images/cat.png")).unwrap(), b"meow");
    }

    #[test]
    fn remote_document_notices_outside_changes() {
        let temp = TempFolder::new();
        let real = temp.write("a.md", "first");
        let mirror = mirror_of("doc-host", &real);
        let Opened::Text(mut document) = Document::open(&mirror).unwrap() else {
            panic!("text expected");
        };
        assert_eq!(document.text, "first");
        assert!(!document.disk_state().is_news());

        document.text.push_str(" edit");
        document.on_text_changed();
        document.save().unwrap();
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "first edit");
        assert!(!document.disk_state().is_news());

        std::fs::write(&real, "changed elsewhere").unwrap();
        assert_eq!(document.disk_state(), DiskState::Modified);
        assert_eq!(document.read_disk_text().unwrap(), "changed elsewhere");
        document.accept_disk_text("changed elsewhere".to_string());
        assert!(!document.disk_state().is_news());

        std::fs::remove_file(&real).unwrap();
        assert_eq!(document.disk_state(), DiskState::Deleted);
        // No answer from the host is no news, not "deleted".
        let unreachable = document.disk_state_from(Err(io::Error::other("timeout")));
        assert_eq!(unreachable, DiskState::Unchanged);
    }
}
