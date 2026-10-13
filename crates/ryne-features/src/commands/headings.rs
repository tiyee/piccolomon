use super::*;
impl Planner<'_> {
    pub fn normalize_heading_caret(&mut self) {
        if !self.state.source_mode
            && !self.in_block_source(self.state.head)
            && self.selection().is_empty()
            && crate::input::fence(&self.state.parse, self.state.head).is_some()
        {
            return;
        }
        if let Some(table) = self.active_table(self.state.head)
            && self.selection().is_empty()
        {
            let (r, c) = table.cell_at(self.state.head);
            self.state.head = self.state.head.clamp(
                table.rows[r].cells[c].content.start,
                table.rows[r].cells[c].content.end,
            );
            self.state.anchor = self.state.head;
            return;
        }
        if !self.state.source_mode
            && self.state.anchor == self.state.head
            && !self.in_block_source(self.state.head)
            && let Some(block) = code_block::blocks(&self.state.parse)
                .iter()
                .find(|block| block.range.contains(&self.state.head))
        {
            let position = block.caret(self.state.head);
            self.state.anchor = position;
            self.state.head = position;
            return;
        }
        if !self.state.source_mode
            && self.state.anchor == self.state.head
            && !self.in_block_source(self.state.head)
            && !self
                .state
                .heading_reveal
                .as_ref()
                .is_some_and(|s| s.range.contains(&self.state.head))
        {
            let position = presentation::heading_caret(&self.state.parse, self.state.head);
            self.state.anchor = position;
            self.state.head = position;
            if let Some(row) = list::rows(&self.state.parse)
                .iter()
                .find(|r| r.line.contains(&self.state.head))
            {
                while let Some(prefix) = row.hidden.iter().find(|r| r.contains(&self.state.head)) {
                    self.state.head = prefix.end;
                }
                self.state.anchor = self.state.head;
            }
        }
    }
    pub fn reveal_clicked_heading(&mut self) {
        if self.state.source_mode || self.in_block_source(self.state.head) {
            return;
        }
        self.state.heading_reveal = self.state.parse.nodes.iter().find_map(|node| {
            (matches!(node.kind, NodeKind::Heading(_)) && node.envelope.contains(&self.state.head))
                .then(|| Session::new(&self.state.source, node.envelope.clone()).ok())
                .flatten()
        });
    }
    /// Only an empty ATX heading at the end of its hash run is an input prefix.
    /// The AST supplies block/container context so code and inline hashes stay literal.
    pub(super) fn heading_hash_run(&self, caret: usize) -> Option<Range<usize>> {
        if self.state.source_mode || self.in_block_source(caret) {
            return None;
        }
        let text = self.state.source.text();
        let count = text
            .get(..caret)?
            .bytes()
            .rev()
            .take_while(|b| *b == b'#')
            .count();
        if !(1..=6).contains(&count) {
            return None;
        }
        // The parser's block backend needs LF for bare-CR documents. Keep offsets
        // identical and leave the authoritative source bytes untouched.
        let normalized;
        let parse = if text
            .bytes()
            .enumerate()
            .any(|(i, b)| b == b'\r' && text.as_bytes().get(i + 1) != Some(&b'\n'))
        {
            let normalized_text = text
                .char_indices()
                .map(|(i, ch)| {
                    if ch == '\r' && text.as_bytes().get(i + 1) != Some(&b'\n') {
                        '\n'
                    } else {
                        ch
                    }
                })
                .collect::<String>();
            normalized = ParseSnapshot::with_math(Source::new(&normalized_text));
            &normalized
        } else {
            &self.state.parse
        };
        parse.nodes.iter().find_map(|node| {
            (node.kind == NodeKind::Heading(count as u8)
                && node.children.is_empty()
                && node.envelope.start <= caret - count
                && text[node.envelope.start..caret - count]
                    .bytes()
                    .all(|b| b == b' ')
                && caret
                    == node.envelope.start
                        + text[node.envelope.clone()]
                            .trim_end_matches(['\r', '\n'])
                            .len())
            .then_some(caret - count..caret)
        })
    }
    pub(crate) fn reveal_heading_input(&mut self) {
        if self.selection().is_empty()
            && let Some(range) = self.heading_hash_run(self.state.head)
        {
            self.state.heading_reveal = Session::new(&self.state.source, range).ok();
        }
    }
}
