//! Basic source editing remains usable with no Markdown feature registrations.
use editable_markdown_parser::{Edit, Error};
use ryne_core::PreparedEdit;
use ryne_extensions::*;
use ryne_projection::*;
use ryne_syntax::ParseSnapshot;
use std::ops::Range;
pub struct SourceEditing;
impl FeatureEngine for SourceEditing {
    fn snapshot(&self) -> Option<std::sync::Arc<dyn FeatureEngine>> {
        Some(std::sync::Arc::new(SourceEditing))
    }
    fn plan(&self, intent: EditIntent, state: &EditingState) -> Result<CommandPlan, String> {
        let mut next = state.clone();
        let base = state.source.id();
        let edit = match intent {
            EditIntent::Insert { range, text, .. } | EditIntent::Replace { range, text } => {
                Some(Edit {
                    range: range
                        .validate(&state.source)
                        .map_err(|e| format!("{e:?}"))?,
                    replacement: text,
                })
            }
            EditIntent::Delete { forward } => {
                let range = if state.selection().is_empty() {
                    let len = state.source.text().len();
                    Display {
                        text: state.source.text().into(),
                        source_len: len,
                        spans: if len == 0 {
                            vec![]
                        } else {
                            vec![Span {
                                source: 0..len,
                                display: 0..len,
                                style: Style {
                                    source: true,
                                    ..Style::default()
                                },
                            }]
                        },
                        ..Display::default()
                    }
                    .deletion_range(state.head, forward)
                } else {
                    Some(state.selection())
                };
                range.map(|range| Edit {
                    range,
                    replacement: String::new(),
                })
            }
            EditIntent::Tab { backward: false } => Some(Edit {
                range: state.selection(),
                replacement: "\t".into(),
            }),
            EditIntent::LineBreak { .. } => Some(Edit {
                range: state.selection(),
                replacement: "\n".into(),
            }),
            EditIntent::Select {
                position, extend, ..
            } => {
                if !state.source.text().is_char_boundary(position) {
                    return Err("invalid selection".into());
                }
                next.head = position;
                if !extend {
                    next.anchor = position
                }
                None
            }
            EditIntent::Normalize | EditIntent::RestoreHeading | EditIntent::RevealHeading => None,
            _ => return Ok(CommandPlan::Consume),
        };
        if let Some(edit) = edit {
            let caret = edit.range.start + edit.replacement.len();
            let prepared = match PreparedEdit::prepare_plan(
                &state.source,
                vec![ryne_core::SourceEdit::from_parser(state.source.id(), edit)],
            ) {
                Ok(p) => p,
                Err(Error::NoChange) => return Ok(CommandPlan::Consume),
                Err(e) => return Err(format!("{e:?}")),
            };
            next.source = prepared.applied().source.clone();
            next.parse = std::sync::Arc::new(ParseSnapshot::with_math(next.source.clone()));
            next.anchor = caret;
            next.head = caret;
            Ok(CommandPlan::Edit {
                prepared,
                state: next,
                handled: true,
            })
        } else {
            Ok(CommandPlan::Select {
                base,
                state: next,
                handled: true,
            })
        }
    }
    fn display(
        &self,
        state: &EditingState,
        _: Option<&mut dyn SyntaxHighlighter>,
        _: &[DiagramViewInput],
    ) -> Display {
        let len = state.source.text().len();
        Display {
            text: state.source.text().into(),
            source_len: len,
            spans: if len == 0 {
                vec![]
            } else {
                vec![Span {
                    source: 0..len,
                    display: 0..len,
                    style: Style {
                        source: true,
                        ..Style::default()
                    },
                }]
            },
            ..Display::default()
        }
    }

    fn block_window(&self, _: &EditingState) -> Option<Range<usize>> {
        None
    }
    fn protects_table_boundary(&self, _: &EditingState, _: bool) -> bool {
        false
    }
    fn preview_inputs(&self, _: &EditingState) -> Vec<PreviewInput> {
        Vec::new()
    }
    fn block_ui(&self, block: &ryne_syntax::block::Block, _: bool) -> AdornmentSpec {
        AdornmentSpec {
            metadata: BlockMetadata::from(block.kind),
            menu: Vec::new(),
        }
    }
    fn insert_templates(&self) -> &[(&'static str, InsertBlock)] {
        &[]
    }
}
