//! Standalone Markdown images, recognized by the parser rather than URL regexes.
use crate::ParseSnapshot;
use ryne_parser::NodeKind;
use std::ops::Range;

#[derive(Clone, Debug)]
pub struct ImageBlock {
    pub range: Range<usize>,
    pub scope: Range<usize>,
    pub anchor: usize,
    pub indent: usize,
    pub destination: String,
}

pub fn blocks(parse: &ParseSnapshot) -> &[ImageBlock] {
    let parse = parse.semantic();
    parse.images.get_or_init(|| {
        parse
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(id, paragraph)| {
                // Tight lists omit Paragraph nodes; their inline children
                // belong directly to Item (optionally after a task marker).
                if !matches!(paragraph.kind, NodeKind::Paragraph | NodeKind::Item) {
                    return None;
                }
                let mut children = paragraph
                    .children
                    .iter()
                    .map(|&id| &parse.nodes[id])
                    .filter(|node| !matches!(node.kind, NodeKind::Task(_)));
                let image = children.next()?;
                if children.next().is_some() {
                    return None;
                }
                let NodeKind::Image { destination, .. } = &image.kind else {
                    return None;
                };
                // Consume the physical first-line prefix with the object, just
                // as fenced previews do. Otherwise up to three leading spaces
                // would place the object in the middle of a text layout line.
                let start = parse
                    .index()
                    .line(paragraph.envelope.start)
                    .map_or(paragraph.envelope.start, |line| line.start);
                let range = start..paragraph.envelope.end;
                let mut scope = range.clone();
                let mut parent = Some(id);
                let mut indent = 0;
                while let Some(id) = parent {
                    let node = &parse.nodes[id];
                    if matches!(node.kind, NodeKind::Item) {
                        indent += 2;
                    }
                    if matches!(
                        node.kind,
                        NodeKind::Item | NodeKind::List(_) | NodeKind::BlockQuote
                    ) {
                        scope = node.envelope.clone();
                        scope.start = parse
                            .index()
                            .line(scope.start)
                            .map_or(scope.start, |l| l.start);
                    }
                    parent = node.parent;
                }
                Some(ImageBlock {
                    range,
                    scope,
                    anchor: image.envelope.start,
                    indent,
                    destination: destination.clone(),
                })
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ryne_parser::Source;
    #[test]
    fn recognizes_images_and_references_only_as_complete_paragraphs() {
        for newline in ["\n", "\r\n", "\r"] {
            let raw = "![说明](https://example.org/a.png \"title\")\n\n> - ![alt][img]\n\n[img]: https://example.org/b.png\n\ntext ![inline](x.png)\n\n`![code](x.png)`\n\n```\n![literal](x.png)\n```\n\n![unfinished](\n".replace('\n', newline);
            let parse = ParseSnapshot::new(Source::new(&raw));
            let images = blocks(&parse);
            assert_eq!(images.len(), 2);
            assert_eq!(images[0].destination, "https://example.org/a.png");
            assert_eq!(images[1].destination, "https://example.org/b.png");
            assert_eq!(images[1].indent, 2);
            assert!(raw[images[1].scope.clone()].starts_with("> - "));
            assert!(raw[images[1].anchor..].starts_with("![alt]"));
            assert!(std::ptr::eq(images, blocks(&parse)));
        }
    }
}
