use crate::commands::Planner;
use editable_markdown_parser::Alignment;
use ryne_extensions::*;
use ryne_syntax::table;
use std::sync::Arc;

pub(crate) struct TableActions {
    pub formulas: Arc<dyn FormulaRenderer>,
}
const COMMANDS: &[&str] = &[
    "table.align-left",
    "table.align-center",
    "table.align-right",
    "table.insert-below",
    "table.insert-above",
    "table.insert-right",
    "table.insert-left",
    "table.move-row-up",
    "table.move-row-down",
    "table.move-column-left",
    "table.move-column-right",
    "table.delete-row",
    "table.delete-column",
];
fn action(
    command: &'static str,
    fallback: &'static str,
    enabled: bool,
    group: u8,
    key: Option<(&'static str, &'static str)>,
) -> RenderAction {
    RenderAction {
        command,
        label: command,
        fallback,
        enabled,
        group,
        checked: false,
        icon: None,
        submenu: None,
        shortcut: key.map(|(key, hint)| RenderShortcut {
            key,
            hint,
            control: group != 2,
            platform: true,
            alt: group == 2,
            shift: command == "table.delete-column",
        }),
    }
}
impl TableActions {
    fn build_controls(
        &self,
        state: &EditingState,
        target: Option<&RenderTarget>,
    ) -> Result<Vec<RenderControl>, String> {
        let tables = match target {
            None => table::cached(&state.parse),
            Some(t) => {
                if t.role != "table.cell" || t.part.len() != 2 {
                    return Ok(Vec::new());
                }
                table::containing(&state.parse, t.owner.start)
                    .map(std::slice::from_ref)
                    .unwrap_or(&[])
            }
        };
        Ok(tables
            .iter()
            .filter(|table| {
                target.is_none_or(|t| {
                    t.role == "table.cell" && t.owner == table.range && t.part.len() == 2
                })
            })
            .flat_map(|table| {
                let owner = table.range.clone();
                let columns = table.align.len();
                let rows = table.rows.len();
                let align = &table.align;
                target
                    .map_or(0..rows, |t| t.part[0]..t.part[0].saturating_add(1))
                    .filter_map(move |r| table.rows.get(r).map(|row| (r, row)))
                    .flat_map(move |(r, row)| {
                        let owner = owner.clone();
                        target
                            .map_or(0..row.cells.len(), |t| {
                                t.part[1]..t.part[1].saturating_add(1)
                            })
                            .filter_map(move |c| row.cells.get(c).map(|cell| (c, cell)))
                            .map(move |(c, cell)| {
                                let mut actions = vec![
                                    action("table.align-left", "Left", true, 0, None),
                                    action("table.align-center", "Center", true, 0, None),
                                    action("table.align-right", "Right", true, 0, None),
                                    action(
                                        "table.insert-below",
                                        "Insert row below",
                                        true,
                                        1,
                                        Some(("down", "⌃⌘↓")),
                                    ),
                                    action(
                                        "table.insert-above",
                                        "Insert row above",
                                        r > 0,
                                        1,
                                        Some(("up", "⌃⌘↑")),
                                    ),
                                    action(
                                        "table.insert-right",
                                        "Insert column after",
                                        true,
                                        1,
                                        Some(("right", "⌃⌘→")),
                                    ),
                                    action(
                                        "table.insert-left",
                                        "Insert column before",
                                        true,
                                        1,
                                        Some(("left", "⌃⌘←")),
                                    ),
                                    action(
                                        "table.move-row-up",
                                        "Move row up",
                                        r > 1,
                                        2,
                                        Some(("up", "⌥⌘↑")),
                                    ),
                                    action(
                                        "table.move-row-down",
                                        "Move row down",
                                        r > 0 && r + 1 < rows,
                                        2,
                                        Some(("down", "⌥⌘↓")),
                                    ),
                                    action(
                                        "table.move-column-left",
                                        "Move column left",
                                        c > 0,
                                        2,
                                        Some(("left", "⌥⌘←")),
                                    ),
                                    action(
                                        "table.move-column-right",
                                        "Move column right",
                                        c + 1 < columns,
                                        2,
                                        Some(("right", "⌥⌘→")),
                                    ),
                                    action(
                                        "table.delete-row",
                                        "Delete row",
                                        r > 0,
                                        3,
                                        Some(("backspace", "⌃⌘⌫")),
                                    ),
                                    action(
                                        "table.delete-column",
                                        "Delete column",
                                        columns > 1,
                                        3,
                                        Some(("backspace", "⌃⇧⌘⌫")),
                                    ),
                                ];
                                for (index, a) in actions.iter_mut().enumerate() {
                                    if index < 3 {
                                        a.submenu = Some(RenderSubmenu {
                                            label: "table.alignment",
                                            fallback: "Alignment",
                                        });
                                        a.checked = match index {
                                            0 => !matches!(
                                                align[c],
                                                Alignment::Center | Alignment::Right
                                            ),
                                            1 => align[c] == Alignment::Center,
                                            _ => align[c] == Alignment::Right,
                                        };
                                    } else {
                                        a.icon = Some(if a.group == 1 {
                                            "table_add"
                                        } else if a.group == 3 {
                                            "delete"
                                        } else {
                                            "arrow_move"
                                        });
                                    }
                                }
                                RenderControl {
                                    provider: "table",
                                    id: format!("{}:{r}:{c}", owner.start),
                                    target: RenderTarget {
                                        document: state.source.id(),
                                        role: "table.cell",
                                        owner: owner.clone(),
                                        part: vec![r, c],
                                    },
                                    focus: cell.content.clone(),
                                    placement: RenderControlPlacement::Trailing,
                                    trailing_space: 24.,
                                    actions,
                                }
                            })
                    })
            })
            .collect())
    }
}
impl RenderActionProvider for TableActions {
    fn cache_dependencies(&self) -> Option<CacheDependencies> {
        Some(CacheDependencies::default())
    }
    fn id(&self) -> &'static str {
        "table"
    }
    fn commands(&self) -> &'static [&'static str] {
        COMMANDS
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
        planner.table_action(request)?;
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
