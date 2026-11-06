use super::*;
use ryne_extensions::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Copy)]
enum Outcome {
    Success,
    Error,
    Panic,
}
impl Outcome {
    fn result(self) -> Result<(), String> {
        match self {
            Self::Success => Ok(()),
            Self::Error => Err("injected error".into()),
            Self::Panic => panic!("injected service panic"),
        }
    }
}
struct Files(Outcome);
impl FileStore for Files {
    fn read(&self, _: &std::path::Path) -> std::io::Result<String> {
        self.0.result().map_err(std::io::Error::other)?;
        Ok("loaded\r\n中😀".into())
    }
    fn write(&self, _: &std::path::Path, _: &ryne_parser::Source) -> std::io::Result<()> {
        Ok(())
    }
}
struct Exporter(Outcome);
impl PdfExporter for Exporter {
    fn export(&self, _: &ryne_projection::Display) -> Result<Vec<u8>, String> {
        self.0.result()?;
        Ok(b"pdf".to_vec())
    }
}
struct Artifacts(Outcome, Arc<AtomicUsize>);
impl ArtifactStore for Artifacts {
    fn write(&self, _: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
        assert_eq!(bytes, b"pdf");
        self.1.fetch_add(1, Ordering::SeqCst);
        self.0.result().map_err(std::io::Error::other)
    }
}
struct Updates(Outcome);
impl UpdateChecker for Updates {
    fn check(&self, _: &str) -> Result<UpdateStatus, String> {
        self.0.result()?;
        Ok(UpdateStatus::Current)
    }
}
fn services(outcome: Outcome) -> ryne_session::SessionServices {
    ryne_session::SessionServices {
        features: Arc::new(
            ryne_features::MarkdownFeatures::new(
                Arc::new(ryne_services::math::FormularyRenderer::default()),
                vec![],
            )
            .unwrap(),
        ),
        files: Arc::new(Files(outcome)),
    }
}

#[test]
fn open_and_update_panics_return_receipts_and_allow_retry() {
    let mut open = OperationSlot::default();
    let mut updates = OperationSlot::default();
    for outcome in [Outcome::Panic, Outcome::Error, Outcome::Success] {
        let token = open.begin().unwrap();
        assert!(open.begin().is_none());
        let receipt =
            std::thread::spawn(move || token.open(Some("virtual.md".into()), services(outcome)))
                .join()
                .unwrap();
        let result = open.complete(receipt).unwrap();
        assert_eq!(result.is_ok(), matches!(outcome, Outcome::Success));
        assert!(!open.is_pending());
        if let Ok(doc) = result {
            assert_eq!(doc.source.text(), "loaded\r\n中😀");
        }
        let token = updates.begin().unwrap();
        let receipt =
            std::thread::spawn(move || token.check_updates(Arc::new(Updates(outcome)), "0.2.5"))
                .join()
                .unwrap();
        assert_eq!(
            updates.complete(receipt).unwrap().is_ok(),
            matches!(outcome, Outcome::Success)
        );
        assert!(!updates.is_pending());
    }
}

#[test]
fn export_and_artifact_failures_leave_live_document_unchanged_and_release_slot() {
    let mut doc = ryne_session::EditorSession::new("before", services(Outcome::Success));
    doc.insert_text(6..6, " 中😀").unwrap();
    let source = doc.source.id();
    let selection = doc.selection();
    let history = doc.history_len(false);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut slot = OperationSlot::default();
    for (export, write, ok, wrote) in [
        (Outcome::Panic, Outcome::Success, false, false),
        (Outcome::Error, Outcome::Success, false, false),
        (Outcome::Success, Outcome::Panic, false, true),
        (Outcome::Success, Outcome::Error, false, true),
        (Outcome::Success, Outcome::Success, true, true),
    ] {
        let token = slot.begin().unwrap();
        let snapshot = doc.export_snapshot();
        let before = calls.load(Ordering::SeqCst);
        let artifacts = Arc::new(Artifacts(write, calls.clone()));
        let receipt = std::thread::spawn(move || {
            token.export(
                snapshot,
                Arc::new(Exporter(export)),
                artifacts,
                "virtual.pdf".into(),
            )
        })
        .join()
        .unwrap();
        assert_eq!(slot.complete(receipt).unwrap().is_ok(), ok);
        assert!(!slot.is_pending());
        assert_eq!(calls.load(Ordering::SeqCst) - before, usize::from(wrote));
        assert_eq!(doc.source.id(), source);
        assert_eq!(doc.selection(), selection);
        assert_eq!(doc.history_len(false), history);
        assert!(doc.dirty());
    }
}

#[test]
fn cancellation_and_stale_receipts_cannot_clear_another_operation() {
    let mut slot = OperationSlot::default();
    let old = slot.begin().unwrap();
    assert!(slot.finish(old));
    let current = slot.begin().unwrap();
    assert!(slot.complete(old.run("late", || Ok(()))).is_none());
    assert!(slot.is_pending());
    let mut other = OperationSlot::default();
    let foreign = other.begin().unwrap();
    assert!(!slot.finish(foreign));
    assert_eq!(
        slot.complete(current.run("current", || Ok(42))),
        Some(Ok(42))
    );
    assert!(!slot.finish(current));
    assert!(other.finish(foreign));
}
