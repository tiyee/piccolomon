use super::*;
impl Planner<'_> {
    pub fn normalize_heading_caret(&mut self) {
        if self.state.source_mode {
            return;
        }
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
            && let Some(block) = {
                let blocks = code_block::cached(&self.state.parse);
                blocks
                    .get(blocks.partition_point(|b| b.range.end <= self.state.head))
                    .filter(|b| b.range.contains(&self.state.head))
            }
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
            if let Some(row) = {
                let rows = list::cached_rows(&self.state.parse);
                rows.get(rows.partition_point(|r| r.line.end <= self.state.head))
                    .filter(|r| r.line.contains(&self.state.head))
            } {
                while let Some(prefix) = row.hidden.iter().find(|r| r.contains(&self.state.head)) {
                    self.state.head = prefix.end;
                }
                self.state.anchor = self.state.head;
            }
            if self
                .state
                .parse
                .index()
                .prefixes(
                    &self.state.parse,
                    self.state.head..self.state.head.saturating_add(1),
                )
                .any(|token| {
                    token.kind == ryne_parser::TokenKind::ContainerPrefix
                        && self.state.parse.nodes[token.owner].kind == NodeKind::BlockQuote
                        && token.range.contains(&self.state.head)
                })
            {
                let display = self.display();
                if display
                    .quotes
                    .iter()
                    .any(|quote| quote.source.contains(&self.state.head))
                {
                    let position = display
                        .source_at(display.position_at(self.state.head), Affinity::Downstream);
                    self.state.head = position;
                    self.state.anchor = position;
                }
            }
        }
    }
    pub fn reveal_clicked_heading(&mut self) {
        if self.state.source_mode || self.in_block_source(self.state.head) {
            return;
        }
        self.state.heading_reveal = self
            .state
            .parse
            .index()
            .headings
            .at(self.state.head)
            .into_iter()
            .map(|id| &self.state.parse.nodes[id])
            .find_map(|node| {
                (matches!(node.kind, NodeKind::Heading(_))
                    && node.envelope.contains(&self.state.head))
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
        let parse = self.state.parse.semantic();
        parse
            .index()
            .headings
            .at(caret)
            .into_iter()
            .map(|id| &parse.nodes[id])
            .find_map(|node| {
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
