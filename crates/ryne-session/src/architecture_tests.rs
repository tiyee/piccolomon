use super::regression_tests::new_document;
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

struct Gate(Arc<AtomicBool>);
impl Validator for Gate {
    fn validate(&self, _: &EditingState, _: &CommandPlan) -> Result<(), String> {
        if self.0.load(Ordering::SeqCst) {
            Err("reject confirmation".into())
        } else {
            Ok(())
        }
    }
}
#[test]
fn composition_failure_retains_preedit_and_retry_is_one_atomic_edit() {
    let mut doc = new_document("**");
    doc.select(2, false, false);
    let source = doc.source.id();
    doc.update_composition(None, "中😀", Some(3..7)).unwrap();
    let preedit = doc.composition().cloned();
    assert_eq!(doc.editable_text(), "**中😀");
    assert_eq!(doc.editable_selection(), 5..9);
    assert!(doc.update_composition(Some(0..1), "bad", None).is_err());
    assert!(doc.update_composition(None, "😀", Some(1..2)).is_err());
    assert_eq!(doc.composition(), preedit.as_ref());
    assert!(doc.insert_text(0..0, "unrelated").is_err());
    let reject = Arc::new(AtomicBool::new(true));
    doc.add_validator(Arc::new(Gate(reject.clone())));
    assert!(doc.commit_composition().is_err());
    assert_eq!(doc.source.id(), source);
    assert_eq!(doc.composition(), preedit.as_ref());
    assert_eq!(doc.history_len(false), 0);
    assert!(!doc.dirty());
    reject.store(false, Ordering::SeqCst);
    assert!(doc.commit_composition().unwrap());
    assert_eq!(doc.source.text(), "**中😀**");
    assert!(doc.composition().is_none());
    assert_eq!(doc.head, "**中😀".len());
    assert_eq!(doc.history_len(false), 1);
    doc.undo(false);
    assert_eq!(doc.source.text(), "**");
    assert_eq!(doc.head, 2);
    doc.undo(true);
    assert_eq!(doc.source.text(), "**中😀**");
}
#[test]
fn composition_platform_ranges_and_projection_preserve_unicode_and_physical_newlines() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw = format!("ab😀cd{newline}tail");
        let mut doc = new_document(&raw);
        doc.select(2, false, false);
        doc.set_block_source(Some(0..raw.len()));
        doc.update_composition(Some(2..6), "临时", Some(3..6))
            .unwrap();
        let source = doc.source.id();
        let (state, views) = doc.projection_state(&[(6..raw.len(), Err("pending".into()))]);
        assert_eq!(state.source.text(), format!("ab临时cd{newline}tail"));
        assert_eq!(state.selection(), 5..8);
        assert_eq!(views[0].0, 8..raw.len() + 2);
        assert_eq!(state.block_source.as_ref().unwrap().range, 0..raw.len() + 2);
        assert_eq!(doc.source.id(), source);
        assert_eq!(doc.history_len(false), 0);
        doc.commit_composition_text(Some(1..5), "Z").unwrap();
        assert_eq!(doc.source.text(), format!("aZ时cd{newline}tail"));
        assert_eq!(doc.head, 2);
        assert_eq!(doc.history_len(false), 1);
        doc.undo(false);
        assert_eq!(doc.source.text(), raw);
        doc.update_composition(None, "cancel", None).unwrap();
        let before = doc.source.id();
        assert!(doc.cancel_composition());
        assert_eq!(doc.source.id(), before);
        assert!(!doc.dirty());
    }
}
#[test]
fn generic_keys_preserve_container_rules_graphemes_and_single_undo() {
    for newline in ["\n", "\r\n", "\r"] {
        for (text, needle, expected) in [
            (
                format!("- a{newline}"),
                "a",
                format!("- a{newline}- {newline}"),
            ),
            (
                format!("> - a{newline}"),
                "a",
                format!("> - a{newline}> - {newline}"),
            ),
            (
                format!("```rust{newline}**literal**{newline}```"),
                "literal**",
                format!("```rust{newline}**literal**{newline}{newline}```"),
            ),
            ("plain".into(), "plain", "plain\n\n".into()),
        ] {
            let mut doc = new_document(&text);
            doc.select(text.find(needle).unwrap() + needle.len(), false, false);
            doc.line_break(false).unwrap();
            assert_eq!(doc.source.text(), expected);
            assert_eq!(doc.history_len(false), 1);
            doc.undo(false);
            assert_eq!(doc.source.text(), text);
        }
    }
    let mut doc = new_document("- a\n- b\n");
    doc.select(6, false, false);
    doc.tab(false).unwrap();
    assert_eq!(doc.source.text(), "- a\n  - b\n");
    assert_eq!(doc.history_len(false), 1);
    doc.tab(true).unwrap();
    assert_eq!(doc.source.text(), "- a\n- b\n");
    let mut doc = new_document("e\u{301}😀");
    doc.select(doc.source.text().len(), false, false);
    doc.delete(false).unwrap();
    assert_eq!(doc.source.text(), "e\u{301}");
    doc.delete(false).unwrap();
    assert_eq!(doc.source.text(), "");
    doc.undo(false);
    assert_eq!(doc.source.text(), "e\u{301}");
}

#[test]
fn unicode_17_deletion_preserves_container_bytes_and_single_undo() {
    let grapheme = "e\u{1acf}";
    for newline in ["\n", "\r\n", "\r"] {
        for source_mode in [false, true] {
            for forward in [false, true] {
                let raw = format!("> - {grapheme}x{newline}> - tail");
                let mut doc = new_document(&raw);
                doc.set_source_mode(source_mode);
                let start = raw.find(grapheme).unwrap();
                let caret = if forward {
                    start
                } else {
                    start + grapheme.len()
                };
                doc.select(caret, false, false);
                let selection = doc.selection();
                assert_eq!(selection, caret..caret);
                assert!(doc.delete(forward).unwrap());
                assert_eq!(doc.source.text(), format!("> - x{newline}> - tail"));
                assert_eq!(doc.history_len(false), 1);
                doc.undo(false);
                assert_eq!(doc.source.text(), raw);
                assert_eq!(doc.selection(), selection);
                assert!(!doc.dirty());
                doc.undo(true);
                assert_eq!(doc.source.text(), format!("> - x{newline}> - tail"));
            }
        }
    }
}

#[test]
fn unicode_17_composition_joins_previous_character_in_one_transaction() {
    let mut doc = new_document("e");
    doc.select(1, false, false);
    let source = doc.source.id();
    doc.update_composition(None, "\u{1acf}", Some(3..3))
        .unwrap();
    assert_eq!(doc.editable_text(), "e\u{1acf}");
    assert_eq!(doc.source.id(), source);
    assert_eq!(doc.history_len(false), 0);
    assert!(!doc.dirty());
    let (state, _) = doc.projection_state(&[]);
    let temporary = doc.project(&state, None, &[]);
    assert_eq!(temporary.deletion_range(4, false), Some(0..4));
    assert!(doc.commit_composition().unwrap());
    assert_eq!(doc.source.text(), "e\u{1acf}");
    assert_eq!(doc.head, 4);
    assert_eq!(doc.history_len(false), 1);
    assert!(doc.delete(false).unwrap());
    assert_eq!(doc.source.text(), "");
    doc.undo(false);
    assert_eq!(doc.source.text(), "e\u{1acf}");
    doc.undo(false);
    assert_eq!(doc.source.text(), "e");
    assert_eq!(doc.head, 1);
    assert!(!doc.dirty());
}

struct OverrideKeys;
impl BuiltinFeature for OverrideKeys {
    fn manifest(&self) -> FeatureManifest {
        FeatureManifest::builtin("override-keys")
    }
    fn register(&self, r: &mut Registrar) -> Result<(), String> {
        r.command(
            Contribution {
                id: "override-keys.command",
                slot: Slot::Command("edit"),
                exclusive: false,
                priority: 400,
            },
            Arc::new(Self),
        )
    }
}
impl CommandHandler for OverrideKeys {
    fn plan(
        &self,
        intent: &EditIntent,
        _: &EditingState,
        _: &FeatureServices<'_>,
    ) -> Result<Option<CommandPlan>, String> {
        Ok(matches!(
            intent,
            EditIntent::Delete { .. } | EditIntent::Tab { .. } | EditIntent::LineBreak { .. }
        )
        .then_some(CommandPlan::Consume))
    }
}
#[test]
fn registered_key_override_intercepts_all_generic_keys_before_builtin_rules() {
    let mut services = super::regression_tests::services();
    services.features = Arc::new(
        ryne_features::MarkdownFeatures::new(
            Arc::new(ryne_services::math::FormularyRenderer::default()),
            vec![],
        )
        .unwrap()
        .with_feature(Arc::new(OverrideKeys))
        .unwrap(),
    );
    let mut doc = EditorSession::new("- a\n- b", services);
    doc.select(7, false, false);
    let id = doc.source.id();
    doc.delete(false).unwrap();
    doc.tab(false).unwrap();
    doc.line_break(false).unwrap();
    assert_eq!(doc.source.id(), id);
    assert_eq!(doc.history_len(false), 0);
}
struct BrokenPresentation(bool);
impl FeatureEngine for BrokenPresentation {
    fn plan(&self, intent: EditIntent, state: &EditingState) -> Result<CommandPlan, String> {
        basic::SourceEditing.plan(intent, state)
    }
    fn display(
        &self,
        state: &EditingState,
        _: Option<&mut dyn SyntaxHighlighter>,
        _: &[DiagramViewInput],
    ) -> Display {
        if self.0 {
            panic!("injected presentation failure");
        }
        let mut display = Display::plain_source(state.source.text());
        display.spans[0].source.end = usize::MAX;
        display
    }
    fn block_window(&self, _: &EditingState) -> Option<Range<usize>> {
        None
    }
    fn protects_table_boundary(&self, _: &EditingState, _: bool) -> bool {
        false
    }
    fn preview_inputs(&self, _: &EditingState) -> Vec<PreviewInput> {
        vec![]
    }
    fn block_ui(&self, b: &ryne_syntax::block::Block, _: bool) -> AdornmentSpec {
        AdornmentSpec {
            metadata: b.kind.into(),
            menu: vec![],
        }
    }
    fn insert_templates(&self) -> &[(&'static str, InsertBlock)] {
        &[]
    }
}
#[test]
fn presentation_faults_fall_back_to_editable_source_without_mutating_session() {
    for panic in [false, true] {
        let mut services = super::regression_tests::services();
        services.features = Arc::new(BrokenPresentation(panic));
        let doc = EditorSession::new("**中😀**\r\nnext\rlast", services);
        let id = doc.source.id();
        let batch = doc.project_batch(&doc, None, &[]);
        assert_eq!(batch.diagnostics.len(), 1);
        assert_eq!(batch.display.text, "**中😀**\nnext\nlast");
        batch.display.validate(doc.source.text()).unwrap();
        for position in doc.source.text().char_indices().map(|(p, _)| p) {
            let display = batch.display.position_at(position);
            let source = batch
                .display
                .source_at(display, ryne_core::Affinity::Downstream);
            assert!(doc.source.text().is_char_boundary(source));
        }
        assert_eq!(doc.source.id(), id);
        assert_eq!(doc.history_len(false), 0);
        assert!(!doc.dirty());
    }
}
struct BadHighlight(u8);
impl SyntaxHighlighter for BadHighlight {
    fn begin_frame(&mut self) {
        if self.0 == 0 {
            panic!("begin");
        }
    }
    fn end_frame(&mut self) {
        if self.0 == 1 {
            panic!("end");
        }
    }
    fn spans(&mut self, _: &str, _: &ryne_syntax::code_block::CodeBlock) -> Vec<HighlightSpan> {
        if self.0 == 2 {
            panic!("spans");
        }
        vec![HighlightSpan {
            range: 0..usize::MAX,
            class: ryne_projection::Class::Keyword,
        }]
    }
}
#[test]
fn broken_highlighting_does_not_remove_structured_code_or_escape_the_host() {
    let doc = new_document("```rust\nlet x = \"中😀\";\n```\n");
    for mode in 0..4 {
        let batch = doc.project_batch(&doc, Some(&mut BadHighlight(mode)), &[]);
        assert!(!batch.diagnostics.is_empty());
        assert_eq!(batch.display.code_blocks.len(), 1);
        assert!(batch.display.text.contains("中😀"));
        batch.display.validate(doc.source.text()).unwrap();
    }
}

#[test]
fn batched_list_controls_keep_large_task_lists_exact_for_all_newlines() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw = (0..1000)
            .map(|n| {
                format!(
                    "- [{}] 中😀 {n}{newline}",
                    if n % 2 == 0 { "x" } else { " " }
                )
            })
            .collect::<String>();
        let mut doc = new_document(&raw);
        let controls = doc.render_controls().unwrap();
        assert_eq!(controls.len(), 2000);
        let toggles = controls
            .iter()
            .filter(|c| matches!(c.placement, RenderControlPlacement::Marker { .. }))
            .collect::<Vec<_>>();
        for (n, control) in toggles.iter().enumerate() {
            assert_eq!(control.actions[0].checked, n % 2 == 0);
            assert!(raw[control.target.owner.clone()].ends_with(newline));
            assert_eq!(
                raw[control.target.owner.clone()].matches(newline).count(),
                1
            );
        }
        let last = toggles.last().unwrap();
        doc.execute_render_action(&RenderActionRequest {
            provider: last.provider,
            target: last.target.clone(),
            command: "list.toggle-task",
        })
        .unwrap();
        assert!(doc
            .source
            .text()
            .ends_with(&format!("- [x] 中😀 999{newline}")));
        assert_eq!(doc.history_len(false), 1);
        doc.undo(false);
        assert_eq!(doc.source.text(), raw);
    }
}

#[test]
fn indexed_platform_ranges_match_overlay_and_do_not_publish_preedit() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw = format!("a😀中{newline}tail");
        for range in [0..0, 1..5, 5..raw.len()] {
            let mut doc = new_document(&raw);
            let source = doc.source.id();
            doc.update_composition(Some(range), "e\u{301}😀", None)
                .unwrap();
            let overlay = doc.editable_text();
            for at in 0..=overlay.len() + 1 {
                let expected = overlay
                    .get(..at)
                    .map(|s| s.encode_utf16().count())
                    .ok_or(editable_markdown_parser::Error::InvalidRange);
                assert_eq!(doc.editable_byte_to_utf16(at), expected);
            }
            for at in 0..=overlay.encode_utf16().count() + 1 {
                assert_eq!(
                    doc.editable_utf16_to_byte(at),
                    editable_markdown_parser::utf16_to_utf8(&overlay, at)
                );
            }
            assert!(doc.editable_utf16_to_byte(usize::MAX).is_err());
            assert_eq!(doc.source.id(), source);
            assert!(!doc.dirty());
            assert_eq!(doc.history_len(false), 0);
            doc.cancel_composition();
            assert!(matches!(
                doc.editable_text_cow(),
                std::borrow::Cow::Borrowed(_)
            ));
        }
    }
}
