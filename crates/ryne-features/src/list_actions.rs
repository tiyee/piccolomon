use crate::commands::Planner;
use ryne_extensions::*;
use ryne_syntax::{code_block, list};
use std::{ops::Range, sync::Arc};

pub(crate) struct ListActions {
    pub formulas: Arc<dyn FormulaRenderer>,
}
/// Per-snapshot physical context, shared by all item controls in a batch.
struct Extents<'a> {
    state: &'a EditingState,
    lines: &'a [Range<usize>],
    code: &'a [code_block::CodeBlock],
    #[cfg(test)]
    code_comparisons: std::cell::Cell<usize>,
}
impl<'a> Extents<'a> {
    fn new(state: &'a EditingState) -> Self {
        Self {
            state,
            lines: &state.parse.semantic().index().lines,
            code: code_block::cached(&state.parse),
            #[cfg(test)]
            code_comparisons: Default::default(),
        }
    }
    fn get(&self, item: &list::Item) -> Range<usize> {
        let text = self.state.source.text();
        let boundary = item.range.end.max(item.body);
        let index = self.lines.partition_point(|line| line.end < boundary);
        let mut end = self
            .lines
            .get(index)
            .filter(|line| line.start < boundary && boundary <= line.end)
            .map_or(boundary, |line| line.end);
        let first = self
            .lines
            .partition_point(|line| line.start <= item.range.start);
        let last = self.lines.partition_point(|line| line.start < end);
        for line in self.lines[first.min(last)..last].iter().rev() {
            // Parser-owned code blocks are ordered and do not overlap. Borrow
            // their snapshot directory instead of scanning every block per row.
            let code = self.code.partition_point(|block| {
                #[cfg(test)]
                self.code_comparisons.set(self.code_comparisons.get() + 1);
                block.range.end <= line.start
            });
            if self
                .code
                .get(code)
                .is_some_and(|b| b.range.contains(&line.start))
            {
                break;
            }
            let start = list::quote_end(&self.state.parse, line);
            if !text[start..line.end].trim().is_empty() {
                break;
            }
            end = line.start;
        }
        item.range.start..end
    }
}
pub(crate) fn extent(state: &EditingState, item: &list::Item) -> Range<usize> {
    Extents::new(state).get(item)
}
impl ListActions {
    fn build_controls(
        &self,
        state: &EditingState,
        target: Option<&RenderTarget>,
    ) -> Result<Vec<RenderControl>, String> {
        let items = list::cached_items(&state.parse);
        if items.is_empty() {
            return Ok(Vec::new());
        }
        let selected = match target {
            None => items,
            Some(t) => items
                .iter()
                .find(|item| t.role == "list.item" && t.part.as_slice() == [item.marker.start])
                .map(std::slice::from_ref)
                .unwrap_or(&[]),
        };
        if selected.is_empty() {
            return Ok(Vec::new());
        }
        let context = Extents::new(state);
        let owners = selected
            .iter()
            .map(|item| context.get(item))
            .collect::<Vec<_>>();
        let mut siblings = std::collections::BTreeMap::new();
        for item in items
            .iter()
            .filter(|item| target.is_none() || item.list == selected[0].list)
        {
            let bounds = siblings
                .entry(item.list)
                .or_insert((item.marker.start, item.marker.start));
            bounds.0 = bounds.0.min(item.marker.start);
            bounds.1 = bounds.1.max(item.marker.start);
        }
        let mut controls = selected
            .iter()
            .zip(&owners)
            .map(|(item, owner)| {
                let (first, last) = siblings[&item.list];
                let before = first < item.marker.start;
                let after = item.marker.start < last;
                let owner = owner.clone();
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
        for (item, owner) in selected.iter().zip(&owners) {
            let Some(task) = &item.task else {
                continue;
            };
            let owner = owner.clone();
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
}
impl RenderActionProvider for ListActions {
    fn cache_dependencies(&self) -> Option<CacheDependencies> {
        Some(CacheDependencies::default())
    }
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
        self.build_controls(state, None)
    }
    fn controls_for_target(
        &self,
        state: &EditingState,
        target: &RenderTarget,
    ) -> Result<Option<Vec<RenderControl>>, String> {
        self.build_controls(state, Some(target)).map(Some)
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
            mapping: None,
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

#[cfg(test)]
#[path = "list_extent_tests.rs"]
mod extent_tests;
