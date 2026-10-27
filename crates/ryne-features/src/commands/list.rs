use super::*;
impl Planner<'_> {
    pub fn indent_list(&mut self, outdent: bool) -> Result<bool, Error> {
        if self.state.source_mode
            || self.in_block_source(self.state.head)
            || code_block::cached(&self.state.parse)
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
        let rows = list::cached_rows(&self.state.parse);
        // Hidden-prefix mapping is only relevant to this physical list row.
        let at = rows.partition_point(|r| r.line.end < self.state.head);
        if !rows
            .get(at)
            .is_some_and(|r| r.line.start <= self.state.head && self.state.head <= r.line.end)
        {
            return Ok(false);
        }
        let display = self.display();
        let caret = display.position_at(self.state.head);
        let Some(index) = rows[at..]
            .iter()
            .take_while(|r| r.line.start <= self.state.head)
            .position(|r| {
                caret
                    == display.position_at(if forward {
                        r.content.end
                    } else {
                        r.content.start
                    })
            })
        else {
            return Ok(false);
        };
        let index = at + index;
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

impl Planner<'_> {
    pub fn list_line_break(&mut self, hard: bool) -> Result<bool, Error> {
        let selection = self.selection();
        let newline = self.newline_at(selection.start);
        if !self.state.source_mode
            && !self.in_block_source(selection.start)
            && let Some(row) = list::rows(&self.state.parse)
                .into_iter()
                .find(|r| r.content.start <= selection.start && selection.start <= r.content.end)
        {
            if row.first
                && self.state.source.text()[row.content.clone()]
                    .trim()
                    .is_empty()
                && selection.is_empty()
            {
                self.indent_list(true)?;
                return Ok(true);
            }
            let prefix = if hard {
                format!("  {newline}{}", row.item.continuation)
            } else {
                format!(
                    "{newline}{}{}",
                    &self.state.source.text()[row.item.range.start..row.item.marker.start],
                    row.item.next_marker
                )
            };
            if !hard {
                let mut edits = vec![Edit {
                    range: selection.clone(),
                    replacement: prefix,
                }];
                let before = list::columns(&row.item.continuation);
                let after = list::columns(&format!(
                    "{}{}",
                    &self.state.source.text()[row.item.range.start..row.item.marker.start],
                    row.item.next_marker
                ));
                // Keep the remainder of this item inside the new item's content
                // column when an ordered marker grows (9 -> 10) or shrinks.
                if before != after {
                    for line in code_block::lines(self.state.source.text())
                        .into_iter()
                        .filter(|line| {
                            selection.end < line.start && line.start < row.item.range.end
                        })
                    {
                        if self.state.source.text()[line.clone()].trim().is_empty() {
                            continue;
                        }
                        let start = list::quote_end(&self.state.parse, &line);
                        if after > before {
                            edits.push(Edit {
                                range: start..start,
                                replacement: " ".repeat(after - before),
                            });
                        } else if let Some((range, replacement)) =
                            list::unindent(self.state.source.text(), &line, start, before - after)
                        {
                            edits.push(Edit { range, replacement });
                        }
                    }
                }
                return self.apply_edits(edits, Some(selection.end)).map(|()| true);
            }
            return self.replace(selection, &prefix).map(|()| true);
        }
        Ok(false)
    }
}
