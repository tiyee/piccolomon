use super::*;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::SeqCst};

pub(crate) struct CommandOnly;
impl FeatureModule for CommandOnly {
    fn manifest(&self) -> FeatureManifest {
        FeatureManifest::builtin("third-command")
    }
    fn register(&self, r: &mut Registrar) -> Result<(), String> {
        r.command(
            Contribution {
                id: "third.command",
                slot: Slot::Command("edit"),
                exclusive: false,
                priority: 10,
            },
            Arc::new(Self),
        )
    }
}
impl CommandHandler for CommandOnly {
    fn plan(
        &self,
        _: &EditIntent,
        _: &EditingState,
        _: &FeatureServices<'_>,
    ) -> Result<Option<CommandPlan>, String> {
        Ok(None)
    }
}
fn command_features() -> ryne_features::MarkdownFeatures {
    ryne_features::MarkdownFeatures::new(
        Arc::new(ryne_services::math::FormularyRenderer::default()),
        vec![],
    )
    .unwrap()
    .with_feature(Arc::new(CommandOnly))
    .unwrap()
}
#[test]
fn command_extensions_preserve_reuse_and_all_source_visibility_boundaries() {
    for nl in ["\n", "\r\n", "\r"] {
        let raw="# heading\n\nplain 中😀 tail **bold**\n\n> - first\n>   - nested\n\n| a | b |\n| --- | --- |\n| x | y |\n\n```rust\nlet x = 1;\n```\n".replace('\n',nl);
        let mut services = crate::regression_tests::services();
        services.features = Arc::new(command_features());
        let mut doc = EditorSession::new(&raw, services);
        doc.select(raw.find("plain").unwrap(), false, false);
        let a = doc.cached_project_batch(&doc, None, &[]).display;
        let controls = doc.cached_render_control_batch(&doc).unwrap().controls;
        for word in ["中", "tail", "nested", "x | y", "let x"] {
            doc.select(raw.find(word).unwrap(), false, false);
            let b = doc.cached_project_batch(&doc, None, &[]).display;
            assert!(Arc::ptr_eq(&a, &b), "{word}");
            assert_eq!(*b, doc.display());
            assert!(Arc::ptr_eq(
                &controls,
                &doc.cached_render_control_batch(&doc).unwrap().controls
            ));
        }
        doc.select(raw.find("bold").unwrap() + 1, false, false);
        let b = doc.cached_project_batch(&doc, None, &[]).display;
        assert!(!Arc::ptr_eq(&a, &b));
        assert_eq!(*b, doc.display());
        doc.set_block_source(Some(0..raw.len()));
        assert!(doc
            .cached_render_control_batch(&doc)
            .unwrap()
            .controls
            .is_empty());
        let source = doc.cached_project_batch(&doc, None, &[]).display;
        assert_eq!(*source, doc.display());
        doc.set_block_source(None);
        assert_eq!(
            doc.cached_render_control_batch(&doc)
                .unwrap()
                .controls
                .len(),
            controls.len()
        );
        doc.set_source_mode(true);
        assert!(doc
            .cached_render_control_batch(&doc)
            .unwrap()
            .controls
            .is_empty());
        doc.set_source_mode(false);
        doc.select(raw.len(), false, false);
        let before = doc.cached_render_control_batch(&doc).unwrap().controls;
        doc.update_composition(None, "临时😀", Some(0..0)).unwrap();
        let (temporary, views) = doc.projection_state(&[]);
        let p = doc.cached_project_batch(&temporary, None, &views).display;
        assert!(!Arc::ptr_eq(
            &p,
            &doc.cached_project_batch(&temporary, None, &views).display
        ));
        let c = doc
            .cached_render_control_batch(&temporary)
            .unwrap()
            .controls;
        assert!(!Arc::ptr_eq(
            &c,
            &doc.cached_render_control_batch(&temporary)
                .unwrap()
                .controls
        ));
        doc.cancel_composition();
        assert!(Arc::ptr_eq(
            &before,
            &doc.cached_render_control_batch(&doc).unwrap().controls
        ));
        assert_eq!(doc.source.text(), raw);
        assert!(!doc.dirty());
        assert_eq!(doc.history_len(false), 0);
        let old = doc.cached_project_batch(&doc, None, &[]).display;
        doc.insert_text(raw.len()..raw.len(), "new").unwrap();
        assert!(!Arc::ptr_eq(
            &old,
            &doc.cached_project_batch(&doc, None, &[]).display
        ));
        doc.undo(false);
        assert_eq!(doc.source.text(), raw);
        assert!(!doc.dirty());
        assert_eq!(
            *doc.cached_project_batch(&doc, None, &[]).display,
            doc.display()
        );
    }
}
struct EvolvingFormula {
    generation: AtomicU64,
    calls: AtomicUsize,
    change: AtomicBool,
    renderer: ryne_services::math::FormularyRenderer,
}
impl FormulaRenderer for EvolvingFormula {
    fn presentation_generation(&self) -> Option<u64> {
        Some(self.generation.load(SeqCst))
    }
    fn render(&self, text: &str) -> Option<Arc<ryne_projection::Formula>> {
        self.calls.fetch_add(1, SeqCst);
        if self.change.swap(false, SeqCst) {
            self.generation.fetch_add(1, SeqCst);
        }
        self.renderer.render(text)
    }
}
#[test]
fn primary_resource_change_during_build_is_not_cached_under_an_old_key() {
    let formulas = Arc::new(EvolvingFormula {
        generation: AtomicU64::new(0),
        calls: AtomicUsize::new(0),
        change: AtomicBool::new(true),
        renderer: Default::default(),
    });
    let mut services = crate::regression_tests::services();
    services.features = Arc::new(
        ryne_features::MarkdownFeatures::new(formulas.clone(), vec![])
            .unwrap()
            .with_feature(Arc::new(CommandOnly))
            .unwrap(),
    );
    let doc = EditorSession::new("$x^2$\n\nplain", services);
    let first = doc.cached_project_batch(&doc, None, &[]).display;
    let second = doc.cached_project_batch(&doc, None, &[]).display;
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(Arc::ptr_eq(
        &second,
        &doc.cached_project_batch(&doc, None, &[]).display
    ));
    assert_eq!(formulas.calls.load(SeqCst), 2);
}
