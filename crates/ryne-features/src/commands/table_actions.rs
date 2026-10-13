use super::*;
use ryne_extensions::RenderActionRequest;
use unicode_segmentation::UnicodeSegmentation;

impl Planner<'_> {
    pub(crate) fn table_action(&mut self, request: &RenderActionRequest) -> Result<(), String> {
        request.target.validate(&self.state)?;
        let target = &request.target;
        if target.role != "table.cell" || target.part.len() != 2 {
            return Err("invalid table target".into());
        }
        let table = table::tables(&self.state.parse)
            .into_iter()
            .find(|t| t.range == target.owner)
            .ok_or("table disappeared")?;
        let (row, column) = (target.part[0], target.part[1]);
        let cell = table
            .rows
            .get(row)
            .and_then(|r| r.cells.get(column))
            .ok_or("cell disappeared")?;
        let focused = self.state.selection().is_empty()
            && cell.content.start <= self.state.head
            && self.state.head <= cell.content.end;
        let relative = self.state.head.saturating_sub(cell.content.start);
        let text = self.state.source.text();
        let delimiter = code_block::lines(text)
            .into_iter()
            .find(|l| l.start == table.rows[0].line.end)
            .ok_or("missing delimiter")?;
        let edits = match request.command {
            "table.delete-row" if row > 0 => vec![Edit {
                range: table.rows[row].line.clone(),
                replacement: String::new(),
            }],
            "table.insert-below" | "table.insert-above" => {
                if request.command == "table.insert-above" && row == 0 {
                    return Err("cannot insert before header".into());
                }
                let at = if request.command == "table.insert-above" {
                    table.rows[row].line.start
                } else if row == 0 {
                    delimiter.end
                } else {
                    table.rows[row].line.end
                };
                let newline = self.newline_at(cell.content.start);
                let body = format!(
                    "{}| {} |",
                    table.continuation,
                    vec![""; table.align.len()].join(" | ")
                );
                let replacement = if text[..at].ends_with(['\r', '\n']) {
                    format!("{body}{newline}")
                } else {
                    format!("{newline}{body}")
                };
                vec![Edit {
                    range: at..at,
                    replacement,
                }]
            }
            "table.move-row-up" | "table.move-row-down" => {
                let other = if request.command == "table.move-row-up" {
                    row.checked_sub(1)
                } else {
                    Some(row + 1)
                }
                .ok_or("row boundary")?;
                if row == 0 || other == 0 || other >= table.rows.len() {
                    return Err("row boundary".into());
                }
                let (a, b) = if row < other {
                    (&table.rows[row].line, &table.rows[other].line)
                } else {
                    (&table.rows[other].line, &table.rows[row].line)
                };
                if a.end != b.start {
                    return Err("nonadjacent rows".into());
                }
                let av = &text[a.clone()];
                let bv = &text[b.clone()];
                let ab = av.trim_end_matches(['\r', '\n']);
                let bb = bv.trim_end_matches(['\r', '\n']);
                vec![Edit {
                    range: a.start..b.end,
                    replacement: format!("{}{}{}{}", bb, &av[ab.len()..], ab, &bv[bb.len()..]),
                }]
            }
            command => {
                let body = &text[delimiter.clone()];
                let first =
                    delimiter.start + body.len() - body.trim_start_matches([' ', '\t', '>']).len();
                let mut lines = table
                    .rows
                    .iter()
                    .map(|r| (r.line.clone(), r.cells[0].content.start, false))
                    .collect::<Vec<_>>();
                lines.push((delimiter, first, true));
                let alignment = match command {
                    "table.align-left" => Some((true, false)),
                    "table.align-center" => Some((true, true)),
                    "table.align-right" => Some((false, true)),
                    _ => None,
                };
                lines
                    .into_iter()
                    .filter(|(_, _, separator)| alignment.is_none() || *separator)
                    .map(|(line, first, separator)| {
                        let fields = Fields::parse(text, line, first, table.align.len())?;
                        if let Some((left, right)) = alignment {
                            let range = fields.ranges[column].clone();
                            let raw = &text[range.clone()];
                            let trim = raw.trim();
                            let start = range.start + raw.len() - raw.trim_start().len();
                            let dashes = trim.trim_matches(':');
                            return Ok(Edit {
                                range: start..start + trim.len(),
                                replacement: format!(
                                    "{}{dashes}{}",
                                    if left { ":" } else { "" },
                                    if right { ":" } else { "" }
                                ),
                            });
                        }
                        let mut parts = fields
                            .ranges
                            .iter()
                            .map(|r| text[r.clone()].to_owned())
                            .collect::<Vec<_>>();
                        match command {
                            "table.delete-column" if parts.len() > 1 => {
                                parts.remove(column);
                            }
                            "table.insert-right" => parts.insert(
                                column + 1,
                                if separator {
                                    " --- ".into()
                                } else {
                                    "  ".into()
                                },
                            ),
                            "table.insert-left" => parts.insert(
                                column,
                                if separator {
                                    " --- ".into()
                                } else {
                                    "  ".into()
                                },
                            ),
                            "table.move-column-left" if column > 0 => {
                                parts.swap(column, column - 1)
                            }
                            "table.move-column-right" if column + 1 < parts.len() => {
                                parts.swap(column, column + 1)
                            }
                            _ => return Err("table action unavailable".into()),
                        }
                        let mut replacement = parts.join("|");
                        if !fields.leading && parts.first().is_some_and(|p| p.trim().is_empty()) {
                            replacement.insert(0, '|');
                        }
                        if !fields.trailing && parts.last().is_some_and(|p| p.trim().is_empty()) {
                            replacement.push('|');
                        }
                        Ok(Edit {
                            range: fields.start..fields.end,
                            replacement,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?
            }
        };
        let edits = edits
            .into_iter()
            .filter(|e| text[e.range.clone()] != e.replacement)
            .collect::<Vec<_>>();
        if edits.is_empty() {
            return Ok(());
        }
        self.apply_edits(edits, None)
            .map_err(|e| format!("{e:?}"))?;
        let updated = table::tables(&self.state.parse)
            .into_iter()
            .find(|t| t.range.start == table.range.start)
            .ok_or("table operation did not preserve structure")?;
        if focused {
            let new_row = match request.command {
                "table.insert-below" | "table.move-row-down" => row + 1,
                "table.move-row-up" => row - 1,
                _ => row,
            }
            .min(updated.rows.len() - 1);
            let new_column = match request.command {
                "table.insert-right" | "table.move-column-right" => column + 1,
                "table.move-column-left" => column - 1,
                _ => column,
            }
            .min(updated.align.len() - 1);
            let cell = &updated.rows[new_row].cells[new_column].content;
            let at = cell.start + relative.min(cell.len());
            let text = &self.state.source.text()[cell.clone()];
            let at = text
                .grapheme_indices(true)
                .map(|(i, _)| cell.start + i)
                .chain(std::iter::once(cell.end))
                .take_while(|p| *p <= at)
                .last()
                .unwrap_or(cell.start);
            self.select(at, false, false);
        }
        Ok(())
    }
}

struct Fields {
    ranges: Vec<Range<usize>>,
    start: usize,
    end: usize,
    leading: bool,
    trailing: bool,
}
impl Fields {
    fn parse(text: &str, line: Range<usize>, first: usize, count: usize) -> Result<Self, String> {
        let end = line.start + text[line.clone()].trim_end_matches(['\r', '\n']).len();
        let mut slashes = 0;
        let mut pipes = Vec::new();
        for (offset, ch) in text[line.start..end].char_indices() {
            if ch == '|' && slashes % 2 == 0 {
                pipes.push(line.start + offset);
            }
            slashes = if ch == '\\' { slashes + 1 } else { 0 };
        }
        let leading =
            text.as_bytes().get(first) == Some(&b'|') || pipes.first().is_some_and(|p| *p < first);
        let trailing = pipes.len() == count + usize::from(leading)
            && pipes
                .last()
                .is_some_and(|p| text[*p + 1..end].trim().is_empty());
        if pipes.len() < usize::from(leading) + usize::from(trailing) {
            return Err("invalid pipes".into());
        }
        let start = if leading { pipes[0] + 1 } else { first };
        let end = if trailing {
            *pipes.last().ok_or("missing pipe")?
        } else {
            end
        };
        let separators = &pipes[usize::from(leading)..pipes.len() - usize::from(trailing)];
        if separators.len() + 1 != count {
            return Err("ambiguous table delimiters".into());
        }
        let mut ranges = Vec::new();
        let mut a = start;
        for b in separators.iter().copied().chain(std::iter::once(end)) {
            ranges.push(a..b);
            a = b + 1;
        }
        Ok(Self {
            ranges,
            start,
            end,
            leading,
            trailing,
        })
    }
}
