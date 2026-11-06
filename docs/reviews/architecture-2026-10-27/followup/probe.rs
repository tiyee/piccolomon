use ryne_extensions::*;
use ryne_session::{EditorSession, SessionServices};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Instant;

fn document(text: &str) -> EditorSession {
    EditorSession::new(
        text,
        SessionServices {
            features: Arc::new(
                ryne_features::MarkdownFeatures::new(
                    Arc::new(ryne_services::math::FormularyRenderer::default()),
                    vec![],
                )
                .unwrap(),
            ),
            files: Arc::new(ryne_services::files::AtomicFileStore),
        },
    )
}

struct Gate {
    reject: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
}
impl Validator for Gate {
    fn validate(&self, _: &EditingState, _: &CommandPlan) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.reject.load(Ordering::SeqCst) {
            Err("gate closed".into())
        } else {
            Ok(())
        }
    }
}

struct Control {
    fail_identity: Arc<AtomicBool>,
    healthy: bool,
}
impl RenderActionProvider for Control {
    fn id(&self) -> &'static str {
        if !self.healthy && self.fail_identity.load(Ordering::SeqCst) {
            panic!("injected runtime identity failure");
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
    fn controls(&self, state: &EditingState) -> Result<Vec<RenderControl>, String> {
        if !self.healthy {
            return Ok(Vec::new());
        }
        Ok(vec![RenderControl {
            provider: "healthy",
            id: "healthy-control".into(),
            target: RenderTarget {
                document: state.source.id(),
                role: "test",
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

fn main() {
    // Intentionally injected panics are captured; do not print misleading fatal messages.
    std::panic::set_hook(Box::new(|_| {}));
    let mut doc = document("base");
    let reject = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    doc.add_validator(Arc::new(Gate {
        reject: reject.clone(),
        calls: calls.clone(),
    }));
    doc.insert_text(4..4, " edited").unwrap();
    reject.store(true, Ordering::SeqCst);
    assert!(doc.insert_text(0..0, "blocked").is_err());
    let before = calls.load(Ordering::SeqCst);
    doc.undo(false);
    println!(
        "undo_gate source={:?} validator_calls_delta={}",
        doc.source.text(),
        calls.load(Ordering::SeqCst) - before
    );
    assert_eq!(doc.source.text(), "base");
    let before = calls.load(Ordering::SeqCst);
    doc.undo(true);
    println!(
        "redo_gate source={:?} validator_calls_delta={}",
        doc.source.text(),
        calls.load(Ordering::SeqCst) - before
    );

    let fail_identity = Arc::new(AtomicBool::new(false));
    let features = ryne_features::MarkdownFeatures::new(
        Arc::new(ryne_services::math::FormularyRenderer::default()),
        vec![],
    )
    .unwrap()
    .with_render_action_provider(Arc::new(Control {
        fail_identity: fail_identity.clone(),
        healthy: true,
    }))
    .unwrap()
    .with_render_action_provider(Arc::new(Control {
        fail_identity: fail_identity.clone(),
        healthy: false,
    }))
    .unwrap();
    let doc = EditorSession::new(
        "x",
        SessionServices {
            features: Arc::new(features),
            files: Arc::new(ryne_services::files::AtomicFileStore),
        },
    );
    let before = doc
        .cached_render_control_batch(&doc)
        .unwrap()
        .controls
        .len();
    fail_identity.store(true, Ordering::SeqCst);
    let after = doc.cached_render_control_batch(&doc);
    println!(
        "provider_identity healthy_before={before} entire_batch_error={:?}",
        after.err()
    );

    let doc = document("| a |\n| --- |\n| b |\n");
    let mut display = doc.display();
    assert_eq!(display.tables.len(), 1);
    display.tables[0].range = 0..0;
    let accepted = display.validate(doc.source.text()).is_ok();
    let next_start = display.tables[0].range.end;
    println!(
        "zero_progress_table validation_accepts={accepted} layout_start=0 next_start={next_start}"
    );
    assert!(accepted);

    let doc = document("$x$ $y$ $z$");
    let mut display = doc.display();
    assert_eq!(display.formulas.len(), 3);
    display.formulas.swap(1, 2);
    let accepted = display.validate(doc.source.text()).is_ok();
    println!(
        "unordered_formulas validation_accepts={accepted} starts={:?}",
        display
            .formulas
            .iter()
            .map(|f| f.display.start)
            .collect::<Vec<_>>()
    );
    assert!(accepted);

    // Prewarm parsing and all lazy indexes before measuring the actual block query.
    for bytes in [64 * 1024, 256 * 1024, 1024 * 1024] {
        let raw = "plain paragraph words\n\n".repeat(bytes / 23 + 1);
        let parse =
            ryne_syntax::ParseSnapshot::with_math(ryne_parser::Source::new(&raw));
        let caret = raw.len() - 3;
        std::hint::black_box(ryne_syntax::block::at(&parse, caret));
        let mut samples = Vec::new();
        for _ in 0..5 {
            let started = Instant::now();
            for _ in 0..10 {
                std::hint::black_box(ryne_syntax::block::at(&parse, caret));
            }
            samples.push(started.elapsed().as_secs_f64() * 1000. / 10.);
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "block_at_debug bytes={} prewarmed_p50_ms={:.3}",
            raw.len(),
            samples[2]
        );
    }
}
