//! Runtime host boundaries. Startup registration and aborts are outside this boundary.
use crate::*;

/// A bounded, deduplicated log shared with detached jobs. Readers obtain owned
/// entries rather than mutable access to the backing collection.
#[derive(Clone, Default)]
pub struct DiagnosticLog(Arc<Mutex<Vec<Diagnostic>>>);
impl DiagnosticLog {
    pub fn record(&self, mut diagnostic: Diagnostic) {
        fn limit(text: &mut String, bytes: usize) {
            let mut end = text.len().min(bytes);
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            // Reallocate even a short value: a provider may pass a String with
            // a huge spare capacity, which truncate alone would keep alive.
            *text = text[..end].to_owned();
        }
        limit(&mut diagnostic.feature, 256);
        limit(&mut diagnostic.message, 4096);
        let mut log = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if !log.contains(&diagnostic) {
            if log.len() >= 64 {
                log.remove(0);
            }
            log.push(diagnostic);
        }
    }
    pub fn entries(&self) -> Vec<Diagnostic> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    pub fn len(&self) -> usize {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub fn catch_callback<T>(
    feature: &str,
    operation: &str,
    call: impl FnOnce() -> T,
) -> Result<T, Diagnostic> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)).map_err(|_| Diagnostic {
        feature: feature.into(),
        message: format!("{operation} callback panicked; retaining editable source"),
    })
}

/// One adapter per document; raw services remain available for fresh sessions.
pub struct GuardedFeatureEngine {
    inner: Arc<dyn FeatureEngine>,
    diagnostics: DiagnosticLog,
}
impl GuardedFeatureEngine {
    pub fn new(inner: Arc<dyn FeatureEngine>, diagnostics: DiagnosticLog) -> Self {
        Self { inner, diagnostics }
    }
    fn call<T>(&self, operation: &str, call: impl FnOnce() -> T) -> Result<T, String> {
        catch_callback("features", operation, call).map_err(|diagnostic| {
            let message = diagnostic.message.clone();
            self.diagnostics.record(diagnostic);
            message
        })
    }
}
impl FeatureEngine for GuardedFeatureEngine {
    fn presentation_key(&self, state: &EditingState) -> Option<PresentationKey> {
        self.call("presentation_key", || self.inner.presentation_key(state))
            .ok()
            .flatten()
    }
    fn cache_source_controls(&self) -> bool {
        self.call("cache_source_controls", || {
            self.inner.cache_source_controls()
        })
        .unwrap_or(false)
    }
    fn render_control_batch_cached(
        &self,
        state: &EditingState,
        cache: &mut RenderControlCache,
    ) -> Result<CachedRenderControlBatch, String> {
        let result = self.call("render_control_batch_cached", || {
            self.inner.render_control_batch_cached(state, cache)
        });
        let batch = match result {
            Ok(Ok(batch)) => batch,
            Ok(Err(error)) | Err(error) => {
                cache.clear();
                return Err(error);
            }
        };
        for diagnostic in &batch.diagnostics {
            self.diagnostics.record(diagnostic.clone());
        }
        Ok(batch)
    }
    fn snapshot(&self) -> Option<Arc<dyn FeatureEngine>> {
        self.call("snapshot", || self.inner.snapshot())
            .ok()
            .flatten()
            .map(|inner| {
                Arc::new(Self::new(inner, self.diagnostics.clone())) as Arc<dyn FeatureEngine>
            })
    }
    fn plan(&self, intent: EditIntent, state: &EditingState) -> Result<CommandPlan, String> {
        self.call("plan", || self.inner.plan(intent, state))?
    }
    fn display(
        &self,
        state: &EditingState,
        highlighter: Option<&mut dyn SyntaxHighlighter>,
        diagrams: &[DiagramViewInput],
    ) -> Display {
        // project_checked owns this boundary and its diagnostic-bearing fallback,
        // so a failed projection can never enter the successful projection cache.
        self.inner.display(state, highlighter, diagrams)
    }
    fn block_window(&self, state: &EditingState) -> Option<Range<usize>> {
        self.call("block_window", || self.inner.block_window(state))
            .unwrap_or_else(|_| state.block_source.as_ref().map(|s| s.range.clone()))
    }
    fn protects_table_boundary(&self, state: &EditingState, forward: bool) -> bool {
        self.call("protects_table_boundary", || {
            self.inner.protects_table_boundary(state, forward)
        })
        .unwrap_or(true)
    }
    fn preview_inputs(&self, state: &EditingState) -> Vec<PreviewInput> {
        let batch = self
            .call("preview_inputs", || self.inner.preview_batch(state))
            .unwrap_or_default();
        for diagnostic in batch.diagnostics {
            self.diagnostics.record(diagnostic);
        }
        batch.inputs
    }
    fn preview_batch_cached(&self, state: &EditingState, cache: &mut PreviewCache) -> PreviewBatch {
        let batch = self.call("preview_inputs", || {
            self.inner.preview_batch_cached(state, cache)
        });
        let batch = match batch {
            Ok(batch) => batch,
            Err(_) => {
                cache.clear();
                PreviewBatch::default()
            }
        };
        for diagnostic in &batch.diagnostics {
            self.diagnostics.record(diagnostic.clone());
        }
        batch
    }
    fn block_ui(&self, block: &ryne_syntax::block::Block, source_open: bool) -> AdornmentSpec {
        self.call("block_ui", || self.inner.block_ui(block, source_open))
            .unwrap_or_else(|_| AdornmentSpec {
                metadata: BlockMetadata::from(block.kind),
                menu: vec![MenuItemSpec {
                    command: BlockCommand::Source,
                    name: "source",
                    order: 0,
                }],
            })
    }
    fn insert_templates(&self) -> &[(&'static str, InsertBlock)] {
        self.call("insert_templates", || self.inner.insert_templates())
            .unwrap_or(&[])
    }
    fn render_controls(&self, state: &EditingState) -> Result<Vec<RenderControl>, String> {
        self.render_control_batch(state).map(|b| b.controls)
    }
    fn render_control_batch(&self, state: &EditingState) -> Result<RenderControlBatch, String> {
        let batch = self.call("render_control_batch", || {
            self.inner.render_control_batch(state)
        })??;
        for diagnostic in &batch.diagnostics {
            self.diagnostics.record(diagnostic.clone());
        }
        Ok(batch)
    }
    fn plan_render_action(
        &self,
        state: &EditingState,
        request: &RenderActionRequest,
    ) -> Result<CommandPlan, String> {
        self.call("plan_render_action", || {
            self.inner.plan_render_action(state, request)
        })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_log_bounds_duplicates_count_bytes_and_unicode() {
        let log = DiagnosticLog::default();
        for i in 0..100 {
            let value = Diagnostic {
                feature: format!("provider-{i}"),
                message: "中😀".repeat(2000),
            };
            log.record(value.clone());
            log.record(value);
        }
        let entries = log.entries();
        assert_eq!(entries.len(), 64);
        assert_eq!(entries[0].feature, "provider-36");
        assert!(entries.iter().all(|d| d.message.len() <= 4096));
        log.record(Diagnostic {
            feature: "😀".repeat(200),
            message: "last".into(),
        });
        assert!(log.entries().last().unwrap().feature.len() <= 256);
        let mut oversized_capacity = String::with_capacity(1024 * 1024);
        oversized_capacity.push_str("short");
        log.record(Diagnostic {
            feature: "capacity".into(),
            message: oversized_capacity,
        });
        assert!(log.0.lock().unwrap().last().unwrap().message.capacity() <= 4096);
    }
}
