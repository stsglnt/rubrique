//! Unsaved edits kept as drafts: written a few seconds after the last edit
//! and on quit, put back into their tabs on the next start, and deleted once
//! the document is clean or its tab is closed.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui::Context;

use super::{OpenFile, RubriqueApp};
use crate::document::draft::{self, Draft, DraftStore, Restore};
use crate::document::text_file::{DiskState, Document};
use crate::explorer::paths::name_text;
use crate::settings::drafts_folder;

/// How long after the last edit the draft is written (crash safety).
const DRAFT_DELAY_SECONDS: f64 = 3.0;

/// Bookkeeping for the drafts folder.
#[derive(Debug)]
pub(super) struct DraftState {
    /// `None` when there is no config folder.
    store: Option<DraftStore>,
    /// Drafts on disk, by [`draft::key`], with the revision they hold.
    written: HashMap<PathBuf, u64>,
    /// The active document's revision in the last frame.
    last_revision: u64,
    /// egui time of the last edit seen.
    changed_at: f64,
}

impl Default for DraftState {
    fn default() -> Self {
        DraftState {
            store: drafts_folder().map(DraftStore::new),
            written: HashMap::new(),
            last_revision: 0,
            changed_at: 0.0,
        }
    }
}

/// How one draft came back at start.
enum Restored {
    Continued,
    /// The file changed on disk since; the tab asks to merge.
    Merge,
    /// The file is gone; the tab holds the edits.
    Missing(PathBuf),
}

impl RubriqueApp {
    /// Reopen last session's tabs, with their drafts, then the tabs that
    /// only have a draft (opened after the last clean quit, before a crash).
    pub(super) fn reopen_tabs(&mut self, saved_tabs: Vec<PathBuf>) {
        let mut drafts: HashMap<PathBuf, Draft> = match &self.drafts.store {
            Some(store) => store.all().into_iter().map(|draft| (draft.path.clone(), draft)).collect(),
            None => HashMap::new(),
        };
        let mut restored = Vec::new();
        for path in saved_tabs {
            if !self.roots.iter().any(|root| path.starts_with(&root.path)) {
                continue;
            }
            match drafts.remove(&draft::key(&path)) {
                Some(found) => restored.extend(self.open_with_draft(found)),
                None if path.is_file() => self.open_in_tab(path),
                None => {}
            }
        }
        let remote_on = self.settings.remote.enabled;
        let mut orphans: Vec<Draft> =
            drafts.into_values().filter(|draft| remote_on || !crate::remote::is_mirror(&draft.path)).collect();
        orphans.sort_by(|a, b| a.path.cmp(&b.path));
        for found in orphans {
            restored.extend(self.open_with_draft(found));
        }
        self.report_restored(&restored);
    }

    /// Open the draft's file in a tab with the edits put back. `None` when
    /// there was nothing to restore (the draft is then deleted).
    fn open_with_draft(&mut self, found: Draft) -> Option<Restored> {
        let path = found.path.clone();
        if !path.exists() {
            let document = Document::missing(&path, found.text, found.base_text);
            self.remember_draft(&document);
            self.add_tab(OpenFile::Text {
                document,
                preview: None,
                disk_notice: Some(DiskState::Deleted),
                editor: None,
                source: false,
            });
            return Some(Restored::Missing(path));
        }
        self.open_in_tab(path.clone());
        let OpenFile::Text { document, disk_notice, .. } = &mut self.open_file else {
            // Not text any more: leave the draft alone rather than lose it.
            return None;
        };
        if document.path != path {
            return None;
        }
        let outcome = draft::decide(&found, Some(&document.text));
        let restored = match outcome {
            Restore::Nothing => None,
            Restore::Continue => Some(Restored::Continued),
            Restore::Merge => {
                *disk_notice = Some(DiskState::Modified);
                Some(Restored::Merge)
            }
            Restore::Missing => unreachable!("the file was just read"),
        };
        if restored.is_some() {
            document.restore_edits(found.text, found.base_text);
            let key = draft::key(&document.path);
            let revision = document.revision();
            self.drafts.written.insert(key, revision);
        } else if let Some(store) = &self.drafts.store {
            // Best effort: a draft equal to the file is harmless if it stays.
            let _ = store.remove(&path);
        }
        restored
    }

    /// Note that the draft on disk holds `document` as it is now.
    fn remember_draft(&mut self, document: &Document) {
        self.drafts.written.insert(draft::key(&document.path), document.revision());
    }

    /// Say what came back, and open the merge screen when the file on
    /// screen changed on disk since its edits were kept.
    fn report_restored(&mut self, restored: &[Restored]) {
        if restored.is_empty() {
            return;
        }
        let missing: Vec<String> = restored
            .iter()
            .filter_map(|item| match item {
                Restored::Missing(path) => Some(name_text(path)),
                _ => None,
            })
            .collect();
        let merge = restored.iter().any(|item| matches!(item, Restored::Merge));
        let message = restored_message(restored.len(), &missing, merge);
        self.show_status(message);
        if matches!(&self.open_file, OpenFile::Text { disk_notice: Some(DiskState::Modified), .. }) {
            self.start_merge();
        }
    }

    /// Keep drafts in step with the tabs every frame: deleted at once when
    /// clean or closed, written once edits pause for a few seconds.
    pub(super) fn poll_drafts(&mut self, ctx: &Context) {
        let now = ctx.input(|input| input.time);
        let revision = match &self.open_file {
            OpenFile::Text { document, .. } => document.revision(),
            _ => 0,
        };
        if revision != self.drafts.last_revision {
            self.drafts.last_revision = revision;
            self.drafts.changed_at = now;
        }
        let waited = now - self.drafts.changed_at;
        let write = waited >= DRAFT_DELAY_SECONDS;
        if !write && self.open_file.is_dirty() {
            ctx.request_repaint_after(Duration::from_secs_f64(DRAFT_DELAY_SECONDS - waited));
        }
        if let Err(error) = self.sync_drafts(write) {
            // Try again after another pause rather than every frame.
            self.drafts.changed_at = now;
            self.show_status(format!("Could not keep a draft of unsaved edits: {error}"));
        }
    }

    /// Write every draft that is behind, now (on quit). Says why on failure.
    pub(super) fn keep_drafts(&mut self) -> bool {
        self.finish_block_edit();
        match self.sync_drafts(true) {
            Ok(()) => true,
            Err(error) => {
                self.show_status(format!("Could not keep unsaved edits: {error}"));
                false
            }
        }
    }

    /// Delete drafts of clean or closed documents and, with `write`, write
    /// those of dirty ones that are behind.
    ///
    /// # Errors
    ///
    /// The last draft that could not be written (deleting is best effort).
    fn sync_drafts(&mut self, write: bool) -> io::Result<()> {
        let DraftState { store: Some(store), written, .. } = &mut self.drafts else {
            return Ok(());
        };
        let mut result = Ok(());
        let mut open = HashSet::new();
        let documents = std::iter::once(&self.open_file).chain(self.tabs.iter()).filter_map(|file| match file {
            OpenFile::Text { document, .. } => Some(document),
            _ => None,
        });
        for document in documents {
            let key = draft::key(&document.path);
            if !document.is_dirty() {
                forget(store, written, &key);
            } else if write && written.get(&key) != Some(&document.revision()) {
                match store.write(&Draft::of(document)) {
                    Ok(()) => {
                        written.insert(key.clone(), document.revision());
                    }
                    Err(error) => result = Err(error),
                }
            }
            open.insert(key);
        }
        let closed: Vec<PathBuf> = written.keys().filter(|key| !open.contains(*key)).cloned().collect();
        for key in closed {
            forget(store, written, &key);
        }
        result
    }
}

/// "Restored unsaved edits in 2 files", plus the files that are gone and
/// whether some changed on disk since.
fn restored_message(count: usize, missing: &[String], merge: bool) -> String {
    let files = if count == 1 { "1 file".to_string() } else { format!("{count} files") };
    let mut parts = vec![format!("Restored unsaved edits in {files}")];
    if !missing.is_empty() {
        parts.push(format!("deleted on disk: {}", missing.join(", ")));
    }
    if merge {
        parts.push("changed on disk since, merge to continue".to_string());
    }
    parts.join("; ")
}

/// Delete the draft for `key` if one was written.
fn forget(store: &DraftStore, written: &mut HashMap<PathBuf, u64>, key: &Path) {
    if written.remove(key).is_some()
        && let Err(error) = store.remove(key)
    {
        eprintln!("rubrique: could not delete the draft of {}: {error}", key.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_message_counts_files_and_names_missing_ones() {
        assert_eq!(restored_message(2, &[], false), "Restored unsaved edits in 2 files");
        assert_eq!(restored_message(1, &[], false), "Restored unsaved edits in 1 file");
        assert_eq!(
            restored_message(2, &["a.md".to_string()], true),
            "Restored unsaved edits in 2 files; deleted on disk: a.md; changed on disk since, merge to continue"
        );
    }
}
