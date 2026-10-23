use super::*;
pub(super) fn select(
    parse: &ParseSnapshot,
    tables: &[table::Table],
    source_mode: bool,
    block_window: Option<&Range<usize>>,
) -> Vec<list::Row> {
    if source_mode {
        return Vec::new();
    }
    ryne_syntax::SemanticQuery::new(parse)
        .list_rows()
        .iter()
        .filter(|row| {
            !source_mode
                && !tables.iter().any(|table| {
                    table.range.start < row.line.end && row.line.start < table.range.end
                })
                && !block_window
                    .is_some_and(|w| w.start < row.item.range.end && row.item.range.start < w.end)
        })
        .cloned()
        .collect::<Vec<_>>()
}
pub(super) fn prefixes(rows: &[list::Row]) -> BlockContribution {
    let mut patch = BlockContribution::default();
    for row in rows {
        for range in &row.hidden {
            patch.hide(range.clone(), true);
        }
    }
    patch
}
