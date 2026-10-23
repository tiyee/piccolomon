use super::*;
use ryne_extensions::{InputDraft, InputOrigin};
impl Planner<'_> {
    pub(super) fn apply_draft(&mut self, draft: &InputDraft) -> Result<(), Error> {
        self.state.source.check(draft.base)?;
        let range = draft.range.validate(&self.state.source)?;
        self.replace(range.clone(), &draft.text)?;
        self.state.head = range.start + draft.caret;
        self.state.anchor = self.state.head;
        self.state.closers.extend(
            draft
                .closers
                .iter()
                .map(|r| range.start + r.start..range.start + r.end),
        );
        Ok(())
    }
    pub(super) fn complete_inline(&mut self, draft: &mut InputDraft) -> Result<bool, Error> {
        let range = draft.bytes();
        let text = draft.text.as_str();
        if !range.is_empty()
            || self.state.source_mode
            || self.in_block_source(range.start)
            || text.is_empty()
        {
            return Ok(false);
        }
        if let Some(index) = self.state.closers.iter().position(|closing| {
            closing.start == range.start
                && self.state.source.text()[closing.clone()].starts_with(text)
                && presentation::active_window(&self.state.parse, range.start)
                    .is_some_and(|active| active.start < closing.start && closing.end <= active.end)
        }) {
            self.state.closers[index].start += text.len();
            self.state.closers.retain(|range| !range.is_empty());
            self.select(range.start + text.len(), false, false);
            return Ok(true);
        }
        let Some(completion) = crate::input::inline(&self.state.parse, range.start, text) else {
            return Ok(false);
        };
        draft
            .complete(
                completion.replacement,
                completion.caret - range.start,
                completion.closing.start - range.start..completion.closing.end - range.start,
            )
            .map_err(|_| Error::InvalidRange)?;
        self.apply_draft(draft)?;
        self.state.reveal = presentation::active_window(&self.state.parse, self.state.head)
            .and_then(|range| Session::new(&self.state.source, range).ok());
        Ok(true)
    }
    pub fn insert_text(
        &mut self,
        range: Range<usize>,
        text: &str,
        origin: InputOrigin,
    ) -> Result<(), Error> {
        let mut draft = InputDraft::new(self.state.source.id(), range.clone(), text, origin);
        draft.range.validate(&self.state.source)?;
        if let Some(table) = self.active_table(range.start) {
            let (r, c) = table.cell_at(range.start);
            let cell = &table.rows[r].cells[c].content;
            if cell.start <= range.start && range.end <= cell.end {
                let preceding = &self.state.source.text()[cell.start..range.start];
                let mut input = table::cell_input(text, preceding);
                // A final backslash in a compact row must not escape its column separator.
                if range.end == cell.end
                    && self.state.source.text().as_bytes().get(range.end) == Some(&b'|')
                    && preceding
                        .chars()
                        .chain(input.chars())
                        .rev()
                        .take_while(|ch| *ch == '\\')
                        .count()
                        % 2
                        == 1
                {
                    input.push(' ');
                }
                draft.normalize(input);
                if !self.complete_inline(&mut draft)? {
                    self.apply_draft(&draft)?;
                }
                self.normalize_heading_caret();
                return Ok(());
            }
        }
        if !self.state.source_mode
            && !self.in_block_source(range.start)
            && let Some(block) = code_block::cached(&self.state.parse).iter().find(|block| {
                block.range.contains(&range.start)
                    || (block.closing.is_none() && range.start == block.range.end)
            })
            && !block.continuation.is_empty()
        {
            let mut replacement = String::new();
            if block.closing.is_none()
                && (block.rows.is_empty()
                    || (range.start == block.range.end
                        && self.state.source.text()[..range.start].ends_with(['\r', '\n'])))
            {
                replacement.push_str(&block.continuation);
            }
            for line in code_block::lines(text) {
                replacement.push_str(&text[line.clone()]);
                if text[..line.end].ends_with(['\r', '\n']) {
                    replacement.push_str(&block.continuation);
                }
            }
            draft.normalize(replacement);
            return self.apply_draft(&draft);
        }
        if !self.state.source_mode
            && !self.in_block_source(range.start)
            && text.contains(['\r', '\n'])
            && let Some(row) = list::cached_rows(&self.state.parse)
                .iter()
                .find(|row| row.content.start <= range.start && range.end <= row.content.end)
        {
            let mut replacement = String::new();
            for line in code_block::lines(text) {
                replacement.push_str(&text[line.clone()]);
                if text[..line.end].ends_with(['\r', '\n']) {
                    replacement.push_str(&row.item.continuation);
                }
            }
            draft.normalize(replacement);
            return self.apply_draft(&draft);
        }
        if range.is_empty()
            && !text.is_empty()
            && !text.contains(['\r', '\n'])
            && let Some(markers) = self.heading_hash_run(range.start)
        {
            let hashes = text.bytes().take_while(|b| *b == b'#').count();
            if markers.len() + hashes <= 6 && hashes < text.len() {
                // Commit the first body character and the required ATX separator
                // together, including IME confirmation, as one undoable edit.
                let replacement = if matches!(text.as_bytes()[hashes], b' ' | b'\t') {
                    text.to_owned()
                } else {
                    format!("{} {}", &text[..hashes], &text[hashes..])
                };
                draft.normalize(replacement);
                self.apply_draft(&draft)?;
                self.state.heading_reveal = None;
                return Ok(());
            }
        }
        if self.complete_inline(&mut draft)? {
            return Ok(());
        }
        self.apply_draft(&draft)?;
        self.reveal_heading_input();
        Ok(())
    }
}
