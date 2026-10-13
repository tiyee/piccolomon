use super::*;
impl Planner<'_> {
    pub fn remove_code_boundary(&mut self, forward: bool) -> Result<bool, Error> {
        if self.state.source_mode
            || self.in_block_source(self.state.head)
            || !self.selection().is_empty()
        {
            return Ok(false);
        }
        for block in code_block::blocks(&self.state.parse) {
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
