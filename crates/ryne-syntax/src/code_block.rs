use editable_markdown_parser::{NodeKind, ParseSnapshot, TokenKind};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub struct CodeRow {
    pub line: Range<usize>,
    pub content: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CodeBlock {
    pub range: Range<usize>,
    pub scope: Range<usize>,
    pub rows: Vec<CodeRow>,
    pub language: String,
    pub closing: Option<Range<usize>>,
    pub anchor: usize,
    pub indent: usize,
    pub continuation: String,
}

pub fn lines(text: &str) -> Vec<Range<usize>> {
    let mut result = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = text[start..]
            .find(['\r', '\n'])
            .map_or(text.len(), |i| start + i);
        if text.as_bytes().get(end) == Some(&b'\r') {
            end += 1;
        }
        if text.as_bytes().get(end) == Some(&b'\n') {
            end += 1;
        }
        result.push(start..end);
        start = end;
    }
    result
}

fn content_end(text: &str, line: &Range<usize>) -> usize {
    line.start + text[line.clone()].trim_end_matches(['\r', '\n']).len()
}

fn prefix_end(parse: &ParseSnapshot, line: &Range<usize>) -> usize {
    let mut start = line.start;
    for token in &parse.syntax.tokens {
        if token.kind == TokenKind::ContainerPrefix && token.range.start == start {
            start = token
                .range
                .end
                .min(content_end(parse.source().text(), line));
        }
    }
    start
}

// The semantic AST decides what is code. Physical lines are used only to
// recover exact indentation and fences, including fences inside containers.
pub fn blocks(parse: &ParseSnapshot) -> Vec<CodeBlock> {
    if let Some(view) = crate::parser::normalized(parse) {
        return blocks_lf(&view);
    }
    blocks_lf(parse)
}

fn blocks_lf(parse: &ParseSnapshot) -> Vec<CodeBlock> {
    let text = parse.source().text();
    let source_lines = lines(text);
    let mut result = Vec::new();
    for (id, node) in parse.nodes.iter().enumerate() {
        let NodeKind::CodeBlock(language) = &node.kind else {
            continue;
        };
        let Some(first) = source_lines
            .iter()
            .find(|line| line.start <= node.envelope.start && node.envelope.start < line.end)
        else {
            continue;
        };
        let first_end = content_end(text, first);
        let raw = &text[node.envelope.start..first_end];
        let trimmed = raw.trim_start_matches([' ', '\t']);
        let fence_char = trimmed
            .as_bytes()
            .first()
            .copied()
            .filter(|c| matches!(c, b'`' | b'~'));
        let fence_width = fence_char.map_or(0, |c| trimmed.bytes().take_while(|&b| b == c).count());
        let fenced = fence_width >= 3
            && !node
                .children
                .iter()
                .any(|&child| parse.nodes[child].envelope.start < first.end);
        // An opening fence still being typed remains editable source until Return.
        if fenced && first.end == first_end {
            continue;
        }
        let opening = fenced.then(|| first.clone());
        let mut semantic_end = node.envelope.end;
        if !fenced {
            for line in source_lines
                .iter()
                .filter(|line| line.start >= node.envelope.end)
            {
                let value = &text[line.start..content_end(text, line)];
                if line.start == semantic_end
                    && value.len() >= 4
                    && value.bytes().all(|b| matches!(b, b' ' | b'\t'))
                {
                    semantic_end = line.end;
                } else {
                    break;
                }
            }
        }
        let closing = if fenced {
            source_lines
                .iter()
                .rev()
                .find(|line| {
                    if line.start < first.end || line.start >= node.envelope.end {
                        return false;
                    }
                    let start = prefix_end(parse, line);
                    let raw = text[start..content_end(text, line)].trim_start_matches(' ');
                    let fence_start = content_end(text, line) - raw.len();
                    if node
                        .children
                        .iter()
                        .any(|&child| parse.nodes[child].envelope.contains(&fence_start))
                    {
                        return false;
                    }
                    let width = raw.bytes().take_while(|&b| Some(b) == fence_char).count();
                    width >= fence_width && raw[width..].trim_matches([' ', '\t']).is_empty()
                })
                .cloned()
        } else {
            None
        };
        let body_start = opening.as_ref().map_or(first.start, |line| line.end);
        let body_end = closing.as_ref().map_or(semantic_end, |line| line.start);
        let opening_indent = if fenced {
            node.envelope.start.saturating_sub(prefix_end(parse, first)) + raw.len() - trimmed.len()
        } else {
            4
        };
        let rows = source_lines
            .iter()
            .filter(|line| body_start <= line.start && line.start < body_end)
            .map(|line| {
                let end = content_end(text, line);
                let semantic_start = node
                    .children
                    .iter()
                    .filter_map(|&child| {
                        let child = &parse.nodes[child];
                        (matches!(child.kind, NodeKind::Text(_))
                            && child.envelope.start < line.end
                            && line.start < child.envelope.end)
                            .then_some(child.envelope.start.max(line.start).min(end))
                    })
                    .min();
                let mut start = semantic_start.unwrap_or_else(|| prefix_end(parse, line));
                let mut columns = 0;
                while semantic_start.is_none() && start < end && columns < opening_indent {
                    match text.as_bytes()[start] {
                        b' ' => columns += 1,
                        b'\t' => columns += 4 - columns % 4,
                        _ => break,
                    }
                    start += 1;
                }
                CodeRow {
                    line: line.clone(),
                    content: start..end,
                }
            })
            .collect::<Vec<_>>();
        let mut scope = node.envelope.start..semantic_end;
        let mut parent = parse.nodes[id].parent;
        let mut indent = 0;
        while let Some(p) = parent {
            if matches!(
                parse.nodes[p].kind,
                NodeKind::BlockQuote | NodeKind::List(_) | NodeKind::Item
            ) {
                scope = parse.nodes[p].envelope.clone();
                if !matches!(parse.nodes[p].kind, NodeKind::List(_)) {
                    indent += 1;
                }
            }
            parent = parse.nodes[p].parent;
        }
        if let Some(line) = source_lines
            .iter()
            .find(|line| line.start <= scope.start && scope.start < line.end)
        {
            scope.start = line.start;
        }
        let anchor = rows.first().map_or_else(
            || {
                closing
                    .as_ref()
                    .map_or(body_start, |line| prefix_end(parse, line))
            },
            |row| row.content.start,
        );
        let container_end = node.envelope.start;
        let mut continuation = text[first.start..container_end].to_owned();
        // A list's first-line marker becomes hanging indentation on code lines.
        for token in &parse.syntax.tokens {
            if token.kind == TokenKind::ContainerPrefix
                && token.range.start >= first.start
                && token.range.end <= container_end
                && matches!(parse.nodes[token.owner].kind, NodeKind::Item)
            {
                let from = token.range.start - first.start;
                let to = token.range.end - first.start;
                continuation.replace_range(from..to, &" ".repeat(to - from));
            }
        }
        result.push(CodeBlock {
            range: first.start..closing.as_ref().map_or(semantic_end, |line| line.end),
            scope,
            rows,
            language: language.clone(),
            closing,
            anchor,
            indent,
            continuation,
        });
    }
    result
}

impl CodeBlock {
    pub fn caret(&self, offset: usize) -> usize {
        if let Some(row) = self
            .rows
            .iter()
            .find(|row| row.line.start <= offset && offset < row.line.end)
        {
            return offset.clamp(row.content.start, row.content.end);
        }
        if offset < self.anchor {
            self.anchor
        } else {
            self.rows.last().map_or(self.anchor, |row| row.content.end)
        }
    }
}
