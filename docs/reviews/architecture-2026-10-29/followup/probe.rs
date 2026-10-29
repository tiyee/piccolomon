//! Read-only review probes. File mutations are confined to the runner's temporary directory.
use ryne_extensions::*;
use ryne_session::{EditorSession, SessionServices};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn features() -> ryne_features::MarkdownFeatures {
    ryne_features::MarkdownFeatures::new(
        Arc::new(ryne_services::math::FormularyRenderer::default()),
        vec![],
    )
    .unwrap()
}
fn services(features: ryne_features::MarkdownFeatures) -> SessionServices {
    SessionServices {
        features: Arc::new(features),
        files: Arc::new(ryne_services::files::AtomicFileStore),
    }
}

struct EmptyControls;
impl RenderActionProvider for EmptyControls {
    fn id(&self) -> &'static str {
        "empty"
    }
    fn commands(&self) -> &'static [&'static str] {
        &[]
    }
    fn controls(&self, _: &EditingState) -> Result<Vec<RenderControl>, String> {
        Ok(vec![])
    }
    fn plan(&self, _: &EditingState, _: &RenderActionRequest) -> Result<CommandPlan, String> {
        Ok(CommandPlan::Consume)
    }
}

#[derive(Clone)]
struct Candidates(Arc<AtomicUsize>);
impl PreviewProvider for Candidates {
    fn source_cache_generation(&self) -> Option<u64> {
        Some(0)
    }
    fn input(
        &self,
        _: &EditingState,
        _: &ryne_syntax::code_block::CodeBlock,
    ) -> Option<PreviewInput> {
        self.0.fetch_add(1, Ordering::Relaxed);
        None
    }
}
impl FeatureModule for Candidates {
    fn manifest(&self) -> FeatureManifest {
        FeatureManifest::builtin("candidates")
    }
    fn register(&self, registrar: &mut Registrar) -> Result<(), String> {
        registrar.preview(
            Contribution {
                id: "candidates.preview",
                slot: Slot::Renderer("review"),
                exclusive: true,
                priority: 0,
            },
            Arc::new(self.clone()),
        )
    }
}

fn main() {
    // R5: even an empty, undeclared provider changes the batch identity on every refresh.
    let engine = features()
        .with_render_action_provider(Arc::new(EmptyControls))
        .unwrap();
    let mut doc = EditorSession::new("plain text", services(engine));
    let first = doc.cached_render_control_batch(&doc).unwrap().controls;
    let display = doc.cached_project_batch(&doc, None, &[]).display;
    let mut changed = 0;
    for p in 1..=10 {
        doc.select(p, false, false);
        let controls = doc.cached_render_control_batch(&doc).unwrap().controls;
        assert!(controls.is_empty());
        changed += usize::from(!Arc::ptr_eq(&first, &controls));
        assert!(Arc::ptr_eq(
            &display,
            &doc.cached_project_batch(&doc, None, &[]).display
        ));
    }
    assert_eq!(changed, 10);
    println!("empty_controls new_batch_identity={changed}/10 projection_identity_preserved=true");

    // R6 closure: the older probe deliberately used an undeclared provider. Test opt-in here.
    let calls = Arc::new(AtomicUsize::new(0));
    let engine = features()
        .with_feature(Arc::new(Candidates(calls.clone())))
        .unwrap();
    let raw = "```text\nordinary\n```\n\n".repeat(1000) + "tail";
    let mut doc = EditorSession::new(&raw, services(engine));
    assert!(doc.diagram_views(false, &[]).0.is_empty());
    let cold = calls.load(Ordering::Relaxed);
    for p in 1..=10 {
        doc.select(p, false, false);
        doc.diagram_views(false, &[]);
    }
    let warm = calls.load(Ordering::Relaxed) - cold;
    assert_eq!((cold, warm), (1000, 0));
    println!("declared_preview cold_calls={cold} calls_for_ten_moves={warm}");

    // A single real built-in table action currently rebuilds the complete provider batch.
    // Report its public input size; no timing claim or production instrumentation is made.
    let raw = "| a | b | c |\n| --- | --- | --- |\n".to_owned() + &"| x | y | z |\n".repeat(800);
    let doc = EditorSession::new(&raw, services(features()));
    let controls = doc.render_controls().unwrap();
    println!(
        "table_action_validation table_rows=801 controls={}",
        controls.len()
    );

    // Actual session/save receipt path, with no user files involved.
    #[cfg(unix)]
    {
        let root = std::path::PathBuf::from(std::env::args().nth(1).expect("temporary directory"));
        let target = root.join("target.md");
        let link = root.join("link.md");
        std::fs::write(&target, "original\r\n").unwrap();
        std::os::unix::fs::symlink("target.md", &link).unwrap();
        let mut doc = EditorSession::open(&link, services(features())).unwrap();
        assert_eq!(doc.source.text(), "original\r\n");
        doc.replace(0..8, "edited").unwrap();
        let request = doc.queue_save(link.clone()).unwrap();
        let completion = doc.complete_save(request.run());
        let is_link = std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink();
        assert!(completion.accepted && completion.error.is_none() && completion.clean);
        assert!(is_link);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "edited\r\n");
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "edited\r\n");
        println!(
            "symlink_save success=true clean=true link_preserved={is_link} target_updated=true"
        );
    }
}
