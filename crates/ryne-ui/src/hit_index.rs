//! Source-order preserving geometric candidates, owned by one immutable layout.
use crate::visibility::YIndex;
use gpui::{Bounds, Pixels};
use std::collections::HashMap;

struct CellBand {
    bounds: Bounds<Pixels>,
    rows: Vec<usize>,
}
#[derive(Default)]
pub(super) struct HitIndex {
    cells: Vec<CellBand>,
    cell_y: YIndex,
    ordinary: YIndex,
    starts: Vec<(Pixels, usize)>,
    ends: Vec<(Pixels, usize)>,
}
pub(super) struct HitRow {
    pub row: Option<usize>,
    pub in_cell: bool,
    pub visited: usize,
}
impl HitIndex {
    pub fn new(
        rows: impl Iterator<Item = (usize, Pixels, Pixels, Option<Bounds<Pixels>>)>,
    ) -> Self {
        let mut index = Self::default();
        let mut cells = HashMap::new();
        let mut ordinary = Vec::new();
        for (id, top, bottom, cell) in rows {
            if let Some(bounds) = cell {
                let key = [
                    bounds.left(),
                    bounds.top(),
                    bounds.size.width,
                    bounds.size.height,
                ]
                .map(|n| {
                    if n == gpui::px(0.) {
                        0
                    } else {
                        f32::from(n).to_bits()
                    }
                });
                let cell = *cells.entry(key).or_insert_with(|| {
                    let id = index.cells.len();
                    index.cells.push(CellBand {
                        bounds,
                        rows: Vec::new(),
                    });
                    id
                });
                index.cells[cell].rows.push(id);
            } else {
                ordinary.push((id, top, bottom));
            }
        }
        index.starts = ordinary.iter().map(|&(id, t, _)| (t, id)).collect();
        index.ends = ordinary.iter().map(|&(id, _, b)| (b, id)).collect();
        for edges in [&mut index.starts, &mut index.ends] {
            edges.sort_by(|a, b| {
                a.0.partial_cmp(&b.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.1.cmp(&b.1))
            });
            edges.dedup_by(|a, b| a.0 == b.0);
        }
        index.ordinary = YIndex::new(ordinary.into_iter());
        index.cell_y = YIndex::new(
            index
                .cells
                .iter()
                .enumerate()
                .map(|(i, c)| (i, c.bounds.top(), c.bounds.bottom())),
        );
        index
    }
    pub fn locate(
        &self,
        x: Pixels,
        y: Pixels,
        bounds: impl Fn(usize) -> (Pixels, Pixels),
    ) -> HitRow {
        let cells = self.cell_y.visible(y, y);
        let mut visited = cells.len();
        let cell = cells
            .into_iter()
            .filter_map(|i| self.cells.get(i))
            .filter(|c| c.bounds.top() <= y && y < c.bounds.bottom())
            .min_by(|a, b| {
                let distance = |c: &CellBand| {
                    (c.bounds.left() - x).max(gpui::px(0.))
                        + (x - c.bounds.right()).max(gpui::px(0.))
                };
                distance(a)
                    .partial_cmp(&distance(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        if let Some(cell) = cell {
            let i = cell.rows.partition_point(|&r| bounds(r).1 <= y);
            return HitRow {
                row: cell.rows.get(i).or_else(|| cell.rows.last()).copied(),
                in_cell: true,
                visited: visited + 1,
            };
        }
        let mut candidates = self.ordinary.visible(y, y);
        if let Some(&(_, id)) = self
            .starts
            .get(self.starts.partition_point(|&(t, _)| t <= y))
        {
            candidates.push(id);
        }
        if let Some(i) = self.ends.partition_point(|&(b, _)| b <= y).checked_sub(1) {
            candidates.push(self.ends[i].1);
        }
        candidates.sort_unstable();
        candidates.dedup();
        visited += candidates.len();
        let row = candidates.into_iter().min_by(|&a, &b| {
            let distance = |id| {
                let (top, bottom) = bounds(id);
                (top - y).max(gpui::px(0.)) + (y - bottom).max(gpui::px(0.))
            };
            let inside = |id| {
                let (top, bottom) = bounds(id);
                top <= y && y < bottom
            };
            inside(b).cmp(&inside(a)).then_with(|| {
                distance(a)
                    .partial_cmp(&distance(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        });
        HitRow {
            row,
            in_cell: false,
            visited,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size};
    #[test]
    fn overlap_padding_gaps_ties_and_long_documents_match_linear_selection() {
        let cell_a = Bounds::new(point(px(0.), px(40.)), size(px(100.), px(100.)));
        let cell_b = Bounds::new(point(px(100.), px(40.)), size(px(100.), px(100.)));
        let mut rows = vec![
            (px(0.), px(30.), None),
            (px(48.), px(78.), Some(cell_a)),
            (px(78.), px(128.), Some(cell_a)),
            (px(48.), px(78.), Some(cell_b)),
            (px(160.), px(190.), None),
        ];
        rows.extend(
            (0..10000).map(|i| (px(210. + i as f32 * 30.), px(240. + i as f32 * 30.), None)),
        );
        let index = HitIndex::new(rows.iter().enumerate().map(|(i, &(t, b, c))| (i, t, b, c)));
        for y in [
            -5., 0., 30., 35., 40., 47., 48., 77., 78., 128., 139.9, 140., 150., 160., 190., 250.,
            300209., 300250.,
        ] {
            for x in [-10., 50., 100., 150., 210.] {
                let y = px(y);
                let x = px(x);
                let cell = rows
                    .iter()
                    .filter_map(|r| r.2)
                    .filter(|c| c.top() <= y && y < c.bottom())
                    .min_by(|a, b| {
                        let d = |c: &Bounds<Pixels>| {
                            (c.left() - x).max(px(0.)) + (x - c.right()).max(px(0.))
                        };
                        d(a).partial_cmp(&d(b)).unwrap()
                    });
                let expected = if cell.is_some() {
                    rows.iter()
                        .enumerate()
                        .find(|(_, r)| r.2 == cell && y < r.1)
                        .or_else(|| rows.iter().enumerate().rev().find(|(_, r)| r.2 == cell))
                        .map(|(i, _)| i)
                } else {
                    rows.iter()
                        .enumerate()
                        .filter(|(_, r)| r.2.is_none())
                        .min_by(|(_, a), (_, b)| {
                            let inside =
                                |r: &(Pixels, Pixels, Option<Bounds<Pixels>>)| r.0 <= y && y < r.1;
                            let d = |r: &(Pixels, Pixels, Option<Bounds<Pixels>>)| {
                                (r.0 - y).max(px(0.)) + (y - r.1).max(px(0.))
                            };
                            inside(b)
                                .cmp(&inside(a))
                                .then_with(|| d(a).partial_cmp(&d(b)).unwrap())
                        })
                        .map(|(i, _)| i)
                };
                let found = index.locate(x, y, |i| (rows[i].0, rows[i].1));
                assert_eq!(found.row, expected, "x={x:?}, y={y:?}");
                assert_eq!(found.in_cell, cell.is_some());
                assert!(found.visited <= 8, "visited {}", found.visited);
            }
        }
    }
}
