use super::*;
use crate::regression_tests::new_document;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn local_source_control_cache_matches_rebuild_in_both_directions() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw = concat!(
            "- [ ] one\n  - nested\n\n",
            "> | a | b |\n> | --- | --- |\n> | x | y |\n\n",
            "> ```rust\n> fn main() {}\n> ```\n\ntail"
        )
        .replace('\n', newline);
        let mut doc = new_document(&raw);
        doc.select(raw.len(), false, false);
        let source = doc.source.id();
        let selection = doc.selection();
        let all = doc.cached_render_control_batch(&doc).unwrap().controls;
        assert!(!all.is_empty());
        for scope in [
            Some(0..raw.find('>').unwrap()),
            Some(raw.find("> |").unwrap()..raw.find("> ```").unwrap()),
            Some(raw.find("> ```").unwrap()..raw.find("tail").unwrap()),
            Some(0..raw.len()),
            None,
        ] {
            doc.set_block_source(scope);
            let cached = doc.cached_render_control_batch(&doc).unwrap().controls;
            let fresh = doc.project_render_control_batch(&doc).unwrap().controls;
            assert_eq!(format!("{cached:?}"), format!("{fresh:?}"));
            assert!(Arc::ptr_eq(
                &cached,
                &doc.cached_render_control_batch(&doc).unwrap().controls
            ));
            assert_eq!(doc.source.id(), source);
            assert_eq!(doc.selection(), selection);
            assert_eq!(doc.history_len(false), 0);
            assert!(!doc.dirty());
        }
    }
}

struct BrokenPreview(Arc<AtomicUsize>);
impl BuiltinFeature for BrokenPreview {
    fn manifest(&self) -> FeatureManifest {
        FeatureManifest::builtin("broken-preview")
    }
    fn register(&self, r: &mut Registrar) -> Result<(), String> {
        r.preview(
            Contribution {
                id: "broken-preview.input",
                slot: Slot::Renderer("broken"),
                exclusive: false,
                priority: 100,
            },
            Arc::new(Self(self.0.clone())),
        )
    }
}
impl PreviewProvider for BrokenPreview {
    fn input(
        &self,
        _: &EditingState,
        _: &ryne_syntax::code_block::CodeBlock,
    ) -> Option<PreviewInput> {
        self.0.fetch_add(1, Ordering::Relaxed);
        panic!("preview input failure")
    }
}
#[test]
fn preview_fault_keeps_healthy_provider_jobs_and_editable_code() {
    let mut services = crate::regression_tests::services();
    let calls = Arc::new(AtomicUsize::new(0));
    services.features = Arc::new(
        ryne_features::MarkdownFeatures::new(
            Arc::new(ryne_services::math::FormularyRenderer::default()),
            vec![ryne_features::diagrams::FencedPreview {
                id: "healthy",
                language: "mermaid",
                renderer: Arc::new(ryne_services::mermaid::MermanRenderer),
            }],
        )
        .unwrap()
        .with_feature(Arc::new(BrokenPreview(calls.clone())))
        .unwrap(),
    );
    let raw = "```custom\nliteral **code**\n```\n\n```mermaid\nflowchart LR\nA-->B\n```\n\ntail";
    let mut doc = EditorSession::new(raw, services);
    doc.select(raw.len(), false, false);
    let (_, request) = doc.diagram_views(false, &[]);
    assert_eq!(request.unwrap().key.feature, "healthy");
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(doc.display().text.contains("literal **code**"));
    assert!(!doc.diagnostics.is_empty());
    assert_eq!(doc.source.text(), raw);
    assert_eq!(doc.history_len(false), 0);
    assert!(!doc.dirty());
}

#[test]
fn many_observer_failures_keep_distinct_bounded_diagnostics() {
    let mut doc = new_document("a");
    let calls = Arc::new(AtomicUsize::new(0));
    for _ in 0..100 {
        doc.add_observer(Arc::new(ObserverProbe {
            calls: calls.clone(),
            broken: true,
        }));
    }
    doc.select(0, false, false);
    doc.select(0, false, false);
    assert_eq!(calls.load(Ordering::Relaxed), 100);
    assert_eq!(doc.diagnostics.len(), 64);
    let entries = doc.diagnostics.entries();
    assert!(entries.first().unwrap().feature.starts_with("observer:36:"));
    assert!(entries.last().unwrap().feature.starts_with("observer:99:"));
}

struct BrokenGetters;
impl FeatureEngine for BrokenGetters {
    fn plan(&self, intent: EditIntent, state: &EditingState) -> Result<CommandPlan, String> {
        crate::basic::SourceEditing.plan(intent, state)
    }
    fn display(
        &self,
        state: &EditingState,
        _: Option<&mut dyn SyntaxHighlighter>,
        _: &[DiagramViewInput],
    ) -> Display {
        Display::plain_source(state.source.text())
    }
    fn presentation_key(&self, _: &EditingState) -> Option<PresentationKey> {
        panic!("key")
    }
    fn cache_source_controls(&self) -> bool {
        panic!("controls key")
    }
    fn snapshot(&self) -> Option<Arc<dyn FeatureEngine>> {
        panic!("snapshot")
    }
    fn block_window(&self, _: &EditingState) -> Option<Range<usize>> {
        panic!("window")
    }
    fn protects_table_boundary(&self, _: &EditingState, _: bool) -> bool {
        panic!("boundary")
    }
    fn preview_inputs(&self, _: &EditingState) -> Vec<PreviewInput> {
        panic!("previews")
    }
    fn block_ui(&self, _: &ryne_syntax::block::Block, _: bool) -> AdornmentSpec {
        panic!("ui")
    }
    fn insert_templates(&self) -> &[(&'static str, InsertBlock)] {
        panic!("templates")
    }
}
#[test]
fn runtime_getter_faults_and_detached_export_stay_inside_host() {
    let mut services = crate::regression_tests::services();
    services.features = Arc::new(BrokenGetters);
    let mut doc = EditorSession::new("literal 中😀\r\n", services);
    doc.set_block_source(Some(0..7));
    assert_eq!(doc.block_window(), Some(0..7));
    assert!(doc.remove_table_boundary(false));
    assert!(doc.insert_templates().is_empty());
    let block = ryne_syntax::block::at(&doc.parse, 0).unwrap();
    assert!(matches!(
        doc.block_ui(&block, false).menu[0].command,
        BlockCommand::Source
    ));
    assert!(doc.preview_at_caret().is_none());
    assert!(doc.diagram_views(false, &[]).1.is_none());
    let first = doc.cached_project_batch(&doc, None, &[]).display;
    assert!(!Arc::ptr_eq(
        &first,
        &doc.cached_project_batch(&doc, None, &[]).display
    ));
    doc.cached_render_control_batch(&doc).unwrap();
    let export = doc.export_snapshot().render().unwrap();
    assert_eq!(export.text, "literal 中😀\n");
    let errors = doc.diagnostics.entries();
    for operation in [
        "block_window",
        "protects_table_boundary",
        "insert_templates",
        "block_ui",
        "preview_inputs",
        "presentation_key",
        "cache_source_controls",
        "snapshot",
    ] {
        assert!(
            errors.iter().any(|d| d.message.starts_with(operation)),
            "{operation}"
        );
    }
    assert_eq!(doc.source.text(), "literal 中😀\r\n");
    assert_eq!(doc.history_len(false), 0);
    assert!(!doc.dirty());
    // A fork gets a fresh log rather than sharing the parent's host wrapper.
    let fork = doc.fork("fresh");
    assert!(fork.diagnostics.is_empty());
}

#[test]
fn many_auto_blocks_finish_then_reuse_projection_through_moves_edits_and_undo() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw = (0..80)
            .map(|i| format!("```\nfn main_{i}() {{}}\n```\n\n"))
            .collect::<String>()
            .replace('\n', newline);
        let mut doc = new_document(&raw);
        doc.select(raw.len(), false, false);
        let mut h = ryne_services::highlight::CodeHighlighter::default();
        for phase in 0..3 {
            let mut complete = false;
            for _ in 0..82 {
                let batch = doc.cached_project_batch(&doc, Some(&mut h), &[]);
                assert!(batch.diagnostics.is_empty());
                if !h.has_pending_work() {
                    assert!(Arc::ptr_eq(
                        &batch.display,
                        &doc.cached_project_batch(&doc, Some(&mut h), &[]).display
                    ));
                    assert_eq!(batch.display.code_blocks.len(), 80);
                    assert!(batch.display.spans.iter().any(|s| s.style.syntax.is_some()));
                    complete = true;
                    break;
                }
            }
            assert!(complete);
            match phase {
                0 => {
                    doc.replace(0..0, &format!("before{newline}{newline}"))
                        .unwrap();
                }
                1 => doc.undo(false),
                _ => (),
            }
        }
        assert_eq!(doc.source.text(), raw);
        assert!(!doc.dirty());
        assert_eq!(doc.history_len(false), 0);
    }
}

struct ObserverProbe {
    calls: Arc<AtomicUsize>,
    broken: bool,
}
impl Observer for ObserverProbe {
    fn observe(&self, _: &SessionEvent, _: &EditingState) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        assert!(!self.broken, "observer failure");
    }
}
#[test]
fn failed_observer_is_disabled_without_starving_healthy_observers_or_history() {
    let bad = Arc::new(AtomicUsize::new(0));
    let good = Arc::new(AtomicUsize::new(0));
    let mut doc = new_document("a");
    for (calls, broken) in [(bad.clone(), true), (good.clone(), false)] {
        doc.add_observer(Arc::new(ObserverProbe { calls, broken }));
    }
    doc.insert_text(1..1, "中😀").unwrap();
    for _ in 0..100 {
        doc.select(0, false, false);
    }
    doc.undo(false);
    assert_eq!(doc.source.text(), "a");
    doc.undo(true);
    assert_eq!(doc.source.text(), "a中😀");
    assert_eq!(bad.load(Ordering::Relaxed), 1);
    assert_eq!(good.load(Ordering::Relaxed), 103);
    assert_eq!(doc.diagnostics.len(), 1);
}
