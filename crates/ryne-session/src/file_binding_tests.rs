#![cfg(unix)]
use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "ryne-binding-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn symlink_save_keeps_display_path_target_bytes_and_saved_snapshot() {
    for newline in ["\n", "\r\n", "\r"] {
        let dir = Directory::new();
        let nested = dir.0.join("nested");
        std::fs::create_dir(&nested).unwrap();
        let target = nested.join("target.md");
        let link = dir.0.join("link.md");
        let raw = format!("原😀{newline}tail");
        std::fs::write(&target, &raw).unwrap();
        std::os::unix::fs::symlink("nested/target.md", &link).unwrap();
        let mut doc = EditorSession::open(&link, regression_tests::services()).unwrap();
        doc.replace(0..0, "x").unwrap();
        let request = doc.queue_save(link.clone()).unwrap();
        doc.replace(0..0, "y").unwrap();
        let completion = doc.complete_save(request.run());
        assert!(completion.error.is_none() && !completion.clean);
        assert_eq!(doc.files.path.as_ref(), Some(&link));
        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), format!("x{raw}"));
        doc.save(&link).unwrap();
        assert!(!doc.dirty());
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            format!("yx{raw}")
        );
        assert_eq!(std::fs::read_dir(&nested).unwrap().count(), 1);
    }
}
#[test]
fn redirected_or_dangling_link_is_rejected_without_losing_dirty_and_can_retry() {
    let dir = Directory::new();
    let a = dir.0.join("a.md");
    let b = dir.0.join("b.md");
    let link = dir.0.join("link.md");
    std::fs::write(&a, "a").unwrap();
    std::fs::write(&b, "b").unwrap();
    std::os::unix::fs::symlink(&a, &link).unwrap();
    let mut doc = EditorSession::open(&link, regression_tests::services()).unwrap();
    doc.replace(0..1, "edit").unwrap();
    let request = doc.queue_save(link.clone()).unwrap();
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(&b, &link).unwrap();
    let completion = doc.complete_save(request.run());
    assert!(completion.error.is_some());
    assert!(doc.dirty() && !doc.files.saving);
    assert_eq!(std::fs::read_to_string(&a).unwrap(), "a");
    assert_eq!(std::fs::read_to_string(&b).unwrap(), "b");
    let copy = dir.0.join("copy.md");
    doc.save(&copy).unwrap();
    assert_eq!(std::fs::read_to_string(&copy).unwrap(), "edit");
    let mut doc = EditorSession::open(&link, regression_tests::services()).unwrap();
    doc.replace(0..1, "new").unwrap();
    doc.save(&link).unwrap();
    assert_eq!(std::fs::read_to_string(&b).unwrap(), "new");
    std::fs::remove_file(&b).unwrap();
    assert!(doc.save(&link).is_err());
    assert!(EditorSession::open(&link, regression_tests::services()).is_err());
    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    std::fs::write(&b, "restored").unwrap();
    doc.save(&link).unwrap();
    assert_eq!(std::fs::read_to_string(&b).unwrap(), "new");
}
#[test]
fn direct_atomic_store_preserves_link_and_new_file_parent_resolution() {
    let dir = Directory::new();
    let target = dir.0.join("target.md");
    let link = dir.0.join("link.md");
    std::fs::write(&target, "before").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let store = ryne_services::files::AtomicFileStore;
    store.write(&link, &Source::new("after\r\n")).unwrap();
    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(std::fs::read(&target).unwrap(), b"after\r\n");
    let path = dir.0.join("new.md");
    store.write(&path, &Source::new("new")).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "new");
    let wrong = dir.0.join("wrong.md");
    assert!(store
        .write_bound(&link, &wrong, &Source::new("wrong"))
        .is_err());
    let directory = dir.0.join("directory");
    std::fs::create_dir(&directory).unwrap();
    assert!(store.write(&directory, &Source::new("fail")).is_err());
    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "after\r\n");
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 4);
}
