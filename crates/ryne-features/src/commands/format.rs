//! Explicit source formatting: lossless ranges -> patches -> semantic validation.
//! Never serialize the AST or touch literal code, HTML, references or soft breaks.
use super::*;
use ryne_parser::TokenKind;
use unicode_segmentation::UnicodeSegmentation;

const MAX_BYTES: usize = 1024 * 1024;
const MAX_LAYOUT_BYTES: usize = 4 * MAX_BYTES;
const MAX_PARTS: usize = 20_000;

impl Planner<'_> {
    pub fn format_source(&mut self) -> Result<(), String> {
        if self.state.source.text().len() > MAX_BYTES {
            return Err("source formatting is limited to 1 MiB".into());
        }
        if self.state.parse.nodes.len() > MAX_PARTS
            || code_block::lines(self.state.source.text()).len() > MAX_PARTS
        {
            return Err("source formatting is limited to 20000 nodes and lines".into());
        }
        let input = self.state.parse.clone();
        let normalized = ryne_syntax::parser::normalized(&input);
        let parse = normalized.unwrap_or(&input);
        let text = self.state.source.text();
        let mut edits = Vec::new();
        let front_matter = front_matter_end(text);
        let mut repairs = Vec::new();
        for rule in RULES
            .iter()
            .filter(|rule| rule.effect == RuleEffect::RepairStructure)
        {
            (rule.collect)(parse, text, &mut repairs);
        }
        repairs.retain(|edit| edit.range.start >= front_matter);
        repairs.sort_by_key(|edit| (edit.range.start, edit.range.end));
        // Only explicit repair rules may change block semantics.
        // All cosmetic rules must preserve the semantics of this baseline.
        let mut baseline_text = text.to_owned();
        for edit in repairs.iter().rev() {
            baseline_text.replace_range(edit.range.clone(), &edit.replacement);
        }
        let baseline = ParseSnapshot::with_dialect_and_math(
            Source::new(&baseline_text),
            parse.dialect,
            parse.math_enabled,
        );
        let normalized_baseline = ryne_syntax::parser::normalized(&baseline);
        let baseline = normalized_baseline.unwrap_or(&baseline);
        edits.extend(repairs);
        for rule in RULES
            .iter()
            .filter(|rule| rule.effect == RuleEffect::PreserveSemantics)
        {
            (rule.collect)(parse, text, &mut edits);
        }
        edits.retain(|edit| edit.range.start >= front_matter);
        edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
        // The transaction API rejects touching edits when either is an
        // insertion. Empty table cells can produce just such adjacent gaps.
        let mut combined: Vec<Edit> = Vec::new();
        for edit in edits {
            if let Some(previous) = combined.last_mut()
                && previous.range.end == edit.range.start
                && (previous.range.is_empty() || edit.range.is_empty())
            {
                previous.range.end = edit.range.end;
                previous.replacement.push_str(&edit.replacement);
            } else {
                combined.push(edit);
            }
        }
        let edits = combined;
        if edits.is_empty() {
            return Ok(());
        }
        let replacement_bytes = edits
            .iter()
            .map(|edit| edit.replacement.len())
            .sum::<usize>();
        let removed_bytes = edits.iter().map(|edit| edit.range.len()).sum::<usize>();
        if replacement_bytes > MAX_LAYOUT_BYTES
            || text
                .len()
                .saturating_add(replacement_bytes)
                .saturating_sub(removed_bytes)
                > MAX_BYTES
        {
            return Err("formatted source exceeds the 1 MiB limit; source preserved".into());
        }
        // Build the complete candidate on the isolated planner. Any mismatch
        // discards the plan before the session can publish source or history.
        let original = self.state.source.clone();
        let anchor = self.state.anchor;
        let head = self.state.head;
        let tables = table::tables(parse);
        let cell_anchor = |position| {
            tables.iter().enumerate().find_map(|(t, table)| {
                table.rows.iter().enumerate().find_map(|(r, row)| {
                    row.cells.iter().enumerate().find_map(|(c, cell)| {
                        ((cell.content.is_empty() && cell.content.start == position)
                            || (anchor == head && cell.content.end == position))
                            .then_some((t, r, c))
                    })
                })
            })
        };
        let anchor_cell = cell_anchor(anchor);
        let head_cell = cell_anchor(head);
        let at_cell_end = head_cell.is_some();
        self.apply_edits(edits, None)
            .map_err(|error| format!("{error:?}"))?;
        let after_normalized = ryne_syntax::parser::normalized(&self.state.parse);
        let after = after_normalized.unwrap_or(&self.state.parse);
        if !same_semantics(baseline, after) {
            return Err(format!(
                "formatting would change Markdown semantics; source preserved (rules: {})",
                RULES
                    .iter()
                    .map(|rule| rule.id)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if let Some(prepared) = &self.prepared {
            let positions = &prepared.applied().positions;
            let affinity = |position| {
                if (anchor != head && position == anchor.max(head))
                    || (anchor == head && at_cell_end)
                {
                    Affinity::Upstream
                } else {
                    Affinity::Downstream
                }
            };
            self.state.anchor = positions
                .map_cursor(&original, &self.state.source, anchor, affinity(anchor))
                .map_err(|e| format!("{e:?}"))?;
            self.state.head = positions
                .map_cursor(&original, &self.state.source, head, affinity(head))
                .map_err(|e| format!("{e:?}"))?;
        }
        // Empty cell positions are semantic insertion anchors inside newly
        // generated padding; an affine source boundary alone is insufficient.
        let after_tables = table::tables(after);
        let cell_end = |key: Option<(usize, usize, usize)>| {
            let (t, r, c) = key?;
            Some(after_tables.get(t)?.rows.get(r)?.cells.get(c)?.content.end)
        };
        if let Some(position) = cell_end(anchor_cell) {
            self.state.anchor = position;
        }
        if let Some(position) = cell_end(head_cell) {
            self.state.head = position;
        }
        Ok(())
    }
}

fn patch(text: &str, mut range: Range<usize>, mut replacement: String, edits: &mut Vec<Edit>) {
    if text.get(range.clone()) == Some(replacement.as_str()) {
        return;
    }
    // Keep unchanged bytes out of edits so positions inside cell content and
    // unchanged markup retain their exact source anchors.
    let original = &text[range.clone()];
    let prefix = original
        .chars()
        .zip(replacement.chars())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum::<usize>();
    range.start += prefix;
    replacement.drain(..prefix);
    let suffix = text[range.clone()]
        .chars()
        .rev()
        .zip(replacement.chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum::<usize>();
    range.end -= suffix;
    replacement.truncate(replacement.len() - suffix);
    edits.push(Edit { range, replacement });
}

fn format_tables(parse: &ParseSnapshot, text: &str, edits: &mut Vec<Edit>) {
    for table in table::tables(parse) {
        let widths = (0..table.align.len())
            .map(|column| {
                table
                    .rows
                    .iter()
                    .map(|row| {
                        text[row.cells[column].content.clone()]
                            .graphemes(true)
                            .count()
                    })
                    .max()
                    .unwrap_or(0)
                    .max(match table.align[column] {
                        ryne_parser::Alignment::Center => 5,
                        ryne_parser::Alignment::Left | ryne_parser::Alignment::Right => 4,
                        ryne_parser::Alignment::None => 3,
                    })
            })
            .collect::<Vec<_>>();
        // A narrow input with one very long cell must not allocate its width
        // for thousands of otherwise tiny rows during source alignment.
        let row_bytes = widths.iter().sum::<usize>() + 3 * widths.len() + 2;
        if row_bytes.saturating_mul(table.rows.len() + 1) > MAX_LAYOUT_BYTES {
            continue;
        }
        for row in &table.rows {
            let Some(node) = parse.nodes.iter().find(|node| {
                matches!(node.kind, NodeKind::TableHead | NodeKind::TableRow)
                    && row.line.contains(&node.envelope.start)
                    && node.children.len() == widths.len()
            }) else {
                continue;
            };
            let mut start = node.envelope.start;
            for (column, cell) in row.cells.iter().enumerate() {
                patch(
                    text,
                    start..cell.content.start,
                    if column == 0 {
                        "| ".into()
                    } else {
                        let previous = &row.cells[column - 1];
                        format!(
                            "{} | ",
                            " ".repeat(
                                widths[column - 1]
                                    - text[previous.content.clone()].graphemes(true).count()
                            )
                        )
                    },
                    edits,
                );
                start = cell.content.end;
            }
            let end = row.line.start + text[row.line.clone()].trim_end_matches(['\r', '\n']).len();
            let Some(last) = row.cells.last() else {
                continue;
            };
            patch(
                text,
                start..end,
                format!(
                    "{} |",
                    " ".repeat(
                        widths[widths.len() - 1]
                            - text[last.content.clone()].graphemes(true).count()
                    )
                ),
                edits,
            );
        }
        let header = &table.rows[0];
        let Some(separator) = code_block::lines(text)
            .into_iter()
            .find(|line| line.start == header.line.end)
        else {
            continue;
        };
        let end = separator.start + text[separator.clone()].trim_end_matches(['\r', '\n']).len();
        let start = separator.start + table.continuation.len();
        if start > end
            || !text[separator.start..end].starts_with(&table.continuation)
            || !text[start..end]
                .bytes()
                .all(|b| matches!(b, b'|' | b':' | b'-' | b' ' | b'\t'))
        {
            continue;
        }
        let cells = table
            .align
            .iter()
            .zip(&widths)
            .map(|(align, width)| {
                use ryne_parser::Alignment;
                match align {
                    Alignment::None => "-".repeat(*width),
                    Alignment::Left => format!(":{}", "-".repeat((*width - 1).max(3))),
                    Alignment::Right => format!("{}:", "-".repeat((*width - 1).max(3))),
                    Alignment::Center => format!(":{}:", "-".repeat((*width - 2).max(3))),
                }
            })
            .collect::<Vec<_>>();
        patch(
            text,
            start..end,
            format!("| {} |", cells.join(" | ")),
            edits,
        );
    }
}

/// Compare payloads and the complete tree shape; byte envelopes and trivia may
/// change, but text, code, math, links, list starts, tasks and alignment may not.
fn same_semantics(before: &ParseSnapshot, after: &ParseSnapshot) -> bool {
    before.nodes.len() == after.nodes.len()
        && before
            .nodes
            .iter()
            .zip(&after.nodes)
            .all(|(a, b)| a.kind == b.kind && a.parent == b.parent && a.children == b.children)
}

fn front_matter_end(text: &str) -> usize {
    let lines = code_block::lines(text);
    let Some(first) = lines.first() else { return 0 };
    let delimiter = text[first.clone()]
        .trim_start_matches('\u{feff}')
        .trim_end_matches(['\r', '\n']);
    if !matches!(delimiter, "---" | "+++") {
        return 0;
    }
    lines
        .iter()
        .skip(1)
        .find(|line| {
            let raw = text[(*line).clone()].trim_end_matches(['\r', '\n']);
            raw == delimiter || (delimiter == "---" && raw == "...")
        })
        .map_or(text.len(), |line| line.end)
}

/// Intentional repair, separate from semantics-preserving formatting rules:
/// a standalone, single-line paragraph starting with 1–6 hashes is a heading
/// candidate. Never reinterpret literal code, HTML, tables or multiline prose.
fn repair_heading_spaces(parse: &ParseSnapshot, text: &str, edits: &mut Vec<Edit>) {
    let lines = code_block::lines(text);
    for node in &parse.nodes {
        if node.kind != NodeKind::Paragraph {
            continue;
        }
        let Some(line) = lines.get(lines.partition_point(|line| line.end <= node.envelope.start))
        else {
            continue;
        };
        if node.envelope.end > line.end {
            continue;
        }
        let raw = &text[node.envelope.clone()];
        let indent = raw.len() - raw.trim_start_matches(' ').len();
        if indent > 3 {
            continue;
        }
        let raw = raw[indent..].trim_end_matches(['\r', '\n']);
        let hashes = raw.bytes().take_while(|b| *b == b'#').count();
        if !(1..=6).contains(&hashes) {
            continue;
        }
        let body = &raw[hashes..];
        if body.is_empty() || body.starts_with(char::is_whitespace) || body.starts_with('!') {
            continue;
        }
        let at = node.envelope.start + indent + hashes;
        edits.push(Edit {
            range: at..at,
            replacement: " ".into(),
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RuleEffect {
    PreserveSemantics,
    RepairStructure,
}

struct FormatRule {
    id: &'static str,
    effect: RuleEffect,
    collect: fn(&ParseSnapshot, &str, &mut Vec<Edit>),
}

// Deterministic built-in policy. Add conditions and regression cases alongside
// each rule; structural repairs are opt-in here, never inferred by a serializer.
const RULES: &[FormatRule] = &[
    FormatRule {
        id: "heading.missing-space",
        effect: RuleEffect::RepairStructure,
        collect: repair_heading_spaces,
    },
    FormatRule {
        id: "task.missing-space",
        effect: RuleEffect::RepairStructure,
        collect: repair_task_spaces,
    },
    FormatRule {
        id: "markers.canonical",
        effect: RuleEffect::PreserveSemantics,
        collect: format_markers,
    },
    FormatRule {
        id: "fence.backticks",
        effect: RuleEffect::PreserveSemantics,
        collect: format_fences,
    },
    FormatRule {
        id: "table.padding",
        effect: RuleEffect::PreserveSemantics,
        collect: format_tables,
    },
    FormatRule {
        id: "blocks.blank-lines",
        effect: RuleEffect::PreserveSemantics,
        collect: format_blank_lines,
    },
];

fn repair_task_spaces(parse: &ParseSnapshot, text: &str, edits: &mut Vec<Edit>) {
    for token in &parse.syntax.tokens {
        if token.kind != TokenKind::ContainerPrefix
            || parse.nodes[token.owner].kind != NodeKind::Item
        {
            continue;
        }
        let marker = text[token.range.clone()].trim();
        let list_marker = marker == "-"
            || marker == "+"
            || marker == "*"
            || marker.strip_suffix(['.', ')']).is_some_and(|digits| {
                !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
            });
        if !list_marker {
            continue;
        }
        let start = token.range.end;
        let body = &text[start..];
        if !(body.starts_with("[x]") || body.starts_with("[X]") || body.starts_with("[ ]")) {
            continue;
        }
        let after = &body[3..];
        if after.is_empty() || after.starts_with(char::is_whitespace) {
            continue;
        }
        edits.push(Edit {
            range: start + 3..start + 3,
            replacement: " ".into(),
        });
    }
}

fn format_markers(parse: &ParseSnapshot, text: &str, edits: &mut Vec<Edit>) {
    // Different bullet characters can deliberately separate adjacent lists.
    // Preserve both lists' markers rather than merge their structures.
    let separate_lists = parse
        .nodes
        .iter()
        .flat_map(|node| node.children.windows(2))
        .filter(|pair| {
            pair.iter()
                .all(|id| matches!(parse.nodes[*id].kind, NodeKind::List(None)))
        })
        .flat_map(|pair| pair.iter().copied())
        .collect::<std::collections::HashSet<_>>();
    for token in &parse.syntax.tokens {
        let raw = &text[token.range.clone()];
        match (&parse.nodes[token.owner].kind, token.kind) {
            (NodeKind::Heading(_), TokenKind::OpenMarker) if raw.starts_with('#') => {
                let hashes = raw.bytes().take_while(|b| *b == b'#').count();
                if !parse.nodes[token.owner].children.is_empty()
                    && hashes <= 6
                    && raw[hashes..].bytes().all(|b| matches!(b, b' ' | b'\t'))
                {
                    patch(
                        text,
                        token.range.start + hashes..token.range.end,
                        " ".into(),
                        edits,
                    );
                }
            }
            (
                NodeKind::Emphasis | NodeKind::Strong,
                TokenKind::OpenMarker | TokenKind::CloseMarker,
            ) if raw.bytes().all(|b| b == b'_') => {
                patch(text, token.range.clone(), "*".repeat(raw.len()), edits);
            }
            (NodeKind::Item, TokenKind::ContainerPrefix)
                if raw.starts_with(['*', '+'])
                    && parse.nodes[token.owner].parent.is_some_and(|id| {
                        matches!(parse.nodes[id].kind, NodeKind::List(None))
                            && !separate_lists.contains(&id)
                    }) =>
            {
                patch(
                    text,
                    token.range.start..token.range.start + 1,
                    "-".into(),
                    edits,
                );
            }
            _ => (),
        }
    }
}
fn format_fences(parse: &ParseSnapshot, text: &str, edits: &mut Vec<Edit>) {
    for block in code_block::blocks(parse) {
        let Some(closing) = block.closing else {
            continue;
        };
        let Some(node) = parse.nodes.iter().find(|node| {
            matches!(node.kind, NodeKind::CodeBlock(_))
                && block.range.contains(&node.envelope.start)
        }) else {
            continue;
        };
        let start = node.envelope.start;
        let raw = &text[start..block.range.end];
        let indent = raw.len() - raw.trim_start_matches([' ', '\t']).len();
        let start = start + indent;
        let Some(&character) = text.as_bytes().get(start) else {
            continue;
        };
        if !matches!(character, b'`' | b'~') {
            continue;
        }
        let width = text[start..]
            .bytes()
            .take_while(|b| *b == character)
            .count();
        if width < 3 {
            continue;
        }
        let first_end = start
            + text[start..]
                .find(['\r', '\n'])
                .unwrap_or(text.len() - start);
        // Backticks in the info string are legal for tilde fences only.
        if text[start + width..first_end].contains('`') {
            continue;
        }
        let body_width = block
            .rows
            .iter()
            .map(|row| {
                text[row.content.clone()]
                    .trim_start_matches(' ')
                    .bytes()
                    .take_while(|b| *b == b'`')
                    .count()
            })
            .max()
            .unwrap_or(0);
        let fence = "`".repeat(3.max(body_width + 1));
        let close_start = closing.start
            + text[closing.clone()]
                .find(character as char)
                .unwrap_or(closing.len());
        let close_width = text[close_start..closing.end]
            .bytes()
            .take_while(|b| *b == character)
            .count();
        patch(text, start..start + width, fence.clone(), edits);
        patch(text, close_start..close_start + close_width, fence, edits);
    }
}
fn format_blank_lines(parse: &ParseSnapshot, text: &str, edits: &mut Vec<Edit>) {
    // Collapse only document-owned blank-line trivia. Container blank lines,
    // code, HTML, front matter and incomplete source are left byte-for-byte intact.
    let lines = code_block::lines(text);
    let mut blanks = 0;
    for line in &lines {
        let blank = text[line.clone()]
            .trim_matches([' ', '\t', '\r', '\n'])
            .is_empty()
            && parse.syntax.tokens[parse
                .syntax
                .tokens
                .partition_point(|token| token.range.end <= line.start)..]
                .iter()
                .take_while(|token| token.range.start < line.end)
                .all(|token| token.owner == 0 && token.kind == TokenKind::Trivia);
        if blank {
            blanks += 1;
            if blanks > 1 {
                patch(text, line.clone(), String::new(), edits);
            }
        } else {
            blanks = 0;
        }
    }
}
