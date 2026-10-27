use super::*;
impl Planner<'_> {
    pub(super) fn paragraph_ranges(&self) -> &[Range<usize>] {
        self.state.parse.paragraphs()
    }
    pub(super) fn newline_at(&self, caret: usize) -> &'static str {
        let text = self.state.source.text();
        let index = text[caret..]
            .find(['\r', '\n'])
            .map(|i| caret + i)
            .or_else(|| text[..caret].rfind(['\r', '\n']));
        match index.map(|i| (i, text.as_bytes()[i])) {
            Some((i, b'\r')) if text.as_bytes().get(i + 1) == Some(&b'\n') => "\r\n",
            Some((i, b'\n')) if i > 0 && text.as_bytes()[i - 1] == b'\r' => "\r\n",
            Some((_, b'\r')) => "\r",
            _ => "\n",
        }
    }
    pub fn join_paragraphs(&mut self, forward: bool) -> Result<bool, Error> {
        if self.state.source_mode
            || self.in_block_source(self.state.head)
            || !self.selection().is_empty()
        {
            return Ok(false);
        }
        let caret = self.state.head;
        let text = self.state.source.text();
        let ranges = self.paragraph_ranges();
        let boundary = if forward {
            if !ranges
                .get(ranges.partition_point(|range| range.end < caret))
                .is_some_and(|range| range.end == caret)
            {
                return Ok(false);
            }
            let end = ranges
                .get(ranges.partition_point(|range| range.start <= caret))
                .map_or(text.len(), |range| range.start);
            caret..end
        } else {
            if caret != text.len()
                && ranges
                    .binary_search_by_key(&caret, |range| range.start)
                    .is_err()
            {
                return Ok(false);
            }
            let start = ranges
                .partition_point(|range| range.end < caret)
                .checked_sub(1)
                .and_then(|i| ranges.get(i))
                .map_or(caret, |range| range.end);
            start..caret
        };
        let gap = &text[boundary.clone()];
        if gap
            .chars()
            .any(|ch| !matches!(ch, '\r' | '\n' | ' ' | '\t'))
            || gap
                .replace("\r\n", "\n")
                .chars()
                .filter(|&ch| matches!(ch, '\r' | '\n'))
                .count()
                < 2
        {
            return Ok(false);
        }
        self.replace(boundary, "")?;
        Ok(true)
    }
}

impl Planner<'_> {
    pub fn paragraph_line_break(&mut self, hard: bool) -> Result<(), Error> {
        let selection = self.selection();
        let newline = self.newline_at(selection.start);
        let paragraph = !self.state.source_mode && !self.in_block_source(selection.start) && {
            let ranges = self.paragraph_ranges();
            ranges
                .get(ranges.partition_point(|range| range.end < selection.start))
                .is_some_and(|range| range.start <= selection.start && selection.start <= range.end)
        };
        let replacement = if paragraph && hard {
            format!("  {newline}")
        } else if paragraph {
            newline.repeat(2)
        } else {
            newline.into()
        };
        self.replace(selection, &replacement)
    }
    pub fn delete_grapheme(&mut self, forward: bool) -> Result<bool, Error> {
        if !self.selection().is_empty() {
            self.replace(self.selection(), "")?;
            return Ok(true);
        }
        if let Some(range) = self.display().deletion_range(self.state.head, forward) {
            self.replace(range, "")?;
        }
        Ok(true)
    }
}
