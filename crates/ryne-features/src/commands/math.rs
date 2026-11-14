//! Display-math edits use the same source transaction and container context.
use super::*;
impl Planner<'_> {
    pub fn math_line_break(&mut self) -> Result<bool, Error> {
        let selection = self.selection();
        if self.state.source_mode || self.in_block_source(selection.start) {
            return Ok(false);
        }
        let newline = self.newline_at(selection.start);
        if let Some(formula) = ryne_syntax::math::cached(self.state.parse.semantic())
            .iter()
            .find(|f| f.block && f.range.start < selection.start && selection.end < f.range.end)
        {
            let prefix = ryne_syntax::math::continuation(&self.state.parse, formula.range.start);
            self.replace(selection, &format!("{newline}{prefix}"))?;
            return Ok(true);
        }
        if selection.is_empty() {
            if let Some(fence) = crate::input::math_fence(&self.state.parse, selection.start) {
                let replacement = format!(
                    "{newline}{}{newline}{}$$",
                    fence.continuation, fence.continuation
                );
                self.replace(selection.clone(), &replacement)?;
                self.state.head = selection.start + newline.len() + fence.continuation.len();
                self.state.anchor = self.state.head;
                return Ok(true);
            }
            // An empty newly completed body is not a backend math node yet.
            // Probe with literal content; only parser-confirmed candidates qualify.
            if let Some(prefix) = crate::input::empty_math_body(&self.state.parse, selection.start)
            {
                self.replace(selection, &format!("{newline}{prefix}"))?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}
