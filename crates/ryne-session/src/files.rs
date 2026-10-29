//! Serial save requests bind the exact snapshot written by the file service.
use editable_markdown_parser::Source;
use ryne_extensions::FileStore;
use std::{path::PathBuf, sync::Arc};
#[derive(Default)]
pub struct FileSession {
    pub path: Option<PathBuf>,
    pub saving: bool,
    pub(crate) binding: Option<(PathBuf, PathBuf)>,
    pub(crate) active: Option<u64>,
    pub(crate) queued: Option<SaveRequest>,
    pub(crate) next: u64,
}
pub struct SaveRequest {
    pub(crate) id: u64,
    pub(crate) snapshot: Source,
    pub(crate) path: PathBuf,
    pub(crate) store: Arc<dyn FileStore>,
    pub(crate) target: Option<PathBuf>,
}
pub struct SaveReceipt {
    pub(crate) id: u64,
    pub(crate) snapshot: Source,
    pub(crate) path: PathBuf,
    pub(crate) result: std::io::Result<PathBuf>,
}
pub struct SaveCompletion {
    pub accepted: bool,
    pub error: Option<String>,
    pub next: Option<SaveRequest>,
    pub clean: bool,
}
impl SaveRequest {
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
