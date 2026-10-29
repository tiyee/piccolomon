use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
struct Policy {
    reject: AtomicBool,
    panic: AtomicBool,
    calls: AtomicUsize,
    input_only: bool,
}
impl Validator for Policy {
    fn validate(&self, _: &EditingState, _: &CommandPlan) -> Result<(), String> {
        assert!(!self.panic.load(SeqCst), "injected policy panic");
        if self.reject.load(SeqCst) {
            Err("blocked".into())
        } else {
            Ok(())
        }
    }
    fn validate_history(
        &self,
        before: &EditingState,
        plan: &CommandPlan,
        _: HistoryDirection,
    ) -> Result<(), String> {
        self.calls.fetch_add(1, SeqCst);
        if self.input_only {
            Ok(())
        } else {
            self.validate(before, plan)
        }
    }
}
fn fingerprint(doc: &EditorSession) -> String {
    format!(
        "{:?}|{}|{}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        doc.source.id(),
        doc.dirty(),
        doc.history_len(false),
        doc.history_len(true),
        doc.selection(),
        doc.closers,
        doc.reveal,
        doc.heading_reveal,
        doc.block_source,
        doc.composition()
    )
}
#[test]
fn history_policy_rejection_and_panic_are_atomic_and_retryable() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw = format!("**中😀**{newline}tail");
        let mut doc = regression_tests::new_document(&raw);
        doc.select(raw.len(), false, false);
        doc.insert_text(raw.len()..raw.len(), "x").unwrap();
        doc.set_block_source(Some(0..doc.source.text().len()));
        let policy = Arc::new(Policy {
            reject: AtomicBool::new(true),
            panic: AtomicBool::new(false),
            calls: AtomicUsize::new(0),
            input_only: false,
        });
        doc.add_validator(policy.clone());
        for redo in [false, true] {
            let before = fingerprint(&doc);
            assert!(doc.try_undo(redo).is_err());
            assert_eq!(fingerprint(&doc), before);
            policy.panic.store(true, SeqCst);
            assert!(doc.try_undo(redo).is_err());
            assert_eq!(fingerprint(&doc), before);
            policy.panic.store(false, SeqCst);
            policy.reject.store(false, SeqCst);
            assert!(doc.try_undo(redo).unwrap());
            policy.reject.store(true, SeqCst);
        }
        assert_eq!(policy.calls.load(SeqCst), 6);
        assert_eq!(doc.source.text(), raw + "x");
    }
}
#[test]
fn default_validation_applies_to_history_and_input_only_policy_can_opt_out() {
    struct DefaultReject;
    impl Validator for DefaultReject {
        fn validate(&self, _: &EditingState, _: &CommandPlan) -> Result<(), String> {
            Err("default blocks history".into())
        }
    }
    let mut doc = regression_tests::new_document("plain");
    assert!(!doc.try_undo(false).unwrap());
    doc.replace(0..0, "x").unwrap();
    doc.add_validator(Arc::new(DefaultReject));
    let before = fingerprint(&doc);
    doc.undo(false);
    assert_eq!(fingerprint(&doc), before);
    assert!(doc
        .diagnostics
        .entries()
        .iter()
        .any(|d| d.feature == "history"));
    let mut doc = regression_tests::new_document("plain");
    doc.replace(0..0, "x").unwrap();
    doc.add_validator(Arc::new(Policy {
        reject: AtomicBool::new(true),
        panic: AtomicBool::new(false),
        calls: AtomicUsize::new(0),
        input_only: true,
    }));
    assert!(doc.replace(0..0, "bad").is_err());
    assert!(doc.try_undo(false).unwrap());
    assert_eq!(doc.source.text(), "plain");
    assert!(doc.try_undo(true).unwrap());
    doc.update_composition(None, "临", None).unwrap();
    let before = fingerprint(&doc);
    assert!(doc.try_undo(false).is_err());
    assert_eq!(fingerprint(&doc), before);
}
struct Corrupt(Arc<AtomicBool>);
impl CommandHandler for Corrupt {
    fn plan(
        &self,
        intent: &EditIntent,
        state: &EditingState,
        _: &FeatureServices<'_>,
    ) -> Result<Option<CommandPlan>, String> {
        if self.0.load(SeqCst) && matches!(intent, EditIntent::RestoreHeading) {
            let mut next = state.clone();
            next.closers = std::iter::once(0..usize::MAX).collect();
            return Ok(Some(CommandPlan::View {
                base: state.source.id(),
                state: next,
                handled: true,
            }));
        }
        Ok(None)
    }
}
impl FeatureModule for Corrupt {
    fn manifest(&self) -> FeatureManifest {
        FeatureManifest::builtin("corrupt-history")
    }
    fn register(&self, r: &mut Registrar) -> Result<(), String> {
        r.command(
            Contribution {
                id: "corrupt-history.restore",
                slot: Slot::Command("aaa"),
                exclusive: false,
                priority: 100,
            },
            Arc::new(Corrupt(self.0.clone())),
        )
    }
}
#[test]
fn structural_history_checks_cannot_be_disabled_by_policy() {
    let corrupt = Arc::new(AtomicBool::new(false));
    let mut services = regression_tests::services();
    services.features = Arc::new(
        ryne_features::MarkdownFeatures::new(
            Arc::new(ryne_services::math::FormularyRenderer::default()),
            vec![],
        )
        .unwrap()
        .with_feature(Arc::new(Corrupt(corrupt.clone())))
        .unwrap(),
    );
    let mut doc = EditorSession::new("plain", services);
    doc.replace(0..0, "x").unwrap();
    doc.add_validator(Arc::new(Policy {
        reject: AtomicBool::new(true),
        panic: AtomicBool::new(false),
        calls: AtomicUsize::new(0),
        input_only: true,
    }));
    corrupt.store(true, SeqCst);
    let before = fingerprint(&doc);
    assert!(doc.try_undo(false).is_err());
    assert_eq!(fingerprint(&doc), before);
    corrupt.store(false, SeqCst);
    assert!(doc.try_undo(false).unwrap());
}
