//! A detached snapshot for export. No caret, source-mode, history or live jobs change.
use super::*;
pub struct ExportSnapshot {
    source: Source,
    features: Arc<dyn FeatureEngine>,
}
impl EditorSession {
    pub fn export_snapshot(&self) -> ExportSnapshot {
        ExportSnapshot {
            source: self.source.clone(),
            features: self
                .services
                .features
                .snapshot()
                .unwrap_or_else(|| self.services.features.clone()),
        }
    }
}
impl ExportSnapshot {
    /// Run on a background executor. Preview failures retain editable code in the projection.
    pub fn render(self) -> Result<Display, String> {
        if self.source.text().len() > 4 * 1024 * 1024 {
            return Err("PDF export exceeds 4 MiB of source".into());
        }
        let state = EditingState::new(self.source);
        let mut views = Vec::new();
        let mut bytes = 0;
        for input in self.features.preview_inputs(&state).into_iter().take(32) {
            let result = input.input.and_then(|source| {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    input.renderer.render(&source, CancelControl::default())
                }))
                .unwrap_or_else(|_| Err("Diagram rendering failed during export".into()))
            });
            let result = result.and_then(|diagram| {
                bytes += diagram.bytes;
                if bytes > 32 * 1024 * 1024 {
                    Err("Export diagram budget exceeds 32 MiB".into())
                } else {
                    Ok(diagram)
                }
            });
            views.push((input.block.range, result));
        }
        Ok(project_checked(self.features.as_ref(), &state, None, &views).display)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ryne_extensions::{FormulaRenderer, MathFont};
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

        // Exercise both actual PDF encoders with each embedded font. A formula-
        // only document does not depend on platform fallback text fonts.
        let export_document = document.fork("$\\frac{a_1}{\\sqrt{b}}$\n");
        let mut displays = Vec::new();
        for font in MathFont::ALL {
            formulas.set_font(font);
            displays.push(export_document.export_snapshot().render().unwrap());
        }
        for exporter in [
            &ryne_services::pdf::PdfExporter as &dyn ryne_extensions::PdfExporter,
            &ryne_services::pdf_v2::KrillaPdfExporter as &dyn ryne_extensions::PdfExporter,
        ] {
            let lete_pdf = exporter.export(&displays[0]).unwrap();
            let stix_pdf = exporter.export(&displays[1]).unwrap();
            assert!(lete_pdf.starts_with(b"%PDF-"));
            assert!(stix_pdf.starts_with(b"%PDF-"));
            assert_ne!(lete_pdf, stix_pdf);
        }
    }
}
