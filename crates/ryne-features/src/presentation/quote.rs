use super::*;
pub(super) type QuoteRegion = (usize, Range<usize>, usize, usize);
pub(super) fn regions(
    parse: &ParseSnapshot,
    source_mode: bool,
    block_window: Option<&Range<usize>>,
) -> Vec<QuoteRegion> {
    if source_mode {
        return Vec::new();
    }
    let items = ryne_syntax::SemanticQuery::new(parse).list_items();
    parse
        .index()
        .quote_nodes
        .iter()
        .map(|&id| (id, &parse.nodes[id]))
        .filter_map(|(id, node)| {
            if source_mode
                || node.kind != NodeKind::BlockQuote
                || node.children.is_empty()
                || block_window
                    .is_some_and(|w| w.start < node.envelope.end && node.envelope.start < w.end)
            {
                return None;
            }
            let mut in_alert = ryne_syntax::alert::for_owner(parse, id).is_some();
            let mut depth = 1;
            let mut inset = 0;
            let mut parent = node.parent;
            while let Some(p) = parent {
                in_alert |= ryne_syntax::alert::for_owner(parse, p).is_some();
                if parse.nodes[p].kind == NodeKind::BlockQuote {
                    depth += 1;
                }
                if let Some(item) = items
                    .binary_search_by_key(&p, |item| item.id)
                    .ok()
                    .map(|i| &items[i])
                {
                    inset = inset.max(item.inset);
                }
                parent = parse.nodes[p].parent;
            }
            Some((
                id,
                node.envelope.clone(),
                depth,
                inset + depth * 32 - if in_alert { 12 } else { 0 },
            ))
        })
        .collect::<Vec<_>>()
}
pub(super) fn prefixes(parse: &ParseSnapshot, quotes: &[QuoteRegion]) -> BlockContribution {
    let mut patch = BlockContribution::default();
    // Only parser-owned quote prefixes fold; `>` in code or ordinary text is literal.
    let owners: std::collections::HashSet<_> = quotes.iter().map(|q| q.0).collect();
    for token in &parse.syntax.tokens {
        if token.kind == TokenKind::ContainerPrefix && owners.contains(&token.owner) {
            patch.hide(token.range.clone(), true);
        }
    }
    patch
}

pub(super) fn alert_view(
    parse: &ParseSnapshot,
    owner: usize,
    window: Option<&Range<usize>>,
) -> Option<AlertView> {
    let source = ryne_syntax::alert::for_owner(parse, owner)?.clone();
    let folded =
        !window.is_some_and(|w| w.start < source.header.end && source.header.start < w.end);
    Some(AlertView { source, folded })
}
pub(super) fn alert_headers(
    parse: &ParseSnapshot,
    quotes: &[QuoteRegion],
    window: Option<&Range<usize>>,
) -> BlockContribution {
    let mut patch = BlockContribution::default();
    for (id, _, _, _) in quotes {
        if let Some(alert) = alert_view(parse, *id, window)
            && alert.folded
        {
            patch.hide(alert.source.header, true);
        }
    }
    patch
}
