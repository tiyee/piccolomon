//! Conservative Y interval queries preserve original draw order, including table row overlaps.
use gpui::Pixels;
#[derive(Default)]
pub(super) struct YIndex(ryne_syntax::RangeIndex);
fn floor(value: Pixels) -> usize {
    (f32::from(value) as f64 * 1024.).floor().max(0.) as usize
}
fn ceil(value: Pixels) -> usize {
    (f32::from(value) as f64 * 1024.).ceil().max(0.) as usize
}
impl YIndex {
    pub fn new(items: impl Iterator<Item = (usize, Pixels, Pixels)>) -> Self {
        Self(ryne_syntax::RangeIndex::new(
            items
                .map(|(id, top, bottom)| (floor(top)..ceil(bottom).saturating_add(1), id))
                .collect(),
        ))
    }
    pub fn visible(&self, top: Pixels, bottom: Pixels) -> Vec<usize> {
        self.0
            .overlapping(floor(top)..ceil(bottom).saturating_add(1))
    }
}
#[derive(Default)]
pub(super) struct Visibility {
    pub lines: YIndex,
    pub rows: YIndex,
    pub cells: YIndex,
    pub codes: YIndex,
    pub lists: YIndex,
    pub quotes: YIndex,
    pub leading: YIndex,
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;
    #[test]
    fn y_queries_preserve_overlap_order_fractional_edges_and_long_panels() {
        let items = [
            (0, px(0.), px(30.)),
            (1, px(30.), px(90.)),
            (2, px(30.), px(50.)),
            (3, px(0.125), px(10000.5)),
        ];
        let index = YIndex::new(items.into_iter());
        for (top, bottom) in [
            (0., 30.),
            (30., 31.),
            (49.5, 50.5),
            (9000., 9100.),
            (11000., 12000.),
        ] {
            let found = index.visible(px(top), px(bottom));
            let expected: Vec<_> = items
                .iter()
                .filter(|(_, a, b)| *a <= px(bottom) && px(top) <= *b)
                .map(|(i, _, _)| *i)
                .collect();
            assert_eq!(found, expected);
        }
    }
}
