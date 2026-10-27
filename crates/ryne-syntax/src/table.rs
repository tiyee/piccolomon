use crate::list;
use crate::ParseSnapshot;
use editable_markdown_parser::{Alignment, NodeKind, TokenKind};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub content: Range<usize>,
}
pub(crate) struct CellLocation {
    range: Range<usize>,
    table: usize,
    row: usize,
    column: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub line: Range<usize>,
    pub cells: Vec<Cell>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub range: Range<usize>,
    pub scope: Range<usize>,
    pub rows: Vec<Row>,
    pub align: Vec<Alignment>,
    pub continuation: String,
    pub inset: usize,
}

impl Table {
    pub fn cell_at(&self, position: usize) -> (usize, usize) {
        let row = self.rows.partition_point(|row| {
            row.cells
                .last()
                .is_some_and(|cell| cell.content.end < position)
        });
        let next = self.rows.get(row).and_then(|r| {
            let col = r.cells.partition_point(|cell| cell.content.end < position);
            r.cells.get(col).map(|cell| (row, col, cell))
        });
        let previous = match next {
            Some((r, c, _)) if c > 0 => self.rows[r].cells.get(c - 1).map(|cell| (r, c - 1, cell)),
            Some((r, _, _)) if r > 0 => self.rows[r - 1]
                .cells
                .last()
                .map(|cell| (r - 1, self.rows[r - 1].cells.len() - 1, cell)),
            None => self.rows.last().and_then(|r| {
                r.cells
                    .last()
                    .map(|cell| (self.rows.len() - 1, r.cells.len() - 1, cell))
            }),
            _ => None,
        };
        let distance = |cell: &Cell| {
            if position < cell.content.start {
                cell.content.start - position
            } else {
                position.saturating_sub(cell.content.end)
            }
        };
        previous
            .into_iter()
            .chain(next)
            .min_by_key(|(_, _, cell)| distance(cell))
            .map_or((0, 0), |(r, c, _)| (r, c))
    }
}

/// Only project tables whose complete cell content is accounted for by the AST.
/// Ragged rows with missing or ignored extra cells retain editable source.
pub fn tables(parse: &ParseSnapshot) -> Vec<Table> {
    cached(parse).to_vec()
}
pub fn cached(parse: &ParseSnapshot) -> &[Table] {
    let parse = parse.semantic();
    parse.tables.get_or_init(|| build_tables(parse))
}
/// Exact containment in a validated table, excluding an adjacent boundary.
pub fn containing(parse: &ParseSnapshot, position: usize) -> Option<&Table> {
    let tables = cached(parse);
    tables
        .get(tables.partition_point(|t| t.range.end <= position))
        .filter(|t| t.range.contains(&position))
}
pub fn cell_containing(parse: &ParseSnapshot, position: usize) -> Option<&Cell> {
    let parse = parse.semantic();
    let tables = cached(parse);
    let cells = parse.table_cells.get_or_init(|| {
        tables
            .iter()
            .enumerate()
            .flat_map(|(t, table)| {
                table.rows.iter().enumerate().flat_map(move |(r, row)| {
                    row.cells
                        .iter()
                        .enumerate()
                        .map(move |(c, cell)| CellLocation {
                            range: cell.content.clone(),
                            table: t,
                            row: r,
                            column: c,
                        })
                })
            })
            .collect::<Vec<_>>()
    });
    let location = cells.get(cells.partition_point(|c| c.range.end <= position))?;
    location
        .range
        .contains(&position)
        .then(|| &tables[location.table].rows[location.row].cells[location.column])
}
#[cfg(test)]
mod containment_tests {
    use super::*;
    use editable_markdown_parser::Source;
    #[test]
    fn indexed_membership_matches_linear_tables_and_cells_in_containers() {
        for newline in ["\n", "\r\n", "\r"] {
            let raw = concat!(
                "before `ordinary`\n\n",
                "> | a | b |\n> | --- | --- |\n> | `中\\|😀` | |\n\n",
                "- item\n\n  | c | d |\n  | --- | --- |\n  | **bold** | `literal` |\n\n",
                "| ragged | row |\n| --- | --- |\n| extra | cell | ignored |\n\nafter `code`\n"
            )
            .replace('\n', newline);
            let parse = ParseSnapshot::with_math(Source::new(&raw));
            let tables = cached(&parse);
            assert_eq!(tables.len(), 2);
            for p in 0..=raw.len() {
                assert_eq!(
                    containing(&parse, p),
                    tables.iter().find(|t| t.range.contains(&p))
                );
                assert_eq!(
                    cell_containing(&parse, p),
                    tables
                        .iter()
                        .flat_map(|t| &t.rows)
                        .flat_map(|r| &r.cells)
                        .find(|c| c.content.contains(&p))
                );
            }
        }
    }
}
fn build_tables(parse: &ParseSnapshot) -> Vec<Table> {
    let text = parse.source().text();
    let lines = &parse.index().lines;
    let items = list::cached_items(parse);
    parse
        .index()
        .table_nodes
        .iter()
        .map(|&id| &parse.nodes[id])
        .filter_map(|node| {
            let NodeKind::Table(align) = &node.kind else {
                return None;
            };
            if align.is_empty() {
                return None;
            }
            let first = parse.index().line(node.envelope.start)?;
            let mut rows = Vec::new();
            for &row_id in &node.children {
                let row = &parse.nodes[row_id];
                if !matches!(row.kind, NodeKind::TableHead | NodeKind::TableRow) {
                    return None;
                }
                let line = parse.index().line(row.envelope.start)?.clone();
                let end = line.start + text[line.clone()].trim_end_matches(['\r', '\n']).len();
                if row.children.len() != align.len() {
                    return None;
                }
                let mut cells = Vec::new();
                for &cell_id in &row.children {
                    let raw = parse.nodes[cell_id].envelope.clone();
                    if parse.nodes[cell_id].kind != NodeKind::TableCell
                        || raw.start < line.start
                        || raw.start >= end
                        || raw.end > end
                    {
                        return None;
                    }
                    let body = &text[raw.clone()];
                    let content = if body.trim_matches([' ', '\t']).is_empty() {
                        let anchor = raw.start + raw.len().min(1);
                        anchor..anchor
                    } else {
                        let start =
                            raw.start + body.len() - body.trim_start_matches([' ', '\t']).len();
                        start..raw.start + body.trim_end_matches([' ', '\t']).len()
                    };
                    cells.push(Cell { content });
                }
                let last = *row.children.last()?;
                if text[parse.nodes[last].envelope.end..end]
                    .chars()
                    .any(|c| !matches!(c, ' ' | '\t' | '|'))
                {
                    return None;
                }
                // Any additional delimiter beyond the optional trailing pipe is an
                // ignored extra cell, even if that cell is empty.
                if text[parse.nodes[last].envelope.end..end]
                    .bytes()
                    .filter(|b| *b == b'|')
                    .count()
                    > 1
                {
                    return None;
                }
                rows.push(Row { line, cells });
            }
            if rows.is_empty() {
                return None;
            }
            let end = lines
                .get(lines.partition_point(|line| line.end < node.envelope.end))?
                .end;
            let mut scope = node.envelope.clone();
            let mut inset = 0;
            let mut ancestor = node.parent;
            while let Some(parent) = ancestor {
                match parse.nodes[parent].kind {
                    NodeKind::Item => {
                        inset += 32;
                        scope = parse.nodes[parent].envelope.clone();
                    }
                    NodeKind::BlockQuote => {
                        inset += 18;
                        scope = parse.nodes[parent].envelope.clone();
                    }
                    NodeKind::List(_) => scope = parse.nodes[parent].envelope.clone(),
                    _ => (),
                }
                ancestor = parse.nodes[parent].parent;
            }
            scope.start = parse
                .index()
                .line(scope.start)
                .map_or(scope.start, |line| line.start);
            let mut continuation = text[first.start..node.envelope.start].to_owned();
            let mut prefixes: Vec<_> = parse
                .index()
                .prefixes(parse, first.start..node.envelope.start)
                .filter(|token| {
                    token.kind == TokenKind::ContainerPrefix
                        && parse.nodes[token.owner].kind == NodeKind::Item
                        && first.start <= token.range.start
                        && token.range.end <= node.envelope.start
                })
                .collect();
            prefixes.sort_by_key(|token| std::cmp::Reverse(token.range.start));
            for token in prefixes {
                continuation.replace_range(
                    token.range.start - first.start..token.range.end - first.start,
                    &" ".repeat(
                        list::columns(&text[first.start..token.range.end])
                            - list::columns(&text[first.start..token.range.start]),
                    ),
                );
            }
            // Keep the table aligned with a surrounding list's content column.
            let mut parent = node.parent;
            while let Some(id) = parent {
                if let Ok(i) = items.binary_search_by_key(&id, |item| item.id) {
                    inset = inset.max(items[i].inset);
                }
                parent = parse.nodes[id].parent;
            }
            Some(Table {
                range: first.start..end,
                scope,
                rows,
                align: align.clone(),
                continuation,
                inset,
            })
        })
        .collect()
}

/// Single-cell input must not accidentally create new Markdown columns or rows.
pub fn cell_input(text: &str, preceding: &str) -> String {
    let mut output = String::new();
    let mut chars = text.chars().peekable();
    let mut slashes = preceding.chars().rev().take_while(|ch| *ch == '\\').count();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' | '\n' => {
                if ch == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                output.push_str("<br>");
                slashes = 0;
            }
            '|' => {
                if slashes % 2 == 0 {
                    output.push('\\');
                }
                output.push(ch);
                slashes = 0;
            }
            '\\' => {
                output.push(ch);
                slashes += 1;
            }
            _ => {
                output.push(ch);
                slashes = 0;
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use editable_markdown_parser::Source;
    #[test]
    fn tables_preserve_cells_alignments_container_prefixes_and_newlines() {
        for newline in ["\n", "\r\n", "\r"] {
            for prefix in ["", "> ", "  "] {
                let raw = format!(
                    "{prefix}| 中😀 | B |{newline}{prefix}| :--- | ---: |{newline}{prefix}| a\\|b | |{newline}"
                );
                let parse = ParseSnapshot::new(Source::new(&raw));
                let tables = tables(&parse);
                assert_eq!(tables.len(), 1, "{raw:?}");
                let table = &tables[0];
                assert_eq!(table.align, vec![Alignment::Left, Alignment::Right]);
                assert_eq!(table.rows.len(), 2);
                assert_eq!(&raw[table.rows[0].cells[0].content.clone()], "中😀");
                assert_eq!(&raw[table.rows[1].cells[0].content.clone()], "a\\|b");
                assert!(table.rows[1].cells[1].content.is_empty());
                assert_eq!(table.range, 0..raw.len());
                assert_eq!(parse.source().text(), raw);
            }
        }
    }
    #[test]
    fn unsupported_ragged_or_incomplete_tables_remain_source() {
        for raw in [
            "| a | b |\n|---|---|\n| short |\n",
            "|a|b|\n|-|-|\n|1|2|extra|\n",
            "| a | b |\n|--|\n",
            "```\n|a|b|\n|---|---|\n```\n",
        ] {
            assert!(
                tables(&ParseSnapshot::new(Source::new(raw))).is_empty(),
                "{raw:?}"
            );
        }
    }
    #[test]
    fn single_cell_input_escapes_pipes_and_serializes_line_breaks() {
        assert_eq!(
            cell_input("a|中😀\r\nb\rc\nd", ""),
            "a\\|中😀<br>b<br>c<br>d"
        );
        assert_eq!(cell_input("a\\|b", ""), "a\\|b");
        assert_eq!(cell_input("a\\\\|b", ""), "a\\\\\\|b");
        assert_eq!(cell_input("|", "x\\"), "|");
    }
}

#[cfg(test)]
mod cell_query_tests {
    use super::*;
    #[test]
    fn nearest_cell_binary_lookup_preserves_separator_and_empty_cell_ties() {
        for newline in ["\n", "\r\n", "\r"] {
            let raw = "| a | b | c |\n| --- | --- | --- |\n| | 中😀 | |\n| next | x | y |\n"
                .replace('\n', newline);
            let parse = ParseSnapshot::with_math(editable_markdown_parser::Source::new(&raw));
            let t = &cached(&parse)[0];
            for position in 0..=raw.len() + 2 {
                let expected = t
                    .rows
                    .iter()
                    .enumerate()
                    .flat_map(|(r, row)| {
                        row.cells
                            .iter()
                            .enumerate()
                            .map(move |(c, cell)| (r, c, cell))
                    })
                    .min_by_key(|(_, _, cell)| {
                        if position < cell.content.start {
                            cell.content.start - position
                        } else {
                            position.saturating_sub(cell.content.end)
                        }
                    })
                    .map(|(r, c, _)| (r, c))
                    .unwrap();
                assert_eq!(t.cell_at(position), expected, "position {position}");
            }
        }
    }
}
