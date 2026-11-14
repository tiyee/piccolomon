// Test-only pre-repair implementation. Never used by production queries.
use super::*;
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
        NodeKind::BlockQuote => crate::alert::for_owner(parse, id)
            .map_or(BlockKind::Quote, |a| BlockKind::Alert(a.kind)),
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
