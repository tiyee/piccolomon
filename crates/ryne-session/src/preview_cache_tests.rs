use super::*;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

struct Renderer(AtomicU64);
impl DiagramRenderer for Renderer {
    fn generation(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
    fn render(&self, _: &str, _: CancelControl) -> ryne_projection::DiagramOutcome {
        Ok(Arc::new(ryne_projection::Diagram {
            pixels: vec![0, 0, 0, 255].into(),
            pixel_width: 1,
            pixel_height: 1,
            width: 1.,
            height: 1.,
            bytes: 4,
        }))
    }
}
#[derive(Clone)]
struct Counter {
    calls: Arc<AtomicUsize>,
    preview: ryne_features::diagrams::FencedPreview,
}
impl PreviewProvider for Counter {
    fn source_cache_generation(&self) -> Option<u64> {
        self.preview.source_cache_generation()
    }
    fn input(
        &self,
        state: &EditingState,
        block: &ryne_syntax::code_block::CodeBlock,
    ) -> Option<PreviewInput> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.preview.input(state, block)
    }
}
impl FeatureModule for Counter {
    fn manifest(&self) -> FeatureManifest {
        FeatureManifest::builtin("candidate-counter")
    }
    fn register(&self, r: &mut Registrar) -> Result<(), String> {
        r.preview(
            Contribution {
                id: "count.preview",
                slot: Slot::Renderer("diagram"),
                exclusive: true,
                priority: 0,
            },
            Arc::new(self.clone()),
        )
    }
}
fn fixture(raw: &str) -> (EditorSession, Arc<AtomicUsize>, Arc<Renderer>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let renderer = Arc::new(Renderer(AtomicU64::new(0)));
    let features = ryne_features::MarkdownFeatures::new(
        Arc::new(ryne_services::math::FormularyRenderer::default()),
        vec![],
    )
    .unwrap()
    .with_feature(Arc::new(Counter {
        calls: calls.clone(),
        preview: ryne_features::diagrams::FencedPreview {
            id: "descriptor",
            language: "diagram",
            renderer: renderer.clone(),
        },
    }))
    .unwrap();
    (
        EditorSession::new(
            raw,
            SessionServices {
                features: Arc::new(features),
                files: Arc::new(ryne_services::files::AtomicFileStore),
            },
        ),
        calls,
        renderer,
    )
}

#[test]
fn caret_refresh_and_preview_navigation_share_candidates_but_jobs_keep_live_identity() {
    let raw = "```text\nordinary\n```\n\n".repeat(1000) + "```diagram\nA-->B\n```\n\ntail";
    let (mut doc, calls, renderer) = fixture(&raw);
    doc.select(raw.len(), false, false);
    let source = doc.source.id();
    let (_, request) = doc.diagram_views(false, &[]);
    doc.complete_job(request.unwrap().run());
    assert_eq!(calls.load(Ordering::Relaxed), 1001);
    let (views, request) = doc.diagram_views(false, &[]);
    assert_eq!(views.len(), 1);
    assert!(request.is_none());
    let owner = views[0].0.clone();
    for _ in 0..10 {
        doc.select(raw.find("A-->B").unwrap(), false, false);
        assert_eq!(doc.preview_at_caret(), Some(owner.clone()));
        assert!(doc.diagram_views(false, &[]).0.is_empty());
        doc.select(raw.len(), false, false);
        assert_eq!(doc.diagram_views(false, &[]).0.len(), 1);
    }
    for mode in [true, false] {
        doc.set_source_mode(mode);
        let (views, request) = doc.diagram_views(false, &[]);
        assert_eq!(views.len(), usize::from(!mode));
        assert!(request.is_none());
    }
    doc.set_block_source(Some(owner.clone()));
    assert!(doc.diagram_views(false, &[]).0.is_empty());
    doc.set_block_source(None);
    assert_eq!(doc.diagram_views(false, &[]).0.len(), 1);
    assert_eq!(calls.load(Ordering::Relaxed), 1001);
    renderer.0.store(1, Ordering::Relaxed);
    let (views, request) = doc.diagram_views(false, &[]);
    assert!(views.is_empty());
    let request = request.unwrap();
    assert_eq!(request.key.generation, 1);
    assert_eq!(
        calls.load(Ordering::Relaxed),
        1001,
        "renderer generation must not re-extract source"
    );
    doc.complete_job(request.run());
    assert_eq!(doc.source.id(), source);
    assert_eq!(doc.source.text(), raw);
    assert!(!doc.dirty());
    assert_eq!(doc.history_len(false), 0);
}

#[test]
fn nested_newlines_edits_history_composition_and_forks_keep_candidates_current() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw = "> - ```diagram\n>   A-->B\n>   ```\n\ntail".replace('\n', newline);
        let (mut doc, calls, _) = fixture(&raw);
        doc.select(raw.len(), false, false);
        let initial = doc.cached_preview_inputs();
        assert_eq!(initial.len(), 1);
        assert_eq!(initial[0].input.as_deref(), Ok("A-->B"));
        let at = raw.find("A-->B").unwrap();
        doc.replace(at..at + 5, "B-->C").unwrap();
        assert_eq!(doc.cached_preview_inputs()[0].input.as_deref(), Ok("B-->C"));
        doc.undo(false);
        assert_eq!(doc.cached_preview_inputs()[0].input.as_deref(), Ok("A-->B"));
        doc.undo(true);
        assert_eq!(doc.cached_preview_inputs()[0].input.as_deref(), Ok("B-->C"));
        let before = calls.load(Ordering::Relaxed);
        doc.update_composition(None, "临", None).unwrap();
        doc.cached_preview_inputs();
        doc.cached_preview_inputs();
        assert_eq!(
            calls.load(Ordering::Relaxed),
            before + 2,
            "preedit bypasses candidate cache"
        );
        doc.cancel_composition();
        doc.cached_preview_inputs();
        assert_eq!(calls.load(Ordering::Relaxed), before + 2);
        let fork = doc.fork(doc.source.text());
        fork.cached_preview_inputs();
        assert_eq!(
            calls.load(Ordering::Relaxed),
            before + 3,
            "fork owns a fresh cache"
        );
        doc.replace(0..doc.source.text().len(), "tail").unwrap();
        assert!(doc.cached_preview_inputs().is_empty());
    }
}

#[test]
fn cached_candidates_do_not_bypass_drag_freeze_or_obsolete_job_cancellation() {
    let raw = "```diagram\nA-->B\n```\n\ntail";
    let (mut doc, _, _) = fixture(raw);
    doc.select(raw.len(), false, false);
    let (_, request) = doc.diagram_views(false, &[]);
    let request = request.unwrap();
    let control = request.control.clone();
    assert!(doc.diagram_views(true, &[]).1.is_none());
    assert!(control.is_cancelled());
    doc.complete_job(request.run());
    let (_, request) = doc.diagram_views(false, &[]);
    let request = request.unwrap();
    let control = request.control.clone();
    doc.replace(0..doc.source.text().len(), "gone").unwrap();
    assert!(doc.diagram_views(false, &[]).0.is_empty());
    assert!(control.is_cancelled());
    doc.complete_job(request.run());
    assert!(!doc.jobs.pending());
}
