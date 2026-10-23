use crate::ParseSnapshot;
use editable_markdown_parser::NodeKind;
use std::ops::Range;

#[derive(Clone, Debug)]
pub struct SourceFormula {
    pub range: Range<usize>,
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
        .filter_map(|node| {
            if !matches!(node.kind, NodeKind::InlineMath(_)) {
                return None;
            }
            let mut parent = node.parent;
            while let Some(id) = parent {
                if matches!(parse.nodes[id].kind, NodeKind::Image { .. }) {
                    return None;
                }
                parent = parse.nodes[id].parent;
            }
            // Multiline math remains editable raw source until block layout exists.
            let raw = &parse.source().text()[node.envelope.clone()];
            (!raw.contains(['\r', '\n'])).then(|| SourceFormula {
                range: node.envelope.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use editable_markdown_parser::Source;
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
