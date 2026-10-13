use super::regression_tests::{new_document, services};
use super::*;
use editable_markdown_parser::Edit;
use ryne_core::PreparedEdit;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
struct Reject {
    calls: Arc<AtomicUsize>,
    reject: bool,
}
impl Validator for Reject {
    fn validate(&self, _: &EditingState, _: &CommandPlan) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.reject {
            Err("reject".into())
        } else {
            Ok(())
        }
    }
}
struct Record(Mutex<Vec<(u64, String)>>);
impl Observer for Record {
    fn observe(&self, e: &SessionEvent, s: &EditingState) {
        assert_eq!(s.source.id(), s.parse.source().id());
        self.0
            .lock()
            .unwrap()
            .push((e.sequence, s.source.text().into()));
    }
}
#[test]
fn all_validators_run_and_rejection_leaves_the_entire_session_unchanged() {
    let mut doc = new_document("**a**");
    doc.select(3, false, false);
    let before = doc.source.id();
    let selection = doc.selection();
    let calls = Arc::new(AtomicUsize::new(0));
    let record = Arc::new(Record(Mutex::new(Vec::new())));
    doc.add_observer(record.clone());
    for reject in [true, false, true] {
        doc.add_validator(Arc::new(Reject {
            calls: calls.clone(),
            reject,
        }));
    }
    assert!(doc.insert_text(selection.clone(), "中😀").is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(doc.source.id(), before);
    assert_eq!(doc.selection(), selection);
    assert_eq!(doc.history_len(false), 0);
    assert!(!doc.dirty());
    assert!(record.0.lock().unwrap().is_empty());
}
#[test]
fn cross_document_and_stale_plans_are_rejected_and_notifications_follow_publication() {
    let mut doc = new_document("a");
    let record = Arc::new(Record(Mutex::new(Vec::new())));
    doc.add_observer(record.clone());
    let state = doc.state.clone();
    let stale = doc
        .services
        .features
        .plan(
            EditIntent::Insert {
                origin: InputOrigin::Keyboard,
                range: ryne_core::SourceRange::new(state.source.id(), 1..1),
                text: "b".into(),
            },
            &state,
        )
        .unwrap();
    doc.insert_text(1..1, "c").unwrap();
    let key = doc.source.id();
    assert!(doc.publish_plan(stale).is_err());
    assert_eq!(doc.source.id(), key);
    let other = new_document("a");
    let foreign = other
        .services
        .features
        .plan(
            EditIntent::Insert {
                origin: InputOrigin::Keyboard,
                range: ryne_core::SourceRange::new(other.source.id(), 1..1),
                text: "x".into(),
            },
            &other.state,
        )
        .unwrap();
    assert!(doc.publish_plan(foreign).is_err());
    let mut invalid = doc.state.clone();
    invalid.head = usize::MAX;
    assert!(
        doc.publish_plan(CommandPlan::Select {
            base: key,
            state: invalid,
            handled: true
        })
        .is_err()
    );
    assert_eq!(record.0.lock().unwrap().as_slice(), [(1, "ac".into())]);
}
#[test]
fn empty_features_support_literal_editing_and_navigation_without_content_history() {
    let mut injection = services();
    injection.features = Arc::new(basic::SourceEditing);
    let mut doc = EditorSession::new("**源码**\r\n", injection);
    assert_eq!(doc.display().text, doc.source.text());
    doc.select(2, false, false);
    assert_eq!(doc.history_len(false), 0);
    assert!(!doc.dirty());
    doc.insert_text(2..2, "中😀").unwrap();
    assert_eq!(doc.source.text(), "**中😀源码**\r\n");
    doc.undo(false);
    assert_eq!(doc.source.text(), "**源码**\r\n");
    assert!(!doc.dirty());
}
#[test]
fn invalid_multi_edit_and_selection_plans_never_partially_update() {
    let mut doc = new_document("a中\r\nb");
    let before = doc.source.id();
    assert!(
        PreparedEdit::prepare(
            &doc.source,
            vec![
                Edit {
                    range: 0..1,
                    replacement: "x".into()
                },
                Edit {
                    range: 2..3,
                    replacement: "y".into()
                }
            ]
        )
        .is_err()
    );
    assert_eq!(doc.source.id(), before);
    let prepared = PreparedEdit::prepare(
        &doc.source,
        vec![Edit {
            range: 0..1,
            replacement: "x".into(),
        }],
    )
    .unwrap();
    let mut state = EditingState::new(prepared.applied().source.clone());
    state.head = 2;
    assert!(
        doc.publish_plan(CommandPlan::Edit {
            prepared,
            state,
            handled: true
        })
        .is_err()
    );
    assert_eq!(doc.source.id(), before);
    assert_eq!(doc.history_len(false), 0);
    let mut invalid_checkpoint = doc.state.clone();
    invalid_checkpoint.closers.push(2..3); // Inside the UTF-8 character.
    assert!(
        doc.publish_plan(CommandPlan::Select {
            base: before,
            state: invalid_checkpoint,
            handled: true,
        })
        .is_err()
    );
    let mut stale_reveal = doc.state.clone();
    stale_reveal.reveal =
        Some(editable_markdown_parser::Session::new(&Source::new("a"), 0..1).unwrap());
    assert!(
        doc.publish_plan(CommandPlan::View {
            base: before,
            state: stale_reveal,
            handled: true,
        })
        .is_err()
    );
    assert_eq!(doc.source.id(), before);
    assert!(!doc.dirty());
}
struct MemoryFiles {
    writes: Mutex<Vec<(String, String)>>,
    fail: bool,
}
impl FileStore for MemoryFiles {
    fn read(&self, _: &Path) -> std::io::Result<String> {
        Ok(String::new())
    }
    fn write(&self, path: &Path, source: &Source) -> std::io::Result<()> {
        if self.fail {
            return Err(std::io::Error::other("write failure"));
        }
        self.writes
            .lock()
            .unwrap()
            .push((path.display().to_string(), source.text().into()));
        Ok(())
    }
}
#[test]
fn saving_during_editing_and_queued_saves_use_written_snapshots_and_serial_order() {
    let memory = Arc::new(MemoryFiles {
        writes: Mutex::new(Vec::new()),
        fail: false,
    });
    let mut injection = services();
    injection.files = memory.clone();
    let mut doc = EditorSession::new("a\r\n", injection);
    doc.insert_text(0..0, "1").unwrap();
    let first = doc.queue_save("first.md".into()).unwrap();
    doc.insert_text(0..0, "2").unwrap();
    assert!(doc.queue_save("second.md".into()).is_none());
    doc.insert_text(0..0, "3").unwrap();
    assert!(doc.queue_save("third.md".into()).is_none());
    let receipt = first.run();
    let completed = doc.complete_save(receipt);
    assert!(completed.accepted);
    assert!(!completed.clean);
    assert!(doc.files.saving);
    let next = completed.next.unwrap();
    assert_eq!(doc.files.path.as_deref(), Some(Path::new("first.md")));
    let completed = doc.complete_save(next.run());
    assert!(completed.clean);
    assert!(!doc.files.saving);
    assert_eq!(doc.files.path.as_deref(), Some(Path::new("third.md")));
    assert_eq!(
        *memory.writes.lock().unwrap(),
        [
            ("first.md".into(), "1a\r\n".into()),
            ("third.md".into(), "321a\r\n".into())
        ]
    );
    doc.undo(false);
    assert!(doc.dirty());
    doc.undo(true);
    assert!(!doc.dirty());
}
#[test]
fn failed_and_foreign_save_receipts_cannot_change_the_baseline() {
    let mut injection = services();
    injection.files = Arc::new(MemoryFiles {
        writes: Mutex::new(Vec::new()),
        fail: true,
    });
    let mut doc = EditorSession::new("a", injection.clone());
    doc.insert_text(1..1, "b").unwrap();
    let request = doc.queue_save("out.md".into()).unwrap();
    let completion = doc.complete_save(request.run());
    assert!(completion.error.is_some());
    assert!(doc.dirty());
    assert!(doc.files.path.is_none());
    let mut other = EditorSession::new("a", injection);
    let foreign = other.queue_save("out.md".into()).unwrap().run();
    let _live = doc.queue_save("out.md".into()).unwrap();
    assert!(!doc.complete_save(foreign).accepted);
    assert!(doc.files.saving);
    assert!(doc.dirty());
}

#[test]
fn foreign_input_range_is_rejected_even_with_a_current_planning_context() {
    let mut doc = new_document("中文");
    let other = new_document("中文");
    let before = doc.source.id();
    assert!(
        doc.dispatch(EditIntent::Insert {
            range: ryne_core::SourceRange::new(other.source.id(), 0..0),
            text: "x".into(),
            origin: InputOrigin::Paste,
        })
        .is_err()
    );
    assert_eq!(doc.source.id(), before);
    assert_eq!(doc.history_len(false), 0);
    assert!(!doc.dirty());
}

#[test]
fn failing_observer_cannot_rollback_a_published_transaction_or_break_undo() {
    struct Broken;
    impl Observer for Broken {
        fn observe(&self, _: &SessionEvent, _: &EditingState) {
            panic!("observer failure");
        }
    }
    let mut doc = new_document("a");
    doc.add_observer(Arc::new(Broken));
    doc.insert_text(1..1, "中😀").unwrap();
    assert_eq!(doc.source.text(), "a中😀");
    assert_eq!(doc.history_len(false), 1);
    assert_eq!(doc.diagnostics.len(), 1);
    doc.undo(false);
    assert_eq!(doc.source.text(), "a");
    assert!(!doc.dirty());
    assert_eq!(doc.diagnostics.len(), 2);
}
