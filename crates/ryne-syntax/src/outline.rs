//! Labels are derived from parser leaves, so container markers, link targets
//! and Setext underlines never become part of the navigable title.
use crate::ParseSnapshot;
use ryne_parser::NodeKind;
use std::ops::Range;

#[derive(Debug, PartialEq, Eq)]
pub struct OutlineHeading {
    pub level: u8,
    pub depth: usize,
    pub range: Range<usize>,
    pub anchor: usize,
    pub title: String,
}

pub(crate) fn collect(original: &ParseSnapshot) -> Vec<OutlineHeading> {
    let parse = original.semantic();
    let mut levels = Vec::new();
    let mut outline = Vec::new();
    let mut pending = Vec::new();
    for &id in &parse.index().heading_nodes {
        let heading = &parse.nodes[id];
        let NodeKind::Heading(level) = heading.kind else {
            continue;
        };
        let range = heading.envelope.clone();
        if original.source().text().get(range.clone()).is_none() {
            continue;
        }
        while levels.last().is_some_and(|last| *last >= level) {
            levels.pop();
        }
        let depth = levels.len();
        levels.push(level);
        let mut label = String::new();
        let mut anchor = None;
        pending.extend(heading.children.iter().rev().copied());
        while let Some(id) = pending.pop() {
            let leaf = &parse.nodes[id];
            match &leaf.kind {
                NodeKind::Text(s)
                | NodeKind::Code(s)
                | NodeKind::InlineMath(s)
                | NodeKind::DisplayMath(s) => {
                    anchor.get_or_insert_with(|| {
                        parse.syntax.nodes[id]
                            .content
                            .first()
                            .map_or(leaf.envelope.start, |content| content.start)
                    });
                    label.push_str(s);
                }
                NodeKind::HardBreak | NodeKind::SoftBreak => label.push(' '),
                _ => pending.extend(leaf.children.iter().rev().copied()),
            }
        }
        outline.push(OutlineHeading {
            level,
            depth,
            anchor: anchor.unwrap_or(range.start),
            range,
            title: label.split_whitespace().collect::<Vec<_>>().join(" "),
        });
    }
    outline
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SemanticQuery;
    use ryne_parser::Source;
    #[test]
    fn outline_uses_committed_source_ranges_and_inline_text() {
        for newline in ["\n", "\r\n", "\r"] {
            let raw = [
                "# 中文 **é**",
                "",
                "> - #### [link](dest) `code` ![alt](image) $x$",
                "",
                "multi",
                "line",
                "----",
                "",
                "#",
                "",
                "```",
                "# fake",
                "```",
            ]
            .join(newline);
            let parse = ParseSnapshot::with_math(Source::new(&raw));
            let query = SemanticQuery::new(&parse);
            let heads = query.outline();
            assert_eq!(
                heads
                    .iter()
                    .map(|h| (h.title.as_str(), h.depth, h.level))
                    .collect::<Vec<_>>(),
                [
                    ("中文 é", 0, 1),
                    ("link code alt x", 1, 4),
                    ("multi line", 1, 2),
                    ("", 0, 1)
                ]
            );
            assert_eq!(heads[1].anchor, raw.find("link").unwrap());
            assert!(heads.iter().all(|h| raw.get(h.range.clone()).is_some()));
            assert!(std::ptr::eq(heads, query.outline()));
        }
    }
}
