use crate::ParseSnapshot;
use ryne_parser::NodeKind;
use std::ops::Range;

#[derive(Clone, Debug)]
pub struct SourceFormula {
    pub node: usize,
    pub range: Range<usize>,
    pub block: bool,
    pub content: Range<usize>,
}

/// Consume authoritative parser nodes; there is no second Markdown scanner.
pub fn formulas(parse: &ParseSnapshot) -> Vec<SourceFormula> {
    cached(parse).to_vec()
}
pub fn cached(parse: &ParseSnapshot) -> &[SourceFormula] {
    parse.formulas.get_or_init(|| build(parse))
}
fn build(parse: &ParseSnapshot) -> Vec<SourceFormula> {
    parse
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(id, node)| {
            let block = match &node.kind {
                NodeKind::InlineMath(_) => false,
                NodeKind::DisplayMath(_) => true,
                _ => return None,
            };
            if block && !standalone(parse, node) {
                return None;
            }
            let mut parent = node.parent;
            while let Some(id) = parent {
                if matches!(parse.nodes[id].kind, NodeKind::Image { .. }) {
                    return None;
                }
                parent = parse.nodes[id].parent;
            }
            // Multiline single-dollar math remains literal.
            let raw = &parse.source().text()[node.envelope.clone()];
            (block || !raw.contains(['\r', '\n'])).then(|| SourceFormula {
                node: id,
                range: node.envelope.clone(),
                block,
                content: node.envelope.start + if block { 2 } else { 1 }
                    ..node.envelope.end - if block { 2 } else { 1 },
            })
        })
        .collect()
}

/// Display math is a panel only when it owns its entire paragraph (or tight item).
pub fn standalone(parse: &ParseSnapshot, node: &ryne_parser::SemanticNode) -> bool {
    let Some(parent) = node.parent.map(|id| &parse.nodes[id]) else {
        return false;
    };
    if !matches!(parent.kind, NodeKind::Paragraph | NodeKind::Item) {
        return false;
    }
    parent.children.iter().all(|&id| {
        let child = &parse.nodes[id];
        child.envelope == node.envelope
            || matches!(&child.kind, NodeKind::Text(text) if text.trim().is_empty())
            || (parent.kind == NodeKind::Item
                && matches!(
                    child.kind,
                    NodeKind::List(_)
                        | NodeKind::BlockQuote
                        | NodeKind::CodeBlock(_)
                        | NodeKind::Table(_)
                        | NodeKind::Rule
                ))
    })
}

/// Continue parser-owned containers, replacing a first-line list marker by spaces.
pub fn continuation(parse: &ParseSnapshot, start: usize) -> String {
    let parse = parse.semantic();
    let Some(line) = parse.index().line(start) else {
        return String::new();
    };
    let mut prefix = parse.source().text()[line.start..start].to_owned();
    let tokens: Vec<_> = parse.index().prefixes(parse, line.clone()).collect();
    for token in tokens.into_iter().rev() {
        if token.range.start >= line.start
            && token.range.end <= start
            && parse.nodes[token.owner].kind == NodeKind::Item
        {
            prefix.replace_range(
                token.range.start - line.start..token.range.end - line.start,
                &" ".repeat(
                    crate::list::columns(&parse.source().text()[line.start..token.range.end])
                        - crate::list::columns(
                            &parse.source().text()[line.start..token.range.start],
                        ),
                ),
            );
        }
    }
    prefix
}

#[cfg(test)]
mod tests {
    use super::*;
    use ryne_parser::Source;
    #[test]
    fn consumes_parser_nodes_and_excludes_images() {
        let text = "中文 $x_1$ ` $code$ ` ![$alt$](image.png)\n```\n$code$\n```\n";
        let parse = ParseSnapshot::with_math(Source::new(text));
        assert_eq!(
            formulas(&parse)
                .iter()
                .map(|f| &text[f.range.clone()])
                .collect::<Vec<_>>(),
            ["$x_1$"]
        );
        assert!(formulas(&ParseSnapshot::new(Source::new(text))).is_empty());
    }
}
