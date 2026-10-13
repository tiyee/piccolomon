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

pub struct MarkdownFeatures {
    formulas: Arc<dyn FormulaRenderer>,
    registry: Registry,
    previews: Vec<diagrams::FencedPreview>,
    language_detector: Option<Arc<dyn CodeLanguageDetector>>,
    render_actions: RenderActionRegistry,
}
impl MarkdownFeatures {
    pub fn new(
        formulas: Arc<dyn FormulaRenderer>,
        previews: Vec<diagrams::FencedPreview>,
    ) -> Result<Self, String> {
        let registry = builtins::registry(&previews)?;
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
        })
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
    fn render_controls(&self, state: &EditingState) -> Result<Vec<RenderControl>, String> {
        self.render_actions.controls(state)
    }
    fn plan_render_action(
        &self,
        state: &EditingState,
        request: &RenderActionRequest,
    ) -> Result<CommandPlan, String> {
        self.render_actions.plan(state, request)
    }
    fn plan(&self, intent: EditIntent, state: &EditingState) -> Result<CommandPlan, String> {
        let base = state.source.id();
        if state.parse.source().id() != base {
            return Err("stale semantics".into());
        }
        let view = matches!(
            intent,
            EditIntent::Normalize
                | EditIntent::Select { .. }
                | EditIntent::RevealHeading
                | EditIntent::ToggleBlockSource
        );
        let infer_language = match &intent {
            EditIntent::Insert { text, origin, .. } => {
                !text.is_empty()
                    && (matches!(origin, InputOrigin::Paste) || text.contains(['\r', '\n']))
            }
            EditIntent::Replace { text, .. } => text.contains(['\r', '\n']),
            EditIntent::LineBreak { .. } => true,
            _ => false,
        };
        let mut p = Planner {
            state: state.clone(),
            prepared: None,
            renderer: self.formulas.as_ref(),
        };
        let result: Result<bool, editable_markdown_parser::Error> = match intent {
            EditIntent::RestoreHeading => {
                p.reveal_heading_input();
                Ok(true)
            }
            EditIntent::Normalize => {
                p.normalize_heading_caret();
                Ok(true)
            }
            EditIntent::Select {
                position,
                extend,
                frozen,
            } => {
                p.select(position, extend, frozen);
                Ok(true)
            }
            EditIntent::RevealHeading => {
                p.reveal_clicked_heading();
                Ok(true)
            }
            EditIntent::ToggleBlockSource => Ok(p.toggle_block_source()),
            EditIntent::Insert {
                range,
                text,
                origin,
            } => p
                .insert_text(
                    range
                        .validate(&state.source)
                        .map_err(|e| format!("{e:?}"))?,
                    &text,
                    origin,
                )
                .map(|()| true),
            EditIntent::Replace { range, text } => p
                .replace(
                    range
                        .validate(&state.source)
                        .map_err(|e| format!("{e:?}"))?,
                    &text,
                )
                .map(|()| true),
            EditIntent::LineBreak { hard } => p.line_break(hard).map(|()| true),
            EditIntent::NavigateTable { backward, next_row } => {
                p.navigate_table(backward, next_row)
            }
            EditIntent::IndentList { outdent } => p.indent_list(outdent),
            EditIntent::RemoveCodeBoundary { forward } => p.remove_code_boundary(forward),
            EditIntent::RemoveListBoundary { forward } => p.remove_list_boundary(forward),
            EditIntent::JoinParagraphs { forward } => p.join_paragraphs(forward),
            EditIntent::AppendLocalized { template, labels } => {
                p.append_block(template, &labels)?;
                Ok(true)
            }
            EditIntent::Append { template } => {
                p.append_block(template, &TemplateLabels::default())?;
                Ok(true)
            }
        };
        let handled = result.map_err(|e| format!("{e:?}"))?;
        if infer_language && let Some(detector) = &self.language_detector {
            p.complete_code_languages(state, detector.as_ref())
                .map_err(|e| format!("{e:?}"))?;
        }
        Ok(if let Some(prepared) = p.prepared {
            CommandPlan::Edit {
                prepared,
                state: p.state,
                handled,
            }
        } else if view {
            CommandPlan::View {
                base,
                state: p.state,
                handled,
            }
        } else {
            CommandPlan::Select {
                base,
                state: p.state,
                handled,
            }
        })
    }
    fn display(
        &self,
        state: &EditingState,
        highlighter: Option<&mut dyn SyntaxHighlighter>,
        diagrams: &[DiagramViewInput],
    ) -> Display {
        presentation::PresentationBuilder {
            renderer: self.formulas.as_ref(),
        }
        .build(
            presentation::PresentationContext {
                parse: &state.parse,
                window: state.reveal.as_ref().map(|s| &s.range),
                source_mode: state.source_mode,
                heading_window: state.heading_reveal.as_ref().map(|s| &s.range),
                block_window: self.block_window(state).as_ref(),
            },
            highlighter,
            diagrams,
        )
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
            ],
        }
    }
    fn insert_templates(&self) -> &[(&'static str, InsertBlock)] {
        ITEMS
    }
}
