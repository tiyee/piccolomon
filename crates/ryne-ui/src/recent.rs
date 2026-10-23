//! A bounded, non-document preference. JSON preserves paths containing newlines.
use std::{
    io,
    path::{Path, PathBuf},
};
const MAX_RECENT_FILES: usize = 10;

fn normalized_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

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
        // Stored paths are newest first. Deduplicate before limiting the list so
        // repeated entries do not displace older, distinct files on startup.
        for path in paths {
            let path = normalized_path(&path);
            if !recent.paths.contains(&path) {
                recent.paths.push(path);
                if recent.paths.len() == MAX_RECENT_FILES {
                    break;
                }
            }
        }
        recent
    }
    pub fn remember(&mut self, path: &Path) {
        let path = normalized_path(path);
        // A previously missing path may now resolve to an existing file or link.
        self.paths.retain(|p| normalized_path(p) != path);
        self.paths.insert(0, path);
        self.paths.truncate(MAX_RECENT_FILES);
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "ryne-recent-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn bounded_deduplicated_history_round_trips_unicode_and_newlines() {
        let directory = TestDirectory::new();
        let storage = directory.0.join("recent.json");
        let mut recent = RecentFiles::load(Some(storage.clone()));
        for n in 0..14 {
            recent.remember(&directory.0.join(format!("{n}中文\n.md")));
        }
        let expected = (4..14)
            .rev()
            .map(|n| normalized_path(&directory.0.join(format!("{n}中文\n.md"))))
            .collect::<Vec<_>>();
        assert_eq!(recent.paths, expected);
        let revisited = recent.paths[5].clone();
        recent.remember(&revisited);
        let mut expected = expected;
        expected.retain(|path| path != &revisited);
        expected.insert(0, revisited);
        assert_eq!(recent.paths, expected);
        recent.persist().unwrap();
        assert_eq!(RecentFiles::load(Some(storage.clone())).paths, recent.paths);
    }

    #[test]
    fn loading_deduplicates_before_taking_ten_newest_distinct_paths() {
        let directory = TestDirectory::new();
        let storage = directory.0.join("recent.json");
        let paths = (0..14)
            .flat_map(|n| vec![directory.0.join(format!("{n}.md")); 2])
            .collect::<Vec<_>>();
        std::fs::write(&storage, serde_json::to_vec(&paths).unwrap()).unwrap();

        let recent = RecentFiles::load(Some(storage));
        let expected = (0..10)
            .map(|n| normalized_path(&directory.0.join(format!("{n}.md"))))
            .collect::<Vec<_>>();
        assert_eq!(recent.paths, expected);
    }

    #[test]
    fn path_identity_keeps_same_named_files_in_different_directories() {
        let directory = TestDirectory::new();
        let first = directory.0.join("first/note.md");
        let second = directory.0.join("second/note.md");
        let mut recent = RecentFiles::default();
        recent.remember(&first);
        recent.remember(&second);
        recent.remember(&first);
        assert_eq!(
            recent.paths,
            vec![normalized_path(&first), normalized_path(&second)]
        );
    }

    #[test]
    fn missing_relative_paths_are_stored_as_absolute_paths() {
        let path = PathBuf::from("ryne-recent-missing/note.md");
        let mut recent = RecentFiles::default();
        recent.remember(&path);
        recent.remember(&std::env::current_dir().unwrap().join(&path));
        assert_eq!(recent.paths, vec![std::path::absolute(path).unwrap()]);
    }

    #[cfg(unix)]
    #[test]
    fn aliases_of_an_existing_file_share_one_recent_entry() {
        let directory = TestDirectory::new();
        let file = directory.0.join("note.md");
        let alias = directory.0.join("alias.md");
        let mut recent = RecentFiles::default();
        // Re-evaluate older entries when a formerly missing path becomes a link.
        recent.remember(&alias);
        std::fs::write(&file, "# Note").unwrap();
        std::os::unix::fs::symlink(&file, &alias).unwrap();
        recent.remember(&file);
        recent.remember(&directory.0.join("./note.md"));
        recent.remember(&alias);
        assert_eq!(recent.paths, vec![std::fs::canonicalize(&file).unwrap()]);

        let storage = directory.0.join("recent.json");
        std::fs::write(&storage, serde_json::to_vec(&vec![alias, file]).unwrap()).unwrap();
        assert_eq!(RecentFiles::load(Some(storage)).paths, recent.paths);
    }
}
