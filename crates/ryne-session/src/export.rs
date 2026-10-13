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
            features: self.services.features.clone(),
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
        Ok(self.features.display(&state, None, &views))
    }
}
