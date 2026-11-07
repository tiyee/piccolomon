//! Real-window regression checks; failures return normally through the smoke host.
use super::*;

pub(super) fn sample() -> String {
    (0..180)
        .map(|i| format!("中😀 e\u{301} **word** $x_{{{i}}}^2$ tail "))
        .collect::<String>()
        + "\n\nlast paragraph\n\n| a | b |\n| --- | --- |\n| first $\\frac{a}{b}$ wrapped 中文😀 words words words words words words words words words words words words words words<br>second line | second |\n| | tail |\n\nafter table\n\n| x | y |\n| --- | --- |\n| third | fourth |\n\nlast"
}

pub(super) fn verify(editor: &Editor, geometry: &Geometry, window: &Window) -> Result<(), String> {
    let source = editor.document.source.id();
    let history = (
        editor.document.history_len(false),
        editor.document.history_len(true),
    );
    let display = &editor.viewport.display;
    if display.formulas.len() != 181 {
        return Err("low-risk smoke: missing formulas".into());
    }
    let end = display.text.find('\n').ok_or("missing paragraph end")?;
    let runs: Vec<_> = display
        .spans
        .iter()
        .map(|span| TextRun {
            len: span.display.len(),
            font: editor.fonts.body.clone(),
            color: rgb(theme::Palette::LIGHT.ink).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        })
        .collect();
    for width in [80., 180., 520., 760.] {
        let shaped = shape_inline(display, &runs, 0..end, theme::BODY, px(width), window)?;
        if shaped.len() < 2 {
            return Err("formula paragraph did not wrap".into());
        }
        let mut seen = 0;
        for (i, line) in shaped.iter().enumerate() {
            let row_end = shaped.get(i + 1).map_or(end, |next| next.start);
            let mixed = line.mixed.as_ref().ok_or("missing mixed formula line")?;
            let expected: Vec<_> = display
                .formulas
                .iter()
                .filter(|f| line.start <= f.display.start && f.display.end <= row_end)
                .collect();
            let actual: Vec<_> = mixed
                .pieces
                .iter()
                .filter_map(|piece| match &piece.content {
                    InlineContent::Math(formula) => Some((piece, formula)),
                    _ => None,
                })
                .collect();
            if expected.len() != actual.len() {
                return Err("formula row differs from containment oracle".into());
            }
            for (expected, (piece, formula)) in expected.iter().zip(&actual) {
                if piece.range
                    != (expected.display.start - line.start..expected.display.end - line.start)
                    || !std::sync::Arc::ptr_eq(formula, &expected.formula)
                    || (piece.width - px(formula.width * mixed.font_size)).abs() > px(0.01)
                    || mixed.baseline < px(formula.ascent * mixed.font_size)
                {
                    return Err("formula range, width or baseline changed".into());
                }
            }
            seen += actual.len();
        }
        if seen != 180 {
            return Err("formula was omitted or assigned more than once".into());
        }
    }

    // Compare both ordinary and table navigation against the former full scan,
    // including wraps, formula height, empty cells and entry/exit at both affinities.
    for row in geometry.rows.iter() {
        for offset in [row.range.start, row.range.end] {
            for affinity in [Affinity::Upstream, Affinity::Downstream] {
                let Some(current) = geometry.row_for(offset, affinity) else {
                    continue;
                };
                let index = geometry
                    .rows
                    .iter()
                    .position(|r| std::ptr::eq(r, current))
                    .ok_or("oracle could not find current row")?;
                let x = geometry.position_with_affinity(offset, affinity).x;
                for down in [false, true] {
                    let next = if let Some(cell) = geometry.lines[current.line].cell {
                        let mut same = geometry
                            .rows
                            .iter()
                            .filter(|r| geometry.lines[r.line].cell == Some(cell));
                        let next = if down {
                            same.find(|r| r.top > current.top)
                        } else {
                            same.rfind(|r| r.top < current.top)
                        };
                        next.or_else(|| {
                            let outside = geometry.rows.iter().filter(|r| {
                                if down {
                                    r.top >= cell.bottom()
                                } else {
                                    r.top + r.line_height <= cell.top()
                                }
                            });
                            let compare = |a: &&VisualRow, b: &&VisualRow| {
                                a.top
                                    .partial_cmp(&b.top)
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            };
                            if down {
                                outside.min_by(compare)
                            } else {
                                outside.max_by(compare)
                            }
                        })
                    } else if down {
                        geometry.rows.get(index + 1)
                    } else {
                        index.checked_sub(1).and_then(|i| geometry.rows.get(i))
                    };
                    let expected = next.map_or((offset, affinity), |target| {
                        geometry.hit_caret(point(
                            x,
                            geometry.bounds.top()
                                + px(geometry.page.top)
                                + target.top
                                + target.line_height / 2.
                                - geometry.scroll,
                        ))
                    });
                    if geometry.vertical_target(offset, affinity, x, down) != expected {
                        return Err("vertical navigation differs from oracle".into());
                    }
                }
            }
        }
    }
    if editor.document.source.id() != source
        || editor.document.dirty()
        || history
            != (
                editor.document.history_len(false),
                editor.document.history_len(true),
            )
    {
        return Err("layout checks changed source or history".into());
    }
    Ok(())
}
