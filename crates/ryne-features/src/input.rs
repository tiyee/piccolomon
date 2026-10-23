//! Input completion proposes edits; the parser still decides their meaning.
use editable_markdown_parser::{Edit, NodeKind, TokenKind};
use ryne_syntax::ParseSnapshot;
use std::ops::Range;

use ryne_syntax::parser::normalized;

pub struct InlineCompletion {
    pub replacement: String,
    pub caret: usize,
    pub closing: Range<usize>,
}

pub fn inline(parse: &ParseSnapshot, caret: usize, input: &str) -> Option<InlineCompletion> {
    if input.is_empty() || input.contains(['\r', '\n']) {
        return None;
    }
    if let Some(parse) = normalized(parse) {
        return inline(parse, caret, input);
    }
    let source = parse.source();
    let text = source.text();
    let marker = *text.get(..caret)?.as_bytes().last()?;
    if !matches!(marker, b'*' | b'_' | b'~' | b'`' | b'[' | b'$') {
        return None;
    }
    let count = text[..caret]
        .bytes()
        .rev()
        .take_while(|b| *b == marker)
        .count();
    let start = caret - count;
    if text[..start]
        .bytes()
        .rev()
        .take_while(|b| *b == b'\\')
        .count()
        % 2
        == 1
    {
        return None;
    }
    // Never turn existing syntax, closing delimiters, code, or raw HTML into
    // another opener. The candidate check below also excludes fenced drafts.
    if parse.nodes.iter().any(|node| {
        node.envelope.start <= start
            && caret <= node.envelope.end
            && matches!(
                node.kind,
                NodeKind::Code(_)
                    | NodeKind::CodeBlock(_)
                    | NodeKind::InlineMath(_)
                    | NodeKind::DisplayMath(_)
                    | NodeKind::Html(_)
                    | NodeKind::HtmlBlock
            )
    }) || parse.syntax.tokens.iter().any(|token| {
        token.range.start < caret
            && start < token.range.end
            && matches!(token.kind, TokenKind::OpenMarker | TokenKind::CloseMarker)
            && matches!(
                parse.nodes[token.owner].kind,
                NodeKind::Strong
                    | NodeKind::Emphasis
                    | NodeKind::Strikethrough
                    | NodeKind::Link { .. }
                    | NodeKind::Image { .. }
            )
    }) {
        return None;
    }
    let extra = input.bytes().take_while(|b| *b == marker).count();
    let width = count + extra;
    let body = input.get(extra..)?;
    if body.is_empty()
        || body.starts_with(char::is_whitespace)
        || body.ends_with(char::is_whitespace)
    {
        return None;
    }
    let supported = match marker {
        b'*' | b'_' => (1..=3).contains(&width),
        b'~' => width == 2,
        b'`' => width <= 32,
        b'[' | b'$' => width == 1,
        _ => false,
    };
    if !supported {
        return None;
    }
    let close = if marker == b'[' {
        "]()".to_owned()
    } else {
        (marker as char).to_string().repeat(width)
    };
    if text[caret..].starts_with(&close) {
        return None;
    }
    let replacement = format!("{input}{close}");
    let applied = source
        .apply(
            source.id(),
            vec![Edit {
                range: caret..caret,
                replacement: replacement.clone(),
            }],
        )
        .ok()?;
    let candidate = ParseSnapshot::with_math(applied.source);
    let end = caret + replacement.len();
    let valid = candidate.nodes.iter().any(|node| {
        node.envelope == (start..end)
            && match marker {
                b'*' | b'_' => matches!(node.kind, NodeKind::Emphasis | NodeKind::Strong),
                b'~' => node.kind == NodeKind::Strikethrough,
                b'`' => matches!(node.kind, NodeKind::Code(_)),
                b'[' => matches!(node.kind, NodeKind::Link { .. }),
                b'$' => matches!(node.kind, NodeKind::InlineMath(_)),
                _ => false,
            }
    });
    valid.then_some(InlineCompletion {
        replacement,
        caret: caret + input.len(),
        closing: caret + input.len()..end,
    })
}

pub struct Fence {
    pub range: Range<usize>,
    pub closing: String,
    pub continuation: String,
}

/// An unclosed opening line stays source until Return, including inside containers.
pub fn fence(parse: &ParseSnapshot, caret: usize) -> Option<Fence> {
    if let Some(parse) = normalized(parse) {
        return fence(parse, caret);
    }
    let text = parse.source().text();
    // A fence draft can only end under the caret inside a code node. Avoid
    // allocating all physical line ranges for ordinary caret-only refreshes.
    if !parse
        .index()
        .codes
        .at(caret)
        .into_iter()
        .map(|id| &parse.nodes[id])
        .any(|node| {
            matches!(node.kind, NodeKind::CodeBlock(_))
                && node.envelope.start <= caret
                && caret <= node.envelope.end
        })
    {
        return None;
    }
    parse
        .index()
        .codes
        .at(caret)
        .into_iter()
        .map(|id| &parse.nodes[id])
        .find_map(|node| {
            if !matches!(node.kind, NodeKind::CodeBlock(_)) {
                return None;
            }
            let first = parse.index().line(node.envelope.start)?;
            let end = first.start + text[first.clone()].trim_end_matches(['\r', '\n']).len();
            if caret != end
                || node
                    .children
                    .iter()
                    .any(|id| parse.nodes[*id].envelope.start < first.end)
                || ryne_syntax::code_block::blocks(parse)
                    .iter()
                    .any(|block| block.range.start == first.start && block.closing.is_some())
            {
                return None;
            }
            let raw = text[node.envelope.start..end].trim_start_matches(' ');
            let marker = *raw.as_bytes().first()?;
            if !matches!(marker, b'`' | b'~') {
                return None;
            }
            let width = raw.bytes().take_while(|b| *b == marker).count();
            if width < 3 {
                return None;
            }
            let mut continuation = text[first.start..end - raw.len()].to_owned();
            for token in &parse.syntax.tokens {
                if token.kind == TokenKind::ContainerPrefix
                    && token.range.start >= first.start
                    && token.range.end <= first.start + continuation.len()
                    && parse.nodes[token.owner].kind == NodeKind::Item
                {
                    continuation.replace_range(
                        token.range.start - first.start..token.range.end - first.start,
                        &" ".repeat(token.range.len()),
                    );
                }
            }
            Some(Fence {
                range: first.start..end,
                closing: (marker as char).to_string().repeat(width),
                continuation,
            })
        })
}
