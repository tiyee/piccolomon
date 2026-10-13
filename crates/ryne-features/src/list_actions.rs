use crate::commands::Planner;
use ryne_extensions::*;
use ryne_syntax::{code_block, list};
use std::{ops::Range, sync::Arc};

pub(crate) struct ListActions {
    pub formulas: Arc<dyn FormulaRenderer>,
}
pub(crate) fn extent(state: &EditingState, item: &list::Item) -> Range<usize> {
    let text = state.source.text();
    let end = item.range.end.max(item.body);
    let lines = code_block::lines(text);
    let mut end = lines
        .iter()
        .find(|l| l.start < end && end <= l.end)
        .map_or(end, |l| l.end);
    let code = code_block::blocks(&state.parse);
    for line in lines
        .iter()
        .rev()
        .filter(|l| item.range.start < l.start && l.start < end)
        .cloned()
        .collect::<Vec<_>>()
    {
        if code.iter().any(|b| b.range.contains(&line.start)) {
            break;
        }
        let start = list::quote_end(&state.parse, &line);
        if !text[start..line.end].trim().is_empty() {
            break;
        }
        end = line.start;
    }
    item.range.start..end
}
impl RenderActionProvider for ListActions {
    fn id(&self) -> &'static str {
        "list"
    }
    fn commands(&self) -> &'static [&'static str] {
        &[
            "list.insert-after",
            "list.insert-before",
            "list.move-up",
            "list.move-down",
            "list.indent",
            "list.outdent",
            "list.delete",
            "list.toggle-task",
        ]
    }
    fn controls(&self, state: &EditingState) -> Result<Vec<RenderControl>, String> {
        let items = list::items(&state.parse);
        let mut controls = items
            .iter()
            .map(|item| {
                let before = items
                    .iter()
                    .any(|i| i.list == item.list && i.marker.start < item.marker.start);
                let after = items
                    .iter()
                    .any(|i| i.list == item.list && i.marker.start > item.marker.start);
                let owner = extent(state, item);
                let action = |command, fallback, enabled, group, icon| RenderAction {
                    command,
                    label: command,
                    fallback,
                    enabled,
                    group,
                    icon: Some(icon),
                    checked: false,
                    submenu: None,
                    shortcut: None,
                };
                RenderControl {
                    provider: "list",
                    id: item.marker.start.to_string(),
                    target: RenderTarget {
                        document: state.source.id(),
                        role: "list.item",
                        owner: owner.clone(),
                        part: vec![item.marker.start],
                    },
                    focus: item.body..owner.end,
                    placement: RenderControlPlacement::Leading,
                    trailing_space: 0.,
                    actions: vec![
                        action(
                            "list.insert-after",
                            "Insert item after",
                            true,
                            0,
                            "add_square",
                        ),
                        action(
                            "list.insert-before",
                            "Insert item before",
                            true,
                            0,
                            "add_square",
                        ),
                        action("list.move-up", "Move item up", before, 1, "arrow_move"),
                        action("list.move-down", "Move item down", after, 1, "arrow_move"),
                        action("list.indent", "Indent item", before, 2, "arrow_turn_right"),
                        action(
                            "list.outdent",
                            "Outdent item",
                            item.parent.is_some(),
                            2,
                            "arrow_turn_right",
                        ),
                        action("list.delete", "Delete item and children", true, 3, "delete"),
                    ],
                }
            })
            .collect::<Vec<_>>();
        for item in &items {
            let Some(task) = &item.task else {
                continue;
            };
            let owner = extent(state, item);
            controls.push(RenderControl {
                provider: "list",
                id: format!("task-{}", item.marker.start),
                target: RenderTarget {
                    document: state.source.id(),
                    role: "list.item",
                    owner: owner.clone(),
                    part: vec![item.marker.start],
                },
                focus: item.body..owner.end,
                placement: RenderControlPlacement::Marker {
                    icon: if task.checked {
                        "checkbox_checked"
                    } else {
                        "checkbox_unchecked"
                    },
                },
                trailing_space: 0.,
                actions: vec![RenderAction {
                    command: "list.toggle-task",
                    label: "list.toggle-task",
                    fallback: "Toggle task",
                    enabled: true,
                    group: 0,
                    icon: None,
                    checked: task.checked,
                    submenu: None,
                    shortcut: None,
                }],
            });
        }
        Ok(controls)
    }
    fn plan(
        &self,
        state: &EditingState,
        request: &RenderActionRequest,
    ) -> Result<CommandPlan, String> {
        let mut planner = Planner {
            state: state.clone(),
            prepared: None,
            renderer: self.formulas.as_ref(),
        };
        planner.list_action(request)?;
        Ok(match planner.prepared {
            Some(prepared) => CommandPlan::Edit {
                prepared,
                state: planner.state,
                handled: true,
            },
            None => CommandPlan::Consume,
        })
    }
}
