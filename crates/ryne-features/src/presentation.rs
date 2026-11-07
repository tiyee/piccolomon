mod contribution;
#[cfg(test)]
mod legacy;
mod style_plan;
use contribution::BlockContribution;
use style_plan::{Attr, StylePlan};
mod code;
#[path = "presentation/list.rs"]
mod list_component;
mod quote;
#[path = "presentation/table.rs"]
mod table_component;
use crate::math;
use ryne_extensions::SyntaxHighlighter;
#[cfg(test)]
use ryne_parser::{Affinity, Source};
use ryne_parser::{NodeKind, TokenKind};
use ryne_projection::*;
#[cfg(test)]
use ryne_services::mermaid;
use ryne_syntax::ParseSnapshot;
use ryne_syntax::{code_block, list, table};
use std::ops::Range;

fn inline(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Strong
            | NodeKind::Emphasis
            | NodeKind::Strikethrough
            | NodeKind::Code(_)
            | NodeKind::Link { .. }
    )
}

fn editable_inline(parse: &ParseSnapshot, id: usize) -> bool {
    if !inline(&parse.nodes[id].kind) {
        return false;
    }
    let mut parent = parse.nodes[id].parent;
    while let Some(p) = parent {
        if matches!(
            parse.nodes[p].kind,
            NodeKind::Image { .. } | NodeKind::CodeBlock(_) | NodeKind::HtmlBlock
        ) {
            return false;
        }
        if matches!(parse.nodes[p].kind, NodeKind::Table(_))
            && ryne_syntax::SemanticQuery::new(parse)
                .table_containing(parse.nodes[id].envelope.start)
                .is_none()
        {
            return false;
        }
        parent = parse.nodes[p].parent;
    }
    inline(&parse.nodes[id].kind)
}

fn inline_range(parse: &ParseSnapshot, id: usize) -> Range<usize> {
    let node = &parse.nodes[id];
    if matches!(node.kind, NodeKind::Code(_))
        && let Some(cell) =
            ryne_syntax::SemanticQuery::new(parse).cell_containing(node.envelope.start)
    {
        let text = parse.source().text();
        let width = text[node.envelope.start..cell.content.end]
            .bytes()
            .take_while(|b| *b == b'`')
            .count();
        if width > 0 {
            let mut position = node.envelope.start + width;
            while position < cell.content.end {
                let ticks = text[position..cell.content.end]
                    .bytes()
                    .take_while(|b| *b == b'`')
                    .count();
                if ticks == width {
                    return node.envelope.start..position + width;
                }
                position += if ticks > 0 {
                    ticks
                } else {
                    text[position..].chars().next().map_or(1, char::len_utf8)
                };
            }
        }
    }
    node.envelope.clone()
}

/// Reveal all inline ancestors, stopping before block containers.
pub fn active_window(parse: &ParseSnapshot, caret: usize) -> Option<Range<usize>> {
    let parse = parse.semantic();
    let candidates = parse.index().inline_nodes.at(caret);
    if candidates.is_empty() {
        return None;
    }
    let all_formulas = ryne_syntax::math::cached(parse);
    let first = all_formulas.partition_point(|f| f.range.end <= caret);
    let formulas = &all_formulas[first..all_formulas.partition_point(|f| f.range.start < caret)];
    candidates
        .into_iter()
        .map(|id| (id, &parse.nodes[id]))
        .filter(|(id, _)| {
            let range = inline_range(parse, *id);
            editable_inline(parse, *id)
                && range.start < caret
                && caret < range.end
                && !formulas
                    .iter()
                    .any(|f| f.range.start <= range.start && range.end <= f.range.end)
        })
        .map(|(id, _)| inline_range(parse, id))
        .chain(
            formulas
                .iter()
                .filter(|f| f.range.start < caret && caret < f.range.end)
                .map(|f| f.range.clone()),
        )
        .reduce(|a, b| a.start.min(b.start)..a.end.max(b.end))
}

/// Heading content comes from the authoritative AST, including nested inline
/// envelopes. The remaining heading envelope contains its structural syntax.
fn heading_content(parse: &ParseSnapshot, id: usize) -> Range<usize> {
    let node = &parse.nodes[id];
    let end = node.envelope.start
        + parse.source().text()[node.envelope.clone()]
            .trim_end_matches(['\r', '\n'])
            .len();
    match (node.children.first(), node.children.last()) {
        (Some(&first), Some(&last)) => {
            parse.nodes[first].envelope.start..parse.nodes[last].envelope.end
        }
        _ => end..end,
    }
}

pub fn heading_caret(parse: &ParseSnapshot, caret: usize) -> usize {
    for id in parse.index().headings.at(caret) {
        let node = &parse.nodes[id];
        if matches!(node.kind, NodeKind::Heading(_))
            && node.envelope.start <= caret
            && caret <= node.envelope.end
        {
            let content = heading_content(parse, id);
            let end = node.envelope.start
                + parse.source().text()[node.envelope.clone()]
                    .trim_end_matches(['\r', '\n'])
                    .len();
            if caret < content.start {
                return content.start;
            }
            if content.end < caret && caret <= end {
                return content.end;
            }
        }
    }
    caret
}

/// Headings and code have structural presentation; inline syntax follows the
/// caret. Unsupported syntax remains editable source.
#[cfg(test)]
pub fn build(parse: &ParseSnapshot, window: Option<&Range<usize>>, source_mode: bool) -> Display {
    build_with_heading(parse, window, source_mode, None, None)
}

pub fn build_with_heading_using(
    parse: &ParseSnapshot,
    window: Option<&Range<usize>>,
    source_mode: bool,
    heading_window: Option<&Range<usize>>,
    block_window: Option<&Range<usize>>,
    renderer: &dyn ryne_extensions::FormulaRenderer,
) -> Display {
    build_with_highlights_using(
        parse,
        window,
        source_mode,
        heading_window,
        block_window,
        None,
        renderer,
    )
}

pub fn build_with_highlights_using(
    parse: &ParseSnapshot,
    window: Option<&Range<usize>>,
    source_mode: bool,
    heading_window: Option<&Range<usize>>,
    block_window: Option<&Range<usize>>,
    highlighter: Option<&mut dyn SyntaxHighlighter>,
    renderer: &dyn ryne_extensions::FormulaRenderer,
) -> Display {
    PresentationBuilder { renderer }.build(
        PresentationContext {
            parse,
            window,
            source_mode,
            heading_window,
            block_window,
        },
        highlighter,
        &[],
    )
}

pub struct PresentationContext<'a> {
    pub parse: &'a ParseSnapshot,
    pub window: Option<&'a Range<usize>>,
    pub source_mode: bool,
    pub heading_window: Option<&'a Range<usize>>,
    pub block_window: Option<&'a Range<usize>>,
}
pub struct PresentationBuilder<'a> {
    pub renderer: &'a dyn ryne_extensions::FormulaRenderer,
}
impl PresentationBuilder<'_> {
    pub fn build(
        &self,
        context: PresentationContext<'_>,
        highlighter: Option<&mut dyn SyntaxHighlighter>,
        diagrams: &[DiagramViewInput],
    ) -> Display {
        let PresentationContext {
            parse,
            window,
            source_mode,
            heading_window,
            block_window,
        } = context;
        // The backend misses blocks following a fence with bare CR. Use a same-byte-width
        // semantic view for the entire projection, retaining dialect and optional math.
        // Document bytes and every source offset stay unchanged.
        let normalized = ryne_syntax::parser::normalized(parse);
        let parse = normalized.unwrap_or(parse);
        let text = parse.source().text();
        let mut plan = StylePlan::default();
        let quotes = quote::regions(parse, source_mode, block_window);
        let code_blocks = ryne_syntax::SemanticQuery::new(parse).code_blocks();
        let formulas = math::formulas(parse);
        let code::CodePresentation {
            structured,
            diagrams: mut folded_diagrams,
            patch,
        } = code::contribute(
            text,
            code_blocks,
            source_mode,
            block_window,
            highlighter,
            diagrams,
        );
        if !patch.apply_plan(text, &mut plan) {
            return Display::plain_source(text);
        }
        let all_tables = ryne_syntax::SemanticQuery::new(parse).tables();
        let tables = table_component::select(all_tables, source_mode, block_window);
        let list_rows = list_component::select(parse, &tables, source_mode, block_window);
        if !list_component::prefixes(&list_rows).apply_plan(text, &mut plan)
            || !table_component::contribute(text, &tables).apply_plan(text, &mut plan)
        {
            return Display::plain_source(text);
        }
        if !source_mode {
            for (id, node) in parse.nodes.iter().enumerate() {
                if code_blocks
                    .get(code_blocks.partition_point(|b| b.range.end <= node.envelope.start))
                    .is_some_and(|b| b.range.contains(&node.envelope.start))
                {
                    continue;
                }
                if let NodeKind::Heading(level) = node.kind {
                    let content = heading_content(parse, id);
                    let revealed = heading_window.into_iter().chain(block_window).any(|range| {
                        range.start < node.envelope.end && node.envelope.start < range.end
                    });
                    plan.set(node.envelope.clone(), Attr::Heading, level);
                    plan.set(
                        node.envelope.start..content.start,
                        Attr::Hidden,
                        (!revealed) as u8,
                    );
                    let trimmed_end = node.envelope.start
                        + text[node.envelope.clone()]
                            .trim_end_matches(['\r', '\n'])
                            .len();
                    // Keep the final source newline, but fold ATX closing hashes or
                    // the Setext underline together with its preceding newline.
                    plan.set(content.end..trimmed_end, Attr::Hidden, (!revealed) as u8);
                    for range in [node.envelope.start..content.start, content.end..trimmed_end] {
                        plan.set(range, Attr::Marker, 1);
                    }
                }
            }
        }
        for (id, node) in parse.nodes.iter().enumerate() {
            if !editable_inline(parse, id)
                || code_blocks
                    .get(code_blocks.partition_point(|b| b.range.end <= node.envelope.start))
                    .is_some_and(|b| b.range.contains(&node.envelope.start))
            {
                continue;
            }
            let envelope = inline_range(parse, id);
            if formulas
                .get(formulas.partition_point(|f| f.range.end < envelope.end))
                .is_some_and(|f| f.range.start <= envelope.start && envelope.end <= f.range.end)
            {
                continue;
            }
            let active = source_mode
                || block_window.is_some_and(|w| w.start < envelope.end && envelope.start < w.end)
                || window.is_some_and(|w| w.start < envelope.end && envelope.start < w.end);
            let attr = match node.kind {
                NodeKind::Strong => Some(Attr::Bold),
                NodeKind::Emphasis => Some(Attr::Italic),
                NodeKind::Strikethrough => Some(Attr::Strike),
                NodeKind::Code(_) => Some(Attr::Code),
                NodeKind::Link { .. } => Some(Attr::Link),
                _ => None,
            };
            if let Some(attr) = attr {
                plan.set(envelope.clone(), attr, 1);
            }
            let markers = if matches!(node.kind, NodeKind::Code(_)) && envelope != node.envelope {
                let width = text[envelope.clone()]
                    .bytes()
                    .take_while(|b| *b == b'`')
                    .count();
                vec![
                    envelope.start..envelope.start + width,
                    envelope.end - width..envelope.end,
                ]
            } else {
                parse.syntax.nodes[id]
                    .owned_tokens
                    .iter()
                    .filter_map(|&token_id| {
                        let token = &parse.syntax.tokens[token_id];
                        matches!(token.kind, TokenKind::OpenMarker | TokenKind::CloseMarker)
                            .then(|| token.range.clone())
                    })
                    .collect()
            };
            for range in markers {
                plan.set(range.clone(), Attr::Hidden, (!active) as u8);
                plan.set(range, Attr::Marker, 1);
            }
        }
        // Inline/heading reveal never exposes the enclosing list's structural prefix.
        if !list_component::prefixes(&list_rows).apply_plan(text, &mut plan) {
            return Display::plain_source(text);
        }
        if !quote::prefixes(parse, &quotes).apply_plan(text, &mut plan) {
            return Display::plain_source(text);
        }
        let mut folded_failures = Vec::new();
        if !source_mode {
            for image in ryne_syntax::image::blocks(parse) {
                if block_window
                    .is_some_and(|w| w.start < image.range.end && image.range.start < w.end)
                {
                    continue;
                }
                if let Some((_, result)) = diagrams.iter().find(|(range, _)| *range == image.range)
                {
                    plan.set(image.range.clone(), Attr::Hidden, 1);
                    match result {
                        Ok(resource) => {
                            folded_diagrams.push((crate::images::source(image), resource.clone()))
                        }
                        Err(_) => folded_failures.push((
                            crate::images::source(image),
                            crate::images::failure_label(&image.destination),
                        )),
                    }
                }
            }
        }
        let initial_styles = plan.resolve(text.len());
        let (patch, mut insertions) =
            table_component::substitutions(text, &tables, &initial_styles);
        if !patch.apply_plan(text, &mut plan) {
            return Display::plain_source(text);
        }
        let mut rendered = Vec::new();
        for formula in &formulas {
            let range = &formula.range;
            // LaTeX is literal even when Markdown sees underscores or backticks.
            plan.set(range.clone(), Attr::Hidden, 0);
            let style = initial_styles.at(range.start).0;
            plan.style(
                range.clone(),
                Style {
                    code: true,
                    marker: false,
                    ..style
                },
            );
            plan.set(range.start..range.start + 1, Attr::Marker, 1);
            plan.set(range.end - 1..range.end, Attr::Marker, 1);
            // LaTeX remains literal in both rendered and failed/source-edit views.
            insertions.retain(|(at, _, value)| *value == "\t" || !range.contains(at));
            let active = source_mode
                || window
                    .into_iter()
                    .chain(block_window)
                    .any(|w| w.start < range.end && range.start < w.end);
            if !active
                && let Some(view) = self.renderer.render(&text[range.start + 1..range.end - 1])
            {
                plan.set(range.clone(), Attr::Hidden, 1);
                rendered.push((range.clone(), view));
            }
        }
        for range in source_mode
            .then_some(0..text.len())
            .into_iter()
            .chain(block_window.cloned())
        {
            for attr in [
                Attr::Source,
                Attr::Heading,
                Attr::Bold,
                Attr::Italic,
                Attr::Strike,
                Attr::Link,
                Attr::Marker,
            ] {
                plan.set(range.clone(), attr, matches!(attr, Attr::Source) as u8);
            }
        }
        let resolved = plan.resolve(text.len());
        let mut styles = resolved.cursor();
        let mut display = Display {
            tables: tables
                .into_iter()
                .map(|table| TableView {
                    rows: table
                        .rows
                        .iter()
                        .map(|row| vec![0..0; row.cells.len()])
                        .collect(),
                    source: table,
                    range: 0..0,
                })
                .collect(),
            source_len: text.len(),
            ..Display::default()
        };
        // Resolve source boundaries once. Consuming ordered events preserves
        // separator-before-anchor ordering, including empty cells, without
        // scanning every cell and substitution for every source character.
        let mut table_bounds = Vec::new();
        let mut cell_bounds = Vec::new();
        for (t, table) in display.tables.iter().enumerate() {
            table_bounds.push((table.source.range.start, t, false));
            table_bounds.push((table.source.range.end, t, true));
            for (r, row) in table.source.rows.iter().enumerate() {
                for (c, cell) in row.cells.iter().enumerate() {
                    cell_bounds.push((cell.content.start, t, r, c, false));
                    cell_bounds.push((cell.content.end, t, r, c, true));
                }
            }
        }
        table_bounds.sort_unstable();
        cell_bounds.sort_unstable();
        let mut table_bounds = table_bounds.into_iter().peekable();
        let mut cell_bounds = cell_bounds.into_iter().peekable();
        insertions.sort_by_key(|(at, _, _)| *at);
        let mut insertion_cursor = 0;
        folded_diagrams.sort_by_key(|(b, _)| b.range.start);
        let mut folded_diagrams = folded_diagrams.iter().peekable();
        let mut folded_failures = folded_failures.iter().peekable();
        rendered.sort_by_key(|(range, _)| range.start);
        let mut rendered = rendered.iter().peekable();
        let empty_code_anchors = structured
            .iter()
            .filter(|b| b.rows.is_empty())
            .map(|b| b.anchor)
            .collect::<std::collections::BTreeSet<_>>();
        for (start, ch) in text
            .char_indices()
            .chain(std::iter::once((text.len(), '\0')))
        {
            let (style, hidden) = styles.at(start);
            while let Some((_, t, end)) = table_bounds.next_if(|e| e.0 == start) {
                if end {
                    display.tables[t].range.end = display.text.len();
                } else {
                    display.tables[t].range.start = display.text.len();
                }
            }
            let first_insertion = insertion_cursor;
            while insertions
                .get(insertion_cursor)
                .is_some_and(|e| e.0 == start)
            {
                insertion_cursor += 1;
            }
            let at_insertions = &insertions[first_insertion..insertion_cursor];
            // Cell separators precede the next cell's anchor; literal substitutions follow it.
            for (_, source, value) in at_insertions.iter().filter(|e| e.2 == "\t") {
                let from = display.text.len();
                display.text.push_str(value);
                display.spans.push(Span {
                    source: source.clone(),
                    display: from..display.text.len(),
                    style: Style::default(),
                });
            }
            while let Some((_, t, r, c, end)) = cell_bounds.next_if(|e| e.0 == start) {
                if end {
                    display.tables[t].rows[r][c].end = display.text.len();
                } else {
                    display.tables[t].rows[r][c].start = display.text.len();
                }
            }
            if let Some((block, diagram)) =
                folded_diagrams.next_if(|(block, _)| block.range.start == start)
            {
                let from = display.text.len();
                display.text.push_str(math::OBJECT);
                let range = from..display.text.len();
                display.spans.push(Span {
                    source: block.range.clone(),
                    display: range.clone(),
                    style: Style {
                        diagram: true,
                        ..Style::default()
                    },
                });
                display.diagrams.push(DiagramView {
                    source: block.clone(),
                    display: range,
                    diagram: diagram.clone(),
                });
                if text[block.range.clone()].ends_with(['\r', '\n']) {
                    let from = display.text.len();
                    display.text.push('\n');
                    display.spans.push(Span {
                        source: block.range.end..block.range.end,
                        display: from..from + 1,
                        style: Style::default(),
                    });
                }
            }
            if let Some((block, label)) =
                folded_failures.next_if(|(block, _)| block.range.start == start)
            {
                let from = display.text.len();
                display.text.push_str(label);
                let range = from..display.text.len();
                display.spans.push(Span {
                    source: block.range.clone(),
                    display: range.clone(),
                    style: Style {
                        image_failure: true,
                        strike: true,
                        ..Style::default()
                    },
                });
                display.image_failures.push(ImageFailureView {
                    source: block.clone(),
                    display: range,
                });
                if text[block.range.clone()].ends_with(['\r', '\n']) {
                    let from = display.text.len();
                    display.text.push('\n');
                    display.spans.push(Span {
                        source: block.range.end..block.range.end,
                        display: from..from + 1,
                        style: Style::default(),
                    });
                }
            }
            if let Some((source, formula)) = rendered.next_if(|(range, _)| range.start == start) {
                let from = display.text.len();
                display.text.push_str(math::OBJECT);
                let range = from..display.text.len();
                display.spans.push(Span {
                    source: source.clone(),
                    display: range.clone(),
                    style: Style {
                        code: false,
                        marker: false,
                        math: true,
                        ..style
                    },
                });
                display.formulas.push(MathView {
                    source: source.clone(),
                    display: range,
                    formula: formula.clone(),
                });
            }
            for (_, source, value) in at_insertions.iter().filter(|e| e.2 != "\t") {
                let from = display.text.len();
                display.text.push_str(value);
                display.spans.push(Span {
                    source: source.clone(),
                    display: from..display.text.len(),
                    style,
                });
            }
            if empty_code_anchors.contains(&start) {
                let from = display.text.len();
                display.text.push('\n');
                display.spans.push(Span {
                    source: start..start,
                    display: from..from + 1,
                    style: Style {
                        code_block: true,
                        ..Style::default()
                    },
                });
            }
            if start == text.len() {
                break;
            }
            let end = start + ch.len_utf8();
            if hidden {
                continue;
            }
            let from = display.text.len();
            // GPUI shapes LF lines; preserve original CR/CRLF in the source map.
            if ch == '\r' {
                if text.as_bytes().get(end) == Some(&b'\n') {
                    continue;
                }
                display.text.push('\n');
            } else {
                display.text.push(ch);
            }
            if let Some(last) = display.spans.last_mut().filter(|s| {
                s.source.end == start
                    && s.style == style
                    && ch != '\r'
                    && s.source.len() == s.display.len()
                    && !s.style.math
                    && !s.style.diagram
                    && !s.style.image_failure
            }) {
                last.source.end = end;
                last.display.end = display.text.len();
            } else {
                let source_start = if ch == '\n' && start > 0 && text.as_bytes()[start - 1] == b'\r'
                {
                    start - 1
                } else {
                    start
                };
                display.spans.push(Span {
                    source: source_start..end,
                    display: from..display.text.len(),
                    style,
                });
            }
        }
        let mapping = ryne_projection::MappingIndex::new(&display);
        display.code_blocks = structured
            .into_iter()
            .map(|block| {
                let mut rows = block
                    .rows
                    .iter()
                    .map(|row| {
                        mapping.position_at(&display, row.content.start)
                            ..mapping.position_at(&display, row.content.end)
                    })
                    .collect::<Vec<_>>();
                if rows.is_empty() {
                    let start = mapping.position_at(&display, block.anchor);
                    rows.push(start..start);
                } else if block.closing.is_none()
                    && block.range.end == text.len()
                    && text.ends_with(['\r', '\n'])
                {
                    let end = display.text.len();
                    rows.push(end..end);
                }
                let diagnostic = diagrams
                    .iter()
                    .find(|(range, _)| *range == block.range)
                    .and_then(|(_, result)| result.as_ref().err())
                    .cloned();
                CodeView {
                    source: block,
                    rows,
                    diagnostic,
                }
            })
            .collect();
        display.lists = list_rows
            .into_iter()
            .map(|row| {
                // Include any still-visible quote prefix on this physical line.
                let start = mapping.position_at(&display, row.line.start);
                ListView { source: row, start }
            })
            .collect();
        display.quotes = quotes
            .into_iter()
            .filter_map(|(_, source, depth, inset)| {
                let range = mapping.position_at(&display, source.start)
                    ..mapping.position_at(&display, source.end);
                let empty_tail = ryne_syntax::quote::at(parse, source.end)
                    .filter(|row| {
                        row.content.end == source.end && text[row.content.clone()].trim().is_empty()
                    })
                    .map(|row| row.content.end);
                (!range.is_empty()).then_some(QuoteView {
                    source,
                    display: range,
                    depth,
                    inset,
                    empty_tail,
                })
            })
            .collect();
        if !source_mode {
            for (id, node) in parse.nodes.iter().enumerate() {
                let kind = match node.kind {
                    NodeKind::Heading(level) => FlowKind::Heading(level),
                    NodeKind::CodeBlock(_) | NodeKind::Table(_) => FlowKind::Panel,
                    NodeKind::Paragraph => {
                        let item = node
                            .parent
                            .filter(|&parent| parse.nodes[parent].kind == NodeKind::Item);
                        if let Some(item) =
                            item.filter(|&parent| parse.nodes[parent].children.first() == Some(&id))
                        {
                            FlowKind::Item(parse.nodes[item].parent.unwrap_or(item))
                        } else {
                            FlowKind::Paragraph
                        }
                    }
                    NodeKind::HtmlBlock | NodeKind::Rule | NodeKind::SourceBlock => {
                        FlowKind::Paragraph
                    }
                    _ => continue,
                };
                let range = mapping.position_at(&display, node.envelope.start)
                    ..mapping.position_at(&display, node.envelope.end);
                if !range.is_empty() {
                    display.flow.push(FlowBlock {
                        range,
                        kind,
                        raw: block_window.is_some_and(|w| {
                            w.start < node.envelope.end && node.envelope.start < w.end
                        }),
                    });
                }
            }
            if !display.lists.is_empty() {
                // Resolve tight-list item extents once, preserving source order and innermost flow semantics.
                let coverage = ryne_syntax::RangeIndex::new(
                    display
                        .flow
                        .iter()
                        .enumerate()
                        .map(|(i, b)| (b.range.clone(), i))
                        .collect(),
                );
                let mut item_ends = std::collections::HashMap::<usize, usize>::new();
                for row in &display.lists {
                    item_ends
                        .entry(row.source.item.id)
                        .and_modify(|end| *end = (*end).max(row.source.line.end))
                        .or_insert(row.source.line.end);
                }
                let mut tight_end = 0;
                for row in &display.lists {
                    if !row.source.first
                        || row.start < tight_end
                        || coverage
                            .at(row.start)
                            .into_iter()
                            .any(|i| display.flow[i].range.contains(&row.start))
                    {
                        continue;
                    }
                    let end = mapping.position_at(&display, item_ends[&row.source.item.id]);
                    tight_end = end;
                    display.flow.push(FlowBlock {
                        range: row.start..end,
                        kind: FlowKind::Item(row.source.item.list),
                        raw: false,
                    });
                }
            }
        }
        display
    }
}
#[cfg(test)]
mod typography_tests {
    use super::*;

    #[test]
    fn semantic_spacing_keeps_separators_and_code_literal_for_all_newlines() {
        for newline in ["\n", "\r\n", "\r"] {
            let raw = "paragraph\n\n## 标题\n\n\n- one\n- two\n\n```rust\n# literal\n\n**literal**\n```\n".replace('\n', newline);
            let parse = ParseSnapshot::new(Source::new(&raw));
            let display = build(&parse, None, false);
            let mut spacing = FlowSpacing::default();
            assert_eq!(spacing.enter(&display, 0), 0.);
            let separator = display.text.find("\n\n").unwrap() + 1;
            assert_eq!(spacing.empty_height(&display, separator), 14.);
            let heading = display.text.find("标题").unwrap();
            assert_eq!(spacing.enter(&display, heading), 18.);
            let separator = heading + "标题\n".len();
            assert_eq!(spacing.empty_height(&display, separator), 14.);
            assert_eq!(spacing.empty_height(&display, separator + 1), 30.);
            let one = display.text.find("one").unwrap();
            assert_eq!(spacing.enter(&display, one), 0.);
            let two = display.text.find("two").unwrap();
            assert_eq!(
                spacing.enter(&display, two),
                4.,
                "newline={newline:?}, text={:?}, flow={:?}",
                display.text,
                display.flow
            );
            assert!(display.text.contains("# literal\n\n**literal**"));
            let literal = display.text.find("**literal**").unwrap();
            assert_eq!(
                display.flow[display.flow_at(literal).unwrap()].kind,
                FlowKind::Panel
            );
            assert_eq!(parse.source().text(), raw);
            let source = build(&parse, None, true);
            assert!(source.flow.is_empty());
            assert!(source.spans.iter().all(|span| span.style.source));
        }
    }

    #[test]
    fn local_source_and_nested_paragraphs_keep_semantic_boundaries() {
        let raw = "# 标题\n\n- first\n\n  second **bold**\n\n  > quote\n\nend";
        let parse = ParseSnapshot::new(Source::new(raw));
        let display = build(&parse, None, false);
        let first = display.text.find("first").unwrap();
        let second = display.text.find("second").unwrap();
        assert!(matches!(
            display.flow[display.flow_at(first).unwrap()].kind,
            FlowKind::Item(_)
        ));
        assert_eq!(
            display.flow[display.flow_at(second).unwrap()].kind,
            FlowKind::Paragraph
        );
        let scope = ryne_syntax::block::at(&parse, raw.find("second").unwrap())
            .unwrap()
            .scope;
        let local = build_with_heading(&parse, None, false, None, Some(&scope));
        let second = local.text.find("second").unwrap();
        assert!(local.flow[local.flow_at(second).unwrap()].raw);
        assert!(local
            .spans
            .iter()
            .filter(|span| scope.start <= span.source.start && span.source.start < scope.end)
            .all(|span| span.style.source && !span.style.bold));
        let title = local.text.find("标题").unwrap();
        assert!(!local.flow[local.flow_at(title).unwrap()].raw);
        assert_eq!(
            local.source_at(
                local.position_at(raw.find("second").unwrap()),
                Affinity::Downstream
            ),
            raw.find("second").unwrap()
        );
        assert_eq!(parse.source().text(), raw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ryne_parser::Source;

    #[test]
    fn nested_inline_reveal_keeps_quote_presentation_and_source_unchanged() {
        let parse = ParseSnapshot::new(Source::new("# **a *b* c**\n\n> [**x**](/url)\n"));
        let original = parse.source().text().to_string();
        assert_eq!(build(&parse, None, false).text, "a b c\n\nx\n");
        let window = active_window(&parse, original.find('b').unwrap()).unwrap();
        assert_eq!(
            build(&parse, Some(&window), false).text,
            "**a *b* c**\n\nx\n"
        );
        let window = active_window(&parse, original.find('x').unwrap()).unwrap();
        assert_eq!(
            build(&parse, Some(&window), false).text,
            "a b c\n\n[**x**](/url)\n"
        );
        assert_eq!(parse.source().text(), original);
    }

    #[test]
    fn quote_decoration_preserves_nested_mapping_literal_code_and_source_modes() {
        for newline in ["\n", "\r\n", "\r"] {
            let raw = "> **中😀**\n> continuation\n>\n> > nested\n>\n> - item\n>\n> ```text\n> > **literal**\n> ```\n\noutside > literal\n".replace('\n', newline);
            let parse = ParseSnapshot::new(Source::new(&raw));
            let display = build(&parse, None, false);
            display.validate(&raw).unwrap();
            assert!(
                display.text.starts_with("中😀\ncontinuation\n\nnested\n"),
                "{:?}",
                display.text
            );
            assert!(display.text.contains("> **literal**"));
            assert!(display.text.contains("outside > literal"));
            assert_eq!(display.quotes.len(), 2);
            assert_eq!(
                display
                    .quote_at(display.text.find("nested").unwrap())
                    .unwrap()
                    .depth,
                2
            );
            assert!(display
                .quote_at(display.text.find("outside").unwrap())
                .is_none());
            for needle in [
                "中",
                "😀",
                "continuation",
                "nested",
                "item",
                "> **literal**",
                "outside",
            ] {
                let source = raw.find(needle).unwrap();
                let position = display.position_at(source);
                assert_eq!(
                    display.source_at(position, Affinity::Downstream),
                    source,
                    "{needle}"
                );
            }
            let caret = raw.find("😀").unwrap();
            assert_eq!(display.deletion_range(caret, true), Some(caret..caret + 4));
            let window = active_window(&parse, caret).unwrap();
            let active = build(&parse, Some(&window), false);
            assert!(active.text.starts_with("**中😀**"));
            assert_eq!(active.quotes.len(), 2);
            let scope = ryne_syntax::block::at(&parse, caret).unwrap().scope;
            let local = build_with_heading(&parse, None, false, None, Some(&scope));
            assert!(local.quotes.is_empty());
            assert!(local.text.starts_with("> **中😀**\n> continuation"));
            let source = build(&parse, None, true);
            assert!(source.quotes.is_empty());
            assert_eq!(source.text, raw.replace("\r\n", "\n").replace('\r', "\n"));
            assert_eq!(parse.source().text(), raw);
        }
    }

    #[test]
    fn quotes_in_lists_and_lazy_continuations_use_the_container_range() {
        for raw in [
            "> first\nlazy continuation\n\noutside",
            "- item\n\n  > first\n  > second\n\noutside",
        ] {
            let parse = ParseSnapshot::new(Source::new(raw));
            let display = build(&parse, None, false);
            display.validate(raw).unwrap();
            let quote = display
                .quote_at(display.text.find("first").unwrap())
                .unwrap();
            assert_eq!(quote.inset, if raw.starts_with('-') { 64 } else { 32 });
            assert!(display
                .quote_at(display.text.find("outside").unwrap())
                .is_none());
            if let Some(lazy) = display.text.find("lazy") {
                assert_eq!(display.quote_at(lazy), Some(quote));
            }
        }
    }

    #[test]
    fn empty_quote_drafts_remain_visible_and_editable() {
        for raw in [">", "> ", ">\n", "> >"] {
            let parse = ParseSnapshot::new(Source::new(raw));
            let display = build(&parse, None, false);
            display.validate(raw).unwrap();
            assert!(!display.text.trim().is_empty(), "{raw:?}");
        }
    }

    #[test]
    fn headings_fold_structural_syntax_and_keep_levels_and_source_maps() {
        let raw = "# 一级 #\r\n\n## **二级**\n\n标题\n===\n\n副标题\n---\n\n####### plain\n";
        let parse = ParseSnapshot::new(Source::new(raw));
        let display = build(&parse, None, false);
        assert_eq!(
            display.text,
            "一级\n\n二级\n\n标题\n\n副标题\n\n####### plain\n"
        );
        assert_eq!(display.source_at(0, Affinity::Downstream), 2);
        for (word, level) in [("一级", 1), ("二级", 2), ("标题", 1), ("副标题", 2)] {
            let start = display.text.find(word).unwrap();
            assert_eq!(
                display
                    .spans
                    .iter()
                    .find(|s| s.display.contains(&start))
                    .unwrap()
                    .style
                    .heading,
                level
            );
        }
        let caret = raw.find("二级").unwrap();
        let window = active_window(&parse, caret).unwrap();
        let revealed = build(&parse, Some(&window), false);
        assert!(revealed.text.contains("**二级**"));
        assert!(!revealed.text.contains("## **二级**"));
        assert_eq!(build(&parse, None, true).text, raw.replace("\r\n", "\n"));
        assert_eq!(heading_caret(&parse, 0), 2);
        assert_eq!(parse.source().text(), raw);
        for level in 1..=6 {
            let raw = format!("{} title\n", "#".repeat(level));
            let parse = ParseSnapshot::new(Source::new(&raw));
            let display = build(&parse, None, false);
            assert_eq!(display.text, "title\n");
            assert_eq!(display.spans[0].style.heading, level as u8);
        }
    }

    #[test]
    fn hidden_markers_map_content_and_crlf_without_splitting_unicode() {
        let parse = ParseSnapshot::new(Source::new("a **中😀**\r\nb"));
        let display = build(&parse, None, false);
        assert_eq!(display.text, "a 中😀\nb");
        assert_eq!(display.source_at(2, Affinity::Downstream), 4);
        assert_eq!(display.position_at(4), 2);
        let newline = display.text.find('\n').unwrap();
        assert_eq!(display.source_at(newline, Affinity::Downstream), 13);
        assert_eq!(display.source_at(newline + 1, Affinity::Upstream), 15);
    }

    #[test]
    fn code_images_tables_and_incomplete_inline_remain_editable() {
        let raw = "```\n**literal**\n```\n\n![**alt**](image.png)\n\n| **a** | b |\n| --- | --- |\n| c | *d* |\n\n**unfinished\n";
        let parse = ParseSnapshot::new(Source::new(raw));
        assert_eq!(
            build(&parse, None, false).text,
            "**literal**\n\n![**alt**](image.png)\n\na\tb\nc\td\n\n**unfinished\n"
        );
        assert_eq!(build(&parse, None, true).text, raw);
    }

    #[test]
    fn code_blocks_fold_fences_keep_literal_text_and_map_unicode_and_crlf() {
        for newline in ["\n", "\r\n", "\r"] {
            for fence in ["```", "~~~~"] {
                let raw = format!(
                    "before{newline}{newline}{fence}rust{newline}  **中😀**{newline}{newline}{fence}{newline}{newline}after"
                );
                let parse = ParseSnapshot::new(Source::new(&raw));
                let display = build(&parse, None, false);
                assert_eq!(display.text, "before\n\n  **中😀**\n\n\nafter");
                assert_eq!(display.code_blocks.len(), 1);
                assert_eq!(display.code_blocks[0].source.language, "rust");
                let source = raw.find("中😀").unwrap();
                let position = display.text.find("中😀").unwrap();
                assert_eq!(display.source_at(position, Affinity::Downstream), source);
                assert_eq!(display.position_at(source), position);
                let style = display
                    .spans
                    .iter()
                    .find(|s| s.display.contains(&position))
                    .unwrap()
                    .style;
                assert!(style.code_block);
                assert!(!style.bold);
                assert_eq!(
                    build(&parse, None, true).text,
                    raw.replace("\r\n", "\n").replace('\r', "\n")
                );
                assert_eq!(parse.source().text(), raw);
            }
        }
    }

    #[test]
    fn code_indentation_and_nested_containers_preserve_actual_code_spaces() {
        for (raw, expected) in [
            ("    ```\n    literal\n    ```\n", "```\nliteral\n```\n"),
            ("    a\n        b\n", "a\n    b\n"),
            ("  ```rs\n  a\n    b\n  ```\n", "a\n  b\n"),
            ("  ```rs\n  \n  ```\n", "\n"),
            ("```\n    ```\n", "    ```\n"),
            ("> ```rs\n> a\n>   b\n> ```\n", "a\n  b\n"),
            ("- ```rs\n  a\n    b\n  ```\n", "a\n  b\n"),
            ("- item\n\n      a\n        b\n", "item\n\na\n  b\n"),
        ] {
            let parse = ParseSnapshot::new(Source::new(raw));
            let display = build(&parse, None, false);
            assert_eq!(display.text, expected, "{raw:?}");
            assert_eq!(build(&parse, None, true).text, raw);
        }
    }

    #[test]
    fn empty_code_has_an_editable_anchor_and_drafts_remain_visible() {
        for raw in ["```rust\n```", "> ```\n> ```\n", "~~~\n"] {
            let parse = ParseSnapshot::new(Source::new(raw));
            let display = build(&parse, None, false);
            assert_eq!(display.code_blocks.len(), 1);
            assert_eq!(display.text, "\n");
            assert_eq!(
                display.source_at(0, Affinity::Downstream),
                display.code_blocks[0].source.anchor
            );
        }
        for raw in ["```rust", "~~", "not ``` a fence"] {
            let parse = ParseSnapshot::new(Source::new(raw));
            assert_eq!(build(&parse, None, false).text, raw);
        }
    }
    #[test]
    fn lists_fold_markers_render_semantic_numbers_and_preserve_nested_mapping() {
        let raw = "3) **中😀**\r\n1) second\r\n   - nested\r\n     continuation\r\n\r\n+ other\r\n";
        let parse = ParseSnapshot::new(Source::new(raw));
        let display = build(&parse, None, false);
        assert_eq!(
            display.text,
            "中😀\nsecond\nnested\ncontinuation\n\nother\n"
        );
        let first = display
            .lists
            .iter()
            .filter(|v| v.source.first)
            .collect::<Vec<_>>();
        assert_eq!(
            first
                .iter()
                .map(|v| v.source.item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["3.", "4.", "•", "•"]
        );
        assert!(first[2].source.item.inset > first[1].source.item.inset);
        for body in ["中", "😀", "second", "nested", "continuation", "other"] {
            let source = raw.find(body).unwrap();
            let position = display.position_at(source);
            assert_eq!(display.source_at(position, Affinity::Downstream), source);
        }
        let caret = raw.find('😀').unwrap();
        let active = active_window(&parse, caret).unwrap();
        let revealed = build(&parse, Some(&active), false);
        assert!(revealed.text.starts_with("**中😀**"));
        assert!(!revealed.text.contains("3)"));
        assert_eq!(build(&parse, None, true).text, raw.replace("\r\n", "\n"));
        assert_eq!(parse.source().text(), raw);
    }
    #[test]
    fn empty_lists_unrecognized_markers_and_code_literals_are_not_lost() {
        for (raw, body, count) in [
            ("- ", "", 1),
            ("- \n", "\n", 1),
            ("9. ", "", 1),
            ("-not a list", "-not a list", 0),
            (
                "```\n- literal\n1. literal\n```\n",
                "- literal\n1. literal\n",
                0,
            ),
        ] {
            let parse = ParseSnapshot::new(Source::new(raw));
            let display = build(&parse, None, false);
            assert_eq!(display.text, body);
            assert_eq!(display.lists.len(), count);
        }
    }
    #[test]
    fn table_projection_hides_grid_syntax_styles_cells_and_maps_unicode_escapes_and_empty_cells() {
        let raw =
            "| **中😀** | `x\\|y` | |\r\n| :--- | :---: | ---: |\r\n| a\\|b<br>c | next | |\r\n";
        let parse = ParseSnapshot::new(Source::new(raw));
        let display = build(&parse, None, false);
        assert_eq!(display.text, "中😀\tx|y\t\na|b\nc\tnext\t\n");
        assert_eq!(display.tables.len(), 1);
        let table = &display.tables[0];
        assert_eq!(table.rows.len(), 2);
        for (r, row) in table.rows.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                let source = &table.source.rows[r].cells[c].content;
                assert_eq!(display.position_at(source.start), cell.start);
                assert_eq!(display.position_at(source.end), cell.end);
                if source.is_empty() {
                    assert_eq!(
                        display.source_at(cell.start, Affinity::Downstream),
                        source.start
                    );
                }
            }
        }
        for body in ["中", "😀", "x", "next"] {
            let source = raw.find(body).unwrap();
            assert_eq!(
                display.source_at(display.position_at(source), Affinity::Downstream),
                source
            );
        }
        let pipe = display.text.find('|').unwrap();
        let source = display.source_at(pipe, Affinity::Downstream);
        assert_eq!(
            &raw[source..display.source_at(pipe + 1, Affinity::Upstream)],
            "\\|"
        );
        let active = active_window(&parse, raw.find('y').unwrap()).unwrap();
        let editing = build(&parse, Some(&active), false);
        assert!(editing.text.contains("`x|y`"));
        assert!(!editing.text.contains(":---"));
        assert_eq!(parse.source().text(), raw);
    }
    #[test]
    fn table_raw_fallback_preserves_unaccounted_content_and_source_mode_preserves_markers() {
        for raw in [
            "| a | b |\n|---|---|\n| short |\n",
            "|a|b|\n|---|---|\n|1|2|ignored|\n",
        ] {
            let display = build(&ParseSnapshot::new(Source::new(raw)), None, false);
            assert!(display.tables.is_empty());
            assert_eq!(display.text, raw);
        }
        let raw = "A | B\n:---|---:\nx | y\n";
        let parse = ParseSnapshot::new(Source::new(raw));
        assert_eq!(build(&parse, None, false).text, "A\tB\nx\ty\n");
        assert_eq!(build(&parse, None, true).text, raw);
        assert_eq!(
            build_with_heading(&parse, None, false, None, Some(&(0..raw.len()))).text,
            raw
        );
    }
}

#[cfg(test)]
mod math_tests {
    use super::*;
    use ryne_parser::Source;

    #[test]
    fn short_math_is_atomic_even_when_source_and_object_have_equal_byte_length() {
        let raw = "中$x$😀\r\n";
        let parse = ParseSnapshot::with_math(Source::new(raw));
        let folded = build(&parse, None, false);
        assert_eq!(folded.text, "中\u{fffc}😀\n");
        let math = &folded.formulas[0];
        assert_eq!(math.source.len(), math.display.len());
        assert_eq!(
            folded.position_at(math.source.start + 1),
            math.display.start
        );
        assert_eq!(
            folded.source_at(math.display.start, Affinity::Downstream),
            math.source.start
        );
        assert_eq!(
            folded.source_at(math.display.end, Affinity::Upstream),
            math.source.end
        );
        assert_eq!(
            folded.source_at(math.display.end, Affinity::Downstream),
            math.source.end
        );
        assert_eq!(
            active_window(&parse, math.source.start + 1),
            Some(math.source.clone())
        );
        let open = build(
            &parse,
            active_window(&parse, math.source.start + 1).as_ref(),
            false,
        );
        assert!(open.formulas.is_empty());
        assert_eq!(open.text, "中$x$😀\n");
        assert_eq!(parse.source().text(), raw);
    }

    #[test]
    fn math_reveal_expands_inline_ancestors_and_keeps_latex_literal() {
        let raw = "**外 $x_1 + \\frac{a}{b} + `c`$ 文**";
        let parse = ParseSnapshot::with_math(Source::new(raw));
        let folded = build(&parse, None, false);
        assert_eq!(folded.formulas.len(), 1);
        assert_eq!(folded.text, "外 \u{fffc} 文");
        let active = active_window(&parse, raw.find("frac").unwrap()).unwrap();
        assert_eq!(active, 0..raw.len());
        let open = build(&parse, Some(&active), false);
        assert_eq!(open.text, raw);
        assert!(open
            .spans
            .iter()
            .filter(|s| s.source.contains(&raw.find("frac").unwrap()))
            .all(|s| s.style.code));
    }

    #[test]
    fn math_in_nested_list_and_table_keeps_anchors_and_source_entry() {
        let raw = "> - $x$ 中文\r\n\r\n| $a$ | $b$ |\r\n| --- | --- |\r\n| $c_1$ | 文 |\r\n";
        let parse = ParseSnapshot::with_math(Source::new(raw));
        let folded = build(&parse, None, false);
        assert_eq!(folded.formulas.len(), 4);
        for cell in folded.tables[0].rows.iter().flatten() {
            assert!(!cell.is_empty());
        }
        assert_eq!(
            &folded.text[folded.tables[0].rows[0][0].clone()],
            math::OBJECT
        );
        assert_eq!(
            &folded.text[folded.tables[0].rows[0][1].clone()],
            math::OBJECT
        );
        let scope = folded.tables[0].source.scope.clone();
        let local = build_with_heading(&parse, None, false, None, Some(&scope));
        assert_eq!(local.formulas.len(), 1);
        let source = build(&parse, None, true);
        assert!(source.formulas.is_empty());
        assert_eq!(source.text, raw.replace("\r\n", "\n"));
    }

    #[test]
    fn invalid_and_multiline_formulas_keep_raw_latex() {
        let raw = "$\\unknown{x_1}$ and $a\nb$ and $$c_2$$";
        let parse = ParseSnapshot::with_math(Source::new(raw));
        let display = build(&parse, None, false);
        assert!(display.formulas.is_empty());
        assert_eq!(display.text, raw);
    }

    #[test]
    fn table_formula_source_keeps_literal_break_tags() {
        let raw = "| A | B |\n| --- | --- |\n| $\\unknown{<br>}$ | $x< br >$ |\n";
        let parse = ParseSnapshot::with_math(Source::new(raw));
        let display = build(&parse, None, false);
        let cell = display.tables[0].rows[1][0].clone();
        assert_eq!(&display.text[cell], "$\\unknown{<br>}$");
        let caret = raw.find("x< br").unwrap();
        let window = active_window(&parse, caret).unwrap();
        let open = build(&parse, Some(&window), false);
        let cell = open.tables[0].rows[1][1].clone();
        assert_eq!(&open.text[cell], "$x< br >$");
    }
}

#[cfg(test)]
mod diagram_tests {
    use super::*;
    use ryne_parser::Source;

    #[test]
    fn diagrams_are_atomic_and_keep_adjacent_blocks_and_line_endings() {
        let diagram = mermaid::render("flowchart LR\nA-->B", mermaid::control()).expect("diagram");
        for eol in ["\n", "\r\n", "\r"] {
            let raw = [
                "before **bold**",
                "",
                "```mermaid",
                "flowchart LR",
                "A-->B",
                "```",
                "",
                "| X | Y |",
                "| --- | --- |",
                "| a | b |",
                "",
                "- item",
                "",
            ]
            .join(eol);
            let parse = ParseSnapshot::new(Source::new(&raw));
            let block = code_block::blocks(&parse).remove(0);
            let views = vec![(block.range.clone(), Ok(diagram.clone()))];
            let display = build_with_diagrams(&parse, None, false, None, None, None, &views);
            assert_eq!(display.diagrams.len(), 1);
            assert!(display.code_blocks.is_empty());
            assert_eq!(display.tables.len(), 1, "eol={eol:?}");
            assert!(display.lists.iter().any(|row| row.source.first));
            let view = &display.diagrams[0];
            assert_eq!(&display.text[view.display.clone()], math::OBJECT);
            assert_eq!(
                display.source_at(view.display.start, Affinity::Downstream),
                block.range.start
            );
            assert_eq!(
                display.source_at(view.display.end, Affinity::Upstream),
                block.range.end
            );
            for source in block.range.start..block.range.end {
                assert_eq!(display.position_at(source), view.display.start);
            }
            let cell = &display.tables[0].source.rows[1].cells[0];
            assert_eq!(
                display.source_at(
                    display.position_at(cell.content.start),
                    Affinity::Downstream
                ),
                cell.content.start
            );
            assert_eq!(parse.source().text(), raw);
            let source = build_with_diagrams(&parse, None, true, None, None, None, &views);
            assert!(source.diagrams.is_empty());
            assert_eq!(source.text, raw.replace("\r\n", "\n").replace('\r', "\n"));
            let local =
                build_with_diagrams(&parse, None, false, None, Some(&block.scope), None, &views);
            assert!(local.diagrams.is_empty());
            assert!(local.text.contains("```mermaid"));
        }
    }

    #[test]
    fn failed_render_keeps_literal_editable_code_and_diagnostic() {
        let raw = "```mermaid\nflowchart LR\nA[**literal** --> B\n```\n";
        let parse = ParseSnapshot::new(Source::new(raw));
        let block = code_block::blocks(&parse).remove(0);
        let views = vec![(block.range, Err("missing ]".into()))];
        let display = build_with_diagrams(&parse, None, false, None, None, None, &views);
        assert!(display.diagrams.is_empty());
        assert!(display.text.contains("A[**literal** --> B"));
        assert_eq!(
            display.code_blocks[0].diagnostic.as_deref(),
            Some("missing ]")
        );
        let source = raw.find("literal").expect("body");
        assert_eq!(
            display.source_at(display.position_at(source), Affinity::Downstream),
            source
        );
    }
}

#[cfg(test)]
fn build_with_heading(
    parse: &ParseSnapshot,
    window: Option<&Range<usize>>,
    source_mode: bool,
    heading: Option<&Range<usize>>,
    block: Option<&Range<usize>>,
) -> Display {
    build_with_heading_using(
        parse,
        window,
        source_mode,
        heading,
        block,
        &ryne_services::math::FormularyRenderer::default(),
    )
}
#[cfg(test)]
fn build_with_diagrams(
    parse: &ParseSnapshot,
    window: Option<&Range<usize>>,
    source_mode: bool,
    heading: Option<&Range<usize>>,
    block: Option<&Range<usize>>,
    highlighter: Option<&mut dyn SyntaxHighlighter>,
    diagrams: &[DiagramViewInput],
) -> Display {
    PresentationBuilder {
        renderer: &ryne_services::math::FormularyRenderer::default(),
    }
    .build(
        PresentationContext {
            parse,
            window,
            source_mode,
            heading_window: heading,
            block_window: block,
        },
        highlighter,
        diagrams,
    )
}

pub(crate) struct MarkdownPresentation;
impl ryne_extensions::PresentationProvider for MarkdownPresentation {
    fn cache_key(
        &self,
        state: &ryne_extensions::EditingState,
        services: &ryne_extensions::FeatureServices<'_>,
    ) -> Option<ryne_extensions::PresentationKey> {
        let mut key = ryne_extensions::PresentationKey::for_state(
            state,
            ryne_extensions::CacheDependencies {
                visibility: !state.source_mode,
                generation: services.formulas.presentation_generation()?,
                ..Default::default()
            },
        );
        key.block = (!state.source_mode)
            .then(|| {
                crate::commands::Planner {
                    state: state.clone(),
                    prepared: None,
                    renderer: services.formulas,
                    mapping: None,
                }
                .block_window()
            })
            .flatten();
        Some(key)
    }
    fn display(
        &self,
        state: &ryne_extensions::EditingState,
        services: &ryne_extensions::FeatureServices<'_>,
        highlighter: Option<&mut dyn SyntaxHighlighter>,
        diagrams: &[DiagramViewInput],
    ) -> Display {
        let block = crate::commands::Planner {
            state: state.clone(),
            prepared: None,
            renderer: services.formulas,
            mapping: None,
        }
        .block_window();
        PresentationBuilder {
            renderer: services.formulas,
        }
        .build(
            PresentationContext {
                parse: &state.parse,
                window: state.reveal.as_ref().map(|s| &s.range),
                source_mode: state.source_mode,
                heading_window: state.heading_reveal.as_ref().map(|s| &s.range),
                block_window: block.as_ref(),
            },
            highlighter,
            diagrams,
        )
    }
}
