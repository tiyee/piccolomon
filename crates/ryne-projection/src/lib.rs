//! Platform-independent display model and source mapping.
use editable_markdown_parser::Affinity;
use ryne_syntax::{code_block::CodeBlock, list, table};
use std::{ops::Range, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;
mod code_row_index;
mod indexed;
pub use indexed::{IndexedDisplay, MappingIndex};
mod resources;
mod validation;
pub use code_row_index::CodeRowIndex;
pub use resources::*;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub source: bool,
    pub heading: u8,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub code_block: bool,
    pub syntax: Option<Class>,
    pub link: bool,
    pub marker: bool,
    pub math: bool,
    pub diagram: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub source: Range<usize>,
    pub display: Range<usize>,
    pub style: Style,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Display {
    pub text: String,
    pub spans: Vec<Span>,
    pub code_blocks: Vec<CodeView>,
    pub lists: Vec<ListView>,
    pub quotes: Vec<QuoteView>,
    pub tables: Vec<TableView>,
    pub formulas: Vec<MathView>,
    pub diagrams: Vec<DiagramView>,
    pub flow: Vec<FlowBlock>,
    pub source_len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowKind {
    Paragraph,
    Heading(u8),
    Item(usize),
    Panel,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlowBlock {
    pub range: Range<usize>,
    pub kind: FlowKind,
    pub raw: bool,
}

impl FlowKind {
    pub fn gap(self, next: Self) -> f32 {
        if let (Self::Item(a), Self::Item(b)) = (self, next)
            && a == b
        {
            return 4.;
        }
        let after: f32 = match self {
            Self::Heading(_) => 12.,
            Self::Panel => 16.,
            _ => 14.,
        };
        let before = match next {
            Self::Heading(1 | 2) => 32.,
            Self::Heading(_) => 24.,
            Self::Panel => 16.,
            _ => 14.,
        };
        after.max(before)
    }
}

/// Tracks decorative gaps without adding characters to the display projection.
#[derive(Default)]
pub struct FlowSpacing {
    previous: Option<usize>,
    separator: f32,
}

impl FlowSpacing {
    pub fn enter(&mut self, display: &Display, offset: usize) -> f32 {
        let Some(next) = display.flow_at(offset) else {
            return 0.;
        };
        self.enter_resolved(display, next)
    }
    pub fn enter_indexed(&mut self, display: &IndexedDisplay, offset: usize) -> f32 {
        display
            .flow_at(offset)
            .map_or(0., |next| self.enter_resolved(display, next))
    }
    fn enter_resolved(&mut self, display: &Display, next: usize) -> f32 {
        if self.previous == Some(next) {
            return 0.;
        }
        let gap = self.previous.map_or(0., |previous| {
            let a = &display.flow[previous];
            let b = &display.flow[next];
            let target = if a.raw || b.raw {
                14.
            } else {
                a.kind.gap(b.kind)
            };
            (target - self.separator).max(0.)
        });
        self.previous = Some(next);
        self.separator = 0.;
        gap
    }

    pub fn empty_height_indexed(&mut self, display: &IndexedDisplay, offset: usize) -> f32 {
        if display.flow_at(offset).is_none()
            && self.previous.is_some()
            && display.has_flow_after(offset)
        {
            let height = if self.separator == 0. { 14. } else { 30. };
            self.separator += height;
            height
        } else {
            30.
        }
    }
    pub fn empty_height(&mut self, display: &Display, offset: usize) -> f32 {
        if display.flow_at(offset).is_none()
            && self.previous.is_some()
            && display
                .flow
                .iter()
                .any(|b| b.range.start > offset && !b.raw)
        {
            let height = if self.separator == 0. { 14. } else { 30. };
            self.separator += height;
            height
        } else {
            30.
        }
    }
}

#[derive(Clone, Debug)]
pub struct MathView {
    pub source: Range<usize>,
    pub display: Range<usize>,
    pub formula: Arc<Formula>,
}

#[derive(Clone, Debug)]
pub struct DiagramView {
    pub source: CodeBlock,
    pub display: Range<usize>,
    pub diagram: Arc<Diagram>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CodeView {
    pub source: CodeBlock,
    pub rows: Vec<Range<usize>>,
    pub diagnostic: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListView {
    pub source: list::Row,
    pub start: usize,
}

/// A quote decorates the same editable source projection, including its children.
#[derive(Clone, Debug, PartialEq)]
pub struct QuoteView {
    pub source: Range<usize>,
    pub display: Range<usize>,
    pub depth: usize,
    pub inset: usize,
    /// An empty last physical row has no visible byte, but still belongs to the quote.
    pub empty_tail: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableView {
    pub source: table::Table,
    pub rows: Vec<Vec<Range<usize>>>,
    pub range: Range<usize>,
}

impl PartialEq for MathView {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
            && self.display == other.display
            && Arc::ptr_eq(&self.formula, &other.formula)
    }
}
impl PartialEq for DiagramView {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
            && self.display == other.display
            && Arc::ptr_eq(&self.diagram, &other.diagram)
    }
}

impl Display {
    pub fn quote_at(&self, offset: usize) -> Option<&QuoteView> {
        self.quotes
            .iter()
            .filter(|quote| {
                quote.display.contains(&offset)
                    || (quote.empty_tail.is_some() && offset == quote.display.end)
            })
            .max_by_key(|quote| quote.depth)
    }
    pub fn source_byte_at(
        &self,
        position: ryne_core::DisplayByte,
        affinity: Affinity,
    ) -> ryne_core::SourceByte {
        ryne_core::SourceByte(self.source_at(position.0, affinity))
    }
    pub fn display_byte_at(&self, source: ryne_core::SourceByte) -> ryne_core::DisplayByte {
        ryne_core::DisplayByte(self.position_at(source.0))
    }
    pub fn flow_at(&self, offset: usize) -> Option<usize> {
        self.flow
            .iter()
            .enumerate()
            .filter(|(_, b)| b.range.contains(&offset))
            .min_by_key(|(_, b)| b.range.len())
            .map(|(id, _)| id)
    }
    pub fn list_row(&self, start: usize) -> Option<&ListView> {
        self.lists.iter().find(|row| row.start == start)
    }
    pub fn code_row(&self, start: usize) -> Option<usize> {
        self.code_blocks
            .iter()
            .position(|block| block.rows.iter().any(|row| row.start == start))
    }
    /// Build once per layout, then query code ownership without scanning block rows.
    pub fn code_row_index(&self) -> CodeRowIndex<'_> {
        CodeRowIndex::new(self)
    }
    pub fn source_at(&self, position: usize, affinity: Affinity) -> usize {
        if let Some(source) = self
            .quotes
            .iter()
            .filter(|quote| quote.display.end == position)
            .find_map(|quote| quote.empty_tail)
        {
            return source;
        }
        for table in &self.tables {
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
        self.span_source_at(position, affinity)
    }
    fn span_source_at(&self, position: usize, affinity: Affinity) -> usize {
        let span = if affinity == Affinity::Upstream {
            self.spans
                .get(
                    self.spans
                        .partition_point(|s| s.display.start < position)
                        .wrapping_sub(1),
                )
                .filter(|s| position <= s.display.end)
        } else {
            self.spans
                .get(self.spans.partition_point(|s| s.display.end <= position))
                .filter(|s| s.display.start <= position)
        };
        span.map_or(if position == 0 { 0 } else { self.source_len }, |s| {
            if s.source.len() == s.display.len() && !s.style.math && !s.style.diagram {
                s.source.start + position.saturating_sub(s.display.start).min(s.source.len())
            } else if position <= s.display.start {
                s.source.start
            } else {
                s.source.end
            }
        })
    }

    pub fn position_at(&self, source: usize) -> usize {
        for table in &self.tables {
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
        self.span_position_at(source)
    }
    fn span_position_at(&self, source: usize) -> usize {
        let i = self
            .spans
            .partition_point(|s| s.source.start < source && s.source.end <= source);
        self.spans.get(i).map_or(self.text.len(), |span| {
            if source <= span.source.start {
                span.display.start
            } else if span.source.len() == span.display.len()
                && !span.style.math
                && !span.style.diagram
            {
                span.display.start + source - span.source.start
            } else {
                span.display.start
            }
        })
    }

    /// Resolve one visible grapheme without consuming hidden markers at either edge.
    pub fn deletion_range(&self, caret: usize, forward: bool) -> Option<Range<usize>> {
        let position = self.position_at(caret);
        let adjacent = grapheme_edge(&self.text, position, forward, true)?;
        let start = self.source_at(position.min(adjacent), Affinity::Downstream);
        let end = self.source_at(position.max(adjacent), Affinity::Upstream);
        (start < end).then_some(start..end)
    }
    pub fn snap(&self, offset: usize) -> usize {
        let left = grapheme_edge(&self.text, offset, false, false).unwrap_or(0);
        let right = grapheme_edge(&self.text, offset, true, false).unwrap_or(self.text.len());
        if right.abs_diff(offset) < left.abs_diff(offset) {
            right
        } else {
            left
        }
    }
}

/// Complete text is available to GraphemeCursor; any requested Unicode context is supplied.
fn grapheme_edge(text: &str, position: usize, forward: bool, strict: bool) -> Option<usize> {
    if position > text.len() {
        return (!forward).then_some(text.len());
    }
    let mut at = position;
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    let mut cursor = unicode_segmentation::GraphemeCursor::new(at, text.len(), true);
    let result = loop {
        match cursor.is_boundary(text, 0) {
            Err(unicode_segmentation::GraphemeIncomplete::PreContext(end)) => {
                cursor.provide_context(&text[..end], 0)
            }
            value => break value,
        }
    };
    if matches!(result, Ok(true))
        && ((!strict && (!forward || at == position)) || (!forward && at < position))
    {
        return Some(at);
    }
    loop {
        let result = if forward {
            cursor.next_boundary(text, 0)
        } else {
            cursor.prev_boundary(text, 0)
        };
        match result {
            Ok(value) => return value,
            Err(unicode_segmentation::GraphemeIncomplete::PreContext(end)) => {
                cursor.provide_context(&text[..end], 0)
            }
            Err(_) => break,
        }
    }
    let mut boundaries = text
        .grapheme_indices(true)
        .map(|(p, _)| p)
        .chain(std::iter::once(text.len()));
    if forward {
        boundaries.find(|&p| if strict { p > position } else { p >= position })
    } else {
        boundaries.rfind(|&p| if strict { p < position } else { p <= position })
    }
}
