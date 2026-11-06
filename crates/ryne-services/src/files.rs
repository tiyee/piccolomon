//! Original-byte storage, using exclusive same-directory temporary files.
use ryne_extensions::FileStore;
use ryne_parser::Source;
use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
#[derive(Default)]
pub struct AtomicFileStore;
impl FileStore for AtomicFileStore {
    fn read(&self, path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }
    fn resolve_path(&self, path: &Path) -> std::io::Result<std::path::PathBuf> {
        // Only a missing leaf may be created. A dangling symlink is an error.
        match std::fs::symlink_metadata(path) {
            Ok(_) => std::fs::canonicalize(path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let parent = path
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new("."));
                let name = path
                    .file_name()
                    .ok_or_else(|| std::io::Error::other("missing file name"))?;
                Ok(std::fs::canonicalize(parent)?.join(name))
            }
            Err(error) => Err(error),
        }
    }
    fn write(&self, path: &Path, source: &Source) -> std::io::Result<()> {
        self.write_atomic(path, None, source)
    }
    fn write_bound(&self, path: &Path, target: &Path, source: &Source) -> std::io::Result<()> {
        self.write_atomic(path, Some(target), source)
    }
}
impl AtomicFileStore {
    fn write_atomic(
        &self,
        display_path: &Path,
        expected: Option<&Path>,
        source: &Source,
    ) -> std::io::Result<()> {
        let target = self.resolve_path(display_path)?;
        if expected.is_some_and(|expected| expected != target) {
            return Err(std::io::Error::other("file target changed"));
        }
        let path = target.as_path();
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let temp = parent.join(format!(
            ".ryne-{}-{}-{}.tmp",
            std::process::id(),
            source.revision(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        let result = (|| {
            use std::io::Write;
            file.write_all(source.text().as_bytes())?;
            if let Ok(metadata) = std::fs::metadata(path) {
                file.set_permissions(metadata.permissions())?;
            }
            file.sync_all()?;
            if self.resolve_path(display_path)? != target {
                return Err(std::io::Error::other("file target changed during save"));
            }
            // Close before rename/cleanup, including on Windows.
            Ok(())
        })();
        drop(file);
        let result = result.and_then(|()| std::fs::rename(&temp, path));
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result
    }
}

/// Binary export adapter, independent of Markdown document identity and history.
pub struct AtomicArtifactStore;
impl ryne_extensions::ArtifactStore for AtomicArtifactStore {
    fn write(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let temporary = parent.join(format!(
            ".ryne-artifact-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let result = file.write_all(bytes).and_then(|()| file.sync_all());
        drop(file);
        let result = result.and_then(|()| std::fs::rename(&temporary, path));
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }
}

#[cfg(test)]
mod artifact_tests {
    use super::*;
    use ryne_extensions::ArtifactStore;
    #[test]
    fn replaces_binary_output_and_cleans_up_failed_rename() {
        let dir = std::env::temp_dir().join(format!("ryne-artifact-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("output.pdf");
        AtomicArtifactStore.write(&path, b"first").unwrap();
        AtomicArtifactStore.write(&path, b"\0\xffsecond").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"\0\xffsecond");
        let destination = dir.join("directory");
        std::fs::create_dir(&destination).unwrap();
        assert!(AtomicArtifactStore.write(&destination, b"fail").is_err());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
