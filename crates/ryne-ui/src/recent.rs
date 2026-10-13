//! A bounded, non-document preference. JSON preserves paths containing newlines.
use std::{
    io,
    path::{Path, PathBuf},
};
#[derive(Default)]
pub(super) struct RecentFiles {
    pub paths: Vec<PathBuf>,
    storage: Option<PathBuf>,
}
impl RecentFiles {
    pub fn load(storage: Option<PathBuf>) -> Self {
        let paths = storage
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .filter(|bytes| bytes.len() <= 256 * 1024)
            .and_then(|bytes| serde_json::from_slice::<Vec<PathBuf>>(&bytes).ok())
            .unwrap_or_default();
        let mut recent = Self {
            paths: Vec::new(),
            storage,
        };
        for path in paths.into_iter().take(10).rev() {
            recent.remember(&path);
        }
        recent
    }
    pub fn remember(&mut self, path: &Path) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.paths.retain(|p| p != &path);
        self.paths.insert(0, path);
        self.paths.truncate(10);
    }
    pub fn persist(&self) -> io::Result<()> {
        let Some(storage) = &self.storage else {
            return Ok(());
        };
        if let Some(parent) = storage.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec(&self.paths)?;
        let temporary = storage.with_extension(format!("{}.tmp", std::process::id()));
        let result =
            std::fs::write(&temporary, bytes).and_then(|()| std::fs::rename(&temporary, storage));
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_deduplicated_history_round_trips_unicode_and_newlines() {
        let storage = std::env::temp_dir().join(format!("ryne-recent-{}.json", std::process::id()));
        let mut recent = RecentFiles::load(Some(storage.clone()));
        for n in 0..14 {
            recent.remember(&PathBuf::from(format!("/nonexistent/{n}中文\n.md")));
        }
        let newest = recent.paths[0].clone();
        recent.remember(&newest);
        assert_eq!(recent.paths.len(), 10);
        recent.persist().unwrap();
        assert_eq!(RecentFiles::load(Some(storage.clone())).paths, recent.paths);
        std::fs::remove_file(storage).unwrap();
    }
}
