use super::*;
use ryne_projection::{Diagram, DiagramOutcome};
use std::sync::atomic::{AtomicU64, Ordering};
struct FakeRenderer(AtomicU64);
impl DiagramRenderer for FakeRenderer {
    fn generation(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
    fn render(&self, _: &str, control: CancelControl) -> DiagramOutcome {
        control.checkpoint().map_err(str::to_string)?;
        Ok(Arc::new(Diagram {
            pixels: vec![0, 0, 255, 255].into(),
            pixel_width: 1,
            pixel_height: 1,
            width: 1.,
            height: 1.,
            bytes: 4,
        }))
    }
}
fn document(renderer: Arc<FakeRenderer>) -> EditorSession {
    let mut services = super::regression_tests::services();
    services.features = Arc::new(
        ryne_features::MarkdownFeatures::new(
            Arc::new(ryne_services::math::FormularyRenderer::default()),
            vec![
                ryne_features::diagrams::FencedPreview {
                    id: "test-alpha",
                    language: "alpha",
                    renderer: renderer.clone(),
                },
                ryne_features::diagrams::FencedPreview {
                    id: "test-beta",
                    language: "beta",
                    renderer,
                },
            ],
        )
        .unwrap(),
    );
    let mut doc = EditorSession::new(
        "```alpha\nsame\n```\n\n```beta\nsame\n```\n\ntail",
        services,
    );
    let len = doc.source.text().len();
    doc.select(len, false, false);
    doc
}
#[test]
fn additional_fenced_previews_use_the_existing_host_and_keep_separate_resource_keys() {
    let renderer = Arc::new(FakeRenderer(AtomicU64::new(1)));
    let mut doc = document(renderer);
    let (_, first) = doc.diagram_views(false, &[]);
    let first = first.unwrap();
    assert_eq!(first.key.feature, "test-alpha");
    let result = first.run();
    let key = doc.source.id();
    assert!(doc.jobs.complete(result, key));
    let (views, second) = doc.diagram_views(false, &[]);
    assert_eq!(views.len(), 1);
    let second = second.unwrap();
    assert_eq!(second.key.feature, "test-beta");
    assert_eq!(second.key.source, "same");
    let result = second.run();
    assert!(doc.jobs.complete(result, key));
    let (views, request) = doc.diagram_views(false, &[]);
    assert_eq!(views.len(), 2);
    assert!(request.is_none());
    // A source edit shifts both targets. Resource identity survives and ranges rebind.
    let resources = views
        .iter()
        .map(|v| v.1.as_ref().unwrap().clone())
        .collect::<Vec<_>>();
    doc.replace(0..0, "before\n\n").unwrap();
    doc.select(doc.source.text().len(), false, false);
    let (moved, request) = doc.diagram_views(false, &[]);
    assert!(request.is_none());
    for (i, view) in moved.iter().enumerate() {
        assert_eq!(view.0.start, views[i].0.start + 8);
        assert!(Arc::ptr_eq(&resources[i], view.1.as_ref().unwrap()))
    }
    // Emptying the document evicts the cache, but an already published frame owns it.
    doc.replace(0..doc.source.text().len(), "").unwrap();
    let (empty, _) = doc.diagram_views(false, &[]);
    assert!(empty.is_empty());
    assert_eq!(resources[0].pixels.len(), 4);
}
#[test]
fn obsolete_targets_documents_and_service_generations_discard_results() {
    let renderer = Arc::new(FakeRenderer(AtomicU64::new(1)));
    let mut doc = document(renderer.clone());
    let (_, request) = doc.diagram_views(false, &[]);
    let request = request.unwrap();
    let control = request.control.clone();
    doc.replace(0..doc.source.text().len(), "gone").unwrap();
    let _ = doc.diagram_views(false, &[]);
    assert!(control.is_cancelled());
    let result = request.run();
    assert!(!doc.jobs.complete(result, doc.source.id()));
    let mut doc = document(renderer.clone());
    let (_, request) = doc.diagram_views(false, &[]);
    let result = request.unwrap().run();
    let other = Source::new("other");
    assert!(!doc.jobs.complete(result, other.id()));
    let mut doc = document(renderer.clone());
    let (_, request) = doc.diagram_views(false, &[]);
    let result = request.unwrap().run();
    renderer.0.store(2, Ordering::SeqCst);
    assert!(!doc.jobs.complete(result, doc.source.id()));
    let (_, request) = doc.diagram_views(false, &[]);
    assert_eq!(request.unwrap().key.generation, 2);
}

#[test]
fn timed_out_content_keeps_diagnostic_and_does_not_starve_later_jobs() {
    let renderer = Arc::new(FakeRenderer(AtomicU64::new(1)));
    let mut doc = document(renderer.clone());
    let (_, request) = doc.diagram_views(false, &[]);
    let mut request = request.unwrap();
    request.control = CancelControl::with_timeout(std::time::Duration::ZERO);
    assert!(doc.jobs.complete(request.run(), doc.source.id()));
    let (views, next) = doc.diagram_views(false, &[]);
    assert_eq!(views.len(), 1);
    assert!(views[0].1.as_ref().unwrap_err().contains("超时"));
    let next = next.unwrap();
    assert_eq!(next.key.feature, "test-beta");
    assert!(doc.jobs.complete(next.run(), doc.source.id()));
    for _ in 0..3 {
        let (views, next) = doc.diagram_views(false, &[]);
        assert_eq!(views.len(), 2);
        assert!(next.is_none());
    }
    // A new renderer generation is allowed to retry the same source.
    renderer.0.store(2, Ordering::SeqCst);
    let (_, next) = doc.diagram_views(false, &[]);
    let next = next.unwrap();
    assert_eq!(next.key.feature, "test-alpha");
    assert_eq!(next.key.generation, 2);
    assert!(doc.jobs.complete(next.run(), doc.source.id()));
    let start = doc.source.text().find("same").unwrap();
    doc.replace(start..start + 4, "changed").unwrap();
    doc.select(doc.source.text().len(), false, false);
    let (_, next) = doc.diagram_views(false, &[]);
    assert_eq!(next.unwrap().key.source, "changed");
}
