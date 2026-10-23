use crate::Display;

/// Exact display-row starts mapped to their first owning code block.
/// Borrowing the display prevents the index from surviving mutations of its rows.
pub struct CodeRowIndex<'a> {
    rows: Vec<(usize, usize)>,
    _display: &'a Display,
}

impl<'a> CodeRowIndex<'a> {
    pub(crate) fn new(display: &'a Display) -> Self {
        let mut rows = display
            .code_blocks
            .iter()
            .enumerate()
            .flat_map(|(block, view)| view.rows.iter().map(move |row| (row.start, block)))
            .collect::<Vec<_>>();
        // Sorting both fields preserves code_row's first-block precedence even
        // when callers supply overlapping or unordered rows.
        rows.sort_unstable();
        rows.dedup_by_key(|row| row.0);
        Self {
            rows,
            _display: display,
        }
    }

    pub fn code_row(&self, start: usize) -> Option<usize> {
        self.rows
            .binary_search_by_key(&start, |row| row.0)
            .ok()
            .map(|index| self.rows[index].1)
    }
}

#[cfg(test)]
mod tests {
    use crate::{CodeView, Display};
    use ryne_syntax::code_block::CodeBlock;
    use std::ops::Range;

    fn view(rows: Vec<Range<usize>>) -> CodeView {
        CodeView {
            source: CodeBlock {
                range: 0..0,
                scope: 0..0,
                rows: Vec::new(),
                language: String::new(),
                closing: None,
                anchor: 0,
                indent: 0,
                continuation: String::new(),
            },
            rows,
            diagnostic: None,
        }
    }

    #[test]
    fn matches_scan_for_empty_duplicate_unordered_and_non_start_positions() {
        let display = Display {
            code_blocks: vec![
                view(vec![12..15, 4..4, 8..11, 8..11]),
                view(Vec::new()),
                view(vec![0..3, 8..11, 20..20]),
            ],
            ..Display::default()
        };
        let index = display.code_row_index();
        for position in 0..=24 {
            assert_eq!(index.code_row(position), display.code_row(position));
        }
        assert_eq!(index.code_row(8), Some(0));
        assert_eq!(index.code_row(4), Some(0));
        assert_eq!(index.code_row(20), Some(2));
        assert_eq!(index.code_row(9), None);
        assert_eq!(Display::default().code_row_index().code_row(0), None);
    }

    #[test]
    fn matches_scan_for_long_code_and_rebuilt_display() {
        let mut display = Display {
            code_blocks: vec![view((0..10_000).map(|i| i * 4..i * 4 + 3).collect())],
            ..Display::default()
        };
        {
            let index = display.code_row_index();
            for position in [0, 1, 4, 20_000, 39_996, 39_997, 40_000] {
                assert_eq!(index.code_row(position), display.code_row(position));
            }
        }
        display.code_blocks[0].rows = std::iter::once(40_000..40_000).collect();
        let index = display.code_row_index();
        assert_eq!(index.code_row(0), None);
        assert_eq!(index.code_row(40_000), Some(0));
    }
}
