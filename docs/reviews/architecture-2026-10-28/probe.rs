//! Audit probes: remaining defects and repaired registration/save/identity boundaries.
use ryne_extensions::*;
use ryne_session::{EditorSession, SessionServices};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

fn features() -> ryne_features::MarkdownFeatures {
    ryne_features::MarkdownFeatures::new(
        Arc::new(ryne_services::math::FormularyRenderer::default()),
        vec![],
    )
    .unwrap()
}
fn document(text: &str, features: ryne_features::MarkdownFeatures) -> EditorSession {
    EditorSession::new(
        text,
        SessionServices {
            features: Arc::new(features),
            files: Arc::new(ryne_services::files::AtomicFileStore),
        },
    )
}
struct Gate(Arc<AtomicBool>, Arc<AtomicUsize>);
impl Validator for Gate {
    fn validate(&self, _: &EditingState, _: &CommandPlan) -> Result<(), String> {
        self.1.fetch_add(1, Ordering::SeqCst);
        if self.0.load(Ordering::SeqCst) {
            Err("blocked".into())
        } else {
            Ok(())
        }
    }
}
struct Control {
    fail: Arc<AtomicBool>,
    healthy: bool,
}
impl RenderActionProvider for Control {
    fn id(&self) -> &'static str {
        if !self.healthy && self.fail.load(Ordering::SeqCst) {
            panic!("injected identity panic");
        }
        if self.healthy { "healthy" } else { "flaky" }
    }
    fn commands(&self) -> &'static [&'static str] {
        if self.healthy {
            &["healthy.action"]
        } else {
            &["flaky.action"]
        }
    }
    fn controls(&self, s: &EditingState) -> Result<Vec<RenderControl>, String> {
        if !self.healthy {
            return Ok(vec![]);
        }
        Ok(vec![RenderControl {
            provider: "healthy",
            id: "one".into(),
            target: RenderTarget {
                document: s.source.id(),
                role: "audit",
                owner: 0..1,
                part: vec![],
            },
            focus: 0..1,
            placement: RenderControlPlacement::Trailing,
            trailing_space: 12.,
            actions: vec![RenderAction {
                command: "healthy.action",
                label: "action",
                fallback: "action",
                icon: None,
                enabled: true,
                checked: false,
                group: 0,
                submenu: None,
                shortcut: None,
            }],
        }])
    }
    fn plan(&self, _: &EditingState, _: &RenderActionRequest) -> Result<CommandPlan, String> {
        Ok(CommandPlan::Consume)
    }
}
struct PreviewCounter(Arc<AtomicUsize>);
impl PreviewProvider for PreviewCounter {
    fn input(
        &self,
        _: &EditingState,
        _: &ryne_syntax::code_block::CodeBlock,
    ) -> Option<PreviewInput> {
        self.0.fetch_add(1, Ordering::SeqCst);
        None
    }
}
impl FeatureModule for PreviewCounter {
    fn manifest(&self) -> FeatureManifest {
        FeatureManifest::builtin("audit-preview")
    }
    fn register(&self, r: &mut Registrar) -> Result<(), String> {
        r.preview(
            Contribution {
                id: "audit.preview",
                slot: Slot::Renderer("audit"),
                exclusive: true,
                priority: 0,
            },
            Arc::new(Self(self.0.clone())),
        )
    }
}
struct Accept(usize, Arc<AtomicUsize>);
impl CommandHandler for Accept {
    fn plan(
        &self,
        _: &EditIntent,
        _: &EditingState,
        _: &FeatureServices<'_>,
    ) -> Result<Option<CommandPlan>, String> {
        self.1.store(self.0, Ordering::SeqCst);
        Ok(Some(CommandPlan::Consume))
    }
}
struct RecoverDuplicate(Arc<AtomicUsize>);
impl FeatureModule for RecoverDuplicate {
    fn manifest(&self) -> FeatureManifest {
        FeatureManifest::builtin("audit-duplicate")
    }
    fn register(&self, r: &mut Registrar) -> Result<(), String> {
        let c = || Contribution {
            id: "audit.command",
            slot: Slot::Command("edit"),
            exclusive: false,
            priority: 0,
        };
        r.command(c(), Arc::new(Accept(1, self.0.clone())))?;
        // A module is allowed to handle an optional registration failure.
        let failed = r.command(c(), Arc::new(Accept(2, self.0.clone()))).is_err();
        assert!(failed);
        println!("duplicate_registration returned_error={failed}");
        Ok(())
    }
}
struct PanicStore;
impl FileStore for PanicStore {
    fn read(&self, _: &std::path::Path) -> std::io::Result<String> {
        Ok(String::new())
    }
    fn write(
        &self,
        _: &std::path::Path,
        _: &editable_markdown_parser::Source,
    ) -> std::io::Result<()> {
        panic!("injected file port panic")
    }
}
fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let mut doc = document("base", features());
    let reject = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    doc.add_validator(Arc::new(Gate(reject.clone(), calls.clone())));
    doc.insert_text(4..4, " edited").unwrap();
    reject.store(true, Ordering::SeqCst);
    assert!(doc.insert_text(0..0, "blocked").is_err());
    let before = calls.load(Ordering::SeqCst);
    assert!(doc.try_undo(false).is_err());
    assert_eq!(doc.source.text(), "base edited");
    reject.store(false, Ordering::SeqCst);
    assert!(doc.try_undo(false).unwrap());
    assert_eq!(doc.source.text(), "base");
    reject.store(true, Ordering::SeqCst);
    assert!(doc.try_undo(true).is_err());
    assert_eq!(doc.source.text(), "base");
    reject.store(false, Ordering::SeqCst);
    assert!(doc.try_undo(true).unwrap());
    assert_eq!(doc.source.text(), "base edited");
    assert_eq!(calls.load(Ordering::SeqCst) - before, 4);
    println!(
        "history rejected_undo_and_redo=true validator_calls_for_four_attempts={}",
        calls.load(Ordering::SeqCst) - before
    );

    let fail = Arc::new(AtomicBool::new(false));
    let f = features()
        .with_render_action_provider(Arc::new(Control {
            fail: fail.clone(),
            healthy: true,
        }))
        .unwrap()
        .with_render_action_provider(Arc::new(Control {
            fail: fail.clone(),
            healthy: false,
        }))
        .unwrap();
    let doc = document("x", f);
    assert_eq!(
        doc.cached_render_control_batch(&doc)
            .unwrap()
            .controls
            .len(),
        1
    );
    fail.store(true, Ordering::SeqCst);
    let batch = doc.cached_render_control_batch(&doc).unwrap();
    assert_eq!(batch.controls.len(), 1);
    assert_eq!(batch.controls[0].provider, "healthy");
    assert!(batch.diagnostics.is_empty());
    println!("identity_panic entire_control_batch_failed=false healthy_controls=1");

    for count in [100, 1000] {
        let calls = Arc::new(AtomicUsize::new(0));
        let f = features()
            .with_feature(Arc::new(PreviewCounter(calls.clone())))
            .unwrap();
        let raw = "```text\nx\n```\n\n".repeat(count) + "plain tail";
        let mut doc = document(&raw, f);
        doc.select(raw.len(), false, false);
        let first = doc.cached_project_batch(&doc, None, &[]).display;
        let mut hits = 0;
        for i in 0..10 {
            doc.select(raw.len() - i % 2, false, false);
            let (views, _) = doc.diagram_views(false, &[]);
            let next = doc.cached_project_batch(&doc, None, &views).display;
            hits += usize::from(Arc::ptr_eq(&first, &next));
        }
        assert_eq!(hits, 10);
        assert_eq!(calls.load(Ordering::SeqCst), count * 10);
        println!(
            "preview code_blocks={count} projection_cache_hits={hits}/10 provider_calls={}",
            calls.load(Ordering::SeqCst)
        );
    }

    let selected = Arc::new(AtomicUsize::new(0));
    let registry = Registry::build(&[&RecoverDuplicate(selected.clone())]).unwrap();
    let doc = document("x", features());
    let formulas = ryne_services::math::FormularyRenderer::default();
    registry
        .plan(
            &EditIntent::Normalize,
            &doc,
            &FeatureServices {
                formulas: &formulas,
                planning: Default::default(),
            },
        )
        .unwrap();
    assert_eq!(selected.load(Ordering::SeqCst), 1);
    println!(
        "duplicate_registration executed_handler={}",
        selected.load(Ordering::SeqCst)
    );

    let mut doc = EditorSession::new(
        "x",
        SessionServices {
            features: Arc::new(features()),
            files: Arc::new(PanicStore),
        },
    );
    doc.insert_text(1..1, " dirty").unwrap();
    let request = doc.queue_save("unused-path.md".into()).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| request.run()));
    let panicked = result.is_err();
    assert!(!panicked);
    let completion = doc.complete_save(result.unwrap());
    assert!(completion.accepted && completion.error.is_some() && doc.dirty());
    let saving = doc.files.saving;
    let next_dispatched = doc.queue_save("unused-path.md".into()).is_some();
    assert!(!saving && next_dispatched);
    println!(
        "file_port panicked={panicked} saving_after_failure={saving} next_request_dispatched={next_dispatched}"
    );
}
