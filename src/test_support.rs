//! Helpers shared by unit tests (compiled only for `cargo test`).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

/// A fresh folder under the system temp dir, deleted on drop.
#[derive(Debug)]
pub struct TempFolder {
    path: PathBuf,
}

impl TempFolder {
    /// Create an empty folder unique to this test process and call.
    pub fn new() -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("rubrique-test-{}-{id}", std::process::id()));
        // Leftover from an earlier crashed run with the same process id; it
        // usually does not exist, so the error is expected and ignored.
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        // Canonical path, so tests compare cleanly on macOS where /var -> /private/var.
        let path = fs::canonicalize(path).unwrap();
        TempFolder { path }
    }

    /// The folder's canonical path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Write a file (creating parent folders) and return its path.
    pub fn write(&self, relative: &str, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.path.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }

    /// Create a folder (and its parents) and return its path.
    pub fn make_folder(&self, relative: &str) -> PathBuf {
        let path = self.path.join(relative);
        fs::create_dir_all(&path).unwrap();
        path
    }
}

impl Drop for TempFolder {
    fn drop(&mut self) {
        // Best-effort cleanup: a leftover temp folder must not fail a test.
        let _ = fs::remove_dir_all(&self.path);
    }
}
