//! Existing rules now plan against an isolated read-only session snapshot.
//! Only the host publishes the returned candidate and history.
mod append;
mod code;
pub(crate) mod dispatch;
mod format;
mod headings;
mod inline;
mod list_actions;
#[path = "commands/list.rs"]
mod list_rules;
mod math;
mod paragraph;
mod quote;
mod table_actions;
#[path = "commands/table.rs"]
mod table_rules;
use crate::presentation;
use ryne_core::PreparedEdit;
use ryne_extensions::{
    CommandDisplayCache, EditingState, FormulaRenderer, InsertBlock, PresentationKey,
};
use ryne_parser::{Affinity, Edit, Error, NodeKind, Session, Source};
use ryne_projection::IndexedDisplay;
use ryne_syntax::ParseSnapshot;
use ryne_syntax::{block, code_block, list, table};
use std::ops::Range;
pub(crate) struct Planner<'a> {
    pub state: EditingState,
    pub prepared: Option<PreparedEdit>,
    pub renderer: &'a dyn FormulaRenderer,
    pub mapping: Option<&'a CommandDisplayCache>,
}
impl Planner<'_> {
    pub fn selection(&self) -> Range<usize> {
        self.state.selection()
    }
    pub fn display(&self) -> std::sync::Arc<IndexedDisplay> {
        let block = self.block_window();
        let key = PresentationKey {
            document: self.state.source.id(),
            source_mode: self.state.source_mode,
            inline: self.state.reveal.as_ref().map(|s| s.range.clone()),
            heading: self.state.heading_reveal.as_ref().map(|s| s.range.clone()),
            block: block.clone(),
            selection: None,
            closers: None,
            generation: ryne_extensions::catch_callback("formulas", "planning generation", || {
                self.renderer.presentation_generation()
            })
            .ok()
            .flatten()
            .unwrap_or(0),
        };
        let build = || {
            presentation::build_with_heading_using(
                &self.state.parse,
                self.state.reveal.as_ref().map(|s| &s.range),
                self.state.source_mode,
                self.state.heading_reveal.as_ref().map(|s| &s.range),
                block.as_ref(),
                self.renderer,
            )
        };
        if let Some(cache) = self.mapping {
            cache.get_or_build(key, build)
        } else {
            std::sync::Arc::new(IndexedDisplay::new(build()))
        }
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
            let active = (!self.state.source_mode)
                .then(|| presentation::active_window(&self.state.parse, position))
                .flatten();
            self.state.reveal = active.and_then(|r| Session::new(&self.state.source, r).ok());
        }
    }
    pub fn replace(&mut self, range: Range<usize>, text: &str) -> Result<(), Error> {
        let empty_code = !self.state.source_mode
            && !self.in_block_source(range.start)
            && range.is_empty()
            && !text.is_empty()
            && code_block::cached(&self.state.parse).iter().any(|block| {
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
    pub(crate) fn apply_edits(
        &mut self,
        edits: Vec<Edit>,
        collapse: Option<usize>,
    ) -> Result<(), Error> {
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
