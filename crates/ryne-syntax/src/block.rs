//! Block controls refer to the same semantic AST and source as the editor.
use crate::{code_block, list, table};
use editable_markdown_parser::{NodeKind, ParseSnapshot};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub range: Range<usize>,
    /// Local source includes the complete enclosing container context.
    pub scope: Range<usize>,
    pub anchor: usize,
    /// Icon filename under `assets/icons`; None keeps the text label when no
    /// unambiguous asset exists.
    pub icon: Option<&'static str>,
    pub label: &'static str,
    pub name: &'static str,
}

impl Block {
    pub fn is_list(&self) -> bool {
        matches!(
            self.icon,
            Some("text_bullet_list_ltr" | "text_number_list_ltr" | "task_list_ltr")
        )
    }
}

/// Physical sibling insertion boundary and the existing container continuation.
/// A block sharing a list marker's line uses the full enclosing container so
/// insertion never separates that marker from its first child.
pub fn insertion_context(parse: &ParseSnapshot, block: &Block) -> Option<(Range<usize>, String)> {
    if let Some(view) = crate::parser::normalized(parse) {
        return insertion_context(&view, block);
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
        return at(&view, caret);
    }
    let contains = |range: &Range<usize>| {
        range.contains(&caret) || (caret == text.len() && caret == range.end)
    };
    if let Some(code) = code_block::blocks(parse)
        .into_iter()
        .find(|b| contains(&b.range))
    {
        let diagram = code.language.split_whitespace().next() == Some("mermaid");
        return Some(Block {
            range: code.range,
            scope: code.scope,
            anchor: code.anchor,
            icon: Some(if diagram { "mermaid" } else { "code" }),
            label: if diagram { "◇" } else { "</>" },
            name: if diagram { "Mermaid" } else { "代码块" },
        });
    }
    if let Some(table) = table::tables(parse)
        .into_iter()
        .find(|t| contains(&t.range))
    {
        let anchor = table.rows.first()?.cells.first()?.content.start;
        return Some(Block {
            range: table.range,
            scope: table.scope,
            anchor,
            icon: Some("table"),
            label: "▦",
            name: "表格",
        });
    }
    let (id, node) = parse
        .nodes
        .iter()
        .enumerate()
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
    let node = &parse.nodes[target];
    if node.kind == NodeKind::Item
        && let Some(item) = list::items(parse).into_iter().find(|i| i.id == target)
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
        let first = list::items(parse).into_iter().find(|i| i.list == list)?;
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
            scope: first.scope,
            anchor: first.body,
            icon: Some(if task {
                "task_list_ltr"
            } else if ordered {
                "text_number_list_ltr"
            } else {
                "text_bullet_list_ltr"
            }),
            label: if ordered { "1." } else { "•" },
            name: if task {
                "任务列表"
            } else if ordered {
                "有序列表"
            } else {
                "无序列表"
            },
        });
    }

    let (icon, label, name) = match node.kind {
        NodeKind::Heading(level) => (
            Some(match level {
                1 => "text_header_1",
                2 => "text_header_2",
                3 => "text_header_3",
                4 => "text_header_4",
                5 => "text_header_5",
                _ => "text_header_6",
            }),
            match level {
                1 => "H1",
                2 => "H2",
                3 => "H3",
                4 => "H4",
                5 => "H5",
                _ => "H6",
            },
            "标题",
        ),
        NodeKind::BlockQuote => (Some("text_quote"), "❯", "引用"),
        NodeKind::CodeBlock(_) => (Some("code"), "</>", "代码块"),
        NodeKind::Table(_) => (Some("table"), "▦", "表格"),
        NodeKind::HtmlBlock | NodeKind::SourceBlock => (Some("code"), "<>", "原始块"),
        NodeKind::Rule => (Some("divider"), "—", "分隔线"),
        _ if node.children.len() == 1
            && matches!(parse.nodes[node.children[0]].kind, NodeKind::Image { .. }) =>
        {
            (Some("image"), "IMG", "图片")
        }
        _ => (Some("text_t"), "¶", "段落"),
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
        icon,
        label,
        name,
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
        for (text, needle, label) in [
            ("###### title\n", "title", "H6"),
            ("title\n===\n", "title", "H1"),
            ("plain **bold**\n", "bold", "¶"),
            ("![alt](img.png)\n", "alt", "IMG"),
            ("> quote\n", "quote", "❯"),
            ("1. item\n", "item", "1."),
            ("- item\n", "item", "•"),
            ("```rust\n# literal\n```\n", "literal", "</>"),
            ("```mermaid\ngraph LR\n```\n", "graph", "◇"),
            ("| a | b |\n|---|---|\n| c | d |\n", "d |", "▦"),
        ] {
            assert_eq!(block(text, needle).label, label, "{text}");
        }
    }

    #[test]
    fn list_control_is_shared_by_all_items_including_nested_tasks() {
        let text = "- outer\n  - [ ] pending\n  - [x] done\n- ordinary\n\noutside\n";
        let outer = block(text, "outer");
        assert_eq!(outer.icon, Some("text_bullet_list_ltr"));
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
        assert_eq!(block(task, "pending").icon, Some("task_list_ltr"));
        assert_eq!(block(task, "pending").name, "任务列表");
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
            assert_eq!(first.icon, Some("text_number_list_ltr"));
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
        assert_eq!(second.icon, Some("text_number_list_ltr"));
    }

    #[test]
    fn nested_block_scope_retains_prefix_and_line_endings() {
        let text = "> - outer\r\n>   - inner\r\n>\r\n>   ```rs\r\n>   literal\r\n>   ```\r\n";
        for needle in ["inner", "literal"] {
            let b = block(text, needle);
            assert_eq!(&text[b.scope], text);
            assert!(b.range.contains(&text.find(needle).unwrap()));
        }
        assert_eq!(block(text, "inner").label, "•");
        assert_eq!(block(text, "literal").label, "</>");
    }

    #[test]
    fn blank_separators_do_not_inherit_the_previous_block() {
        let text = "# heading\n\nparagraph";
        let parse = ParseSnapshot::new(Source::new(text));
        assert!(at(&parse, text.find("\n\n").unwrap() + 1).is_none());
        assert_eq!(at(&parse, text.len()).unwrap().label, "¶");
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
            assert_eq!(block(&text, "second").label, "H2");
            assert_eq!(block(&text, "literal").label, "</>");
        }
    }
}
