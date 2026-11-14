//! Disposable, snapshot-owned indexes. Parser ownership and document identity stay unchanged.
use crate::{code_block, list, table};
use ryne_parser::{Dialect, NodeKind, Source, SyntaxToken};
use std::{
    ops::{Deref, Range},
    sync::OnceLock,
};

pub struct ParseSnapshot {
    inner: ryne_parser::ParseSnapshot,
    normalized: OnceLock<Option<Box<Self>>>,
    pub(crate) alerts: OnceLock<Vec<crate::alert::Alert>>,
    pub(crate) codes: OnceLock<Vec<code_block::CodeBlock>>,
    pub(crate) images: OnceLock<Vec<crate::image::ImageBlock>>,
    pub(crate) items: OnceLock<Vec<list::Item>>,
    pub(crate) rows: OnceLock<Vec<list::Row>>,
    pub(crate) tables: OnceLock<Vec<table::Table>>,
    pub(crate) table_cells: OnceLock<Vec<table::CellLocation>>,
    pub(crate) outline: OnceLock<Vec<crate::OutlineHeading>>,
    paragraphs: OnceLock<Vec<Range<usize>>>,
    index: OnceLock<SemanticIndex>,
    utf16: OnceLock<crate::Utf16Index>,
    pub(crate) formulas: OnceLock<Vec<crate::math::SourceFormula>>,
    block_directory: OnceLock<crate::block::BlockDirectory>,
}
impl Deref for ParseSnapshot {
    type Target = ryne_parser::ParseSnapshot;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl ParseSnapshot {
    pub fn new(source: Source) -> Self {
        Self::with_dialect_and_math(source, Dialect::Gfm029, false)
    }
    pub fn with_math(source: Source) -> Self {
        Self::with_dialect_and_math(source, Dialect::Gfm029, true)
    }
    pub fn with_dialect_and_math(source: Source, dialect: Dialect, math_enabled: bool) -> Self {
        Self {
            inner: ryne_parser::ParseSnapshot::with_dialect_and_math(source, dialect, math_enabled),
            normalized: OnceLock::new(),
            alerts: OnceLock::new(),
            codes: OnceLock::new(),
            images: OnceLock::new(),
            items: OnceLock::new(),
            rows: OnceLock::new(),
            tables: OnceLock::new(),
            table_cells: OnceLock::new(),
            outline: OnceLock::new(),
            paragraphs: OnceLock::new(),
            index: OnceLock::new(),
            utf16: OnceLock::new(),
            formulas: OnceLock::new(),
            block_directory: OnceLock::new(),
        }
    }
    pub fn normalized(&self) -> Option<&Self> {
        self.normalized
            .get_or_init(|| {
                let text = self.source().text();
                text.bytes()
                    .enumerate()
                    .any(|(i, b)| b == b'\r' && text.as_bytes().get(i + 1) != Some(&b'\n'))
                    .then(|| {
                        let text: String = text
                            .char_indices()
                            .map(|(i, c)| {
                                if c == '\r' && text.as_bytes().get(i + 1) != Some(&b'\n') {
                                    '\n'
                                } else {
                                    c
                                }
                            })
                            .collect();
                        Box::new(Self::with_dialect_and_math(
                            Source::new(&text),
                            self.dialect,
                            self.math_enabled,
                        ))
                    })
            })
            .as_deref()
    }
    pub fn semantic(&self) -> &Self {
        self.normalized().unwrap_or(self)
    }
    pub fn utf16(&self) -> &crate::Utf16Index {
        self.utf16
            .get_or_init(|| crate::Utf16Index::new(self.source().clone()))
    }
    pub fn index(&self) -> &SemanticIndex {
        self.index.get_or_init(|| SemanticIndex::new(self))
    }
    pub(crate) fn block_directory(&self) -> &crate::block::BlockDirectory {
        let parse = self.semantic();
        parse
            .block_directory
            .get_or_init(|| crate::block::BlockDirectory::new(parse))
    }
    pub fn paragraphs(&self) -> &[Range<usize>] {
        let parse = self.semantic();
        parse.paragraphs.get_or_init(|| {
            parse
                .nodes
                .iter()
                .filter(|node| {
                    node.kind == NodeKind::Paragraph
                        && node
                            .parent
                            .is_some_and(|p| parse.nodes[p].kind == NodeKind::Document)
                })
                .map(|node| {
                    node.envelope.start
                        ..node.envelope.start
                            + parse.source().text()[node.envelope.clone()]
                                .trim_end_matches(['\r', '\n'])
                                .len()
                })
                .collect()
        })
    }
}

pub struct SemanticIndex {
    pub lines: Vec<Range<usize>>,
    pub first_content: Option<usize>,
    prefixes: Vec<usize>,
    pub code_nodes: Vec<usize>,
    pub item_nodes: Vec<usize>,
    pub table_nodes: Vec<usize>,
    pub heading_nodes: Vec<usize>,
    pub quote_nodes: Vec<usize>,
    pub inline_nodes: RangeIndex,
    pub quotes: RangeIndex,
    pub codes: RangeIndex,
    pub headings: RangeIndex,
    pub blocks: RangeIndex,
}
impl SemanticIndex {
    fn new(parse: &ParseSnapshot) -> Self {
        #[cfg(any(test, feature = "query-metrics"))]
        crate::query_metrics::record(|m| m.semantic_indexes += 1);
        let mut index = Self {
            lines: code_block::lines(parse.source().text()),
            first_content: parse.source().text().find(|c: char| !c.is_whitespace()),
            prefixes: Vec::new(),
            code_nodes: Vec::new(),
            item_nodes: Vec::new(),
            table_nodes: Vec::new(),
            heading_nodes: Vec::new(),
            quote_nodes: Vec::new(),
            inline_nodes: RangeIndex::default(),
            quotes: RangeIndex::default(),
            codes: RangeIndex::default(),
            headings: RangeIndex::default(),
            blocks: RangeIndex::default(),
        };
        let mut inline = Vec::new();
        let mut blocks = Vec::new();
        for (id, node) in parse.nodes.iter().enumerate() {
            if matches!(
                node.kind,
                NodeKind::Paragraph
                    | NodeKind::List(_)
                    | NodeKind::Heading(_)
                    | NodeKind::Item
                    | NodeKind::BlockQuote
                    | NodeKind::CodeBlock(_)
                    | NodeKind::Table(_)
                    | NodeKind::HtmlBlock
                    | NodeKind::Rule
                    | NodeKind::DisplayMath(_)
                    | NodeKind::SourceBlock
            ) {
                blocks.push((node.envelope.clone(), id));
            }
            match node.kind {
                NodeKind::CodeBlock(_) => index.code_nodes.push(id),
                NodeKind::Item => index.item_nodes.push(id),
                NodeKind::Table(_) => index.table_nodes.push(id),
                NodeKind::Heading(_) => index.heading_nodes.push(id),
                NodeKind::BlockQuote => index.quote_nodes.push(id),
                NodeKind::Strong
                | NodeKind::Emphasis
                | NodeKind::Strikethrough
                | NodeKind::Code(_)
                | NodeKind::Link { .. }
                | NodeKind::InlineMath(_) => {
                    let mut range = node.envelope.clone();
                    if matches!(node.kind, NodeKind::Code(_)) {
                        let mut parent = node.parent;
                        while let Some(p) = parent {
                            if matches!(parse.nodes[p].kind, NodeKind::TableCell) {
                                range = parse.nodes[p].envelope.clone();
                                break;
                            }
                            parent = parse.nodes[p].parent;
                        }
                    }
                    inline.push((range, id));
                }
                _ => (),
            }
        }
        index.prefixes = parse
            .syntax
            .tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| t.kind == ryne_parser::TokenKind::ContainerPrefix)
            .map(|(i, _)| i)
            .collect();
        index
            .prefixes
            .sort_by_key(|&i| parse.syntax.tokens[i].range.start);
        index.inline_nodes = RangeIndex::new(inline);
        index.blocks = RangeIndex::new(blocks);
        index.quotes = RangeIndex::new(
            index
                .quote_nodes
                .iter()
                .map(|&i| (parse.nodes[i].envelope.clone(), i))
                .collect(),
        );
        index.codes = RangeIndex::new(
            index
                .code_nodes
                .iter()
                .map(|&i| (parse.nodes[i].envelope.clone(), i))
                .collect(),
        );
        index.headings = RangeIndex::new(
            index
                .heading_nodes
                .iter()
                .map(|&i| (parse.nodes[i].envelope.clone(), i))
                .collect(),
        );
        index
    }
    pub fn line(&self, position: usize) -> Option<&Range<usize>> {
        self.lines
            .get(self.lines.partition_point(|line| line.end <= position))
            .filter(|line| line.contains(&position))
    }
    pub fn prefixes<'a>(
        &'a self,
        parse: &'a ParseSnapshot,
        range: Range<usize>,
    ) -> impl Iterator<Item = &'a SyntaxToken> {
        let first = self
            .prefixes
            .partition_point(|&i| parse.syntax.tokens[i].range.end <= range.start);
        self.prefixes[first..]
            .iter()
            .map(|&i| &parse.syntax.tokens[i])
            .take_while(move |t| t.range.start < range.end)
    }
}

/// Augmented range directory; overlapping containers retain their original identities.
#[derive(Default, Debug)]
pub struct RangeIndex {
    ranges: Vec<(Range<usize>, usize)>,
    max_end: Vec<usize>,
}
impl RangeIndex {
    pub fn overlapping(&self, range: Range<usize>) -> Vec<usize> {
        let mut found = Vec::new();
        if !range.is_empty() {
            if self.max_end.is_empty() {
                let first = self.ranges.partition_point(|(r, _)| r.end <= range.start);
                found.extend(
                    self.ranges[first..]
                        .iter()
                        .take_while(|(r, _)| r.start < range.end)
                        .map(|(_, id)| *id),
                );
            } else {
                self.overlap(1, 0, self.ranges.len(), &range, &mut found);
            }
        }
        found.sort_unstable();
        found
    }
    fn overlap(
        &self,
        node: usize,
        left: usize,
        right: usize,
        range: &Range<usize>,
        found: &mut Vec<usize>,
    ) {
        if self.max_end[node] <= range.start || self.ranges[left].0.start >= range.end {
            return;
        }
        if right - left == 1 {
            found.push(self.ranges[left].1);
            return;
        }
        let mid = (left + right) / 2;
        self.overlap(node * 2, left, mid, range, found);
        self.overlap(node * 2 + 1, mid, right, range, found);
    }
    pub fn new(mut ranges: Vec<(Range<usize>, usize)>) -> Self {
        ranges.sort_by_key(|(r, id)| (r.start, *id));
        let disjoint = ranges.windows(2).all(|w| w[0].0.end <= w[1].0.start);
        let mut result = Self {
            max_end: if disjoint {
                Vec::new()
            } else {
                vec![0; ranges.len().saturating_mul(4)]
            },
            ranges,
        };
        if !result.max_end.is_empty() {
            result.build(1, 0, result.ranges.len());
        }
        result
    }
    fn build(&mut self, node: usize, left: usize, right: usize) -> usize {
        let end = if right - left == 1 {
            self.ranges[left].0.end
        } else {
            let mid = (left + right) / 2;
            self.build(node * 2, left, mid)
                .max(self.build(node * 2 + 1, mid, right))
        };
        self.max_end[node] = end;
        end
    }
    pub fn at(&self, position: usize) -> Vec<usize> {
        let mut found = Vec::new();
        if self.max_end.is_empty() {
            let first = self.ranges.partition_point(|(r, _)| r.end < position);
            found.extend(
                self.ranges[first..]
                    .iter()
                    .take_while(|(r, _)| r.start <= position)
                    .map(|(_, id)| *id),
            );
        } else {
            self.visit(1, 0, self.ranges.len(), position, &mut found);
        }
        found.sort_unstable();
        found
    }
    fn visit(
        &self,
        node: usize,
        left: usize,
        right: usize,
        position: usize,
        found: &mut Vec<usize>,
    ) {
        if self.max_end[node] < position || self.ranges[left].0.start > position {
            return;
        }
        if right - left == 1 {
            found.push(self.ranges[left].1);
            return;
        }
        let mid = (left + right) / 2;
        self.visit(node * 2, left, mid, position, found);
        self.visit(node * 2 + 1, mid, right, position, found);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalized_and_structure_indexes_are_owned_by_the_original_snapshot() {
        let raw = "> - one\r>   ```rust\r>   literal\r>   ```\r";
        let source = Source::new(raw);
        let id = source.id();
        let parse = ParseSnapshot::with_math(source);
        assert_eq!(parse.source().id(), id);
        assert_eq!(parse.source().text(), raw);
        let normalized = parse.normalized().unwrap();
        assert!(std::ptr::eq(normalized, parse.normalized().unwrap()));
        assert_ne!(normalized.source().id(), id);
        assert!(std::ptr::eq(
            crate::code_block::cached(&parse),
            crate::code_block::cached(&parse)
        ));
        let fresh = ParseSnapshot::with_math(Source::new(raw));
        assert!(!std::ptr::eq(parse.index(), fresh.index()));
    }
    #[test]
    fn range_queries_match_overlapping_and_boundary_scans() {
        for ranges in [
            vec![(0..20, 7), (3..8, 2), (3..8, 1), (12..12, 4), (15..18, 5)],
            vec![(0..3, 7), (3..8, 2), (12..12, 4), (15..18, 5)],
        ] {
            let index = RangeIndex::new(ranges.clone());
            for p in 0..=25 {
                let mut expected: Vec<_> = ranges
                    .iter()
                    .filter(|(r, _)| r.start <= p && p <= r.end)
                    .map(|(_, i)| *i)
                    .collect();
                expected.sort_unstable();
                assert_eq!(index.at(p), expected);
                for end in p..=25 {
                    let mut expected: Vec<_> = ranges
                        .iter()
                        .filter(|(r, _)| p < end && r.start < end && p < r.end)
                        .map(|(_, i)| *i)
                        .collect();
                    expected.sort_unstable();
                    assert_eq!(index.overlapping(p..end), expected);
                }
            }
        }
    }
}
