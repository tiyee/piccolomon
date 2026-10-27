//! Temporary input and its source mapping belong to the session, never to GUI geometry.
use super::*;
use editable_markdown_parser::Session;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Composition {
    pub range: Range<usize>,
    pub text: String,
    pub selection: Range<usize>,
}
impl EditorSession {
    pub fn composition(&self) -> Option<&Composition> {
        self.composition.as_ref()
    }
    pub fn editable_text(&self) -> String {
        let mut text = self.source.text().to_owned();
        if let Some(c) = &self.composition {
            text.replace_range(c.range.clone(), &c.text);
        }
        text
    }
    pub fn editable_text_cow(&self) -> std::borrow::Cow<'_, str> {
        if self.composition.is_none() {
            std::borrow::Cow::Borrowed(self.source.text())
        } else {
            std::borrow::Cow::Owned(self.editable_text())
        }
    }
    pub fn editable_utf16_to_byte(
        &self,
        units: usize,
    ) -> Result<usize, editable_markdown_parser::Error> {
        let index = self.parse.utf16();
        let Some(c) = &self.composition else {
            return index.utf16_to_byte(units);
        };
        let prefix = index.byte_to_utf16(c.range.start)?;
        if units <= prefix {
            return index.utf16_to_byte(units);
        }
        let added = c.text.encode_utf16().count();
        if units - prefix <= added {
            return editable_markdown_parser::utf16_to_utf8(&c.text, units - prefix)
                .map(|p| c.range.start + p);
        }
        let removed = index.byte_to_utf16(c.range.end)? - prefix;
        index
            .utf16_to_byte(
                units
                    .checked_sub(added)
                    .and_then(|u| u.checked_add(removed))
                    .ok_or(editable_markdown_parser::Error::InvalidRange)?,
            )
            .map(|p| p - c.range.len() + c.text.len())
    }
    pub fn editable_byte_to_utf16(
        &self,
        position: usize,
    ) -> Result<usize, editable_markdown_parser::Error> {
        let index = self.parse.utf16();
        let Some(c) = &self.composition else {
            return index.byte_to_utf16(position);
        };
        if position <= c.range.start {
            return index.byte_to_utf16(position);
        }
        let prefix = index.byte_to_utf16(c.range.start)?;
        if position - c.range.start <= c.text.len() {
            return c
                .text
                .get(..position - c.range.start)
                .map(|s| prefix + s.encode_utf16().count())
                .ok_or(editable_markdown_parser::Error::InvalidRange);
        }
        let at = position
            .checked_sub(c.text.len())
            .and_then(|p| p.checked_add(c.range.len()))
            .ok_or(editable_markdown_parser::Error::InvalidRange)?;
        let removed = index.byte_to_utf16(c.range.end)? - prefix;
        Ok(index.byte_to_utf16(at)? - removed + c.text.encode_utf16().count())
    }
    pub fn editable_selection(&self) -> Range<usize> {
        self.composition
            .as_ref()
            .map_or_else(|| self.selection(), |c| c.selection.clone())
    }
    pub fn editable_caret(&self) -> usize {
        self.composition
            .as_ref()
            .map_or(self.head, |c| c.selection.end)
    }
    /// Ranges are UTF-8 bytes in the current temporary text; selection is local to `text`.
    /// Validate everything before replacing the previous preedit.
    pub fn update_composition(
        &mut self,
        requested: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
    ) -> Result<(), String> {
        let local = selection.unwrap_or(text.len()..text.len());
        if text.get(local.clone()).is_none() {
            return Err("invalid composition selection".into());
        }
        let (range, value, marked_start) = if let Some(old) = &self.composition {
            let marked = old.range.start..old.range.start + old.text.len();
            let requested = requested.unwrap_or(marked.clone());
            if requested.start < marked.start
                || requested.end > marked.end
                || requested.start > requested.end
            {
                return Err("composition replacement is outside the marked range".into());
            }
            let local_range = requested.start - marked.start..requested.end - marked.start;
            if old.text.get(local_range.clone()).is_none() {
                return Err("invalid composition replacement".into());
            }
            let mut value = old.text.clone();
            value.replace_range(local_range.clone(), text);
            (old.range.clone(), value, local_range.start)
        } else {
            (
                requested.unwrap_or_else(|| self.selection()),
                text.to_owned(),
                0,
            )
        };
        if self.source.text().get(range.clone()).is_none() {
            return Err("invalid composition source range".into());
        }
        let start = range.start + marked_start;
        self.composition = Some(Composition {
            range,
            text: value,
            selection: start + local.start..start + local.end,
        });
        Ok(())
    }
    pub fn cancel_composition(&mut self) -> bool {
        self.composition.take().is_some()
    }
    pub fn commit_composition(&mut self) -> Result<bool, String> {
        let Some(c) = &self.composition else {
            return Ok(false);
        };
        let text = c.text.clone();
        self.commit_composition_text(None, &text)?;
        Ok(true)
    }
    /// A platform confirmation may replace a different range in the temporary text.
    /// Its source edit and final selection are validated and published atomically.
    pub fn commit_composition_text(
        &mut self,
        requested: Option<Range<usize>>,
        text: &str,
    ) -> Result<(), String> {
        let c = self.composition.as_ref().ok_or("no active composition")?;
        let marked = c.range.start..c.range.start + c.text.len();
        let requested = requested.unwrap_or(marked.clone());
        let (range, replacement, caret) = if requested == marked {
            (c.range.clone(), text.to_owned(), None)
        } else {
            let mut result = self.editable_text();
            if result.get(requested.clone()).is_none() {
                return Err("invalid composition confirmation range".into());
            }
            result.replace_range(requested.clone(), text);
            let caret = requested.start + text.len();
            // Keep unchanged bytes outside the union of the platform edit and preedit.
            let start = requested.start.min(c.range.start);
            let end = if requested.end > marked.end {
                c.range.end + requested.end - marked.end
            } else {
                c.range.end
            };
            let result_end = result.len() - (self.source.text().len() - end);
            (
                start..end,
                result[start..result_end].to_owned(),
                Some(caret),
            )
        };
        let intent = EditIntent::Insert {
            range: ryne_core::SourceRange::new(self.source.id(), range),
            text: replacement,
            origin: InputOrigin::Ime,
        };
        let mut plan = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.features.plan(intent, &self.state)
        }))
        .map_err(|_| "composition planning failed".to_string())??;
        if let Some(caret) = caret {
            match &mut plan {
                CommandPlan::Edit { state, .. }
                | CommandPlan::Select { state, .. }
                | CommandPlan::View { state, .. } => {
                    state.anchor = caret;
                    state.head = caret;
                }
                CommandPlan::Consume => {}
            }
        }
        self.publish_plan_inner(plan, true)?;
        Ok(())
    }
    /// Read-only projection input. No temporary source is ever published to history.
    pub fn projection_state(
        &self,
        diagrams: &[DiagramViewInput],
    ) -> (EditingState, Vec<DiagramViewInput>) {
        let Some(c) = &self.composition else {
            return (self.state.clone(), diagrams.to_vec());
        };
        let source = Source::new(&self.editable_text());
        let map = |position: usize, end: bool| {
            if position < c.range.start {
                position
            } else if position > c.range.end || (!c.range.is_empty() && position == c.range.end) {
                position - c.range.len() + c.text.len()
            } else {
                c.range.start + if end { c.text.len() } else { 0 }
            }
        };
        let range = |r: &Range<usize>| map(r.start, false)..map(r.end, true);
        let mut window = c.range.start..c.range.start + c.text.len();
        if let Some(reveal) = &self.reveal {
            let mapped = range(&reveal.range);
            window.start = window.start.min(mapped.start);
            window.end = window.end.max(mapped.end);
        }
        let mut state = self.state.clone();
        state.source = source;
        state.parse = Arc::new(ParseSnapshot::with_math(state.source.clone()));
        state.anchor = c.selection.start;
        state.head = c.selection.end;
        state.reveal = Session::new(&state.source, window).ok();
        state.heading_reveal = self
            .heading_reveal
            .as_ref()
            .and_then(|r| Session::new(&state.source, range(&r.range)).ok());
        state.block_source = self
            .block_window()
            .and_then(|r| Session::new(&state.source, range(&r)).ok());
        state.closers = self
            .closers
            .iter()
            .filter(|r| r.end <= c.range.start || c.range.end <= r.start)
            .map(range)
            .collect();
        let diagrams = diagrams
            .iter()
            .map(|(r, outcome)| (range(r), outcome.clone()))
            .collect();
        (state, diagrams)
    }
}
