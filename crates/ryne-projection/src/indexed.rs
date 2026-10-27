//! Immutable published projections own their indexes; mutable builder values never cache them.
use crate::*;
use std::{collections::BTreeMap, ops::Deref};

#[derive(Debug, Default)]
pub struct MappingIndex {
    source_anchors: BTreeMap<usize, usize>,
    empty_cells: BTreeMap<usize, Range<usize>>,
    quote_tails: BTreeMap<usize, usize>,
}
impl MappingIndex {
    pub fn new(display: &Display) -> Self {
        let mut index = Self::default();
        for table in &display.tables {
            for (r, row) in table.source.rows.iter().enumerate() {
                for (c, cell) in row.cells.iter().enumerate() {
                    let mapped = &table.rows[r][c];
                    index
                        .source_anchors
                        .entry(cell.content.start)
                        .or_insert(mapped.start);
                    index
                        .source_anchors
                        .entry(cell.content.end)
                        .or_insert(mapped.end);
                    if mapped.is_empty() {
                        index
                            .empty_cells
                            .entry(mapped.start)
                            .or_insert_with(|| cell.content.clone());
                    }
                }
            }
        }
        for quote in &display.quotes {
            if let Some(tail) = quote.empty_tail {
                index.quote_tails.entry(quote.display.end).or_insert(tail);
            }
        }
        index
    }
    pub fn position_at(&self, display: &Display, source: usize) -> usize {
        self.source_anchors
            .get(&source)
            .copied()
            .unwrap_or_else(|| display.span_position_at(source))
    }
    pub fn source_at(&self, display: &Display, position: usize, affinity: Affinity) -> usize {
        if let Some(tail) = self.quote_tails.get(&position) {
            return *tail;
        }
        if let Some(cell) = self.empty_cells.get(&position) {
            return if affinity == Affinity::Downstream {
                cell.start
            } else {
                cell.end
            };
        }
        display.span_source_at(position, affinity)
    }
}

#[derive(Debug)]
struct GraphemeIndex {
    bits: Vec<u64>,
    occupied: Vec<usize>,
    len: usize,
}
impl GraphemeIndex {
    fn new(text: &str) -> Self {
        let mut bits = vec![0; (text.len() + 1).div_ceil(64)];
        for at in text
            .grapheme_indices(true)
            .map(|(p, _)| p)
            .chain(std::iter::once(text.len()))
        {
            bits[at / 64] |= 1 << (at % 64);
        }
        let occupied = bits
            .iter()
            .enumerate()
            .filter(|(_, v)| **v != 0)
            .map(|(i, _)| i)
            .collect();
        Self {
            bits,
            occupied,
            len: text.len(),
        }
    }
    fn before(&self, at: usize) -> Option<usize> {
        let at = at.min(self.len);
        let word = at / 64;
        let bit = at % 64;
        let value = self.bits[word] & (u64::MAX >> (63 - bit));
        if value != 0 {
            return Some(word * 64 + 63 - value.leading_zeros() as usize);
        }
        let prior = self
            .occupied
            .partition_point(|&i| i < word)
            .checked_sub(1)?;
        let word = self.occupied[prior];
        Some(word * 64 + 63 - self.bits[word].leading_zeros() as usize)
    }
    fn after(&self, at: usize) -> Option<usize> {
        if at > self.len {
            return None;
        }
        let word = at / 64;
        let bit = at % 64;
        let value = self.bits[word] & (u64::MAX << bit);
        if value != 0 {
            return Some(word * 64 + value.trailing_zeros() as usize);
        }
        let word = *self
            .occupied
            .get(self.occupied.partition_point(|&i| i <= word))?;
        Some(word * 64 + self.bits[word].trailing_zeros() as usize)
    }
    fn snap(&self, at: usize) -> usize {
        let left = self.before(at).unwrap_or(0);
        self.after(at)
            .filter(|right| right.abs_diff(at) < left.abs_diff(at))
            .unwrap_or(left)
    }
}

#[derive(Debug)]
pub struct IndexedDisplay {
    inner: Display,
    mapping: MappingIndex,
    graphemes: GraphemeIndex,
    flow_index: ryne_syntax::RangeIndex,
    quote_index: ryne_syntax::RangeIndex,
    list_starts: BTreeMap<usize, usize>,
    raw_after: Vec<usize>,
}
impl Deref for IndexedDisplay {
    type Target = Display;
    fn deref(&self) -> &Display {
        &self.inner
    }
}
impl PartialEq for IndexedDisplay {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
impl PartialEq<Display> for IndexedDisplay {
    fn eq(&self, other: &Display) -> bool {
        self.inner == *other
    }
}
impl IndexedDisplay {
    pub fn new(inner: Display) -> Self {
        let mapping = MappingIndex::new(&inner);
        let graphemes = GraphemeIndex::new(&inner.text);
        let flow = ryne_syntax::RangeIndex::new(
            inner
                .flow
                .iter()
                .enumerate()
                .map(|(i, b)| (b.range.clone(), i))
                .collect(),
        );
        let quotes = ryne_syntax::RangeIndex::new(
            inner
                .quotes
                .iter()
                .enumerate()
                .map(|(i, b)| (b.display.clone(), i))
                .collect(),
        );
        let mut list_starts = BTreeMap::new();
        for (i, row) in inner.lists.iter().enumerate() {
            list_starts.entry(row.start).or_insert(i);
        }
        let mut raw_after: Vec<_> = inner
            .flow
            .iter()
            .filter(|b| !b.raw)
            .map(|b| b.range.start)
            .collect();
        raw_after.sort_unstable();
        Self {
            inner,
            mapping,
            graphemes,
            flow_index: flow,
            quote_index: quotes,
            list_starts,
            raw_after,
        }
    }
    pub fn position_at(&self, source: usize) -> usize {
        self.mapping.position_at(self, source)
    }
    pub fn source_at(&self, position: usize, affinity: Affinity) -> usize {
        self.mapping.source_at(self, position, affinity)
    }
    pub fn source_byte_at(&self, p: ryne_core::DisplayByte, a: Affinity) -> ryne_core::SourceByte {
        ryne_core::SourceByte(self.source_at(p.0, a))
    }
    pub fn display_byte_at(&self, p: ryne_core::SourceByte) -> ryne_core::DisplayByte {
        ryne_core::DisplayByte(self.position_at(p.0))
    }
    pub fn snap(&self, offset: usize) -> usize {
        self.graphemes.snap(offset)
    }
    /// Strict adjacent boundaries in the complete published display, including
    /// positions inside a multi-byte character or a multi-scalar grapheme.
    pub fn next_grapheme(&self, offset: usize) -> Option<usize> {
        self.graphemes.after(offset.checked_add(1)?)
    }
    pub fn previous_grapheme(&self, offset: usize) -> Option<usize> {
        self.graphemes.before(offset.checked_sub(1)?)
    }
    pub fn deletion_range(&self, caret: usize, forward: bool) -> Option<Range<usize>> {
        let position = self.position_at(caret);
        let adjacent = if forward {
            self.next_grapheme(position)
        } else {
            self.previous_grapheme(position)
        }?;
        let start = self.source_at(position.min(adjacent), Affinity::Downstream);
        let end = self.source_at(position.max(adjacent), Affinity::Upstream);
        (start < end).then_some(start..end)
    }
    pub fn flow_at(&self, offset: usize) -> Option<usize> {
        self.flow_index
            .at(offset)
            .into_iter()
            .filter(|&i| self.inner.flow[i].range.contains(&offset))
            .min_by_key(|&i| self.inner.flow[i].range.len())
    }
    pub fn quote_at(&self, offset: usize) -> Option<&QuoteView> {
        self.quote_index
            .at(offset)
            .into_iter()
            .map(|i| &self.inner.quotes[i])
            .filter(|q| {
                q.display.contains(&offset) || (q.empty_tail.is_some() && offset == q.display.end)
            })
            .max_by_key(|q| q.depth)
    }
    pub fn list_row(&self, start: usize) -> Option<&ListView> {
        self.list_starts.get(&start).map(|&i| &self.inner.lists[i])
    }
    pub fn has_flow_after(&self, offset: usize) -> bool {
        self.raw_after.partition_point(|&s| s <= offset) < self.raw_after.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adjacent_boundaries_match_complete_unicode_segmentation_at_every_byte() {
        for text in [
            String::new(),
            "中😀e\u{301}👩‍👩‍👦🇨🇳🇫🇷\r\nend".into(),
            format!("e{}z", "\u{301}".repeat(20000)),
        ] {
            let display = IndexedDisplay::new(Display::plain_source(&text));
            let boundaries = display
                .text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .chain(std::iter::once(display.text.len()))
                .collect::<Vec<_>>();
            for at in 0..=display.text.len() + 1 {
                assert_eq!(
                    display.next_grapheme(at),
                    boundaries.iter().copied().find(|&p| p > at)
                );
                assert_eq!(
                    display.previous_grapheme(at),
                    boundaries.iter().copied().take_while(|&p| p < at).last()
                );
            }
            assert_eq!(display.next_grapheme(usize::MAX), None);
        }
    }
    fn slow_source(display: &Display, position: usize, affinity: Affinity) -> usize {
        if let Some(source) = display
            .quotes
            .iter()
            .filter(|quote| quote.display.end == position)
            .find_map(|quote| quote.empty_tail)
        {
            return source;
        }
        for table in &display.tables {
            for (r, row) in table.rows.iter().enumerate() {
                for (c, cell) in row.iter().enumerate() {
                    if cell.is_empty()
                        && ((affinity == Affinity::Downstream && position == cell.start)
                            || (affinity == Affinity::Upstream && position == cell.end))
                    {
                        let source = &table.source.rows[r].cells[c].content;
                        return if affinity == Affinity::Downstream {
                            source.start
                        } else {
                            source.end
                        };
                    }
                }
            }
        }
        let span = if affinity == Affinity::Upstream {
            display
                .spans
                .iter()
                .rev()
                .find(|s| s.display.start < position && position <= s.display.end)
        } else {
            display
                .spans
                .iter()
                .find(|s| s.display.start <= position && position < s.display.end)
        };
        span.map_or(if position == 0 { 0 } else { display.source_len }, |s| {
            if s.source.len() == s.display.len() && !s.style.math && !s.style.diagram {
                s.source.start + position.saturating_sub(s.display.start).min(s.source.len())
            } else if position <= s.display.start {
                s.source.start
            } else {
                s.source.end
            }
        })
    }

    fn slow_position(display: &Display, source: usize) -> usize {
        for table in &display.tables {
            for (r, row) in table.source.rows.iter().enumerate() {
                for (c, cell) in row.cells.iter().enumerate() {
                    if source == cell.content.start {
                        return table.rows[r][c].start;
                    }
                    if source == cell.content.end {
                        return table.rows[r][c].end;
                    }
                }
            }
        }
        for span in &display.spans {
            if source <= span.source.start {
                return span.display.start;
            }
            if source < span.source.end {
                return if span.source.len() == span.display.len()
                    && !span.style.math
                    && !span.style.diagram
                {
                    span.display.start + source - span.source.start
                } else {
                    span.display.start
                };
            }
        }
        display.text.len()
    }

    #[test]
    fn indexed_mapping_and_graphemes_match_full_scans() {
        for text in [
            "",
            "a中😀e\u{301}👩‍👩‍👧‍👦🇨🇳\r\nb",
            "e\u{301}\u{301}\u{301}",
            "e\u{1acf}👩‍💻\r\n中",
        ] {
            let plain = Display::plain_source(text);
            let indexed = IndexedDisplay::new(plain.clone());
            for at in 0..=plain.text.len() + 2 {
                let expected = plain
                    .text
                    .grapheme_indices(true)
                    .map(|(p, _)| p)
                    .chain(std::iter::once(plain.text.len()))
                    .min_by_key(|p| p.abs_diff(at))
                    .unwrap_or(0);
                assert_eq!(indexed.snap(at), expected);
                assert_eq!(plain.snap(at), expected);
                for affinity in [Affinity::Upstream, Affinity::Downstream] {
                    assert_eq!(
                        indexed.source_at(at, affinity),
                        slow_source(&plain, at, affinity)
                    );
                }
            }
            for at in 0..=text.len() + 2 {
                for forward in [false, true] {
                    assert_eq!(
                        plain.deletion_range(at, forward),
                        indexed.deletion_range(at, forward)
                    );
                }
                assert_eq!(indexed.position_at(at), slow_position(&plain, at));
            }
        }
        let text = format!("e{}👩‍👩‍👧‍👦", "\u{301}".repeat(8192));
        let indexed = IndexedDisplay::new(Display::plain_source(&text));
        for at in [
            0,
            1,
            63,
            64,
            65,
            4096,
            8192,
            text.len() - 1,
            text.len(),
            usize::MAX,
        ] {
            let expected = text
                .grapheme_indices(true)
                .map(|(p, _)| p)
                .chain(std::iter::once(text.len()))
                .min_by_key(|p| p.abs_diff(at))
                .unwrap();
            assert_eq!(indexed.snap(at), expected);
        }
    }
    #[test]
    fn special_anchor_precedence_and_overlapping_flow_are_preserved() {
        let mut display = Display::plain_source("a😀b");
        display.tables.push(TableView {
            source: table::Table {
                range: 0..6,
                scope: 0..6,
                rows: vec![table::Row {
                    line: 0..6,
                    cells: vec![table::Cell { content: 1..1 }, table::Cell { content: 1..5 }],
                }],
                align: vec![],
                continuation: String::new(),
                inset: 0,
            },
            rows: vec![vec![2..2, 2..5]],
            range: 0..6,
        });
        display.quotes.push(QuoteView {
            source: 0..6,
            display: 0..2,
            depth: 1,
            inset: 32,
            empty_tail: Some(6),
        });
        display.flow = vec![
            FlowBlock {
                range: 0..6,
                kind: FlowKind::Paragraph,
                raw: false,
            },
            FlowBlock {
                range: 1..5,
                kind: FlowKind::Paragraph,
                raw: false,
            },
            FlowBlock {
                range: 1..5,
                kind: FlowKind::Panel,
                raw: true,
            },
        ];
        let indexed = IndexedDisplay::new(display.clone());
        for at in 0..=7 {
            assert_eq!(indexed.position_at(at), slow_position(&display, at));
            assert_eq!(indexed.flow_at(at), display.flow_at(at));
            for affinity in [Affinity::Upstream, Affinity::Downstream] {
                assert_eq!(
                    indexed.source_at(at, affinity),
                    slow_source(&display, at, affinity)
                );
            }
        }
    }
}
