use super::*;
impl Planner<'_> {
    pub fn quote_line_break(&mut self, hard: bool) -> Result<bool, Error> {
        let selection = self.selection();
        if self.state.source_mode || self.in_block_source(selection.start) {
            return Ok(false);
        }
        let normalized = ryne_syntax::parser::normalized(&self.state.parse);
        let parse = normalized.unwrap_or(&self.state.parse);
        let Some(row) = ryne_syntax::quote::at(parse, selection.start) else {
            return Ok(false);
        };
        if selection.start < row.content.start || selection.end > row.content.end {
            return Ok(false);
        }
        // A list inside a quote retains list Enter; a quote inside a list
        // continues the quote while preserving the enclosing list indentation.
        for item in list::items(parse) {
            if item.range.start <= selection.start && selection.start <= item.range.end {
                let mut parent = parse.nodes[item.id].parent;
                while let Some(id) = parent {
                    if id == row.owner {
                        return Ok(false);
                    }
                    parent = parse.nodes[id].parent;
                }
            }
        }
        if !hard
            && selection.is_empty()
            && self.state.source.text()[row.content.clone()]
                .trim()
                .is_empty()
            && let Some(prefix) = row.exit
        {
            self.replace(prefix.start..row.content.end, "")?;
            return Ok(true);
        }
        let newline = self.newline_at(selection.start);
        let replacement = format!(
            "{}{newline}{}",
            if hard { "  " } else { "" },
            row.continuation
        );
        self.replace(selection, &replacement)?;
        Ok(true)
    }
}
