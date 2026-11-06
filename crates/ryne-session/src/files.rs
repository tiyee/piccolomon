//! Serial save requests bind the exact snapshot written by the file service.
use ryne_extensions::FileStore;
use ryne_parser::Source;
use std::{path::PathBuf, sync::Arc};
#[derive(Default)]
pub struct FileSession {
    pub path: Option<PathBuf>,
    pub saving: bool,
    pub(crate) binding: Option<(PathBuf, PathBuf)>,
    pub(crate) active: Option<(u64, PathBuf)>,
    pub(crate) queued: Option<SaveRequest>,
    pub(crate) next: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveIntent {
    Save,
    SaveAs,
}
pub struct SaveRequest {
    pub(crate) intent: SaveIntent,
    pub(crate) id: u64,
    pub(crate) snapshot: Source,
    pub(crate) path: PathBuf,
    pub(crate) store: Arc<dyn FileStore>,
    pub(crate) target: Option<PathBuf>,
}
pub struct SaveReceipt {
    pub(crate) intent: SaveIntent,
    pub(crate) id: u64,
    pub(crate) snapshot: Source,
    pub(crate) path: PathBuf,
    pub(crate) result: std::io::Result<PathBuf>,
}
pub struct SaveCompletion {
    pub intent: SaveIntent,
    pub accepted: bool,
    pub error: Option<String>,
    pub next: Option<SaveRequest>,
    pub clean: bool,
}
impl FileSession {
    /// Display path of the write actually in progress (also for Save As).
    pub fn saving_path(&self) -> Option<&std::path::Path> {
        self.active.as_ref().map(|(_, path)| path.as_path())
    }
}
impl SaveRequest {
    pub(crate) fn active_write(&self) -> (u64, PathBuf) {
        (
            self.id,
            std::path::absolute(&self.path).unwrap_or_else(|_| self.path.clone()),
        )
    }
    pub fn run(self) -> SaveReceipt {
        // Always return a terminal receipt so a failed service cannot strand
        // active/saving or prevent the next queued snapshot from being written.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            write_snapshot(
                self.store.as_ref(),
                &self.path,
                self.target.as_deref(),
                &self.snapshot,
            )
        }))
        .unwrap_or_else(|_| Err(std::io::Error::other("file write callback panicked")));
        SaveReceipt {
            intent: self.intent,
            id: self.id,
            snapshot: self.snapshot,
            path: self.path,
            result,
        }
    }
}

pub(super) fn write_snapshot(
    store: &dyn FileStore,
    path: &std::path::Path,
    expected: Option<&std::path::Path>,
    snapshot: &Source,
) -> std::io::Result<PathBuf> {
    let target = store.resolve_path(path)?;
    if expected.is_some_and(|expected| expected != target) {
        return Err(std::io::Error::other(
            "file target changed; reopen or save to another path",
        ));
    }
    store.write_bound(path, &target, snapshot)?;
    Ok(target)
}
