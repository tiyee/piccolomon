use ryne_features::MarkdownFeatures;
use ryne_services::{files::AtomicFileStore, math::FormularyRenderer};
use ryne_session::{EditorSession, SessionServices};
use std::sync::Arc;
fn doc(raw: &str) -> EditorSession {
    EditorSession::new(
        raw,
        SessionServices {
            features: Arc::new(
                MarkdownFeatures::new(Arc::new(FormularyRenderer::default()), vec![]).unwrap(),
            ),
            files: Arc::new(AtomicFileStore),
        },
    )
}

struct PanicPreview;
impl ryne_extensions::BuiltinFeature for PanicPreview {
    fn manifest(&self) -> ryne_extensions::FeatureManifest {
        ryne_extensions::FeatureManifest::builtin("panic-preview")
    }
    fn register(&self, r: &mut ryne_extensions::Registrar) -> Result<(), String> {
        r.preview(
            ryne_extensions::Contribution {
                id: "panic-preview.input",
                slot: ryne_extensions::Slot::Renderer("panic"),
                exclusive: false,
                priority: 0,
            },
            Arc::new(PanicPreview),
        )
    }
}
impl ryne_extensions::PreviewProvider for PanicPreview {
    fn input(
        &self,
        _: &ryne_extensions::EditingState,
        _: &ryne_syntax::code_block::CodeBlock,
    ) -> Option<ryne_extensions::PreviewInput> {
        panic!("preview failure")
    }
}
struct BrokenObserver;
impl ryne_extensions::Observer for BrokenObserver {
    fn observe(&self, _: &ryne_extensions::SessionEvent, _: &ryne_extensions::EditingState) {
        panic!("observer failure")
    }
}

struct CountMath(Arc<std::sync::atomic::AtomicUsize>);
impl ryne_extensions::FormulaRenderer for CountMath {
    fn render(&self, _: &str) -> Option<Arc<ryne_projection::Formula>> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        None
    }
}
fn main() {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let features = MarkdownFeatures::new(Arc::new(CountMath(calls.clone())), vec![]).unwrap();
    let raw = "plain $x$ tail";
    let mut d = EditorSession::new(
        raw,
        SessionServices {
            features: Arc::new(features),
            files: Arc::new(AtomicFileStore),
        },
    );
    d.select(raw.len(), false, false);
    d.delete(false).unwrap();
    println!(
        "formula requests for one Backspace command outside math, without GUI refresh: {}",
        calls.load(std::sync::atomic::Ordering::Relaxed)
    );

    std::panic::set_hook(Box::new(|_| {}));
    let features = MarkdownFeatures::new(Arc::new(FormularyRenderer::default()), vec![])
        .unwrap()
        .with_feature(Arc::new(PanicPreview))
        .unwrap();
    let mut d = EditorSession::new(
        "```custom\nbody\n```\n",
        SessionServices {
            features: Arc::new(features),
            files: Arc::new(AtomicFileStore),
        },
    );
    let escaped =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| d.diagram_views(false, &[])))
            .is_err();
    println!("preview provider panic escapes session boundary: {escaped}");
    let mut d = doc("plain");
    d.add_observer(Arc::new(BrokenObserver));
    for _ in 0..100 {
        d.select(0, false, false);
    }
    println!(
        "observer diagnostics after 100 selections: {}",
        d.diagnostics.len()
    );

    let raw = "- [ ] one\n- two\n\ntail";
    let mut d = doc(raw);
    let a = d.cached_render_control_batch(&d).unwrap();
    d.set_block_source(Some(0..16));
    let b = d.cached_render_control_batch(&d).unwrap();
    let c = d.project_render_control_batch(&d).unwrap();
    println!(
        "controls local source: before={} cached={} fresh={} reused={}",
        a.controls.len(),
        b.controls.len(),
        c.controls.len(),
        Arc::ptr_eq(&a.controls, &b.controls)
    );
    let mut d = doc(raw);
    d.set_block_source(Some(0..16));
    let a = d.cached_render_control_batch(&d).unwrap();
    d.set_block_source(None);
    let b = d.cached_render_control_batch(&d).unwrap();
    let c = d.project_render_control_batch(&d).unwrap();
    println!(
        "controls leaving local: before={} cached={} fresh={} reused={}",
        a.controls.len(),
        b.controls.len(),
        c.controls.len(),
        Arc::ptr_eq(&a.controls, &b.controls)
    );
    let raw = (0..80)
        .map(|i| format!("```\nfn main_{i}() {{}}\n```\n\n"))
        .collect::<String>();
    let d = doc(&raw);
    let blocks = ryne_syntax::code_block::blocks(&d.parse);
    let mut h = ryne_services::highlight::CodeHighlighter::default();
    for frame in 0..16 {
        h.begin_frame();
        let mut done = 0;
        for b in &blocks {
            if !h.spans(&raw, b).is_empty() {
                done += 1;
            }
        }
        h.end_frame();
        println!(
            "highlight frame={frame} colored={done}/80 generation={:?}",
            ryne_extensions::SyntaxHighlighter::presentation_generation(&h)
        );
    }
}
