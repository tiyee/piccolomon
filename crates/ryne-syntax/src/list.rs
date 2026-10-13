use crate::code_block;
use editable_markdown_parser::{NodeKind, ParseSnapshot, TokenKind};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: usize,
    pub list: usize,
    pub parent: Option<usize>,
    pub range: Range<usize>,
    pub scope: Range<usize>,
    pub marker: Range<usize>,
    pub body: usize,
    pub label: String,
    pub next_marker: String,
    pub continuation: String,
    pub inset: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub line: Range<usize>,
    pub content: Range<usize>,
    pub hidden: Vec<Range<usize>>,
    pub item: Item,
    pub first: bool,
}

pub fn columns(text: &str) -> usize {
    text.chars().fold(0, |column, ch| {
        if ch == '\t' {
            column + 4 - column % 4
        } else {
            column + 1
        }
    })
}

pub fn unindent(
    text: &str,
    line: &Range<usize>,
    start: usize,
    amount: usize,
) -> Option<(Range<usize>, String)> {
    let initial = columns(&text[line.start..start]);
    let mut column = initial;
    let mut end = start;
    for byte in text[start..line.end].bytes() {
        if column - initial >= amount || !matches!(byte, b' ' | b'\t') {
            break;
        }
        column += if byte == b'\t' { 4 - column % 4 } else { 1 };
        end += 1;
    }
    (start < end).then(|| {
        (
            start..end,
            " ".repeat((column - initial).saturating_sub(amount)),
        )
    })
}

fn column(parse: &ParseSnapshot, list: usize) -> usize {
    let node = &parse.nodes[list];
    if let NodeKind::List(Some(start)) = node.kind {
        let last = start.saturating_add(node.children.len().saturating_sub(1) as u64);
        ((last.to_string().len() + 1) * 10 + 12).max(32)
    } else {
        32
    }
}

pub fn items(parse: &ParseSnapshot) -> Vec<Item> {
    let normalized = crate::parser::normalized(parse);
    let parse = normalized.as_ref().unwrap_or(parse);
    let text = parse.source().text();
    let lines = code_block::lines(text);
    parse
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(id, node)| {
            if node.kind != NodeKind::Item {
                return None;
            }
            let list = node.parent?;
            let NodeKind::List(number) = parse.nodes[list].kind else {
                return None;
            };
            let line = lines
                .iter()
                .find(|line| line.contains(&node.envelope.start))?;
            let start = node.envelope.start;
            let mut end = start;
            while text.as_bytes().get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            if !text
                .as_bytes()
                .get(end)
                .is_some_and(|b| matches!(b, b'-' | b'+' | b'*' | b'.' | b')'))
            {
                return None;
            }
            end += 1;
            let delimiter_end = end;
            let line_end = line.start + text[line.clone()].trim_end_matches(['\r', '\n']).len();
            while end < line_end && matches!(text.as_bytes()[end], b' ' | b'\t') {
                end += 1;
            }
            let index = parse.nodes[list]
                .children
                .iter()
                .position(|&child| child == id)?;
            let ordinal = number.map(|n| n.saturating_add(index as u64));
            let label = ordinal.map_or_else(|| "•".into(), |n| format!("{n}."));
            let next_marker = ordinal.map_or_else(
                || format!("{} ", &text[start..delimiter_end]),
                |n| {
                    format!(
                        "{}{} ",
                        // CommonMark accepts at most nine source digits; rendering
                        // still counts subsequent items from the list's start value.
                        if n < 999_999_999 { n + 1 } else { 1 },
                        &text[delimiter_end - 1..delimiter_end]
                    )
                },
            );
            let mut inset = column(parse, list);
            let mut parent = None;
            let mut scope = parse.nodes[list].envelope.clone();
            let mut ancestor = parse.nodes[list].parent;
            while let Some(p) = ancestor {
                if parse.nodes[p].kind == NodeKind::Item && parent.is_none() {
                    parent = Some(p);
                }
                if matches!(parse.nodes[p].kind, NodeKind::List(_)) {
                    inset += column(parse, p);
                }
                if matches!(
                    parse.nodes[p].kind,
                    NodeKind::List(_) | NodeKind::Item | NodeKind::BlockQuote
                ) {
                    scope = parse.nodes[p].envelope.clone();
                }
                ancestor = parse.nodes[p].parent;
            }
            scope.start = lines
                .iter()
                .find(|l| l.contains(&scope.start))
                .map_or(scope.start, |l| l.start);
            let width = columns(&text[line.start..end])
                .saturating_sub(columns(&text[line.start..start]))
                .max(2);
            Some(Item {
                id,
                list,
                parent,
                range: line.start..node.envelope.end,
                scope,
                marker: start..end,
                body: end,
                label,
                next_marker,
                continuation: format!("{}{}", &text[line.start..start], " ".repeat(width)),
                inset,
            })
        })
        .collect()
}

pub fn rows(parse: &ParseSnapshot) -> Vec<Row> {
    let items = items(parse);
    let code = code_block::blocks(parse);
    let text = parse.source().text();
    code_block::lines(text)
        .into_iter()
        .filter_map(|line| {
            if code
                .iter()
                .any(|b| b.range.start < line.end && line.start < b.range.end)
            {
                return None;
            }
            let item = items
                .iter()
                .filter(|item| item.range.start <= line.start && line.start < item.range.end)
                .max_by_key(|item| item.inset)?
                .clone();
            let end = line.start + text[line.clone()].trim_end_matches(['\r', '\n']).len();
            let first = line.contains(&item.marker.start);
            if !first && text[line.start..end].trim().is_empty() {
                return None;
            }
            let mut hidden = parse
                .syntax
                .tokens
                .iter()
                .filter(|token| {
                    token.kind == TokenKind::ContainerPrefix
                        && parse.nodes[token.owner].kind == NodeKind::Item
                        && token.range.start < end
                        && line.start < token.range.end
                })
                .map(|token| token.range.start.max(line.start)..token.range.end.min(end))
                .collect::<Vec<_>>();
            // Bare CR parsing uses the normalized AST above; recover its proven first-line marker.
            if first {
                hidden.push(item.marker.clone());
            }
            let content = hidden
                .iter()
                .map(|r| r.end)
                .max()
                .unwrap_or(line.start)
                .max(if first { item.body } else { line.start });
            Some(Row {
                content: content..end,
                line,
                hidden,
                item,
                first,
            })
        })
        .collect()
}

pub fn quote_end(parse: &ParseSnapshot, line: &Range<usize>) -> usize {
    parse
        .syntax
        .tokens
        .iter()
        .filter(|t| {
            t.kind == TokenKind::ContainerPrefix
                && parse.nodes[t.owner].kind == NodeKind::BlockQuote
                && line.start <= t.range.start
                && t.range.start < line.end
        })
        .map(|t| t.range.end)
        .max()
        .unwrap_or(line.start)
}
