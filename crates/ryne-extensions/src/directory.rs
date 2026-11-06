//! Read-only browser port. Document opens and saves still use FileStore.
use std::{ffi::OsString, io, path::Path, time::SystemTime};

pub const BATCH: usize = 128;
pub const SCAN_LIMIT: usize = 50_000;
pub const ENTRY_LIMIT: usize = 10_000;
pub const MAX_DEPTH: usize = 32;
pub const PREFIX_LIMIT: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntryKind {
    Directory,
    DirectoryLink,
    Document,
}

#[derive(Clone, Debug)]
pub struct DirectoryEntry {
    pub name: OsString,
    pub kind: EntryKind,
    pub modified: Option<SystemTime>,
    pub bytes: Option<u64>,
}
#[derive(Clone, Copy)]
pub struct ScanRequest<'a> {
    pub root: &'a Path,
    pub relative: &'a Path,
    pub current: Option<&'a Path>,
    pub limit: usize,
}
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanStatus {
    #[default]
    Complete,
    Truncated,
    Cancelled,
}
#[derive(Default, Clone, Copy, Debug)]
pub struct ScanEnd {
    pub examined: usize,
    pub status: ScanStatus,
}
pub trait DirectoryBrowser: Send + Sync {
    fn enumerate(
        &self,
        request: ScanRequest<'_>,
        cancelled: &dyn Fn() -> bool,
        emit: &mut dyn FnMut(Vec<DirectoryEntry>) -> bool,
    ) -> io::Result<ScanEnd>;
    fn prefix(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn metadata(&self, path: &Path) -> io::Result<DirectoryEntry>;
}
