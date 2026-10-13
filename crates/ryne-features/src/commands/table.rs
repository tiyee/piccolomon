use super::*;
impl Planner<'_> {
    pub(super) fn active_table(&self, position: usize) -> Option<table::Table> {
        if self.state.source_mode || self.in_block_source(position) {
            return None;
        }
        table::tables(&self.state.parse).into_iter().find(|table| {
            table.range.contains(&position)
                || table
                    .rows
                    .last()
                    .and_then(|row| row.cells.last())
                    .is_some_and(|cell| cell.content.end == position)
        })
    }
    pub fn navigate_table(&mut self, backward: bool, next_row: bool) -> Result<bool, Error> {
        let Some(table) = self.active_table(self.state.head) else {
            return Ok(false);
        };
        let (row, column) = table.cell_at(self.state.head);
        let columns = table.align.len();
        let index = row * columns + column;
        let target = if backward {
            index.checked_sub(1)
        } else if next_row {
            Some(index + columns)
        } else {
            Some(index + 1)
        };
        let Some(target) = target else {
            return Ok(true);
        };
        if target < table.rows.len() * columns {
            let display = self.display();
            let anchor = table.rows[target / columns].cells[target % columns]
                .content
                .start;
            let anchor = display.source_at(display.position_at(anchor), Affinity::Downstream);
            self.select(anchor, false, false);
            return Ok(true);
        }
        let newline = self.newline_at(self.state.head);
        let at = table.range.end;
        let body = format!(
            "{}| {} |",
            table.continuation,
            vec![""; columns].join(" | ")
        );
        let prefix = if self.state.source.text()[..at].ends_with(['\r', '\n']) {
            ""
        } else {
            newline
        };
        let suffix = if prefix.is_empty() { newline } else { "" };
        self.replace(at..at, &format!("{prefix}{body}{suffix}"))?;
        if let Some(updated) = table::tables(&self.state.parse)
            .into_iter()
            .find(|t| t.range.start == table.range.start)
            && let Some(row) = updated.rows.last()
        {
            self.select(row.cells[target % columns].content.start, false, false);
        }
        Ok(true)
    }
    pub fn remove_table_boundary(&self, forward: bool) -> bool {
        let Some(table) = self.active_table(self.state.head) else {
            return false;
        };
        if !self.selection().is_empty() {
            return false;
        }
        let (r, c) = table.cell_at(self.state.head);
        let cell = &table.rows[r].cells[c];
        let display = self.display();
        display.position_at(self.state.head)
            == display.position_at(if forward {
                cell.content.end
            } else {
                cell.content.start
            })
    }
}
