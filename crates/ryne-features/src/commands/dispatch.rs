//! Built-in command families registered as executable contributions.
use super::*;
use ryne_extensions::*;
#[derive(Clone, Copy)]
pub(crate) enum BuiltinCommand {
    Format,
    Navigation,
    Input,
    Paragraph,
    Table,
    List,
    Quote,
    Code,
    Templates,
}
impl BuiltinCommand {
    pub fn priority(self) -> i32 {
        match self {
            Self::Table => 300,
            Self::Code => 200,
            Self::Quote => 150,
            Self::List => 100,
            Self::Paragraph => -100,
            _ => 0,
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Format => "format.execute",
            Self::Navigation => "navigation.execute",
            Self::Input => "input.execute",
            Self::Paragraph => "paragraph.execute",
            Self::Table => "table.execute",
            Self::List => "list.execute",
            Self::Quote => "quote.execute",
            Self::Code => "code.execute",
            Self::Templates => "templates.execute",
        }
    }
}
impl CommandHandler for BuiltinCommand {
    fn plan(
        &self,
        intent: &EditIntent,
        state: &EditingState,
        services: &FeatureServices<'_>,
    ) -> Result<Option<CommandPlan>, String> {
        if !matches!(
            (self, intent),
            (
                Self::Table | Self::Code | Self::List | Self::Paragraph,
                EditIntent::Delete { .. } | EditIntent::LineBreak { .. } | EditIntent::Tab { .. }
            ) | (Self::Quote, EditIntent::LineBreak { .. })
                | (Self::Format, EditIntent::FormatSource)
                | (
                    Self::Navigation,
                    EditIntent::Normalize
                        | EditIntent::RestoreHeading
                        | EditIntent::Select { .. }
                        | EditIntent::RevealHeading
                        | EditIntent::ToggleBlockSource
                )
                | (
                    Self::Input,
                    EditIntent::Insert { .. } | EditIntent::Replace { .. }
                )
                | (Self::Paragraph, EditIntent::JoinParagraphs { .. })
                | (Self::Table, EditIntent::NavigateTable { .. })
                | (
                    Self::List,
                    EditIntent::IndentList { .. } | EditIntent::RemoveListBoundary { .. }
                )
                | (Self::Code, EditIntent::RemoveCodeBoundary { .. })
                | (
                    Self::Templates,
                    EditIntent::Append { .. }
                        | EditIntent::AppendLocalized { .. }
                        | EditIntent::InsertAdjacent { .. }
                )
        ) {
            return Ok(None);
        }
        let delegates = matches!(
            intent,
            EditIntent::Delete { .. } | EditIntent::LineBreak { .. } | EditIntent::Tab { .. }
        );
        let intent = intent.clone();
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
        let mut p = Planner {
            state: state.clone(),
            prepared: None,
            renderer: services.formulas,
            mapping: Some(&services.planning),
        };
        let result: Result<bool, editable_markdown_parser::Error> = match (self, intent) {
            (Self::Format, EditIntent::FormatSource) => {
                p.format_source()?;
                Ok(true)
            }
            (Self::Navigation, EditIntent::RestoreHeading) => {
                p.reveal_heading_input();
                Ok(true)
            }
            (Self::Navigation, EditIntent::Normalize) => {
                p.normalize_heading_caret();
                Ok(true)
            }
            (
                Self::Navigation,
                EditIntent::Select {
                    position,
                    extend,
                    frozen,
                },
            ) => {
                p.select(position, extend, frozen);
                Ok(true)
            }
            (Self::Navigation, EditIntent::RevealHeading) => {
                p.reveal_clicked_heading();
                Ok(true)
            }
            (Self::Navigation, EditIntent::ToggleBlockSource) => Ok(p.toggle_block_source()),
            (
                Self::Input,
                EditIntent::Insert {
                    range,
                    text,
                    origin,
                },
            ) => p
                .insert_text(
                    range
                        .validate(&state.source)
                        .map_err(|e| format!("{e:?}"))?,
                    &text,
                    origin,
                )
                .map(|()| true),
            (Self::Input, EditIntent::Replace { range, text }) => p
                .replace(
                    range
                        .validate(&state.source)
                        .map_err(|e| format!("{e:?}"))?,
                    &text,
                )
                .map(|()| true),
            (Self::Table, EditIntent::LineBreak { hard }) => p.table_line_break(hard),
            (Self::Code, EditIntent::LineBreak { .. }) => p.code_line_break(),
            (Self::List, EditIntent::LineBreak { hard }) => p.list_line_break(hard),
            (Self::Quote, EditIntent::LineBreak { hard }) => p.quote_line_break(hard),
            (Self::Paragraph, EditIntent::LineBreak { hard }) => {
                p.paragraph_line_break(hard).map(|()| true)
            }
            (Self::Table, EditIntent::Delete { forward }) => {
                Ok(p.selection().is_empty() && p.remove_table_boundary(forward))
            }
            (Self::Code, EditIntent::Delete { forward }) => {
                if p.selection().is_empty() {
                    p.remove_code_boundary(forward)
                } else {
                    Ok(false)
                }
            }
            (Self::List, EditIntent::Delete { forward }) => {
                if p.selection().is_empty() {
                    p.remove_list_boundary(forward)
                } else {
                    Ok(false)
                }
            }
            (Self::Paragraph, EditIntent::Delete { forward }) => {
                let joined = if p.selection().is_empty() {
                    p.join_paragraphs(forward)
                } else {
                    Ok(false)
                };
                joined.and_then(|handled| {
                    if handled {
                        Ok(true)
                    } else {
                        p.delete_grapheme(forward)
                    }
                })
            }
            (Self::Table, EditIntent::Tab { backward }) => p.navigate_table(backward, false),
            (Self::List, EditIntent::Tab { backward }) => p.indent_list(backward),
            (Self::Paragraph, EditIntent::Tab { backward }) => {
                if !backward {
                    p.insert_text(p.selection(), "\t", InputOrigin::Keyboard)
                        .map(|()| true)
                } else {
                    Ok(true)
                }
            }
            (Self::Table, EditIntent::NavigateTable { backward, next_row }) => {
                p.navigate_table(backward, next_row)
            }
            (Self::List, EditIntent::IndentList { outdent }) => p.indent_list(outdent),
            (Self::Code, EditIntent::RemoveCodeBoundary { forward }) => {
                p.remove_code_boundary(forward)
            }
            (Self::List, EditIntent::RemoveListBoundary { forward }) => {
                p.remove_list_boundary(forward)
            }
            (Self::Paragraph, EditIntent::JoinParagraphs { forward }) => p.join_paragraphs(forward),
            (
                Self::Templates,
                EditIntent::InsertAdjacent {
                    block,
                    before,
                    template,
                    labels,
                },
            ) => {
                let range = block
                    .validate(&state.source)
                    .map_err(|e| format!("{e:?}"))?;
                p.insert_adjacent(range, before, template, &labels)?;
                Ok(true)
            }
            (Self::Templates, EditIntent::AppendLocalized { template, labels }) => {
                p.append_block(template, &labels)?;
                Ok(true)
            }
            (Self::Templates, EditIntent::Append { template }) => {
                p.append_block(template, &TemplateLabels::default())?;
                Ok(true)
            }
            _ => return Ok(None),
        };
        let handled = result.map_err(|e| format!("{e:?}"))?;
        if delegates && !handled {
            return Ok(None);
        }
        Ok(Some(if let Some(prepared) = p.prepared {
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
        }))
    }
}
