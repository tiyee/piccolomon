use crate::protocol::*;
use editable_markdown_parser::{Affinity, Edit, ParseSnapshot};
use ryne_core::PreparedEdit;
use ryne_extensions::{CommandPlan, EditingState, FileStore, FormulaRenderer};
use ryne_projection::Formula;
use ryne_session::{EditorSession, SessionServices};
use ryne_syntax::{code_block, list, table};
use std::{ops::Range, path::Path, sync::Arc};

struct NoFormula;
impl FormulaRenderer for NoFormula {
    fn render(&self, _: &str) -> Option<Arc<Formula>> {
        None
    }
}
struct NoFiles;
impl FileStore for NoFiles {
    fn read(&self, _: &Path) -> std::io::Result<String> {
        Err(std::io::Error::other("not used"))
    }
    fn write(&self, _: &Path, _: &editable_markdown_parser::Source) -> std::io::Result<()> {
        Err(std::io::Error::other("not used"))
    }
}
pub fn session(text: &str) -> Result<EditorSession, String> {
    Ok(EditorSession::new(
        text,
        SessionServices {
            features: Arc::new(ryne_features::MarkdownFeatures::new(
                Arc::new(NoFormula),
                vec![],
            )?),
            files: Arc::new(NoFiles),
        },
    ))
}
fn action(command: &'static str, enabled: bool) -> Action {
    Action {
        command,
        label_key: command,
        enabled,
        disabled_reason: (!enabled).then_some("table.header-cannot-be-deleted"),
    }
}
pub struct TableActions;
impl TableActions {
    fn resolve(state: &EditingState, target: &Target) -> Option<(table::Table, usize, usize)> {
        target.check(state).ok()?;
        if target.role != "table.cell" || target.part.len() != 2 {
            return None;
        }
        let table = table::tables(&state.parse)
            .into_iter()
            .find(|t| t.range == target.owner)?;
        let (row, column) = (target.part[0], target.part[1]);
        table.rows.get(row)?.cells.get(column)?;
        Some((table, row, column))
    }
}
impl RenderExtension for TableActions {
    fn id(&self) -> &'static str {
        "table"
    }
    fn commands(&self) -> &'static [&'static str] {
        &["table.delete-row", "table.insert-below"]
    }
    fn contribute(&self, state: &EditingState) -> Result<Vec<Contribution>, String> {
        let mut specs = Vec::new();
        for table in table::tables(&state.parse) {
            for (r, row) in table.rows.iter().enumerate() {
                for c in 0..row.cells.len() {
                    specs.push(Contribution {
                        provider: self.id(),
                        id: format!("{}:{r}:{c}", table.range.start),
                        target: Target {
                            document: state.source.id(),
                            role: "table.cell",
                            owner: table.range.clone(),
                            part: vec![r, c],
                        },
                        placement: Placement::Trailing,
                        priority: 10,
                        actions: vec![
                            action("table.delete-row", r > 0),
                            action("table.insert-below", true),
                        ],
                    });
                }
            }
        }
        Ok(specs)
    }
    fn available(&self, state: &EditingState, target: &Target, command: &str) -> bool {
        Self::resolve(state, target).is_some_and(|(_, r, _)| match command {
            "table.delete-row" => r > 0,
            "table.insert-below" => true,
            _ => false,
        })
    }
    fn plan(
        &self,
        state: &EditingState,
        target: &Target,
        command: &str,
    ) -> Result<CommandPlan, String> {
        if !self.available(state, target, command) {
            return Err("invalid table action".into());
        }
        let (table, row, _) = Self::resolve(state, target).ok_or("invalid table target")?;
        if command == "table.delete-row" {
            edit(state, table.rows[row].line.clone(), String::new())
        } else {
            let at = if row == 0 {
                // Header insertion belongs after the delimiter, not before it.
                code_block::lines(state.source.text())
                    .into_iter()
                    .find(|l| l.start == table.rows[0].line.end)
                    .ok_or("missing delimiter")?
                    .end
            } else {
                table.rows[row].line.end
            };
            let body = format!(
                "{}| {} |",
                table.continuation,
                vec![""; table.align.len()].join(" | ")
            );
            let replacement = line_insert(state.source.text(), at, &body);
            edit(state, at..at, replacement)
        }
    }
}

pub struct ListActions;
impl ListActions {
    fn resolve(state: &EditingState, target: &Target) -> Option<list::Item> {
        target.check(state).ok()?;
        if target.role != "list.item" || !target.part.is_empty() {
            return None;
        }
        list::items(&state.parse)
            .into_iter()
            .find(|item| item.range == target.owner)
    }
}
impl RenderExtension for ListActions {
    fn id(&self) -> &'static str {
        "list"
    }
    fn commands(&self) -> &'static [&'static str] {
        &["list.insert-after"]
    }
    fn contribute(&self, state: &EditingState) -> Result<Vec<Contribution>, String> {
        Ok(list::items(&state.parse)
            .into_iter()
            .map(|item| Contribution {
                provider: self.id(),
                id: format!("{}", item.marker.start),
                target: Target {
                    document: state.source.id(),
                    role: "list.item",
                    owner: item.range,
                    part: vec![],
                },
                placement: Placement::After,
                priority: 0,
                actions: vec![action("list.insert-after", true)],
            })
            .collect())
    }
    fn available(&self, state: &EditingState, target: &Target, command: &str) -> bool {
        command == "list.insert-after" && Self::resolve(state, target).is_some()
    }
    fn plan(
        &self,
        state: &EditingState,
        target: &Target,
        command: &str,
    ) -> Result<CommandPlan, String> {
        if !self.available(state, target, command) {
            return Err("invalid list action".into());
        }
        let item = Self::resolve(state, target).ok_or("invalid list target")?;
        let text = state.source.text();
        let first = code_block::lines(text)
            .into_iter()
            .find(|l| l.contains(&item.marker.start))
            .ok_or("missing item line")?;
        let at = code_block::lines(text)
            .into_iter()
            .find(|l| l.start < item.range.end && item.range.end <= l.end)
            .ok_or("missing subtree end")?
            .end;
        let body = format!(
            "{}{}new",
            &text[first.start..item.marker.start],
            item.next_marker
        );
        edit(state, at..at, line_insert(text, at, &body))
    }
}

fn line_insert(text: &str, at: usize, body: &str) -> String {
    let newline = code_block::lines(text)
        .into_iter()
        .filter(|l| l.start < at.max(1))
        .rev()
        .find_map(|l| {
            let line = &text[l];
            if line.ends_with("\r\n") {
                Some("\r\n")
            } else if line.ends_with('\r') {
                Some("\r")
            } else if line.ends_with('\n') {
                Some("\n")
            } else {
                None
            }
        })
        .unwrap_or("\n");
    if text[..at].ends_with(['\r', '\n']) {
        format!("{body}{newline}")
    } else {
        format!("{newline}{body}")
    }
}

// Existing production session does the validation, publication and undo. This
// helper is prototype-only; production should reuse a single checkpoint builder.
pub fn edit(
    state: &EditingState,
    range: Range<usize>,
    replacement: String,
) -> Result<CommandPlan, String> {
    let prepared = PreparedEdit::prepare(
        &state.source,
        vec![Edit {
            range: range.clone(),
            replacement,
        }],
    )
    .map_err(|e| format!("{e:?}"))?;
    let applied = prepared.applied();
    let mut next = state.clone();
    for reveal in [
        &mut next.reveal,
        &mut next.heading_reveal,
        &mut next.block_source,
    ]
    .into_iter()
    .flatten()
    {
        reveal
            .transform(&applied.positions)
            .map_err(|e| format!("{e:?}"))?;
    }
    next.closers = state
        .closers
        .iter()
        .filter_map(|r| {
            if r.start < range.end && range.start < r.end {
                return None;
            }
            let start = applied
                .positions
                .map_bytes(state.source.id(), r.start, Affinity::Downstream)
                .ok()?;
            let end = applied
                .positions
                .map_bytes(state.source.id(), r.end, Affinity::Upstream)
                .ok()?;
            (start < end
                && applied.source.text().get(start..end) == state.source.text().get(r.clone()))
            .then_some(start..end)
        })
        .collect();
    next.anchor = applied
        .positions
        .map_cursor(
            &state.source,
            &applied.source,
            state.anchor,
            Affinity::Downstream,
        )
        .map_err(|e| format!("{e:?}"))?;
    next.head = applied
        .positions
        .map_cursor(
            &state.source,
            &applied.source,
            state.head,
            Affinity::Downstream,
        )
        .map_err(|e| format!("{e:?}"))?;
    next.source = applied.source.clone();
    next.parse = Arc::new(ParseSnapshot::with_math(next.source.clone()));
    Ok(CommandPlan::Edit {
        prepared,
        state: next,
        handled: true,
    })
}
