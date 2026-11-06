use crate::ParseSnapshot;
use ryne_parser::{NodeKind, TokenKind};
use std::ops::Range;

#[cfg(test)]
#[path = "code_language_tests.rs"]
mod language_tests;

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
    #[cfg(any(test, feature = "query-metrics"))]
    crate::query_metrics::record(|m| m.physical_lines += 1);
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
    for token in parse.index().prefixes(parse, line.clone()) {
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
    cached(parse).to_vec()
}
pub fn cached(parse: &ParseSnapshot) -> &[CodeBlock] {
    let parse = parse.semantic();
    parse.codes.get_or_init(|| blocks_lf(parse))
}

/// The editable info token of a fenced block, in original source bytes.
/// Query the snapshot-owned code directory instead of scanning the whole AST.
pub fn language_range(parse: &ParseSnapshot, block: &CodeBlock) -> Option<Range<usize>> {
    if block
        .rows
        .first()
        .is_some_and(|row| row.line.start == block.range.start)
    {
        return None; // Indented code can contain literal fence-looking text.
    }
    let semantic = parse.semantic();
    let candidates = semantic.index().codes.overlapping(block.range.clone());
    #[cfg(any(test, feature = "query-metrics"))]
    crate::query_metrics::record(|m| m.candidates += candidates.len());
    let node = candidates
        .into_iter()
        .map(|id| &semantic.nodes[id])
        .find(|node| block.range.contains(&node.envelope.start))?;
    let text = parse.source().text();
    let start = node.envelope.start;
    let line = text.get(start..)?;
    let marker = *line.as_bytes().first()?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }
    let width = line.bytes().take_while(|b| *b == marker).count();
    if width < 3 {
        return None;
    }
    let end = start + line.find(['\r', '\n']).unwrap_or(line.len());
    let rest = &text[start + width..end];
    let token = start + width + rest.len() - rest.trim_start_matches([' ', '\t']).len();
    let len = text[token..end]
        .find(char::is_whitespace)
        .unwrap_or(end - token);
    Some(token..token + len)
}

fn blocks_lf(parse: &ParseSnapshot) -> Vec<CodeBlock> {
    let text = parse.source().text();
    let source_lines = &parse.index().lines;
    let mut result = Vec::new();
    for &id in &parse.index().code_nodes {
        let node = &parse.nodes[id];
        let NodeKind::CodeBlock(language) = &node.kind else {
            continue;
        };
        let Some(first) = parse.index().line(node.envelope.start) else {
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
                [source_lines.partition_point(|line| line.start < node.envelope.end)..]
                .iter()
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
            source_lines[..source_lines.partition_point(|line| line.start < node.envelope.end)]
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
        let body_lines = source_lines.partition_point(|line| line.start < body_start)
            ..source_lines.partition_point(|line| line.start < body_end);
        let text_children: Vec<_> = node
            .children
            .iter()
            .map(|&id| &parse.nodes[id])
            .filter(|node| matches!(node.kind, NodeKind::Text(_)))
            .collect();
        let rows = source_lines[body_lines]
            .iter()
            .map(|line| {
                let end = content_end(text, line);
                let semantic_start = text_children
                    [text_children.partition_point(|child| child.envelope.end <= line.start)..]
                    .iter()
                    .take_while(|child| child.envelope.start < line.end)
                    .filter_map(|child| {
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
        if let Some(line) = parse.index().line(scope.start) {
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
        for token in parse.index().prefixes(parse, first.start..container_end) {
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
