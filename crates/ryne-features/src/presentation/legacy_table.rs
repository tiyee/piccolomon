use super::*;
type Substitutions = Vec<(usize, Range<usize>, &'static str)>;
pub(super) fn select(
    all_tables: &[table::Table],
    source_mode: bool,
    block_window: Option<&Range<usize>>,
) -> Vec<table::Table> {
    all_tables
        .iter()
        .filter(|table| {
            !source_mode
                && !block_window
                    .is_some_and(|w| w.start < table.range.end && table.range.start < w.end)
        })
        .cloned()
        .collect::<Vec<_>>()
}
pub(super) fn contribute(text: &str, tables: &[table::Table]) -> BlockContribution {
    let mut patch = BlockContribution::default();
    for table in tables {
        patch.hide(table.range.clone(), true);
        for (r, row) in table.rows.iter().enumerate() {
            let end = row.line.start + text[row.line.clone()].trim_end_matches(['\r', '\n']).len();
            patch.hide(end..row.line.end, false);
            for cell in &row.cells {
                patch.hide(cell.content.clone(), false);
                if r == 0 {
                    patch.bold(cell.content.clone());
                }
            }
        }
    }
    patch
}
pub(super) fn substitutions(
    text: &str,
    tables: &[table::Table],
    styles: &[Style],
) -> (BlockContribution, Substitutions) {
    let mut patch = BlockContribution::default();
    let mut insertions = Vec::<(usize, Range<usize>, &str)>::new();
    for table in tables {
        for row in &table.rows {
            for (c, cell) in row.cells.iter().enumerate() {
                if c > 0 {
                    insertions.push((
                        cell.content.start,
                        row.cells[c - 1].content.end..cell.content.start,
                        "\t",
                    ));
                }
                // Escaped pipes are literal cell content, including inside code.
                let mut pos = cell.content.start;
                while pos < cell.content.end {
                    let rest = &text[pos..cell.content.end];
                    let br = ["<br>", "<br/>", "<br />"]
                        .into_iter()
                        .find(|br| rest.starts_with(br));
                    let escaped_pipe = rest.starts_with("\\|")
                        && text[cell.content.start..pos]
                            .bytes()
                            .rev()
                            .take_while(|b| *b == b'\\')
                            .count()
                            % 2
                            == 0;
                    if let Some(br) = br {
                        // A literal <br> in a code span stays literal.
                        if !styles[pos].code {
                            patch.hide(pos..pos + br.len(), true);
                            insertions.push((pos, pos..pos + br.len(), "\n"));
                            pos += br.len();
                            continue;
                        }
                    }
                    if escaped_pipe {
                        patch.hide(pos..pos + 2, true);
                        insertions.push((pos, pos..pos + 2, "|"));
                        pos += 2;
                    } else {
                        pos += text[pos..].chars().next().map_or(1, char::len_utf8);
                    }
                }
            }
        }
    }
    (patch, insertions)
}
