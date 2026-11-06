//! Test-only byte-array oracle retained for interval composition equivalence.
use super::*;
#[path = "legacy_table.rs"]
mod legacy_table;
pub struct LegacyPresentationBuilder<'a> {
    pub renderer: &'a dyn ryne_extensions::FormulaRenderer,
}
impl LegacyPresentationBuilder<'_> {
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
        let mut hidden = vec![false; text.len()];
        let mut styles = vec![Style::default(); text.len()];
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
        if !patch.apply(text, &mut hidden, &mut styles) {
            return Display::plain_source(text);
        }
        let all_tables = ryne_syntax::SemanticQuery::new(parse).tables();
        let tables = legacy_table::select(all_tables, source_mode, block_window);
        let list_rows = list_component::select(parse, &tables, source_mode, block_window);
        if !list_component::prefixes(&list_rows).apply(text, &mut hidden, &mut styles)
            || !legacy_table::contribute(text, &tables).apply(text, &mut hidden, &mut styles)
        {
            return Display::plain_source(text);
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
            if !legacy_editable_inline(parse, id, all_tables)
                || code_blocks
                    .iter()
                    .any(|block| block.range.contains(&node.envelope.start))
            {
                continue;
            }
            let envelope = legacy_inline_range(parse, id, all_tables);
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
        if !list_component::prefixes(&list_rows).apply(text, &mut hidden, &mut styles) {
            return Display::plain_source(text);
        }
        if !quote::prefixes(parse, &quotes).apply(text, &mut hidden, &mut styles) {
            return Display::plain_source(text);
        }
        let (patch, mut insertions) = legacy_table::substitutions(text, &tables, &styles);
        if !patch.apply(text, &mut hidden, &mut styles) {
            return Display::plain_source(text);
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
        display.quotes = quotes
            .into_iter()
            .filter_map(|(_, source, depth, inset)| {
                let range = display.position_at(source.start)..display.position_at(source.end);
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
#[test]
fn interval_composition_matches_byte_oracle_for_structure_reveal_and_source_modes() {
    let renderer = ryne_services::math::FormularyRenderer::default();
    let cases = [
        "# **bold *nested* text**\n\n~~strike~~ `code` [link](url)\n",
        "> - **one**\n>   - `two`\n>\n> tail\n",
        "| a | b |\n| --- | ---: |\n| **x**<br>y | `a<br>b` |\n| a\\|b | |\n",
        "```rust\n**literal**\n\n```\n\nnext\n",
        "    **literal**\n    second\n\ntail",
        "Setext *x*\n===\n\n$x_1$ and **$x^2$**\n",
        "- [x] task\n- \n\n> \n",
        "text e\u{301}👩‍👩‍👧‍👦 **e\u{301}**\n",
        "before `ordinary`\n\n| a | b |\n| --- | --- |\n| `中\\|😀` | **bold** |\n\nbetween `code`\n\n> | x | y |\n> | --- | --- |\n> | `literal` | |\n",
    ];
    for newline in ["\n", "\r\n", "\r"] {
        for case in cases {
            let raw = case.replace('\n', newline);
            let parse = ParseSnapshot::with_math(ryne_parser::Source::new(&raw));
            let legacy = LegacyPresentationBuilder {
                renderer: &renderer,
            };
            let current = PresentationBuilder {
                renderer: &renderer,
            };
            for source_mode in [false, true] {
                for at in (0..=raw.len()).filter(|&p| raw.is_char_boundary(p)) {
                    let reveal = active_window(&parse, at);
                    let heading = ryne_syntax::block::at(&parse, at)
                        .filter(|b| matches!(b.kind, ryne_syntax::block::BlockKind::Heading(_)))
                        .map(|b| b.range);
                    let scope = ryne_syntax::block::at(&parse, at).map(|b| b.scope);
                    for block in [None, scope.as_ref()] {
                        let context = || PresentationContext {
                            parse: &parse,
                            window: reveal.as_ref(),
                            source_mode,
                            heading_window: heading.as_ref(),
                            block_window: block,
                        };
                        assert_eq!(
                            current.build(context(), None, &[]),
                            legacy.build(context(), None, &[]),
                            "{raw:?} at {at} source={source_mode} block={block:?}"
                        );
                    }
                }
            }
        }
    }
}

fn legacy_editable_inline(parse: &ParseSnapshot, id: usize, tables: &[table::Table]) -> bool {
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

fn legacy_inline_range(parse: &ParseSnapshot, id: usize, tables: &[table::Table]) -> Range<usize> {
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
