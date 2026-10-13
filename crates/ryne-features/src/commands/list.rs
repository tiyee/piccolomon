use super::*;
impl Planner<'_> {
    pub fn indent_list(&mut self, outdent: bool) -> Result<bool, Error> {
        if self.state.source_mode
            || self.in_block_source(self.state.head)
            || code_block::blocks(&self.state.parse)
                .iter()
                .any(|b| b.range.contains(&self.state.head))
        {
            return Ok(false);
        }
        let items = list::items(&self.state.parse);
        let Some(item) = items
            .iter()
            .filter(|item| {
                item.range.contains(&self.state.head)
                    || (self.state.head == self.state.source.text().len()
                        && self.state.head == item.range.end)
            })
            .max_by_key(|item| item.inset)
        else {
            return Ok(false);
        };
        // A selection extending outside this item is left intact.
        if self.selection().start < item.range.start || self.selection().end > item.range.end {
            return Ok(true);
        }
        self.indent_list_item(item, outdent)
    }
    pub(super) fn indent_list_item(
        &mut self,
        item: &list::Item,
        outdent: bool,
    ) -> Result<bool, Error> {
        let items = list::items(&self.state.parse);
        let parent = item.parent.and_then(|id| items.iter().find(|i| i.id == id));
        if outdent && parent.is_none() {
            let mut edits = vec![Edit {
                range: item.marker.clone(),
                replacement: String::new(),
            }];
            let width = list::columns(&item.continuation).saturating_sub(list::columns(
                &self.state.source.text()[item.range.start..item.marker.start],
            ));
            for line in code_block::lines(self.state.source.text())
                .into_iter()
                .filter(|l| item.range.start < l.start && l.start < item.range.end)
            {
                let start = list::quote_end(&self.state.parse, &line);
                if let Some((range, replacement)) =
                    list::unindent(self.state.source.text(), &line, start, width)
                {
                    edits.push(Edit { range, replacement });
                }
            }
            self.apply_edits(edits, None)?;
            return Ok(true);
        }
        let amount = if let Some(parent) = parent.filter(|_| outdent) {
            list::columns(&self.state.source.text()[item.range.start..item.marker.start])
                .saturating_sub(list::columns(
                    &self.state.source.text()[parent.range.start..parent.marker.start],
                ))
                .max(1)
        } else {
            let Some(previous) = items
                .iter()
                .rfind(|i| i.list == item.list && i.range.start < item.range.start)
            else {
                return Ok(true);
            };
            list::columns(&previous.continuation)
                .saturating_sub(list::columns(
                    &self.state.source.text()[previous.range.start..previous.marker.start],
                ))
                .max(2)
        };
        let mut edits = Vec::new();
        for line in code_block::lines(self.state.source.text())
            .into_iter()
            .filter(|l| item.range.start <= l.start && l.start < item.range.end)
        {
            let start = list::quote_end(&self.state.parse, &line);
            if outdent {
                if let Some((range, replacement)) =
                    list::unindent(self.state.source.text(), &line, start, amount)
                {
                    edits.push(Edit { range, replacement });
                }
            } else {
                edits.push(Edit {
                    range: start..start,
                    replacement: " ".repeat(amount),
                });
            }
        }
        if !outdent && !edits.is_empty() {
            let candidate = self
                .state
                .source
                .apply(self.state.source.id(), edits.clone())?;
            let body = candidate.positions.map_cursor(
                &self.state.source,
                &candidate.source,
                item.body,
                Affinity::Downstream,
            )?;
            let parse = ParseSnapshot::with_math(candidate.source.clone());
            if !list::items(&parse)
                .iter()
                .any(|i| i.body == body && i.parent.is_some())
            {
                // An empty list cannot interrupt a paragraph. A blank container line
                // prevents its marker from becoming paragraph text or a Setext underline.
                let line = item.range.start..item.marker.end;
                let quote = list::quote_end(&self.state.parse, &line);
                edits[0].replacement = format!(
                    "{}{}{}",
                    self.newline_at(item.body),
                    &self.state.source.text()[item.range.start..quote],
                    edits[0].replacement
                );
            }
        }
        if !edits.is_empty() {
            self.apply_edits(edits, None)?;
        }
        Ok(true)
    }
    pub fn remove_list_boundary(&mut self, forward: bool) -> Result<bool, Error> {
        if self.state.source_mode
            || self.in_block_source(self.state.head)
            || !self.selection().is_empty()
        {
            return Ok(false);
        }
        let rows = list::rows(&self.state.parse);
        let display = self.display();
        let caret = display.position_at(self.state.head);
        let Some(index) = rows.iter().position(|r| {
            caret
                == display.position_at(if forward {
                    r.content.end
                } else {
                    r.content.start
                })
        }) else {
            return Ok(false);
        };
        let row = &rows[index];
        if !forward && row.first {
            return self.indent_list(true);
        }
        let neighbor = if forward {
            rows.get(index + 1)
        } else {
            index.checked_sub(1).and_then(|i| rows.get(i))
        };
        if let Some(next) = neighbor {
            let (a, b) = if forward { (row, next) } else { (next, row) };
            if a.line.end == b.line.start
                && (a.item.id == b.item.id || (forward && a.item.list == b.item.list))
            {
                self.replace(a.content.end..b.content.start, "")?;
            }
        }
        Ok(true)
    }
}
