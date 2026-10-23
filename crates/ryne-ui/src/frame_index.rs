//! Geometry indexes owned by the immutable text layout, never reused across versions.
use gpui::Pixels;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct FrameIndex {
    pub visual_rows: ryne_syntax::RangeIndex,
    pub first_line: HashMap<usize, usize>,
    pub code_lines: HashMap<usize, (usize, usize)>,
    pub item_bottom: HashMap<usize, Pixels>,
}
impl FrameIndex {
    pub fn build(
        lines: impl Iterator<Item = (usize, Option<usize>, Pixels)>,
        items: impl Iterator<Item = (usize, usize)>,
    ) -> Self {
        let mut index = Self::default();
        let mut bottoms = Vec::new();
        for (line, (start, code, bottom)) in lines.enumerate() {
            index.first_line.entry(start).or_insert(line);
            if let Some(code) = code {
                index
                    .code_lines
                    .entry(code)
                    .and_modify(|range| range.1 = line)
                    .or_insert((line, line));
            }
            bottoms.push(bottom);
        }
        for (item, start) in items {
            if let Some(&line) = index.first_line.get(&start) {
                let bottom = bottoms[line];
                index
                    .item_bottom
                    .entry(item)
                    .and_modify(|end| *end = (*end).max(bottom))
                    .or_insert(bottom);
            }
        }
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;
    #[test]
    fn indexes_first_matching_line_code_extents_and_multiline_items() {
        let index = FrameIndex::build(
            [
                (0, None, px(30.)),
                (8, Some(0), px(54.)),
                (16, Some(0), px(102.)), // wrapped code line
                (24, None, px(130.)),
                (24, None, px(160.)), // duplicate start retains first match
                (32, Some(2), px(184.)),
            ]
            .into_iter(),
            [(10, 0), (10, 16), (20, 24), (30, 999)].into_iter(),
        );
        assert_eq!(index.first_line[&24], 3);
        assert_eq!(index.code_lines[&0], (1, 2));
        assert_eq!(index.code_lines[&2], (5, 5));
        assert!(!index.code_lines.contains_key(&1));
        assert_eq!(index.item_bottom[&10], px(102.));
        assert_eq!(index.item_bottom[&20], px(130.));
        assert!(!index.item_bottom.contains_key(&30));
        let empty = FrameIndex::build(std::iter::empty(), std::iter::empty());
        assert!(empty.first_line.is_empty());
    }
}
