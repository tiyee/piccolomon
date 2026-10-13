//! Existing rules now plan against an isolated read-only session snapshot.
//! Only the host publishes the returned candidate and history.
mod append;
mod code;
mod headings;
mod inline;
mod language;
mod list_actions;
#[path = "commands/list.rs"]
mod list_rules;
mod paragraph;
mod table_actions;
#[path = "commands/table.rs"]
mod table_rules;
use crate::presentation;
use editable_markdown_parser::{Affinity, Edit, Error, NodeKind, ParseSnapshot, Session, Source};
use ryne_core::PreparedEdit;
use ryne_extensions::{EditingState, FormulaRenderer, InsertBlock};
use ryne_projection::Display;
use ryne_syntax::{block, code_block, list, table};
use std::ops::Range;
pub(crate) struct Planner<'a> {
    pub state: EditingState,
    pub prepared: Option<PreparedEdit>,
    pub renderer: &'a dyn FormulaRenderer,
}
impl Planner<'_> {
    pub fn selection(&self) -> Range<usize> {
        self.state.selection()
    }
    pub fn display(&self) -> Display {
        let block = self.block_window();
        presentation::build_with_heading_using(
            &self.state.parse,
            self.state.reveal.as_ref().map(|s| &s.range),
            self.state.source_mode,
            self.state.heading_reveal.as_ref().map(|s| &s.range),
            block.as_ref(),
            self.renderer,
        )
    }
    fn in_block_source(&self, position: usize) -> bool {
        self.state
            .block_source
            .as_ref()
            .is_some_and(|s| s.range.start <= position && position <= s.range.end)
    }
    pub fn block_window(&self) -> Option<Range<usize>> {
        self.state
            .block_source
            .as_ref()
            .map(|s| s.range.clone())
            .or_else(|| {
                (!self.state.source_mode && self.selection().is_empty())
                    .then(|| {
                        crate::input::fence(&self.state.parse, self.state.head)
                            .map(|fence| fence.range)
                    })
                    .flatten()
            })
    }
    pub fn toggle_block_source(&mut self) -> bool {
        if self.state.block_source.take().is_some() {
            return true;
        }
        let scope = block::at(&self.state.parse, self.state.head).map(|block| block.scope);
        if let Some(scope) = scope {
            self.state.block_source = Session::new(&self.state.source, scope).ok();
            self.state.source_mode = false;
            self.state.heading_reveal = None;
            self.state.block_source.is_some()
        } else {
            false
        }
    }
    pub fn select(&mut self, position: usize, extend: bool, frozen: bool) {
        if !extend {
            self.state.anchor = position;
        }
        self.state.head = position;
        if !frozen {
            if self
                .state
                .heading_reveal
                .as_ref()
                .is_some_and(|session| !session.range.contains(&position))
            {
                self.state.heading_reveal = None;
            }
            let active = presentation::active_window(&self.state.parse, position);
            self.state.reveal = active.and_then(|r| Session::new(&self.state.source, r).ok());
        }
    }
    pub fn line_break(&mut self, hard: bool) -> Result<(), Error> {
        if hard && self.active_table(self.state.head).is_some() {
            return self.insert_text(
                self.selection(),
                "\n",
                ryne_extensions::InputOrigin::Keyboard,
            );
        }
        if self.navigate_table(false, true)? {
            return Ok(());
        }
        let selection = self.selection();
        let newline = self.newline_at(selection.start);
        if !self.state.source_mode
            && !self.in_block_source(selection.start)
            && selection.is_empty()
            && let Some(fence) = crate::input::fence(&self.state.parse, selection.start)
        {
            let replacement = format!(
                "{newline}{}{newline}{}{}",
                fence.continuation, fence.continuation, fence.closing
            );
            self.replace(selection.clone(), &replacement)?;
            self.state.head = selection.start + newline.len() + fence.continuation.len();
            self.state.anchor = self.state.head;
            self.state.reveal = None;
            return Ok(());
        }
        if !self.state.source_mode
            && !self.in_block_source(selection.start)
            && let Some(block) = code_block::blocks(&self.state.parse)
                .into_iter()
                .find(|block| {
                    block.range.contains(&selection.start)
                        || (block.closing.is_none() && selection.start == block.range.end)
                })
        {
            let content_start = block
                .rows
                .iter()
                .find(|row| {
                    row.content.start <= selection.start && selection.start <= row.content.end
                })
                .map_or(selection.start, |row| row.content.start);
            let whitespace = &self.state.source.text()[content_start..selection.start];
            let indent = whitespace.len() - whitespace.trim_start_matches([' ', '\t']).len();
            let prefix = if block.closing.is_none()
                && (block.rows.is_empty()
                    || (selection.start == block.range.end
                        && self.state.source.text()[..selection.start].ends_with(['\r', '\n'])))
            {
                block.continuation.as_str()
            } else {
                ""
            };
            let replacement = format!(
                "{prefix}{newline}{}{}",
                block.continuation,
                &whitespace[..indent]
            );
            return self.replace(selection, &replacement);
        }
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
                return Ok(());
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
                return self.apply_edits(edits, Some(selection.end));
            }
            return self.replace(selection, &prefix);
        }
        let paragraph = !self.state.source_mode
            && !self.in_block_source(selection.start)
            && self
                .paragraph_ranges()
                .any(|range| range.start <= selection.start && selection.start <= range.end);
        let replacement = if paragraph && hard {
            format!("  {newline}")
        } else if paragraph {
            newline.repeat(2)
        } else {
            newline.into()
        };
        self.replace(selection, &replacement)
    }
    pub fn replace(&mut self, range: Range<usize>, text: &str) -> Result<(), Error> {
        let empty_code = !self.state.source_mode
            && !self.in_block_source(range.start)
            && range.is_empty()
            && !text.is_empty()
            && code_block::blocks(&self.state.parse).iter().any(|block| {
                block.rows.is_empty() && block.closing.is_some() && block.anchor == range.start
            });
        let append_newline = empty_code;
        let replacement = if append_newline {
            format!(
                "{text}{}{prefix}",
                self.newline_at(range.start),
                prefix = code_block::blocks(&self.state.parse)
                    .into_iter()
                    .find(|block| block.anchor == range.start)
                    .map_or(String::new(), |block| block.continuation)
            )
        } else {
            text.into()
        };
        self.apply_edits(
            vec![Edit {
                range: range.clone(),
                replacement,
            }],
            Some(range.end),
        )?;
        if append_newline {
            self.state.head = range.start + text.len();
            self.state.anchor = self.state.head;
        }
        Ok(())
    }
    fn apply_edits(&mut self, edits: Vec<Edit>, collapse: Option<usize>) -> Result<(), Error> {
        let closers = self.state.closers.clone();
        let retained = closers
            .iter()
            .filter(|closing| {
                !edits
                    .iter()
                    .any(|edit| edit.range.start < closing.end && closing.start < edit.range.end)
            })
            .cloned()
            .collect::<Vec<_>>();
        if self.prepared.is_some() {
            return Err(Error::Overlap);
        }
        let prepared = match PreparedEdit::prepare_plan(
            &self.state.source,
            edits
                .into_iter()
                .map(|e| ryne_core::SourceEdit::from_parser(self.state.source.id(), e))
                .collect(),
        ) {
            Err(Error::NoChange) => return Ok(()),
            other => other?,
        };
        let applied = prepared.applied();
        if let Some(session) = &mut self.state.reveal {
            session.transform(&applied.positions)?;
        }
        if let Some(session) = &mut self.state.heading_reveal {
            session.transform(&applied.positions)?;
        }
        if let Some(session) = &mut self.state.block_source {
            session.transform(&applied.positions)?;
        }
        self.state.closers = retained
            .into_iter()
            .filter_map(|range| {
                let start = applied
                    .positions
                    .map_bytes(self.state.source.id(), range.start, Affinity::Downstream)
                    .ok()?;
                let end = applied
                    .positions
                    .map_bytes(self.state.source.id(), range.end, Affinity::Upstream)
                    .ok()?;
                (start < end
                    && applied.source.text().get(start..end) == self.state.source.text().get(range))
                .then_some(start..end)
            })
            .collect();
        self.state.head = applied.positions.map_cursor(
            &self.state.source,
            &applied.source,
            collapse.unwrap_or(self.state.head),
            Affinity::Downstream,
        )?;
        self.state.anchor = if collapse.is_some() {
            self.state.head
        } else {
            applied.positions.map_cursor(
                &self.state.source,
                &applied.source,
                self.state.anchor,
                Affinity::Downstream,
            )?
        };
        self.state.source = applied.source.clone();
        self.state.parse = std::sync::Arc::new(ParseSnapshot::with_math(self.state.source.clone()));
        if self.state.reveal.is_none() {
            self.state.reveal = presentation::active_window(&self.state.parse, self.state.head)
                .and_then(|r| Session::new(&self.state.source, r).ok());
        }
        self.prepared = Some(prepared);
        Ok(())
    }
}
