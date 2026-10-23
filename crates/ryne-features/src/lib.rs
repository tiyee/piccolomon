//! Static Markdown features. All planning and presentation are GUI independent.
mod block_ui;
mod builtins;
mod code_actions;
mod commands;
pub mod diagrams;
pub mod input;
mod list_actions;
mod math;
pub mod presentation;
mod table_actions;
pub mod templates;
use commands::Planner;
pub use ryne_extensions::InsertBlock;
use ryne_extensions::*;
use ryne_projection::{DiagramViewInput, Display};
use std::sync::Arc;

#[derive(Clone)]
pub struct MarkdownFeatures {
    formulas: Arc<dyn FormulaRenderer>,
    registry: Registry,
    previews: Vec<diagrams::FencedPreview>,
    additional: Vec<Arc<dyn BuiltinFeature + Send + Sync>>,
}
impl MarkdownFeatures {
    pub fn new(
        formulas: Arc<dyn FormulaRenderer>,
        previews: Vec<diagrams::FencedPreview>,
    ) -> Result<Self, String> {
        let registry = builtins::registry(&formulas, &previews, &[])?;
        Ok(Self {
            formulas,
            previews,
            registry,
            additional: Vec::new(),
        })
    }
    fn feature_services(&self) -> FeatureServices<'_> {
        FeatureServices {
            formulas: self.formulas.as_ref(),
        }
    }
    pub fn with_feature(
        mut self,
        feature: Arc<dyn BuiltinFeature + Send + Sync>,
    ) -> Result<Self, String> {
        self.additional.push(feature);
        self.registry = builtins::registry(&self.formulas, &self.previews, &self.additional)?;
        Ok(self)
    }
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
    pub fn with_render_action_provider(
        self,
        provider: Arc<dyn RenderActionProvider>,
    ) -> Result<Self, String> {
        self.with_feature(Arc::new(builtins::ControlAdapter(provider)))
    }
}
impl FeatureEngine for MarkdownFeatures {
    fn presentation_key(&self, state: &EditingState) -> Option<PresentationKey> {
        if !self.additional.is_empty() {
            return None;
        }
        Some(PresentationKey {
            document: state.source.id(),
            source_mode: state.source_mode,
            inline: (!state.source_mode)
                .then(|| state.reveal.as_ref().map(|s| s.range.clone()))
                .flatten(),
            heading: (!state.source_mode)
                .then(|| state.heading_reveal.as_ref().map(|s| s.range.clone()))
                .flatten(),
            block: (!state.source_mode)
                .then(|| self.block_window(state))
                .flatten(),
            generation: self.formulas.presentation_generation()?,
        })
    }
    fn cache_source_controls(&self) -> bool {
        self.additional.is_empty()
    }
    fn snapshot(&self) -> Option<Arc<dyn FeatureEngine>> {
        let mut frozen = self.clone();
        frozen.formulas = self.formulas.snapshot()?;
        Some(Arc::new(frozen))
    }
    fn render_controls(&self, state: &EditingState) -> Result<Vec<RenderControl>, String> {
        self.registry.render_actions().controls(state)
    }
    fn render_control_batch(&self, state: &EditingState) -> Result<RenderControlBatch, String> {
        Ok(self.registry.render_actions().control_batch(state))
    }
    fn plan_render_action(
        &self,
        state: &EditingState,
        request: &RenderActionRequest,
    ) -> Result<CommandPlan, String> {
        self.registry.render_actions().plan(state, request)
    }
    fn plan(&self, intent: EditIntent, state: &EditingState) -> Result<CommandPlan, String> {
        self.registry
            .plan(&intent, state, &self.feature_services())?
            .ok_or_else(|| "no registered command handler".into())
    }
    fn display(
        &self,
        state: &EditingState,
        highlighter: Option<&mut dyn SyntaxHighlighter>,
        diagrams: &[DiagramViewInput],
    ) -> Display {
        // The built-in primary presentation is registered together with commands.
        self.registry
            .display(state, &self.feature_services(), highlighter, diagrams)
            .unwrap_or_default()
    }
    fn block_window(&self, state: &EditingState) -> Option<std::ops::Range<usize>> {
        Planner {
            state: state.clone(),
            prepared: None,
            renderer: self.formulas.as_ref(),
        }
        .block_window()
    }
    fn protects_table_boundary(&self, state: &EditingState, forward: bool) -> bool {
        Planner {
            state: state.clone(),
            prepared: None,
            renderer: self.formulas.as_ref(),
        }
        .remove_table_boundary(forward)
    }
    fn preview_inputs(&self, state: &EditingState) -> Vec<PreviewInput> {
        self.registry.preview_inputs(state)
    }
    fn block_ui(&self, block: &ryne_syntax::block::Block, source_open: bool) -> AdornmentSpec {
        self.registry.block_ui(block, source_open)
    }
    fn insert_templates(&self) -> &[(&'static str, InsertBlock)] {
        self.registry.insert_templates()
    }
}
