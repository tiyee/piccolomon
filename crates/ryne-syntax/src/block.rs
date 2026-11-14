//! Block controls refer to the same semantic AST and source as the editor.
use crate::ParseSnapshot;
use crate::{code_block, list, table};
use ryne_parser::NodeKind;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    List { ordered: bool, task: bool },
    Quote,
    Alert(crate::alert::Kind),
    Code,
    Mermaid,
    Formula,
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
    // The first editable position in a container can belong to a child panel.
    if matches!(
        block.kind,
        BlockKind::List { .. } | BlockKind::Quote | BlockKind::Alert(_)
    ) && outermost_at(parse, block.range.start).as_ref() == Some(block)
    {
        return Some((block.range.clone(), String::new()));
    }
    if at(parse, block.anchor).as_ref() != Some(block) {
        return None;
    }
    let node = parse
        .index()
        .blocks
        .at(block.anchor)
        .into_iter()
        .map(|id| &parse.nodes[id])
        .filter(|node| {
            !matches!(node.kind, NodeKind::Item)
                && block.range.start <= node.envelope.start
                && node.envelope.start <= block.anchor
                && node.envelope.end <= block.range.end
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
    insertion_context(parse, block).is_some_and(|(range, _)| {
        parse
            .semantic()
            .index()
            .first_content
            .is_some_and(|first| first < range.start)
    })
}

/// The upper-left type menu belongs to a direct child of the document.
/// Use physical lines for leading indentation and the panel's full source range
/// for trailing fence newlines. Sibling nodes are in source order.
pub fn outermost_at(parse: &ParseSnapshot, caret: usize) -> Option<Block> {
    let parse = parse.semantic();
    let text = parse.source().text();
    if !text.is_char_boundary(caret) {
        return None;
    }
    let line_end = parse
        .index()
        .line(caret)
        .map_or(text.len(), |line| line.end);
    let siblings = &parse.nodes.first()?.children;
    let end = siblings.partition_point(|&id| parse.nodes[id].envelope.start < line_end);
    let &id = siblings[..end].last()?;
    let node = &parse.nodes[id];
    let block = match node.kind {
        NodeKind::List(_) | NodeKind::BlockQuote => block_from_node(parse, id),
        _ => at(parse, node.envelope.start)?,
    };
    (block.range.contains(&caret) || (caret == text.len() && caret == block.range.end))
        .then_some(block)
}

/// Editing commands can address individual semantic blocks inside containers.
pub fn at(parse: &ParseSnapshot, caret: usize) -> Option<Block> {
    #[cfg(any(test, feature = "query-metrics"))]
    crate::query_metrics::record(|m| m.queries += 1);
    let parse = parse.semantic();
    let text = parse.source().text();
    if !text.is_char_boundary(caret) {
        return None;
    }
    let before = |end: usize| {
        #[cfg(any(test, feature = "query-metrics"))]
        crate::query_metrics::record(|m| m.comparisons += 1);
        end < caret || (end == caret && caret != text.len())
    };
    let contains = |r: &Range<usize>| r.contains(&caret) || (caret == text.len() && caret == r.end);
    let codes = code_block::cached(parse);
    if let Some(code) = codes
        .get(codes.partition_point(|b| before(b.range.end)))
        .filter(|b| contains(&b.range))
    {
        return Some(Block {
            range: code.range.clone(),
            scope: code.scope.clone(),
            anchor: code.anchor,
            kind: if code.language.split_whitespace().next() == Some("mermaid") {
                BlockKind::Mermaid
            } else {
                BlockKind::Code
            },
        });
    }
    let formulas = crate::math::cached(parse);
    if let Some(formula) = formulas
        .get(formulas.partition_point(|f| before(f.range.end)))
        .filter(|f| f.block && contains(&f.range))
    {
        let mut block = block_from_node(parse, formula.node);
        block.anchor = formula.content.start;
        return Some(block);
    }
    let tables = table::cached(parse);
    if let Some(table) = tables
        .get(tables.partition_point(|t| before(t.range.end)))
        .filter(|t| contains(&t.range))
    {
        return Some(Block {
            range: table.range.clone(),
            scope: table.scope.clone(),
            anchor: table.rows.first()?.cells.first()?.content.start,
            kind: BlockKind::Table,
        });
    }
    let candidates = parse.index().blocks.at(caret);
    #[cfg(any(test, feature = "query-metrics"))]
    crate::query_metrics::record(|m| m.candidates += candidates.len());
    let (id, node) = candidates
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
    // Editing commands treat prose as part of its list or quote and can
    // address headings, code and tables individually inside those containers.
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
        // Include the entire quotation when targeting its prose.
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
        && let Some(item) = list::cached_items(parse)
            .binary_search_by_key(&target, |i| i.id)
            .ok()
            .map(|i| &list::cached_items(parse)[i])
    {
        // Target the complete list, including nested lists of a different kind.
        let mut list = item.list;
        let mut ancestor = parse.nodes[list].parent;
        while let Some(id) = ancestor {
            if matches!(parse.nodes[id].kind, NodeKind::List(_)) {
                list = id;
            }
            ancestor = parse.nodes[id].parent;
        }
        target = list;
    }
    Some(block_from_node(parse, target))
}

fn block_from_node(parse: &ParseSnapshot, id: usize) -> Block {
    let node = &parse.nodes[id];
    let mut anchor = node.envelope.start;
    let kind = match node.kind {
        NodeKind::List(number) => {
            let (first_body, task) = parse
                .block_directory()
                .lists
                .get(&id)
                .copied()
                .unwrap_or((anchor, false));
            anchor = first_body;
            BlockKind::List {
                ordered: number.is_some(),
                task,
            }
        }
        NodeKind::Heading(level) => BlockKind::Heading(level),
        NodeKind::BlockQuote => crate::alert::for_owner(parse, id)
            .map_or(BlockKind::Quote, |a| BlockKind::Alert(a.kind)),
        NodeKind::CodeBlock(_) => BlockKind::Code,
        NodeKind::Table(_) => BlockKind::Table,
        NodeKind::DisplayMath(_) => BlockKind::Formula,
        NodeKind::HtmlBlock | NodeKind::SourceBlock => BlockKind::Raw,
        NodeKind::Rule => BlockKind::Rule,
        _ if node.children.len() == 1
            && matches!(parse.nodes[node.children[0]].kind, NodeKind::DisplayMath(_)) =>
        {
            BlockKind::Formula
        }
        _ if node.children.len() == 1
            && matches!(parse.nodes[node.children[0]].kind, NodeKind::Image { .. }) =>
        {
            BlockKind::Image
        }
        _ => BlockKind::Paragraph,
    };
    let line_start = |position| {
        parse
            .index()
            .line(position)
            .map_or(position, |line| line.start)
    };
    let end = if kind == BlockKind::Formula {
        let formula = if matches!(node.kind, NodeKind::DisplayMath(_)) {
            node
        } else {
            &parse.nodes[node.children[0]]
        };
        anchor = formula.envelope.start + 2;
        parse
            .index()
            .line(formula.envelope.end.saturating_sub(1))
            .map_or(node.envelope.end, |line| line.end)
    } else {
        node.envelope.end
    };
    let range = line_start(node.envelope.start)..end;
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
    Block {
        range,
        scope,
        anchor,
        kind,
    }
}

/// Only list-wide metadata needs a new directory; ordinary blocks use the
/// existing range/line indexes and their current ancestor chain.
#[derive(Default)]
pub(crate) struct BlockDirectory {
    /// First body anchor and presence of a task in the list's own items.
    lists: std::collections::BTreeMap<usize, (usize, bool)>,
}
impl BlockDirectory {
    pub(crate) fn new(parse: &ParseSnapshot) -> Self {
        #[cfg(any(test, feature = "query-metrics"))]
        crate::query_metrics::record(|m| m.directories += 1);
        let mut result = Self::default();
        for item in list::cached_items(parse) {
            if parse.nodes[item.list].children.first() == Some(&item.id) {
                result.lists.insert(item.list, (item.body, false));
            }
        }
        let mut stack: Vec<_> = parse
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.parent.is_none())
            .map(|(id, _)| (id, None))
            .collect();
        while let Some((id, item)) = stack.pop() {
            let node = &parse.nodes[id];
            if matches!(node.kind, NodeKind::Task(_))
                && let Some(list) = item.and_then(|i: usize| parse.nodes[i].parent)
            {
                result
                    .lists
                    .entry(list)
                    .or_insert((parse.nodes[list].envelope.start, false))
                    .1 = true;
            }
            let item = if node.kind == NodeKind::Item {
                Some(id)
            } else {
                item
            };
            stack.extend(node.children.iter().rev().map(|&child| (child, item)));
        }
        #[cfg(any(test, feature = "query-metrics"))]
        crate::query_metrics::record(|m| {
            m.records += result.lists.len();
            m.record_payload_bytes +=
                result.lists.len() * std::mem::size_of::<(usize, (usize, bool))>();
        });
        result
    }
}

#[cfg(test)]
#[path = "block_oracle.rs"]
mod oracle;

#[cfg(test)]
mod indexed_tests {
    use super::*;
    use ryne_parser::Source;
    #[test]
    fn warm_queries_do_not_rebuild_snapshot_indexes_and_snapshots_release() {
        for unit in [
            "plain\n\n",
            "- first\n  - [ ] nested\n- end\n\n",
            "> a\n>\n> > b\n\n",
            "```\nx\n```\n\n",
            "| a |\n| --- |\n| b |\n\n",
        ] {
            let parse =
                std::sync::Arc::new(ParseSnapshot::with_math(Source::new(&unit.repeat(1000))));
            let weak = std::sync::Arc::downgrade(&parse);
            at(&parse, 0);
            at(&parse, parse.source().text().len() - 3);
            let before = crate::query_metrics::snapshot();
            for _ in 0..100 {
                for position in [
                    0,
                    parse.source().text().len() / 2,
                    parse.source().text().len() - 3,
                ] {
                    at(&parse, position);
                }
            }
            let after = crate::query_metrics::snapshot();
            assert_eq!(after.directories, before.directories);
            assert_eq!(after.semantic_indexes, before.semantic_indexes);
            assert_eq!(after.physical_lines, before.physical_lines);
            assert_eq!(after.queries - before.queries, 300);
            drop(parse);
            assert!(weak.upgrade().is_none());
        }
    }
    #[test]
    fn block_and_insertion_queries_at_every_character_boundary() {
        let samples = [
            "", "\n\n", "# a\n\ntext\n", "title\n===\n\n![alt](x)\n\n---\n\n<div>raw</div>\n",
            "- first\n  - [ ] nested\n- last\n\noutside\n\n1. next\n2. end\n",
            "> first\n>\n> > nested\n> > text\n>\n> ## heading\n>\n> - item\n>   - [x] task\n\ntail\n",
            "> - a\n>   > quote\n>   >\n>   > second\n>\n> - ```rust\n>   literal\n>   ```\n",
            "- ```\n  ```\n- | a | b |\n  | --- | --- |\n  | | $x$ |\n\n    indent\n",
            "## 中😀e\u{301}\n\nplain\n\n~~~mermaid\ngraph LR\nA-->B\n~~~\n\n```unclosed\n",
        ];
        for raw in samples {
            for newline in ["\n", "\r\n", "\r"] {
                let raw = raw.replace('\n', newline);
                let parse = ParseSnapshot::with_math(Source::new(&raw));
                for caret in raw
                    .char_indices()
                    .map(|(p, _)| p)
                    .chain(std::iter::once(raw.len()))
                {
                    let expected = oracle::at(&parse, caret);
                    assert_eq!(at(&parse, caret), expected, "{raw:?} at {caret}");
                    if let Some(block) = expected {
                        // The menu now allows insertion beside the whole container
                        // even when its first editable anchor is inside a child.
                        if matches!(
                            block.kind,
                            BlockKind::List { .. } | BlockKind::Quote | BlockKind::Alert(_)
                        ) && outermost_at(&parse, caret).as_ref() == Some(&block)
                        {
                            assert_eq!(
                                insertion_context(&parse, &block),
                                Some((block.range.clone(), String::new()))
                            );
                            assert_eq!(
                                can_insert_above(&parse, &block),
                                !raw[..block.range.start].trim().is_empty()
                            );
                            continue;
                        }
                        assert_eq!(
                            insertion_context(&parse, &block),
                            oracle::insertion_context(&parse, &block),
                            "{raw:?} at {caret}"
                        );
                        assert_eq!(
                            can_insert_above(&parse, &block),
                            oracle::can_insert_above(&parse, &block)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn deep_container_context_matches_original_without_query_rebuilds() {
        for depth in [1, 8, 32] {
            let quote = "> ".repeat(depth);
            let list = (0..depth)
                .map(|d| format!("{}- level {d}\n", "  ".repeat(d)))
                .collect::<String>();
            for raw in [format!("{quote}first\n{quote}\n{quote}## heading\n{quote}\n{quote}| a |\n{quote}| --- |\n{quote}| $x$ |\n\nend"),format!("{list}\nend")] {
                let parse=ParseSnapshot::with_math(Source::new(&raw));
                for p in raw.char_indices().map(|(p,_)|p).chain(std::iter::once(raw.len())) {
                    assert_eq!(at(&parse,p),oracle::at(&parse,p),"depth {depth}, offset {p}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ryne_parser::Source;

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
            let parse = ParseSnapshot::new(Source::new(text));
            let caret = text.find(needle).unwrap();
            let inner = at(&parse, caret).unwrap();
            assert_eq!(inner.kind, kind, "{text}");
            assert_eq!(outermost_at(&parse, caret), Some(inner), "{text}");
        }
    }

    #[test]
    fn nested_blocks_share_the_document_childs_menu_at_every_source_position() {
        for ending in ["\n", "\r\n", "\r"] {
            for (body, kind) in [
                (
                    "  9. ### title\n     > > 中😀\n\n     | col |\n     | --- |\n     | cell |\n\n  10. last",
                    BlockKind::List { ordered: true, task: false },
                ),
                (
                    "> - [ ] task\n>   - nested\n>\n> ```mermaid\n> graph LR\n> ```\n>\n> ![alt](url)",
                    BlockKind::Quote,
                ),
                ("- ```rs\n  literal\n  ```", BlockKind::List { ordered: false, task: false }),
                ("> | col |\n> | --- |\n> | cell |", BlockKind::Quote),
            ] {
                let body = body.replace('\n', ending);
                let prefix = format!("before{ending}{ending}");
                let text = format!("{prefix}{body}{ending}{ending}after");
                let parse = ParseSnapshot::with_math(Source::new(&text));
                let owner = outermost_at(&parse, prefix.len()).expect("missing outer container");
                assert_eq!(owner.kind, kind);
                assert_eq!(owner.range.start, prefix.len());
                assert_eq!(owner.range, owner.scope);
                for (byte, _) in body.char_indices() {
                    assert_eq!(outermost_at(&parse, prefix.len() + byte).as_ref(), Some(&owner), "{text:?}, {byte}");
                }
                assert_eq!(outermost_at(&parse, text.len()).unwrap().kind, BlockKind::Paragraph);
                assert_eq!(outermost_at(&parse, prefix.len() - 1), None);
            }
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
