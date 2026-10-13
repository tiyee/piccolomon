//! Original-byte storage, using exclusive same-directory temporary files.
use editable_markdown_parser::Source;
use ryne_extensions::FileStore;
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
    fn write(&self, path: &Path, source: &Source) -> std::io::Result<()> {
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
            std::fs::rename(&temp, path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result
    }
}
