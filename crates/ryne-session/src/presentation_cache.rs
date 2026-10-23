//! One bounded, disposable frame cache per session. Preedit never enters it.
use super::*;

pub struct CachedProjection {
    pub display: Arc<ryne_projection::IndexedDisplay>,
    pub diagnostics: Vec<Diagnostic>,
}
pub struct CachedControls {
    pub controls: Arc<Vec<RenderControl>>,
    pub diagnostics: Vec<Diagnostic>,
}
struct ProjectionEntry {
    key: PresentationKey,
    highlighter: Option<u64>,
    diagrams: Vec<DiagramViewInput>,
    display: Arc<ryne_projection::IndexedDisplay>,
}
#[derive(Default)]
pub(super) struct PresentationCache {
    projection: Option<ProjectionEntry>,
    controls: Option<(ryne_core::DocumentKey, bool, Arc<Vec<RenderControl>>)>,
}
fn same_diagrams(a: &[DiagramViewInput], b: &[DiagramViewInput]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|((ar, a), (br, b))| {
            ar == br
                && match (a, b) {
                    (Ok(a), Ok(b)) => Arc::ptr_eq(a, b),
                    (Err(a), Err(b)) => a == b,
                    _ => false,
                }
        })
}
impl EditorSession {
    /// Reuse only explicitly declared feature inputs and completed highlighter work.
    /// Callers must keep the same highlighter instance for the session.
    pub fn cached_project_batch(
        &self,
        state: &EditingState,
        mut highlighter: Option<&mut dyn SyntaxHighlighter>,
        diagrams: &[DiagramViewInput],
    ) -> CachedProjection {
        let key = if self.composition.is_none() {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.services.features.presentation_key(state)
            }))
            .ok()
            .flatten()
        } else {
            None
        };
        let generation = |h: &Option<&mut dyn SyntaxHighlighter>| match h {
            None => Some(None),
            Some(h) => std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                h.presentation_generation()
            }))
            .ok()
            .flatten()
            .map(Some),
        };
        if let (Some(key), Some(generation)) = (&key, generation(&highlighter))
            && let Some(entry) = &self.presentation_cache.borrow().projection
            && entry.key == *key
            && entry.highlighter == generation
            && same_diagrams(&entry.diagrams, diagrams)
        {
            return CachedProjection {
                display: entry.display.clone(),
                diagnostics: Vec::new(),
            };
        }
        let batch = match highlighter.as_mut() {
            Some(h) => self.project_batch(state, Some(&mut **h), diagrams),
            None => self.project_batch(state, None, diagrams),
        };
        let display = Arc::new(ryne_projection::IndexedDisplay::new(batch.display));
        self.presentation_cache.borrow_mut().projection = key
            .zip(generation(&highlighter))
            .filter(|_| batch.diagnostics.is_empty())
            .map(|(key, highlighter)| ProjectionEntry {
                key,
                highlighter,
                diagrams: diagrams.to_vec(),
                display: display.clone(),
            });
        CachedProjection {
            display,
            diagnostics: batch.diagnostics,
        }
    }
    pub fn cached_render_control_batch(
        &self,
        state: &EditingState,
    ) -> Result<CachedControls, String> {
        let cacheable = self.composition.is_none()
            && std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.services.features.cache_source_controls()
            }))
            .unwrap_or(false);
        if cacheable
            && let Some((document, mode, controls)) = &self.presentation_cache.borrow().controls
            && *document == state.source.id()
            && *mode == state.source_mode
        {
            return Ok(CachedControls {
                controls: controls.clone(),
                diagnostics: Vec::new(),
            });
        }
        self.presentation_cache.borrow_mut().controls = None;
        let batch = self.project_render_control_batch(state)?;
        let controls = Arc::new(batch.controls);
        if cacheable && batch.diagnostics.is_empty() {
            self.presentation_cache.borrow_mut().controls =
                Some((state.source.id(), state.source_mode, controls.clone()));
        }
        Ok(CachedControls {
            controls,
            diagnostics: batch.diagnostics,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regression_tests::new_document;

    fn projected(doc: &EditorSession) -> Arc<ryne_projection::IndexedDisplay> {
        let batch = doc.cached_project_batch(doc, None, &[]);
        assert!(batch.diagnostics.is_empty());
        assert_eq!(*batch.display, doc.display());
        batch.display
    }
    #[test]
    fn caret_reuses_projection_and_controls_but_visibility_and_edits_invalidate() {
        for newline in ["\n", "\r\n", "\r"] {
            let raw = concat!(
                "# heading\n\nplain 中😀 **bold** tail\n\n",
                "- item\n  continuation\n  - nested\n\n",
                "> quote\n> second\n\n",
                "| a | b |\n| --- | --- |\n| x | y |\n\n",
                "```rust\nlet value = 1;\n```\n"
            )
            .replace('\n', newline);
            let mut doc = new_document(&raw);
            doc.select(raw.find("plain").unwrap(), false, false);
            let first = projected(&doc);
            let controls = doc.cached_render_control_batch(&doc).unwrap().controls;
            let snapshot = doc.source.id();
            for word in [
                "中",
                "😀",
                "tail",
                "item",
                "continuation",
                "nested",
                "quote",
                "second",
                "x | y",
                "value",
            ] {
                doc.select(raw.find(word).unwrap(), false, false);
                assert!(
                    Arc::ptr_eq(&first, &projected(&doc)),
                    "{word:?} {newline:?}"
                );
                assert!(Arc::ptr_eq(
                    &controls,
                    &doc.cached_render_control_batch(&doc).unwrap().controls
                ));
            }
            doc.select(raw.find("bold").unwrap() + 1, false, false);
            let inline = projected(&doc);
            assert!(!Arc::ptr_eq(&first, &inline));
            doc.select(raw.find("bold").unwrap() + 2, false, false);
            assert!(Arc::ptr_eq(&inline, &projected(&doc)));
            doc.select(raw.find("heading").unwrap(), false, false);
            let heading = projected(&doc);
            doc.reveal_clicked_heading();
            assert!(!Arc::ptr_eq(&heading, &projected(&doc)));
            doc.select(raw.find("item").unwrap(), false, false);
            let normal = projected(&doc);
            doc.set_block_source(Some(0..raw.len()));
            let local = projected(&doc);
            assert!(!Arc::ptr_eq(&normal, &local));
            doc.set_block_source(None);
            projected(&doc);
            doc.set_source_mode(true);
            let source = projected(&doc);
            assert!(doc
                .cached_render_control_batch(&doc)
                .unwrap()
                .controls
                .is_empty());
            doc.select(0, false, false);
            let source_again = projected(&doc);
            assert!(Arc::ptr_eq(&source, &source_again));
            assert_eq!(source.text, source_again.text);
            doc.set_source_mode(false);
            let before = projected(&doc);
            assert_eq!(doc.source.id(), snapshot);
            assert!(!doc.dirty());
            assert_eq!(doc.history_len(false), 0);
            doc.insert_text(0..0, "new").unwrap();
            assert!(!Arc::ptr_eq(&before, &projected(&doc)));
            assert!(!Arc::ptr_eq(
                &controls,
                &doc.cached_render_control_batch(&doc).unwrap().controls
            ));
            doc.undo(false);
            assert_eq!(doc.source.text(), raw);
            projected(&doc);
            doc.undo(true);
            projected(&doc);
        }
    }
    #[test]
    fn preedit_is_uncached_and_cancel_restores_authoritative_projection() {
        let mut doc = new_document("plain 中😀 tail");
        let before = projected(&doc);
        let snapshot = doc.source.id();
        doc.update_composition(Some(0..5), "输入", Some(6..6))
            .unwrap();
        let (state, views) = doc.projection_state(&[]);
        let a = doc.cached_project_batch(&state, None, &views).display;
        let b = doc.cached_project_batch(&state, None, &views).display;
        assert!(!Arc::ptr_eq(&a, &b));
        assert_eq!(*a, doc.project(&state, None, &views));
        assert_ne!(a.text, before.text);
        assert_eq!(doc.source.id(), snapshot);
        assert!(doc.cancel_composition());
        assert_eq!(*before, *projected(&doc));
        assert!(!doc.dirty());
        assert_eq!(doc.history_len(false), 0);
    }
    #[test]
    fn font_and_diagram_resources_are_part_of_projection_identity() {
        let formulas = Arc::new(ryne_services::math::FormularyRenderer::default());
        let mut services = crate::regression_tests::services();
        services.features =
            Arc::new(ryne_features::MarkdownFeatures::new(formulas.clone(), vec![]).unwrap());
        let doc = EditorSession::new("$x^2$\n\nplain", services);
        let first = projected(&doc);
        formulas.set_font(MathFont::StixTwo);
        let second = projected(&doc);
        assert!(!Arc::ptr_eq(&first, &second));
        assert!(Arc::ptr_eq(&second, &projected(&doc)));
        let doc = new_document("```mermaid\nflowchart LR\nA-->B\n```\n\ntail");
        let first = projected(&doc);
        let range = ryne_syntax::code_block::blocks(&doc.parse)[0].range.clone();
        let diagram = Arc::new(ryne_projection::Diagram {
            pixels: vec![0, 0, 0, 255].into(),
            pixel_width: 1,
            pixel_height: 1,
            width: 1.,
            height: 1.,
            bytes: 4,
        });
        let views = vec![(range.clone(), Ok(diagram))];
        let rendered = doc.cached_project_batch(&doc, None, &views).display;
        assert!(!Arc::ptr_eq(&first, &rendered));
        assert_eq!(*rendered, doc.project(&doc, None, &views));
        assert!(Arc::ptr_eq(
            &rendered,
            &doc.cached_project_batch(&doc, None, &views).display
        ));
        let error = vec![(range, Err("failed".into()))];
        let failed = doc.cached_project_batch(&doc, None, &error).display;
        assert!(!Arc::ptr_eq(&rendered, &failed));
        assert_eq!(*failed, doc.project(&doc, None, &error));
    }
    #[test]
    fn unregistered_cache_contract_keeps_recomputing() {
        let mut services = crate::regression_tests::services();
        services.features = Arc::new(crate::basic::SourceEditing);
        let doc = EditorSession::new("literal", services);
        let first = projected(&doc);
        assert!(!Arc::ptr_eq(&first, &projected(&doc)));
        let a = doc.cached_render_control_batch(&doc).unwrap().controls;
        let b = doc.cached_render_control_batch(&doc).unwrap().controls;
        assert!(!Arc::ptr_eq(&a, &b));
    }
    #[test]
    fn deferred_language_detection_gets_another_pass_before_projection_is_reused() {
        let raw = (0..10)
            .map(|i| format!("```\nprint({i})\n```\n\n"))
            .collect::<String>()
            + "tail";
        let mut doc = new_document(&raw);
        doc.select(raw.len(), false, false);
        let mut highlighter = ryne_services::highlight::CodeHighlighter::default();
        let first = doc
            .cached_project_batch(&doc, Some(&mut highlighter), &[])
            .display;
        assert!(highlighter.presentation_generation().is_none());
        let complete = doc
            .cached_project_batch(&doc, Some(&mut highlighter), &[])
            .display;
        assert!(!Arc::ptr_eq(&first, &complete));
        assert!(highlighter.presentation_generation().is_some());
        let reused = doc
            .cached_project_batch(&doc, Some(&mut highlighter), &[])
            .display;
        assert!(Arc::ptr_eq(&complete, &reused));
        assert_eq!(*complete, doc.highlighted_display(&mut highlighter));
    }
    #[test]
    fn unknown_highlighter_and_additional_features_do_not_opt_in_implicitly() {
        struct Unknown;
        impl SyntaxHighlighter for Unknown {
            fn begin_frame(&mut self) {}
            fn end_frame(&mut self) {}
            fn spans(
                &mut self,
                _: &str,
                _: &ryne_syntax::code_block::CodeBlock,
            ) -> Vec<HighlightSpan> {
                Vec::new()
            }
        }
        let doc = new_document("plain");
        let mut highlighter = Unknown;
        let a = doc
            .cached_project_batch(&doc, Some(&mut highlighter), &[])
            .display;
        let b = doc
            .cached_project_batch(&doc, Some(&mut highlighter), &[])
            .display;
        assert!(!Arc::ptr_eq(&a, &b));
        struct Additional;
        impl BuiltinFeature for Additional {
            fn manifest(&self) -> FeatureManifest {
                FeatureManifest::builtin("additional")
            }
            fn register(&self, _: &mut Registrar) -> Result<(), String> {
                Ok(())
            }
        }
        let features = ryne_features::MarkdownFeatures::new(
            Arc::new(ryne_services::math::FormularyRenderer::default()),
            vec![],
        )
        .unwrap()
        .with_feature(Arc::new(Additional))
        .unwrap();
        assert!(features.presentation_key(&doc).is_none());
        assert!(!features.cache_source_controls());
    }
    #[test]
    fn failed_highlighting_is_retried_instead_of_caching_its_fallback() {
        struct Flaky(usize);
        impl SyntaxHighlighter for Flaky {
            fn presentation_generation(&self) -> Option<u64> {
                Some(0)
            }
            fn begin_frame(&mut self) {
                self.0 += 1;
            }
            fn end_frame(&mut self) {}
            fn spans(
                &mut self,
                _: &str,
                _: &ryne_syntax::code_block::CodeBlock,
            ) -> Vec<HighlightSpan> {
                if self.0 == 1 {
                    vec![HighlightSpan {
                        range: usize::MAX..usize::MAX,
                        class: ryne_projection::Class::Keyword,
                    }]
                } else {
                    Vec::new()
                }
            }
        }
        let doc = new_document("```rust\nlet x = 1;\n```\n\ntail");
        let mut highlighter = Flaky(0);
        let first = doc.cached_project_batch(&doc, Some(&mut highlighter), &[]);
        assert!(!first.diagnostics.is_empty());
        let recovered = doc.cached_project_batch(&doc, Some(&mut highlighter), &[]);
        assert!(recovered.diagnostics.is_empty());
        assert!(!Arc::ptr_eq(&first.display, &recovered.display));
        let reused = doc.cached_project_batch(&doc, Some(&mut highlighter), &[]);
        assert!(Arc::ptr_eq(&recovered.display, &reused.display));
        assert_eq!(highlighter.0, 2);
    }
}
