//! Static Markdown features. All planning and presentation are GUI independent.
mod builtins;
mod commands;
pub mod diagrams;
pub mod input;
pub mod templates;
use templates::ITEMS;
mod list_actions;
mod math;
pub mod presentation;
mod table_actions;
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
    language_detector: Option<Arc<dyn CodeLanguageDetector>>,
    render_actions: RenderActionRegistry,
    additional: Vec<Arc<dyn BuiltinFeature + Send + Sync>>,
}
impl MarkdownFeatures {
    pub fn new(
        formulas: Arc<dyn FormulaRenderer>,
        previews: Vec<diagrams::FencedPreview>,
    ) -> Result<Self, String> {
        let registry = builtins::registry(&previews, &[])?;
        let render_actions = RenderActionRegistry::new(vec![
            Arc::new(table_actions::TableActions {
                formulas: formulas.clone(),
            }),
            Arc::new(list_actions::ListActions {
                formulas: formulas.clone(),
            }),
        ])?;
        Ok(Self {
            formulas,
            previews,
            registry,
            language_detector: None,
            render_actions,
            additional: Vec::new(),
        })
    }
    fn feature_services(&self) -> FeatureServices<'_> {
        FeatureServices {
            formulas: self.formulas.as_ref(),
            language_detector: self.language_detector.as_deref(),
        }
    }
    pub fn with_feature(
        mut self,
        feature: Arc<dyn BuiltinFeature + Send + Sync>,
    ) -> Result<Self, String> {
        self.additional.push(feature);
        self.registry = builtins::registry(&self.previews, &self.additional)?;
        Ok(self)
    }
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
    pub fn with_render_action_provider(
        mut self,
        provider: Arc<dyn RenderActionProvider>,
    ) -> Result<Self, String> {
        self.render_actions = self.render_actions.with_provider(provider)?;
        Ok(self)
    }
    pub fn with_language_detector(mut self, detector: Arc<dyn CodeLanguageDetector>) -> Self {
        self.language_detector = Some(detector);
        self
    }
}
impl FeatureEngine for MarkdownFeatures {
    fn snapshot(&self) -> Option<Arc<dyn FeatureEngine>> {
        let mut frozen = self.clone();
        frozen.formulas = self.formulas.snapshot()?;
        Some(Arc::new(frozen))
    }
    fn render_controls(&self, state: &EditingState) -> Result<Vec<RenderControl>, String> {
        self.render_actions.controls(state)
    }
    fn render_control_batch(&self, state: &EditingState) -> Result<RenderControlBatch, String> {
        Ok(self.render_actions.control_batch(state))
    }
    fn plan_render_action(
        &self,
        state: &EditingState,
        request: &RenderActionRequest,
    ) -> Result<CommandPlan, String> {
        self.render_actions.plan(state, request)
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
        ryne_syntax::code_block::blocks(&state.parse)
            .iter()
            .filter_map(|b| self.previews.iter().find_map(|p| p.input(state, b)))
            .take(32)
            .collect()
    }
    fn block_ui(&self, block: &ryne_syntax::block::Block, source_open: bool) -> AdornmentSpec {
        AdornmentSpec {
            metadata: BlockMetadata {
                name: block.name,
                icon: block.icon,
                label: block.label,
            },
            menu: vec![
                MenuItemSpec {
                    command: BlockCommand::Source,
                    name: if source_open {
                        "收起块源码"
                    } else {
                        "编辑块源码"
                    },
                    order: 0,
                },
                MenuItemSpec {
                    command: BlockCommand::Select,
                    name: "选择整块",
                    order: 1,
                },
                MenuItemSpec {
                    command: BlockCommand::Copy,
                    name: "复制块源码",
                    order: 2,
                },
                MenuItemSpec {
                    command: BlockCommand::AddBelow,
                    name: "在下方添加",
                    order: 3,
                },
                MenuItemSpec {
                    command: BlockCommand::AddAbove,
                    name: "在上方添加",
                    order: 4,
                },
            ],
        }
    }
    fn insert_templates(&self) -> &[(&'static str, InsertBlock)] {
        ITEMS
    }
}
