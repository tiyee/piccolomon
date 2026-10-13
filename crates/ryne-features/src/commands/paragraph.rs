use super::*;
impl Planner<'_> {
    pub(super) fn paragraph_ranges(&self) -> impl Iterator<Item = Range<usize>> + '_ {
        self.state.parse.nodes.iter().filter_map(|node| {
            if node.kind != NodeKind::Paragraph
                || !node
                    .parent
                    .is_some_and(|parent| self.state.parse.nodes[parent].kind == NodeKind::Document)
            {
                return None;
            }
            let end = node.envelope.start
                + self.state.source.text()[node.envelope.clone()]
                    .trim_end_matches(['\r', '\n'])
                    .len();
            Some(node.envelope.start..end)
        })
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
        let ranges = self.paragraph_ranges().collect::<Vec<_>>();
        let boundary = if forward {
            if !ranges.iter().any(|range| range.end == caret) {
                return Ok(false);
            }
            let end = ranges
                .iter()
                .find(|range| caret < range.start)
                .map_or(text.len(), |range| range.start);
            caret..end
        } else {
            if caret != text.len() && !ranges.iter().any(|range| range.start == caret) {
                return Ok(false);
            }
            let start = ranges
                .iter()
                .rev()
                .find(|range| range.end < caret)
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
