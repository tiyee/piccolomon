use crate::math;
#[cfg(test)]
use editable_markdown_parser::{Affinity, Source};
use editable_markdown_parser::{NodeKind, ParseSnapshot, TokenKind};
use ryne_extensions::SyntaxHighlighter;
use ryne_projection::*;
#[cfg(test)]
use ryne_services::mermaid;
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

fn editable_inline(parse: &ParseSnapshot, id: usize, tables: &[table::Table]) -> bool {
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
            && !tables
                .iter()
                .any(|t| t.range.contains(&parse.nodes[id].envelope.start))
        {
            return false;
        }
        parent = parse.nodes[p].parent;
    }
    inline(&parse.nodes[id].kind)
}

fn inline_range(parse: &ParseSnapshot, id: usize, tables: &[table::Table]) -> Range<usize> {
    let node = &parse.nodes[id];
    if matches!(node.kind, NodeKind::Code(_))
        && let Some(cell) = tables
            .iter()
            .flat_map(|t| &t.rows)
            .flat_map(|r| &r.cells)
            .find(|cell| cell.content.contains(&node.envelope.start))
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
    let tables = table::tables(parse);
    let formulas = math::formulas(parse);
    parse
        .nodes
        .iter()
        .enumerate()
        .filter(|(id, _)| {
            let range = inline_range(parse, *id, &tables);
            editable_inline(parse, *id, &tables)
                && range.start < caret
                && caret < range.end
                && !formulas
                    .iter()
                    .any(|f| f.range.start <= range.start && range.end <= f.range.end)
        })
        .map(|(id, _)| inline_range(parse, id, &tables))
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
    for (id, node) in parse.nodes.iter().enumerate() {
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
/// caret. Other block prefixes and unsupported syntax remain editable source.
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
        mut highlighter: Option<&mut dyn SyntaxHighlighter>,
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
        let parse = normalized.as_ref().unwrap_or(parse);
        let text = parse.source().text();
        let mut hidden = vec![false; text.len()];
        let mut styles = vec![Style::default(); text.len()];
        let code_blocks = code_block::blocks(parse);
        let formulas = math::formulas(parse);
        let mut structured = Vec::new();
        let mut folded_diagrams = Vec::new();
        if let Some(highlighter) = &mut highlighter {
            highlighter.begin_frame();
        }
        for block in &code_blocks {
            for style in &mut styles[block.range.clone()] {
                style.code_block = true;
            }
            if let Some(highlighter) = &mut highlighter {
                for span in highlighter.spans(text, block) {
                    for style in &mut styles[span.range] {
                        style.syntax = Some(span.class);
                    }
                }
            }
            if source_mode
                || block_window
                    .is_some_and(|w| w.start < block.range.end && block.range.start < w.end)
            {
                continue;
            }
            hidden[block.range.clone()].fill(true);
            if let Some((_, Ok(diagram))) = diagrams.iter().find(|(range, _)| *range == block.range)
            {
                folded_diagrams.push((block.clone(), diagram.clone()));
                continue;
            }
            for row in &block.rows {
                hidden[row.content.start..row.line.end].fill(false);
            }
            structured.push(block.clone());
        }
        if let Some(highlighter) = &mut highlighter {
            highlighter.end_frame();
        }
        let all_tables = table::tables(parse);
        let tables = all_tables
            .iter()
            .filter(|table| {
                !source_mode
                    && !block_window
                        .is_some_and(|w| w.start < table.range.end && table.range.start < w.end)
            })
            .cloned()
            .collect::<Vec<_>>();
        let list_rows = list::rows(parse)
            .into_iter()
            .filter(|row| {
                !source_mode
                    && !tables.iter().any(|table| {
                        table.range.start < row.line.end && row.line.start < table.range.end
                    })
                    && !block_window.is_some_and(|w| {
                        w.start < row.item.range.end && row.item.range.start < w.end
                    })
            })
            .collect::<Vec<_>>();
        for row in &list_rows {
            for range in &row.hidden {
                hidden[range.clone()].fill(true);
            }
        }
        for table in &tables {
            hidden[table.range.clone()].fill(true);
            for (r, row) in table.rows.iter().enumerate() {
                let end =
                    row.line.start + text[row.line.clone()].trim_end_matches(['\r', '\n']).len();
                hidden[end..row.line.end].fill(false);
                for cell in &row.cells {
                    hidden[cell.content.clone()].fill(false);
                    if r == 0 {
                        for style in &mut styles[cell.content.clone()] {
                            style.bold = true;
                        }
                    }
                }
            }
        }
        if !source_mode {
            for (id, node) in parse.nodes.iter().enumerate() {
                if code_blocks
                    .iter()
                    .any(|block| block.range.contains(&node.envelope.start))
                {
                    continue;
                }
                if let NodeKind::Heading(level) = node.kind {
                    let content = heading_content(parse, id);
                    let revealed = heading_window.into_iter().chain(block_window).any(|range| {
                        range.start < node.envelope.end && node.envelope.start < range.end
                    });
                    for style in &mut styles[node.envelope.clone()] {
                        style.heading = level;
                    }
                    hidden[node.envelope.start..content.start].fill(!revealed);
                    let trimmed_end = node.envelope.start
                        + text[node.envelope.clone()]
                            .trim_end_matches(['\r', '\n'])
                            .len();
                    // Keep the final source newline, but fold ATX closing hashes or
                    // the Setext underline together with its preceding newline.
                    hidden[content.end..trimmed_end].fill(!revealed);
                    for range in [node.envelope.start..content.start, content.end..trimmed_end] {
                        for style in &mut styles[range] {
                            style.marker = true;
                        }
                    }
                }
            }
        }
        for (id, node) in parse.nodes.iter().enumerate() {
            if !editable_inline(parse, id, &all_tables)
                || code_blocks
                    .iter()
                    .any(|block| block.range.contains(&node.envelope.start))
            {
                continue;
            }
            let envelope = inline_range(parse, id, &all_tables);
            if formulas
                .iter()
                .any(|f| f.range.start <= envelope.start && envelope.end <= f.range.end)
            {
                continue;
            }
            let active = source_mode
                || block_window.is_some_and(|w| w.start < envelope.end && envelope.start < w.end)
                || window.is_some_and(|w| w.start < envelope.end && envelope.start < w.end);
            for style in &mut styles[envelope.clone()] {
                match node.kind {
                    NodeKind::Strong => style.bold = true,
                    NodeKind::Emphasis => style.italic = true,
                    NodeKind::Strikethrough => style.strike = true,
                    NodeKind::Code(_) => style.code = true,
                    NodeKind::Link { .. } => style.link = true,
                    _ => (),
                }
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
                hidden[range.clone()].fill(!active);
                for style in &mut styles[range] {
                    style.marker = true;
                }
            }
        }
        // Inline/heading reveal never exposes the enclosing list's structural prefix.
        for row in &list_rows {
            for range in &row.hidden {
                hidden[range.clone()].fill(true);
            }
        }
        let mut insertions = Vec::<(usize, Range<usize>, &str)>::new();
        for table in &tables {
            for row in &table.rows {
                for (c, cell) in row.cells.iter().enumerate() {
                    if c > 0 {
                        insertions.push((
                            cell.content.start,
                            row.cells[c - 1].content.end..cell.content.start,
                            "\t",
                        ));
                    }
                    // Escaped pipes are literal cell content, including inside code.
                    let mut pos = cell.content.start;
                    while pos < cell.content.end {
                        let rest = &text[pos..cell.content.end];
                        let br = ["<br>", "<br/>", "<br />"]
                            .into_iter()
                            .find(|br| rest.starts_with(br));
                        let escaped_pipe = rest.starts_with("\\|")
                            && text[cell.content.start..pos]
                                .bytes()
                                .rev()
                                .take_while(|b| *b == b'\\')
                                .count()
                                % 2
                                == 0;
                        if let Some(br) = br {
                            // A literal <br> in a code span stays literal.
                            if !styles[pos].code {
                                hidden[pos..pos + br.len()].fill(true);
                                insertions.push((pos, pos..pos + br.len(), "\n"));
                                pos += br.len();
                                continue;
                            }
                        }
                        if escaped_pipe {
                            hidden[pos..pos + 2].fill(true);
                            insertions.push((pos, pos..pos + 2, "|"));
                            pos += 2;
                        } else {
                            pos += text[pos..].chars().next().map_or(1, char::len_utf8);
                        }
                    }
                }
            }
        }
        let mut rendered = Vec::new();
        for formula in &formulas {
            let range = &formula.range;
            // LaTeX is literal even when Markdown sees underscores or backticks.
            hidden[range.clone()].fill(false);
            let style = styles[range.start];
            styles[range.clone()].fill(Style {
                code: true,
                marker: false,
                ..style
            });
            styles[range.start].marker = true;
            styles[range.end - 1].marker = true;
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
                hidden[range.clone()].fill(true);
                rendered.push((range.clone(), view));
            }
        }
        for (offset, style) in styles.iter_mut().enumerate() {
            if source_mode || block_window.is_some_and(|range| range.contains(&offset)) {
                style.source = true;
                style.heading = 0;
                style.bold = false;
                style.italic = false;
                style.strike = false;
                style.link = false;
                style.marker = false;
            }
        }
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
                        ..styles[start]
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
                    style: styles[start],
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
            if hidden[start] {
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
            let style = styles[start];
            if let Some(last) = display.spans.last_mut().filter(|s| {
                s.source.end == start
                    && s.style == style
                    && ch != '\r'
                    && s.source.len() == s.display.len()
                    && !s.style.math
                    && !s.style.diagram
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
        display.code_blocks = structured
            .into_iter()
            .map(|block| {
                let mut rows = block
                    .rows
                    .iter()
                    .map(|row| {
                        display.position_at(row.content.start)..display.position_at(row.content.end)
                    })
                    .collect::<Vec<_>>();
                if rows.is_empty() {
                    let start = display.position_at(block.anchor);
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
                let start = display.position_at(row.line.start);
                ListView { source: row, start }
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
                let range = display.position_at(node.envelope.start)
                    ..display.position_at(node.envelope.end);
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
            // Tight lists expose inline children directly under Item, without a
            // Paragraph node. Reuse the already-resolved semantic list rows.
            for row in &display.lists {
                if !row.source.first || display.flow_at(row.start).is_some() {
                    continue;
                }
                let end = display
                    .lists
                    .iter()
                    .filter(|candidate| candidate.source.item.id == row.source.item.id)
                    .map(|candidate| display.position_at(candidate.source.line.end))
                    .max()
                    .unwrap_or(row.start);
                display.flow.push(FlowBlock {
                    range: row.start..end,
                    kind: FlowKind::Item(row.source.item.list),
                    raw: false,
                });
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
        assert!(
            local
                .spans
                .iter()
                .filter(|span| scope.start <= span.source.start && span.source.start < scope.end)
                .all(|span| span.style.source && !span.style.bold)
        );
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
    use editable_markdown_parser::Source;

    #[test]
    fn nested_inline_reveal_keeps_block_prefixes_and_source_unchanged() {
        let parse = ParseSnapshot::new(Source::new("# **a *b* c**\n\n> [**x**](/url)\n"));
        let original = parse.source().text().to_string();
        assert_eq!(build(&parse, None, false).text, "a b c\n\n> x\n");
        let window = active_window(&parse, original.find('b').unwrap()).unwrap();
        assert_eq!(
            build(&parse, Some(&window), false).text,
            "**a *b* c**\n\n> x\n"
        );
        let window = active_window(&parse, original.find('x').unwrap()).unwrap();
        assert_eq!(
            build(&parse, Some(&window), false).text,
            "a b c\n\n> [**x**](/url)\n"
        );
        assert_eq!(parse.source().text(), original);
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
    use editable_markdown_parser::Source;

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
        assert!(
            open.spans
                .iter()
                .filter(|s| s.source.contains(&raw.find("frac").unwrap()))
                .all(|s| s.style.code)
        );
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
    use editable_markdown_parser::Source;

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
