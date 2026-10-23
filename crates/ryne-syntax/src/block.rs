//! Block controls refer to the same semantic AST and source as the editor.
use crate::ParseSnapshot;
use crate::{code_block, list, table};
use editable_markdown_parser::NodeKind;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    List { ordered: bool, task: bool },
    Quote,
    Code,
    Mermaid,
    Table,
    Raw,
    Rule,
    Image,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub range: Range<usize>,
    /// Local source includes the complete enclosing container context.
    pub scope: Range<usize>,
    pub anchor: usize,
    pub kind: BlockKind,
}

impl Block {
    pub fn is_list(&self) -> bool {
        matches!(self.kind, BlockKind::List { .. })
    }
}

/// Physical sibling insertion boundary and the existing container continuation.
/// A block sharing a list marker's line uses the full enclosing container so
/// insertion never separates that marker from its first child.
pub fn insertion_context(parse: &ParseSnapshot, block: &Block) -> Option<(Range<usize>, String)> {
    if let Some(view) = crate::parser::normalized(parse) {
        return insertion_context(view, block);
    }
    if at(parse, block.anchor).as_ref() != Some(block) {
        return None;
    }
    let node = parse
        .nodes
        .iter()
        .filter(|node| {
            block.range.start <= node.envelope.start
                && node.envelope.start <= block.anchor
                && node.envelope.end <= block.range.end
                && matches!(
                    node.kind,
                    NodeKind::Paragraph
                        | NodeKind::Heading(_)
                        | NodeKind::List(_)
                        | NodeKind::BlockQuote
                        | NodeKind::CodeBlock(_)
                        | NodeKind::Table(_)
                        | NodeKind::HtmlBlock
                        | NodeKind::Rule
                        | NodeKind::SourceBlock
                )
        })
        .max_by_key(|node| node.envelope.len())?;
    let text = parse.source().text();
    let nested = node
        .parent
        .is_some_and(|parent| parse.nodes[parent].kind != NodeKind::Document);
    let prefix = if nested {
        &text[block.range.start..node.envelope.start]
    } else {
        ""
    };
    if prefix.chars().any(|ch| !matches!(ch, '>' | ' ' | '\t')) {
        return Some((block.scope.clone(), String::new()));
    }
    Some((block.range.clone(), prefix.into()))
}

pub fn can_insert_above(parse: &ParseSnapshot, block: &Block) -> bool {
    insertion_context(parse, block)
        .is_some_and(|(range, _)| !parse.source().text()[..range.start].trim().is_empty())
}

pub fn at(parse: &ParseSnapshot, caret: usize) -> Option<Block> {
    let text = parse.source().text();
    if let Some(view) = crate::parser::normalized(parse) {
        return at(view, caret);
    }
    let contains = |range: &Range<usize>| {
        range.contains(&caret) || (caret == text.len() && caret == range.end)
    };
    if let Some(code) = code_block::cached(parse)
        .iter()
        .find(|b| contains(&b.range))
    {
        let diagram = code.language.split_whitespace().next() == Some("mermaid");
        return Some(Block {
            range: code.range.clone(),
            scope: code.scope.clone(),
            anchor: code.anchor,
            kind: if diagram {
                BlockKind::Mermaid
            } else {
                BlockKind::Code
            },
        });
    }
    if let Some(table) = table::cached(parse).iter().find(|t| contains(&t.range)) {
        let anchor = table.rows.first()?.cells.first()?.content.start;
        return Some(Block {
            range: table.range.clone(),
            scope: table.scope.clone(),
            anchor,
            kind: BlockKind::Table,
        });
    }
    let (id, node) = parse
        .index()
        .blocks
        .at(caret)
        .into_iter()
        .map(|id| (id, &parse.nodes[id]))
        .filter(|(_, n)| {
            contains(&n.envelope)
                && matches!(
                    n.kind,
                    NodeKind::Paragraph
                        | NodeKind::Heading(_)
                        | NodeKind::Item
                        | NodeKind::BlockQuote
                        | NodeKind::CodeBlock(_)
                        | NodeKind::Table(_)
                        | NodeKind::HtmlBlock
                        | NodeKind::Rule
                        | NodeKind::SourceBlock
                )
        })
        .min_by_key(|(_, n)| n.envelope.len())?;
    let mut target = id;
    // Ordinary prose in a list/quote belongs to that container's block control.
    // Headings, code and tables retain their own type inside containers.
    if node.kind == NodeKind::Paragraph
        && let Some(parent) = node.parent
        && matches!(
            parse.nodes[parent].kind,
            NodeKind::Item | NodeKind::BlockQuote
        )
    {
        target = parent;
    }
    if parse.nodes[target].kind == NodeKind::BlockQuote {
        // All prose lines/paragraphs in a quote share its outer block control,
        // including nested quotes. Keep semantic child panels independent.
        let mut ancestor = parse.nodes[target].parent;
        while let Some(parent) = ancestor {
            if parse.nodes[parent].kind == NodeKind::BlockQuote {
                target = parent;
            }
            ancestor = parse.nodes[parent].parent;
        }
    }
    let node = &parse.nodes[target];
    if node.kind == NodeKind::Item
        && let Some(item) = list::cached_items(parse).iter().find(|i| i.id == target)
    {
        // One type control represents the complete outer list. Item controls
        // remain independent, including for nested lists of a different kind.
        let mut list = item.list;
        let mut ancestor = parse.nodes[list].parent;
        while let Some(id) = ancestor {
            if matches!(parse.nodes[id].kind, NodeKind::List(_)) {
                list = id;
            }
            ancestor = parse.nodes[id].parent;
        }
        let first = list::cached_items(parse).iter().find(|i| i.list == list)?;
        let ordered = matches!(parse.nodes[list].kind, NodeKind::List(Some(_)));
        let task = parse.nodes.iter().any(|candidate| {
            if !matches!(candidate.kind, NodeKind::Task(_)) {
                return false;
            }
            let mut parent = candidate.parent;
            while let Some(id) = parent {
                if parse.nodes[id].kind == NodeKind::Item {
                    return parse.nodes[id].parent == Some(list);
                }
                parent = parse.nodes[id].parent;
            }
            false
        });
        return Some(Block {
            range: first.range.start..parse.nodes[list].envelope.end,
            scope: first.scope.clone(),
            anchor: first.body,
            kind: BlockKind::List { ordered, task },
        });
    }

    let kind = match node.kind {
        NodeKind::Heading(level) => BlockKind::Heading(level),
        NodeKind::BlockQuote => BlockKind::Quote,
        NodeKind::CodeBlock(_) => BlockKind::Code,
        NodeKind::Table(_) => BlockKind::Table,
        NodeKind::HtmlBlock | NodeKind::SourceBlock => BlockKind::Raw,
        NodeKind::Rule => BlockKind::Rule,
        _ if node.children.len() == 1
            && matches!(parse.nodes[node.children[0]].kind, NodeKind::Image { .. }) =>
        {
            BlockKind::Image
        }
        _ => BlockKind::Paragraph,
    };
    let lines = code_block::lines(text);
    let line_start = |position| {
        lines
            .iter()
            .find(|l| l.contains(&position))
            .map_or(position, |l| l.start)
    };
    let range = line_start(node.envelope.start)..node.envelope.end;
    let mut scope = range.clone();
    let mut parent = node.parent;
    while let Some(p) = parent {
        let ancestor = &parse.nodes[p];
        if matches!(
            ancestor.kind,
            NodeKind::BlockQuote | NodeKind::Item | NodeKind::List(_)
        ) {
            scope = line_start(ancestor.envelope.start)..ancestor.envelope.end;
        }
        parent = ancestor.parent;
    }
    Some(Block {
        range,
        scope,
        anchor: node.envelope.start,
        kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use editable_markdown_parser::Source;

    fn block(text: &str, needle: &str) -> Block {
        at(
            &ParseSnapshot::new(Source::new(text)),
            text.find(needle).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn types_follow_ast_including_setext_and_literal_code() {
        for (text, needle, kind) in [
            ("###### title\n", "title", BlockKind::Heading(6)),
            ("title\n===\n", "title", BlockKind::Heading(1)),
            ("plain **bold**\n", "bold", BlockKind::Paragraph),
            ("![alt](img.png)\n", "alt", BlockKind::Image),
            ("> quote\n", "quote", BlockKind::Quote),
            (
                "1. item\n",
                "item",
                BlockKind::List {
                    ordered: true,
                    task: false,
                },
            ),
            (
                "- item\n",
                "item",
                BlockKind::List {
                    ordered: false,
                    task: false,
                },
            ),
            ("```rust\n# literal\n```\n", "literal", BlockKind::Code),
            ("```mermaid\ngraph LR\n```\n", "graph", BlockKind::Mermaid),
            ("| a | b |\n|---|---|\n| c | d |\n", "d |", BlockKind::Table),
        ] {
            assert_eq!(block(text, needle).kind, kind, "{text}");
        }
    }

    #[test]
    fn multiline_quote_has_one_control_and_complete_range_for_all_its_prose() {
        for newline in ["\n", "\r\n", "\r"] {
            for prefix in ["", "- "] {
                let continuation = if prefix.is_empty() { "" } else { "  " };
                let quote = [
                    format!("{prefix}> first **中😀**"),
                    format!("{continuation}> continuation"),
                    format!("{continuation}>"),
                    format!("{continuation}> second paragraph"),
                    format!("{continuation}> > nested"),
                    format!("{continuation}> > continuation inside"),
                    String::new(),
                ]
                .join(newline);
                let text = format!("{quote}{newline}outside{newline}{newline}> separate{newline}");
                let first = block(&text, "first");
                assert_eq!(first.kind, BlockKind::Quote);
                assert_eq!(&text[first.range.clone()], quote);
                for needle in [
                    "中😀",
                    "continuation",
                    "second paragraph",
                    "nested",
                    "continuation inside",
                ] {
                    assert_eq!(block(&text, needle), first, "{needle}, newline={newline:?}");
                }
                assert_ne!(block(&text, "outside"), first);
                assert_ne!(block(&text, "separate"), first);
            }
        }
    }

    #[test]
    fn list_control_is_shared_by_all_items_including_nested_tasks() {
        let text = "- outer\n  - [ ] pending\n  - [x] done\n- ordinary\n\noutside\n";
        let outer = block(text, "outer");
        assert_eq!(
            outer.kind,
            BlockKind::List {
                ordered: false,
                task: false
            }
        );
        assert!(outer.is_list());
        assert_eq!(
            &text[outer.range.clone()],
            text.split("outside").next().unwrap()
        );
        for needle in ["pending", "done", "ordinary"] {
            assert_eq!(block(text, needle), outer);
        }
        assert!(!block(text, "outside").is_list());
        let task = "- [ ] pending\n- ordinary\n";
        assert_eq!(block(task, "pending"), block(task, "ordinary"));
        assert_eq!(
            block(task, "pending").kind,
            BlockKind::List {
                ordered: false,
                task: true
            }
        );
        assert_eq!(
            block(task, "pending").kind,
            BlockKind::List {
                ordered: false,
                task: true
            }
        );
    }

    #[test]
    fn whole_list_ranges_preserve_containers_and_physical_line_endings() {
        for newline in ["\n", "\r\n", "\r"] {
            let list = ["> 9. first", ">    - nested", "> 10. second", ""].join(newline);
            let text = format!("{list}{newline}outside{newline}");
            let first = block(&text, "first");
            for needle in ["nested", "second"] {
                assert_eq!(block(&text, needle), first);
            }
            assert_eq!(
                first.kind,
                BlockKind::List {
                    ordered: true,
                    task: false
                }
            );
            assert_eq!(&text[first.range.clone()], list);
            assert_eq!(&text[first.scope.clone()], list);
            assert_eq!(first.anchor, text.find("first").unwrap());
        }
    }

    #[test]
    fn empty_items_share_a_list_control_but_separate_lists_do_not() {
        let text = "- first\n- \n\noutside\n\n1. second\n2. third\n";
        let parse = ParseSnapshot::new(Source::new(text));
        let first = block(text, "first");
        let empty = text.find("- \n").unwrap() + 2;
        assert_eq!(at(&parse, empty).unwrap(), first);
        let second = block(text, "second");
        assert_eq!(second, block(text, "third"));
        assert!(first.range.end < second.range.start);
        assert_eq!(
            second.kind,
            BlockKind::List {
                ordered: true,
                task: false
            }
        );
    }

    #[test]
    fn nested_block_scope_retains_prefix_and_line_endings() {
        let text = "> - outer\r\n>   - inner\r\n>\r\n>   ```rs\r\n>   literal\r\n>   ```\r\n";
        for needle in ["inner", "literal"] {
            let b = block(text, needle);
            assert_eq!(&text[b.scope], text);
            assert!(b.range.contains(&text.find(needle).unwrap()));
        }
        assert_eq!(
            block(text, "inner").kind,
            BlockKind::List {
                ordered: false,
                task: false
            }
        );
        assert_eq!(block(text, "literal").kind, BlockKind::Code);
    }

    #[test]
    fn blank_separators_do_not_inherit_the_previous_block() {
        let text = "# heading\n\nparagraph";
        let parse = ParseSnapshot::new(Source::new(text));
        assert!(at(&parse, text.find("\n\n").unwrap() + 1).is_none());
        assert_eq!(at(&parse, text.len()).unwrap().kind, BlockKind::Paragraph);
    }

    #[test]
    fn physical_line_endings_do_not_change_block_identity() {
        for newline in ["\n", "\r\n", "\r"] {
            let text = [
                "# first",
                "",
                "```rs",
                "literal",
                "```",
                "",
                "## second",
                "",
            ]
            .join(newline);
            assert_eq!(block(&text, "second").kind, BlockKind::Heading(2));
            assert_eq!(block(&text, "literal").kind, BlockKind::Code);
        }
    }
}
