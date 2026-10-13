use super::*;
use crate::list_actions::extent;
use ryne_extensions::RenderActionRequest;

impl Planner<'_> {
    pub(crate) fn list_action(&mut self, request: &RenderActionRequest) -> Result<(), String> {
        request.target.validate(&self.state)?;
        if request.target.role != "list.item" || request.target.part.len() != 1 {
            return Err("invalid item target".into());
        }
        let items = list::items(&self.state.parse);
        let text = self.state.source.text();
        let item = items
            .iter()
            .find(|i| {
                i.marker.start == request.target.part[0]
                    && extent(&self.state, i) == request.target.owner
            })
            .ok_or("item disappeared")?;
        let own = extent(&self.state, item);
        let expected_items = match request.command {
            "list.insert-before" | "list.insert-after" => items.len() + 1,
            "list.delete" => {
                items.len()
                    - items
                        .iter()
                        .filter(|i| own.contains(&i.marker.start))
                        .count()
            }
            _ => items.len(),
        };
        match request.command {
            "list.toggle-task" => {
                let task = item.task.as_ref().ok_or("not a task item")?;
                let at = task.marker.start + 1;
                self.apply_edits(
                    vec![Edit {
                        range: at..at + 1,
                        replacement: if task.checked { " " } else { "x" }.into(),
                    }],
                    None,
                )
                .map_err(|e| format!("{e:?}"))?;
            }
            "list.indent" | "list.outdent" => {
                let mut bounded = item.clone();
                bounded.range = own;
                self.indent_list_item(&bounded, request.command == "list.outdent")
                    .map_err(|e| format!("{e:?}"))?;
            }
            "list.delete" => {
                self.apply_edits(
                    vec![Edit {
                        range: own,
                        replacement: String::new(),
                    }],
                    None,
                )
                .map_err(|e| format!("{e:?}"))?;
            }
            "list.insert-before" | "list.insert-after" => {
                let before = request.command == "list.insert-before";
                let at = if before { own.start } else { own.end };
                let newline = self.newline_at(item.body);
                let marker = if before {
                    text[item.marker.clone()].to_owned()
                } else {
                    item.next_marker.clone()
                };
                let task = if ["[ ] ", "[x] ", "[X] "]
                    .iter()
                    .any(|p| text[item.body..].starts_with(p))
                {
                    "[ ] "
                } else {
                    ""
                };
                let prefix = &text[own.start..item.marker.start];
                let body = format!("{prefix}{marker}{task}");
                let leading = if at > 0 && !text[..at].ends_with(['\r', '\n']) {
                    newline
                } else {
                    ""
                };
                let trailing = if at < text.len() || text.ends_with(['\r', '\n']) {
                    newline
                } else {
                    ""
                };
                let mut replacement = format!("{leading}{body}{trailing}");
                let mut caret = at + leading.len() + body.len();
                let candidate = self
                    .state
                    .source
                    .apply(
                        self.state.source.id(),
                        vec![Edit {
                            range: at..at,
                            replacement: replacement.clone(),
                        }],
                    )
                    .map_err(|e| format!("{e:?}"))?;
                let parsed = ParseSnapshot::with_math(candidate.source);
                if !list::items(&parsed)
                    .iter()
                    .any(|i| i.marker.start == at + leading.len() + prefix.len())
                {
                    // Empty items cannot interrupt a paragraph or become its Setext underline.
                    replacement = format!("{leading}{prefix}{newline}{body}{trailing}");
                    caret += prefix.len() + newline.len();
                }
                self.apply_edits(
                    vec![Edit {
                        range: at..at,
                        replacement,
                    }],
                    None,
                )
                .map_err(|e| format!("{e:?}"))?;
                self.select(caret, false, false);
                if !list::items(&self.state.parse)
                    .iter()
                    .any(|i| i.body <= caret && caret <= extent(&self.state, i).end)
                {
                    return Err("inserted item was not recognized".into());
                }
            }
            "list.move-up" | "list.move-down" => {
                let next = if request.command == "list.move-up" {
                    items
                        .iter()
                        .rev()
                        .find(|i| i.list == item.list && i.marker.start < item.marker.start)
                } else {
                    items
                        .iter()
                        .find(|i| i.list == item.list && i.marker.start > item.marker.start)
                }
                .ok_or("item boundary")?;
                let (a, b) = if item.marker.start < next.marker.start {
                    (item, next)
                } else {
                    (next, item)
                };
                let ar = extent(&self.state, a);
                let br = extent(&self.state, b);
                if ar.end > br.start {
                    return Err("overlapping item subtrees".into());
                }
                let gap = &text[ar.end..br.start];
                if !gap
                    .chars()
                    .all(|c| matches!(c, ' ' | '\t' | '\r' | '\n' | '>'))
                {
                    return Err("nonadjacent item subtrees".into());
                }
                let positions = [self.state.anchor, self.state.head];
                let ra = relocate(&self.state, a, b, &positions)?;
                let rb = relocate(&self.state, b, a, &positions)?;
                let a_end = ending(&text[ar.clone()]);
                let b_end = ending(&text[br.clone()]);
                let a_body = &ra.text[..ra.text.len() - ending(&ra.text).len()];
                let b_body = &rb.text[..rb.text.len() - ending(&rb.text).len()];
                // Keep separator bytes at their positions; move each whole subtree.
                let replacement = format!("{b_body}{a_end}{gap}{a_body}{b_end}");
                let moved_a_start = ar.start + b_body.len() + a_end.len() + gap.len();
                let mapped = positions
                    .iter()
                    .enumerate()
                    .map(|(index, _)| {
                        rb.positions[index]
                            .map(|p| ar.start + p.min(b_body.len()))
                            .or_else(|| {
                                ra.positions[index].map(|p| moved_a_start + p.min(a_body.len()))
                            })
                    })
                    .collect::<Vec<_>>();
                self.apply_edits(
                    vec![Edit {
                        range: ar.start..br.end,
                        replacement,
                    }],
                    None,
                )
                .map_err(|e| format!("{e:?}"))?;
                if let Some(anchor) = mapped[0] {
                    self.state.anchor = anchor;
                }
                if let Some(head) = mapped[1] {
                    self.state.head = head;
                }
            }
            _ => return Err("unknown list command".into()),
        }
        if list::items(&self.state.parse).len() != expected_items {
            return Err("list operation did not preserve item structure".into());
        }
        self.select(self.state.head, true, false);
        Ok(())
    }
}
fn ending(text: &str) -> &str {
    if text.ends_with("\r\n") {
        "\r\n"
    } else if text.ends_with('\r') {
        "\r"
    } else if text.ends_with('\n') {
        "\n"
    } else {
        ""
    }
}
struct Relocated {
    text: String,
    positions: Vec<Option<usize>>,
}
fn relocate(
    state: &ryne_extensions::EditingState,
    from: &list::Item,
    to: &list::Item,
    cursors: &[usize],
) -> Result<Relocated, String> {
    let source = &state.source;
    let text = source.text();
    let own = extent(state, from);
    let mut edits = vec![Edit {
        range: from.marker.clone(),
        replacement: text[to.marker.clone()].to_owned(),
    }];
    let before = list::columns(&text[from.marker.clone()]);
    let after = list::columns(&text[to.marker.clone()]);
    if before != after {
        for line in code_block::lines(text)
            .into_iter()
            .filter(|l| own.start < l.start && l.start < own.end)
        {
            let start = list::quote_end(&state.parse, &line);
            if text[start..line.end].trim().is_empty() {
                continue;
            }
            if after > before {
                edits.push(Edit {
                    range: start..start,
                    replacement: " ".repeat(after - before),
                });
            } else if let Some((range, replacement)) =
                list::unindent(text, &line, start, before - after)
            {
                edits.push(Edit { range, replacement });
            }
        }
    }
    if edits.iter().all(|e| text[e.range.clone()] == e.replacement) {
        return Ok(Relocated {
            text: text[own.clone()].to_owned(),
            positions: cursors
                .iter()
                .map(|&p| {
                    (own.contains(&p) || (p == own.end && p == text.len())).then(|| p - own.start)
                })
                .collect(),
        });
    }
    // This read-only relocation candidate is never independently committed.
    let applied = source
        .apply(source.id(), edits)
        .map_err(|e| format!("{e:?}"))?;
    let start = applied
        .positions
        .map_bytes(source.id(), own.start, Affinity::Upstream)
        .map_err(|e| format!("{e:?}"))?;
    let end = applied
        .positions
        .map_bytes(source.id(), own.end, Affinity::Downstream)
        .map_err(|e| format!("{e:?}"))?;
    let positions = cursors
        .iter()
        .map(|&p| {
            (own.contains(&p) || (p == own.end && p == text.len()))
                .then(|| {
                    applied
                        .positions
                        .map_cursor(source, &applied.source, p, Affinity::Downstream)
                        .map(|p| p - start)
                        .map_err(|e| format!("{e:?}"))
                })
                .transpose()
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Relocated {
        text: applied.source.text()[start..end].to_owned(),
        positions,
    })
}
