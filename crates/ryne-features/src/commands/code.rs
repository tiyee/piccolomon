use super::*;
impl Planner<'_> {
    pub fn remove_code_boundary(&mut self, forward: bool) -> Result<bool, Error> {
        if self.state.source_mode
            || self.in_block_source(self.state.head)
            || !self.selection().is_empty()
        {
            return Ok(false);
        }
        for block in code_block::cached(&self.state.parse) {
            if block.rows.is_empty() && self.state.head == block.anchor {
                return Ok(true);
            }
            for (index, row) in block.rows.iter().enumerate() {
                if !forward && self.state.head == row.content.start {
                    if let Some(previous) = index.checked_sub(1).and_then(|i| block.rows.get(i)) {
                        self.replace(previous.content.end..self.state.head, "")?;
                    }
                    return Ok(true);
                }
                if forward && self.state.head == row.content.end {
                    if let Some(next) = block.rows.get(index + 1) {
                        self.replace(self.state.head..next.content.start, "")?;
                    }
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

impl Planner<'_> {
    pub fn code_line_break(&mut self) -> Result<bool, Error> {
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
            return Ok(true);
        }
        if !self.state.source_mode
            && !self.in_block_source(selection.start)
            && let Some(block) = code_block::cached(&self.state.parse).iter().find(|block| {
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
            return self.replace(selection, &replacement).map(|()| true);
        }
        Ok(false)
    }
}
