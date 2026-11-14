//! A detached snapshot for export. No caret, source-mode, history or live jobs change.
use super::*;
pub struct ExportSnapshot {
    source: Source,
    features: Option<Arc<dyn FeatureEngine>>,
}
impl EditorSession {
    pub fn export_snapshot(&self) -> ExportSnapshot {
        ExportSnapshot {
            source: self.source.clone(),
            features: self.features.snapshot(),
        }
    }
}
impl ExportSnapshot {
    /// Run on a background executor. Preview failures retain editable code in the projection.
    pub fn render(self) -> Result<Display, String> {
        if self.source.text().len() > 4 * 1024 * 1024 {
            return Err("PDF export exceeds 4 MiB of source".into());
        }
        let features = self
            .features
            .ok_or("Export requires a frozen feature snapshot")?;
        let state = EditingState::new(self.source);
        let mut views = Vec::new();
        let mut bytes = 0;
        for input in features.preview_inputs(&state).into_iter().take(32) {
            let result = input.input.and_then(|source| {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    input.renderer.render(&source, CancelControl::default())
                }))
                .unwrap_or_else(|_| Err("Diagram rendering failed during export".into()))
            });
            let result = result
                .and_then(ryne_extensions::validate_diagram)
                .and_then(|diagram| {
                    bytes += diagram.bytes;
                    if bytes > 32 * 1024 * 1024 {
                        Err("Export diagram budget exceeds 32 MiB".into())
                    } else {
                        Ok(diagram)
                    }
                });
            views.push((input.block.range, result));
        }
        Ok(project_checked(features.as_ref(), &state, None, &views).display)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ryne_extensions::{FormulaRenderer, MathFont};

    #[test]
    fn detached_export_survives_later_edits_without_touching_history_or_preedit() {
        for newline in ["\n", "\r\n", "\r"] {
            let original = "# Original\n\n**bold** $x^2$\n".replace('\n', newline);
            let committed = "# Changed\n\nnew committed body\n".replace('\n', newline);
            let mut document = super::super::regression_tests::new_document(&original);
            document.set_source_mode(true);
            document.set_block_source(Some(0..original.len()));
            document.files.path = Some("document.md".into());
            let snapshot = document.export_snapshot();
            document.replace(0..original.len(), &committed).unwrap();
            document.select(committed.len(), false, false);
            let current_snapshot = document.export_snapshot();
            document.update_composition(None, "未确认😀", None).unwrap();
            let source = document.source.clone();
            let selection = document.selection();
            let preedit = document.composition().cloned();
            let history = (document.history_len(false), document.history_len(true));
            let block_source = document.block_source.as_ref().map(|s| s.range.clone());

            let original_display = snapshot.render().unwrap();
            let current_display = current_snapshot.render().unwrap();
            assert!(original_display.text.starts_with("Original\n"));
            assert_eq!(original_display.formulas.len(), 1);
            assert!(!original_display.text.contains("Changed"));
            assert!(current_display.text.starts_with("Changed\n"));
            assert!(current_display.text.contains("new committed body"));
            assert!(!current_display.text.contains("未确认"));
            let exporter = &ryne_services::pdf::PdfExporter as &dyn ryne_extensions::PdfExporter;
            for display in [&original_display, &current_display] {
                let bytes = exporter.export(display).unwrap();
                assert!(bytes.starts_with(b"%PDF-1.7"));
                assert!(bytes.ends_with(b"%%EOF"));
            }
            assert_eq!(document.source.id(), source.id());
            assert_eq!(document.source.text(), committed);
            assert_eq!(document.selection(), selection);
            assert_eq!(document.composition(), preedit.as_ref());
            assert_eq!(
                (document.history_len(false), document.history_len(true)),
                history
            );
            assert_eq!(
                document.block_source.as_ref().map(|s| s.range.clone()),
                block_source
            );
            assert!(document.source_mode);
            assert!(document.dirty());
            assert_eq!(
                document.files.path.as_deref(),
                Some(Path::new("document.md"))
            );
            assert!(!document.files.saving);
            assert!(document.cancel_composition());
            document.undo(false);
            assert_eq!(document.source.text(), original);
            assert!(!document.dirty());
            document.undo(true);
            assert_eq!(document.source.text(), committed);
        }
    }

    #[test]
    fn export_keeps_invalid_incomplete_and_oversized_mermaid_as_literal_code() {
        for (body, closed) in [
            ("not a diagram".to_string(), true),
            ("flowchart LR\nA[Unclosed --> B".into(), true),
            ("flowchart LR\nA-->B".into(), false),
            ("x".repeat(32 * 1024 + 1), true),
            (format!("flowchart LR\n{}", "A-->B\n".repeat(257)), true),
        ] {
            let raw = format!("```mermaid\n{body}\n{}", if closed { "```\n" } else { "" });
            let document = super::super::regression_tests::new_document(&raw);
            let source = document.source.id();
            let display = document.export_snapshot().render().unwrap();
            assert!(display.diagrams.is_empty());
            assert_eq!(display.code_blocks.len(), 1);
            assert!(display.text.contains(&body));
            assert_eq!(document.source.id(), source);
            assert_eq!(document.source.text(), raw);
            assert_eq!(document.history_len(false), 0);
            assert!(!document.dirty());
        }
    }

    #[test]
    fn export_freezes_math_font_and_preserves_the_live_document() {
        let formulas = Arc::new(ryne_services::math::FormularyRenderer::default());
        let features = ryne_features::MarkdownFeatures::new(formulas.clone(), vec![]).unwrap();
        let services = SessionServices {
            features: Arc::new(features),
            files: Arc::new(ryne_services::files::AtomicFileStore),
        };
        let mut document = EditorSession::new(
            "plain😀\r\n\r\n$\\frac{a_1}{\\sqrt{b}}$\r\n\r\n- $x^2$\r\n\r\n| f |\r\n| --- |\r\n| $x_i$ |\r\n",
            services,
        );
        document.select(5, false, false);
        let source = document.source.clone();
        let selection = document.selection();
        let history = (document.history_len(false), document.history_len(true));
        let lete = document.export_snapshot();
        formulas.set_font(MathFont::StixTwo);
        let stix = document.export_snapshot();
        let lete = lete.render().unwrap();
        let stix = stix.render().unwrap();
        assert_eq!(lete.formulas.len(), 3);
        assert_eq!(stix.formulas.len(), 3);
        for (lete, stix) in lete.formulas.iter().zip(&stix.formulas) {
            assert!(lete.formula.path.starts_with("math:lete-sans-math:"));
            assert!(stix.formula.path.starts_with("math:stix-two-math:"));
            assert_ne!(lete.formula.svg, stix.formula.svg);
        }
        assert_eq!(document.source.id(), source.id());
        assert_eq!(document.source.text(), source.text());
        assert_eq!(document.selection(), selection);
        assert_eq!(
            (document.history_len(false), document.history_len(true)),
            history
        );
        assert!(!document.dirty());

        // Exercise the PDF encoder with each embedded font. A formula-
        // only document does not depend on platform fallback text fonts.
        let export_document = document.fork("$\\frac{a_1}{\\sqrt{b}}$\n");
        let mut displays = Vec::new();
        for font in MathFont::ALL {
            formulas.set_font(font);
            displays.push(export_document.export_snapshot().render().unwrap());
        }
        let exporter = &ryne_services::pdf::PdfExporter as &dyn ryne_extensions::PdfExporter;
        let lete_pdf = exporter.export(&displays[0]).unwrap();
        let stix_pdf = exporter.export(&displays[1]).unwrap();
        assert!(lete_pdf.starts_with(b"%PDF-"));
        assert!(stix_pdf.starts_with(b"%PDF-"));
        assert_ne!(lete_pdf, stix_pdf);
    }
    #[test]
    fn export_freezes_preview_renderer_and_rejects_invalid_resources() {
        use std::sync::atomic::{AtomicU64, Ordering};
        struct Renderer {
            value: Arc<AtomicU64>,
            frozen: Option<u64>,
            invalid: bool,
        }
        impl DiagramRenderer for Renderer {
            fn generation(&self) -> u64 {
                self.frozen
                    .unwrap_or_else(|| self.value.load(Ordering::SeqCst))
            }
            fn snapshot(&self) -> Option<Arc<dyn DiagramRenderer>> {
                Some(Arc::new(Self {
                    value: self.value.clone(),
                    frozen: Some(self.generation()),
                    invalid: self.invalid,
                }))
            }
            fn render(&self, _: &str, _: CancelControl) -> ryne_projection::DiagramOutcome {
                Ok(Arc::new(ryne_projection::Diagram {
                    pixels: vec![self.generation() as u8, 0, 0, 255].into(),
                    pixel_width: if self.invalid { 2 } else { 1 },
                    pixel_height: 1,
                    width: 1.,
                    height: 1.,
                    bytes: 4,
                }))
            }
        }
        for invalid in [false, true] {
            let value = Arc::new(AtomicU64::new(1));
            let renderer = Arc::new(Renderer {
                value: value.clone(),
                frozen: None,
                invalid,
            });
            let features = ryne_features::MarkdownFeatures::new(
                Arc::new(ryne_services::math::FormularyRenderer::default()),
                vec![ryne_features::diagrams::FencedPreview {
                    id: "preview",
                    language: "diagram",
                    renderer,
                }],
            )
            .unwrap();
            let services = SessionServices {
                features: Arc::new(features),
                files: Arc::new(ryne_services::files::AtomicFileStore),
            };
            let document = EditorSession::new("```diagram\ncontent\n```\n\ntail", services);
            let snapshot = document.export_snapshot();
            value.store(2, Ordering::SeqCst);
            let display = snapshot.render().unwrap();
            if invalid {
                assert!(display.diagrams.is_empty());
                assert!(display.text.contains("content"));
            } else {
                assert_eq!(display.diagrams.len(), 1);
                assert_eq!(display.diagrams[0].diagram.pixels[0], 1);
            }
        }
    }
    #[test]
    fn export_rejects_unfrozen_additional_module_without_touching_document() {
        struct Extra;
        impl BuiltinFeature for Extra {
            fn manifest(&self) -> FeatureManifest {
                FeatureManifest::builtin("extra-export-test")
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
        .with_feature(Arc::new(Extra))
        .unwrap();
        let services = SessionServices {
            features: Arc::new(features),
            files: Arc::new(ryne_services::files::AtomicFileStore),
        };
        let document = EditorSession::new("source", services);
        let error = document.export_snapshot().render().err().unwrap();
        assert!(error.contains("frozen feature snapshot"));
        assert_eq!(document.source.text(), "source");
        assert!(!document.dirty());
        assert_eq!(document.history_len(false), 0);
    }
}
