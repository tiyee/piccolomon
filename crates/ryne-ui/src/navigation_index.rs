//! Table navigation targets owned by one immutable layout. Queries never scan rows.
use gpui::{Bounds, Pixels};
use std::collections::HashMap;

type Row = (usize, Pixels, Pixels, Option<Bounds<Pixels>>);
#[derive(Default)]
pub(super) struct NavigationIndex {
    targets: HashMap<usize, [Option<usize>; 2]>,
}
impl NavigationIndex {
    pub fn new(rows: impl Iterator<Item = Row>) -> Self {
        let rows = rows.collect::<Vec<_>>();
        let mut cells = HashMap::<[u32; 4], (Bounds<Pixels>, Vec<(Pixels, usize)>)>::new();
        for &(id, top, _, cell) in &rows {
            if let Some(cell) = cell {
                let key = [cell.left(), cell.top(), cell.size.width, cell.size.height].map(|n| {
                    if n == gpui::px(0.) {
                        0
                    } else {
                        f32::from(n).to_bits()
                    }
                });
                cells
                    .entry(key)
                    .or_insert_with(|| (cell, Vec::new()))
                    .1
                    .push((top, id));
            }
        }
        if cells.is_empty() {
            return Self::default();
        }
        let compare = |a: &(Pixels, usize), b: &(Pixels, usize)| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.1.cmp(&b.1))
        };
        let mut starts = rows
            .iter()
            .map(|&(id, top, _, _)| (top, id))
            .collect::<Vec<_>>();
        starts.sort_by(compare);
        let mut ends = rows
            .iter()
            .map(|&(id, top, bottom, _)| (bottom, (top, id)))
            .collect::<Vec<_>>();
        ends.sort_by(|a, b| compare(&(a.0, a.1 .1), &(b.0, b.1 .1)));
        // Eligibility is by bottom, but the old upward scan selects the maximum
        // top (last source row on ties). Sorting by bottom alone is insufficient.
        let mut best = None;
        let before = ends
            .iter()
            .map(|&(_, candidate)| {
                if best.is_none_or(|old| compare(&old, &candidate).is_lt()) {
                    best = Some(candidate);
                }
                best.map(|(_, id)| id)
            })
            .collect::<Vec<_>>();
        let mut targets = HashMap::new();
        for (_, (bounds, mut group)) in cells {
            group.sort_by(compare);
            let next = starts
                .get(starts.partition_point(|(top, _)| *top < bounds.bottom()))
                .map(|r| r.1);
            let previous = ends
                .partition_point(|(bottom, _)| *bottom <= bounds.top())
                .checked_sub(1)
                .and_then(|i| before[i]);
            // Preserve find/rfind source-order precedence even for tied tops.
            let mut prefix = Vec::with_capacity(group.len());
            let mut max = 0;
            for &(_, id) in &group {
                max = max.max(id);
                prefix.push(max);
            }
            let mut suffix = vec![0; group.len()];
            let mut min = usize::MAX;
            for (i, &(_, id)) in group.iter().enumerate().rev() {
                min = min.min(id);
                suffix[i] = min;
            }
            for &(top, id) in &group {
                let up = group
                    .partition_point(|(other, _)| *other < top)
                    .checked_sub(1)
                    .map(|i| prefix[i])
                    .or(previous);
                let down = suffix
                    .get(group.partition_point(|(other, _)| *other <= top))
                    .copied()
                    .or(next);
                targets.insert(id, [up, down]);
            }
        }
        Self { targets }
    }
    pub fn target(&self, row: usize, down: bool) -> Option<usize> {
        self.targets
            .get(&row)
            .and_then(|targets| targets[usize::from(down)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size};
    fn scan(rows: &[Row], index: usize, down: bool) -> Option<usize> {
        let (_, top, _, cell) = rows[index];
        let cell = cell?;
        let mut same = rows.iter().filter(|r| r.3 == Some(cell));
        let target = if down {
            same.find(|r| r.1 > top)
        } else {
            same.rfind(|r| r.1 < top)
        };
        target
            .or_else(|| {
                let outside = rows.iter().filter(|r| {
                    if down {
                        r.1 >= cell.bottom()
                    } else {
                        r.2 <= cell.top()
                    }
                });
                if down {
                    outside.min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                } else {
                    outside.max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                }
            })
            .map(|r| r.0)
    }
    #[test]
    fn table_targets_match_scan_with_wraps_height_ties_and_long_prefixes() {
        for count in [0, 100, 10000] {
            let mut rows = (0..count)
                .map(|i| (i, px(i as f32 * 30.), px(i as f32 * 30. + 30.), None))
                .collect::<Vec<_>>();
            let y = count as f32 * 30.;
            let a = Bounds::new(point(px(0.), px(y + 100.)), size(px(100.), px(120.)));
            let b = Bounds::new(point(px(100.), px(y + 100.)), size(px(100.), px(120.)));
            for (top, bottom, cell) in [
                (80., 100., None),
                (70., 95., None), // top and bottom orders differ
                (108., 138., Some(a)),
                (138., 190., Some(a)),
                (190., 215., Some(a)),
                (108., 140., Some(b)),
                (140., 170., Some(b)),
                (140., 185., Some(b)),
                (240., 270., None),
                (240., 260., None),
            ] {
                rows.push((rows.len(), px(y + top), px(y + bottom), cell));
            }
            let index = NavigationIndex::new(rows.iter().copied());
            for i in count..rows.len() {
                if rows[i].3.is_some() {
                    for down in [false, true] {
                        assert_eq!(index.target(i, down), scan(&rows, i, down));
                    }
                }
            }
            // Rebuild after resize/content changes; no old targets survive.
            let fresh = NavigationIndex::new(rows[..count].iter().copied());
            assert!(fresh.target(count + 2, true).is_none());
        }
    }
}
