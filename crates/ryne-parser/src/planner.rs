use crate::{
    validate, Affinity, Edit, Error, HitKind, Interaction, NodeKind, Origin, ParseSnapshot,
    Projection, ProjectionHit, Session, Transaction,
};
use std::ops::Range;

/// Explicit outcomes keep callers from guessing source ranges.
#[derive(Clone, Debug)]
pub enum EditPlan {
    Commit(Transaction),
    RevealThenRetry {
        window: Range<usize>,
    },
    PendingConfirmation {
        transaction: Transaction,
        selection: Range<usize>,
    },
    Unsupported {
        reason: &'static str,
        window: Range<usize>,
    },
    NoChange,
}
#[derive(Clone, Debug)]
pub enum EditIntent {
    Replace {
        start: ProjectionHit,
        end: ProjectionHit,
        text: String,
    },
    DeleteBackward {
        at: ProjectionHit,
    },
    DeleteForward {
        at: ProjectionHit,
    },
    ToggleTask {
        at: ProjectionHit,
    },
    RemoveFormat {
        at: ProjectionHit,
    },
}
pub struct EditPlanner;
impl EditPlanner {
    pub fn plan(
        parse: &ParseSnapshot,
        projection: &Projection,
        interaction: &Interaction,
        intent: EditIntent,
    ) -> Result<EditPlan, Error> {
        parse.source.check(projection.base)?;
        parse.source.check(interaction.base)?;
        let forward = matches!(intent, EditIntent::DeleteForward { .. });
        match intent {
            EditIntent::Replace { start, end, text } => {
                replace(parse, projection, interaction, start, end, text)
            }
            EditIntent::DeleteBackward { at } | EditIntent::DeleteForward { at } => {
                projection.check_hit(&at)?;
                // Deletion is performed over final projection graphemes.
                use unicode_segmentation::UnicodeSegmentation;
                let boundaries = projection
                    .text
                    .grapheme_indices(true)
                    .map(|(p, _)| p)
                    .chain(std::iter::once(projection.text.len()))
                    .collect::<Vec<_>>();
                let index = boundaries
                    .iter()
                    .position(|&p| p == at.projected)
                    .ok_or(Error::SplitGrapheme)?;
                if forward {
                    let next = boundaries[(index + 1).min(boundaries.len() - 1)];
                    if next == at.projected {
                        return Ok(EditPlan::NoChange);
                    }
                    let end = projection.hit(next, Affinity::Upstream)?;
                    replace(parse, projection, interaction, at, end, String::new())
                } else {
                    let previous = boundaries[index.saturating_sub(1)];
                    if previous == at.projected {
                        return Ok(EditPlan::NoChange);
                    }
                    let start = projection.hit(previous, Affinity::Downstream)?;
                    replace(parse, projection, interaction, start, at, String::new())
                }
            }
            EditIntent::ToggleTask { at } => {
                projection.check_hit(&at)?;
                let task = parse.nodes.iter().find(|n| {
                    matches!(n.kind, NodeKind::Task(_))
                        && at
                            .sources
                            .iter()
                            .any(|r| r.start <= n.envelope.start && n.envelope.end <= r.end)
                });
                let Some(task) = task else {
                    return Ok(EditPlan::Unsupported {
                        reason: "No task marker at hit",
                        window: at.reveal,
                    });
                };
                let raw = &parse.source.text()[task.envelope.clone()];
                let Some(start) = raw.find('[').map(|p| task.envelope.start + p + 1) else {
                    return Ok(EditPlan::RevealThenRetry {
                        window: task.envelope.clone(),
                    });
                };
                let checked = matches!(task.kind, NodeKind::Task(true));
                Ok(commit(
                    parse,
                    vec![Edit {
                        range: start..start + 1,
                        replacement: if checked { " " } else { "x" }.into(),
                    }],
                ))
            }
            EditIntent::RemoveFormat { at } => {
                projection.check_hit(&at)?;
                let node = parse
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| {
                        matches!(
                            n.kind,
                            NodeKind::Emphasis | NodeKind::Strong | NodeKind::Strikethrough
                        ) && n.envelope.start <= at.source
                            && at.source <= n.envelope.end
                    })
                    .min_by_key(|(_, n)| n.envelope.len());
                let Some((id, node)) = node else {
                    return Ok(EditPlan::NoChange);
                };
                let edits = parse.syntax.nodes[id]
                    .owned_tokens
                    .iter()
                    .filter_map(|&t| {
                        let t = &parse.syntax.tokens[t];
                        matches!(
                            t.kind,
                            crate::TokenKind::OpenMarker | crate::TokenKind::CloseMarker
                        )
                        .then(|| Edit {
                            range: t.range.clone(),
                            replacement: String::new(),
                        })
                    })
                    .collect::<Vec<_>>();
                if edits.len() != 2 {
                    return Ok(EditPlan::RevealThenRetry {
                        window: node.envelope.clone(),
                    });
                }
                Ok(commit(parse, edits))
            }
        }
    }
}
fn commit(parse: &ParseSnapshot, edits: Vec<Edit>) -> EditPlan {
    if edits
        .iter()
        .all(|e| parse.source.text().get(e.range.clone()) == Some(e.replacement.as_str()))
    {
        return EditPlan::NoChange;
    }
    EditPlan::Commit(Transaction {
        base: parse.source.id(),
        edits,
    })
}
fn literal(value: &str, table: bool) -> String {
    let mut out = String::new();
    for c in value.chars() {
        if c == '|' && table {
            out.push_str("\\|");
        } else if c.is_ascii_punctuation() {
            out.push_str(&format!("&#{};", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}
fn replace(
    parse: &ParseSnapshot,
    projection: &Projection,
    interaction: &Interaction,
    start: ProjectionHit,
    end: ProjectionHit,
    text: String,
) -> Result<EditPlan, Error> {
    projection.check_hit(&start)?;
    projection.check_hit(&end)?;
    projection.validate_cursor(start.projected)?;
    projection.validate_cursor(end.projected)?;
    if start.projected > end.projected {
        return Err(Error::InvalidRange);
    }
    let raw = start.source.min(end.source)..start.source.max(end.source);
    validate(parse.source.text(), &raw)?;
    let in_source = interaction
        .sessions
        .iter()
        .chain(interaction.composition.iter())
        .any(|s| {
            s.base == parse.source.id() && s.range.start <= raw.start && raw.end <= s.range.end
        });
    if in_source {
        if let Some(hidden) = projection
            .hidden
            .iter()
            .find(|h| raw.start < h.source.end && h.source.start < raw.end)
        {
            return Ok(EditPlan::RevealThenRetry {
                window: hidden.reveal.clone(),
            });
        }
        return Ok(commit(
            parse,
            vec![Edit {
                range: raw,
                replacement: text,
            }],
        ));
    }
    if matches!(start.kind, HitKind::Hidden | HitKind::Generated)
        || matches!(end.kind, HitKind::Hidden | HitKind::Generated)
    {
        return Ok(EditPlan::RevealThenRetry {
            window: start.reveal.start.min(end.reveal.start)..start.reveal.end.max(end.reveal.end),
        });
    }
    let touched = projection
        .runs
        .iter()
        .filter(|r| {
            if start.projected < end.projected {
                r.projected.start < end.projected && start.projected < r.projected.end
            } else {
                (r.projected.start <= start.projected && start.projected < r.projected.end)
                    || (start.projected == projection.text.len()
                        && r.projected.end == start.projected)
            }
        })
        .collect::<Vec<_>>();
    if touched.iter().any(|r| {
        matches!(
            r.origin,
            Origin::Object { .. } | Origin::MultiSource(_) | Origin::Generated { .. }
        )
    }) {
        return Ok(EditPlan::RevealThenRetry {
            window: start.reveal.start.min(end.reveal.start)..start.reveal.end.max(end.reveal.end),
        });
    }
    // Expand a partial transformed field and encode the retained decoded text.
    let transforms = touched
        .iter()
        .filter(|r| matches!(r.origin, Origin::Transform(_)))
        .collect::<Vec<_>>();
    if !transforms.is_empty() && touched.len() > 1 {
        return Ok(EditPlan::RevealThenRetry {
            window: start.reveal.start.min(end.reveal.start)..start.reveal.end.max(end.reveal.end),
        });
    }
    let table = parse.nodes.iter().any(|n| {
        matches!(n.kind, NodeKind::TableCell)
            && n.envelope.start <= raw.start
            && raw.end <= n.envelope.end
    });
    if text.contains(['\n', '\r']) {
        return Ok(EditPlan::RevealThenRetry {
            window: start.reveal.clone(),
        });
    }
    let expected = format!(
        "{}{}{}",
        &projection.text[..start.projected],
        text,
        &projection.text[end.projected..]
    );
    let (mut range, replacement) = if let Some(run) = transforms.first() {
        let Origin::Transform(range) = &run.origin else {
            unreachable!()
        };
        let value = format!(
            "{}{}{}",
            &projection.text[run.projected.start..start.projected],
            text,
            &projection.text[end.projected..run.projected.end]
        );
        // Code spans and other transformed structures require their own encoder.
        let original = &parse.source.text()[range.clone()];
        if !original.starts_with('&') && !original.starts_with('\\') {
            return Ok(EditPlan::RevealThenRetry {
                window: start.reveal.clone(),
            });
        }
        (range.clone(), literal(&value, table))
    } else {
        (raw, literal(&text, table))
    };
    if replacement.is_empty() && start.projected < end.projected {
        // Removing all displayed content of a format removes its delimiters too.
        if let Some(node) = parse
            .nodes
            .iter()
            .filter(|n| {
                matches!(
                    n.kind,
                    NodeKind::Emphasis | NodeKind::Strong | NodeKind::Strikethrough
                ) && n.envelope.start <= range.start
                    && range.end <= n.envelope.end
            })
            .filter(|n| {
                projection
                    .runs
                    .iter()
                    .filter(|r| {
                        r.origin
                            .sources()
                            .iter()
                            .any(|s| n.envelope.start <= s.start && s.end <= n.envelope.end)
                    })
                    .all(|r| {
                        start.projected <= r.projected.start && r.projected.end <= end.projected
                    })
            })
            .max_by_key(|n| n.envelope.len())
        {
            range = node.envelope.clone();
        }
    }
    let transaction = Transaction {
        base: parse.source.id(),
        edits: vec![Edit {
            range: range.clone(),
            replacement,
        }],
    };
    if projection
        .hidden
        .iter()
        .any(|h| range.start < h.source.start && h.source.end < range.end)
    {
        Ok(EditPlan::PendingConfirmation {
            transaction,
            selection: range,
        })
    } else {
        let mut candidate = parse.source.text().to_owned();
        for edit in transaction.edits.iter().rev() {
            candidate.replace_range(edit.range.clone(), &edit.replacement);
        }
        if candidate == parse.source.text() {
            return Ok(EditPlan::NoChange);
        }
        let reparsed = ParseSnapshot::with_dialect_and_math(
            crate::Source::new(&candidate),
            parse.dialect,
            parse.math_enabled,
        );
        let preview = crate::project(&reparsed, &Interaction::new(reparsed.source()))?;
        if preview.text != expected {
            return Ok(EditPlan::RevealThenRetry {
                window: start.reveal.start.min(end.reveal.start)
                    ..start.reveal.end.max(end.reveal.end),
            });
        }
        Ok(EditPlan::Commit(transaction))
    }
}
/// Clipboard policy data for host adapters. Actual clipboard I/O belongs to the host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CopyFormat {
    #[default]
    Markdown,
    PlainText,
}
pub fn copy_selection(
    parse: &ParseSnapshot,
    projection: &Projection,
    selection: &Session,
    format: CopyFormat,
) -> Result<String, Error> {
    parse.source.check(projection.base)?;
    parse.source.check(selection.base)?;
    validate(parse.source.text(), &selection.range)?;
    match format {
        CopyFormat::Markdown => Ok(parse.source.text()[selection.range.clone()].into()),
        CopyFormat::PlainText => {
            let start = projection
                .source_to_projection(&parse.source, selection.range.start, Affinity::Downstream)?
                .projected;
            let end = projection
                .source_to_projection(&parse.source, selection.range.end, Affinity::Upstream)?
                .projected;
            Ok(projection.text[start.min(end)..start.max(end)].into())
        }
    }
}
