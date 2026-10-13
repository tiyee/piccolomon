//! Serial save requests bind the exact snapshot written by the file service.
use editable_markdown_parser::Source;
use ryne_extensions::FileStore;
use std::{path::PathBuf, sync::Arc};
#[derive(Default)]
pub struct FileSession {
    pub path: Option<PathBuf>,
    pub saving: bool,
    pub(crate) active: Option<u64>,
    pub(crate) queued: Option<SaveRequest>,
    pub(crate) next: u64,
}
pub struct SaveRequest {
    pub(crate) id: u64,
    pub(crate) snapshot: Source,
    pub(crate) path: PathBuf,
    pub(crate) store: Arc<dyn FileStore>,
}
pub struct SaveReceipt {
    pub(crate) id: u64,
    pub(crate) snapshot: Source,
    pub(crate) path: PathBuf,
    pub(crate) result: std::io::Result<()>,
}
pub struct SaveCompletion {
    pub accepted: bool,
    pub error: Option<String>,
    pub next: Option<SaveRequest>,
    pub clean: bool,
}
impl SaveRequest {
    pub fn run(self) -> SaveReceipt {
        let result = self.store.write(&self.path, &self.snapshot);
        SaveReceipt {
            id: self.id,
            snapshot: self.snapshot,
            path: self.path,
            result,
        }
    }
}
