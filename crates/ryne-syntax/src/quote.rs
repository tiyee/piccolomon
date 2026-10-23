//! Quote line context comes from parser-owned container prefixes.
use crate::ParseSnapshot;
use crate::{list, parser};
use editable_markdown_parser::{NodeKind, TokenKind};
use std::ops::Range;

pub struct Row {
    pub owner: usize,
    pub content: Range<usize>,
    pub continuation: String,
    pub exit: Option<Range<usize>>,
}

pub fn at(parse: &ParseSnapshot, caret: usize) -> Option<Row> {
    let normalized = parser::normalized(parse);
    let parse = normalized.unwrap_or(parse);
    let text = parse.source().text();
    let lines = &parse.index().lines;
    let line = lines
        .get(lines.partition_point(|line| line.end <= caret))
        .or_else(|| lines.last())
        .filter(|line| {
            line.start <= caret
                && caret <= line.start + text[(*line).clone()].trim_end_matches(['\r', '\n']).len()
        })?;
    let end = line.start + text[line.clone()].trim_end_matches(['\r', '\n']).len();
    let quote_token = |token: &&editable_markdown_parser::SyntaxToken| {
        token.kind == TokenKind::ContainerPrefix
            && parse.nodes[token.owner].kind == NodeKind::BlockQuote
    };
    let token = parse
        .index()
        .prefixes(parse, line.clone())
        .filter(quote_token)
        .filter(|token| line.start <= token.range.start && token.range.start < line.end)
        .max_by_key(|token| token.range.end);
    let owner = token.map(|token| token.owner).or_else(|| {
        parse
            .index()
            .quotes
            .at(caret)
            .into_iter()
            .map(|id| (id, &parse.nodes[id]))
            .filter(|(_, node)| node.kind == NodeKind::BlockQuote && node.envelope.contains(&caret))
            .min_by_key(|(_, node)| node.envelope.len())
            .map(|(id, _)| id)
    })?;
    let prefix = token.or_else(|| {
        parse
            .syntax
            .tokens
            .iter()
            .filter(quote_token)
            .filter(|token| token.owner == owner && token.range.start < caret)
            .max_by_key(|token| token.range.start)
    })?;
    let prefix_line = parse.index().line(prefix.range.start)?;
    let prefix_end = prefix.range.end.min(
        prefix_line.start
            + text[prefix_line.clone()]
                .trim_end_matches(['\r', '\n'])
                .len(),
    );
    let mut continuation = text[prefix_line.start..prefix_end].to_owned();
    // A list marker on the quote's first line becomes continuation indentation.
    let items = list::cached_items(parse);
    let first = items.partition_point(|item| item.marker.start < prefix_line.start);
    let last = items.partition_point(|item| item.marker.start < prefix_end);
    for item in items[first..last].iter().rev() {
        if prefix_line.start <= item.marker.start && item.marker.end <= prefix_end {
            let range = item.marker.start - prefix_line.start..item.marker.end - prefix_line.start;
            let width = list::columns(&text[prefix_line.start..item.marker.end])
                - list::columns(&text[prefix_line.start..item.marker.start]);
            continuation.replace_range(range, &" ".repeat(width));
        }
    }
    let start = token.map_or(line.start, |token| token.range.end.min(end));
    Some(Row {
        owner,
        content: start..end,
        continuation,
        exit: token.map(|token| token.range.clone()),
    })
}
