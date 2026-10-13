use super::*;
pub(super) const BLOCK_CONTROL_HEIGHT: f32 = 24.;
#[derive(Clone)]
pub(super) struct Line {
    pub(super) shaped: WrappedLine,
    pub(super) start: usize,
    pub(super) top: Pixels,
    pub(super) line_height: Pixels,
    pub(super) diagram: Option<usize>,
    pub(super) code: Option<usize>,
    pub(super) inset: Pixels,
    pub(super) cell: Option<Bounds<Pixels>>,
    pub(super) align: TextAlign,
    pub(super) width: Pixels,
    pub(super) mixed: Option<MixedLine>,
}

#[derive(Clone)]
pub(super) enum InlineContent {
    Text(std::sync::Arc<ShapedLine>),
    Math(std::sync::Arc<ryne_projection::Formula>),
}

#[derive(Clone)]
pub(super) struct InlinePiece {
    pub(super) range: Range<usize>,
    pub(super) x: Pixels,
    pub(super) width: Pixels,
    pub(super) content: InlineContent,
}

#[derive(Clone)]
pub(super) struct MixedLine {
    pub(super) pieces: Vec<InlinePiece>,
    pub(super) width: Pixels,
    pub(super) baseline: Pixels,
    pub(super) font_size: f32,
}

impl Line {
    pub(super) fn x_for_index(&self, index: usize) -> Pixels {
        if self.diagram.is_some() {
            return if index == 0 { px(0.) } else { self.width };
        }
        if let Some(mixed) = &self.mixed {
            for piece in &mixed.pieces {
                if index < piece.range.end {
                    return piece.x
                        + match &piece.content {
                            InlineContent::Text(text) => {
                                text.x_for_index(index.saturating_sub(piece.range.start))
                            }
                            InlineContent::Math(_) => px(0.),
                        };
                }
            }
            mixed.width
        } else {
            self.shaped.unwrapped_layout.x_for_index(index)
        }
    }
}

pub(super) struct TypesetLine {
    pub(super) shaped: WrappedLine,
    pub(super) start: usize,
    pub(super) line_height: Pixels,
    pub(super) mixed: Option<MixedLine>,
}

pub(super) fn text_runs(display: &Display, runs: &[TextRun], range: Range<usize>) -> Vec<TextRun> {
    let start = display
        .spans
        .partition_point(|span| span.display.end <= range.start);
    display.spans[start..]
        .iter()
        .zip(&runs[start..])
        .take_while(|(span, _)| span.display.start < range.end)
        .filter_map(|(span, run)| {
            let start = span.display.start.max(range.start);
            let end = span.display.end.min(range.end);
            (start < end).then(|| TextRun {
                len: end - start,
                ..run.clone()
            })
        })
        .collect()
}

/// Formula boxes participate in wrapping and metrics, rather than overlaying text.
pub(super) fn shape_inline(
    display: &Display,
    runs: &[TextRun],
    range: Range<usize>,
    metrics: (f32, f32),
    width: Pixels,
    window: &Window,
) -> Vec<TypesetLine> {
    let (font_size, line_height) = metrics;
    // Text shaping is independent of focus and caret position. Keep each text
    // segment intact so glyph shaping, strike lines and code backgrounds remain continuous.
    let first_formula = display
        .formulas
        .partition_point(|f| f.display.start < range.start);
    let formulas = display.formulas[first_formula..]
        .iter()
        .take_while(|f| f.display.end <= range.end)
        .collect::<Vec<_>>();
    if formulas.is_empty() {
        return window
            .text_system()
            .shape_text(
                display.text[range.clone()].to_owned().into(),
                px(font_size),
                &text_runs(display, runs, range.clone()),
                Some(width),
                None,
            )
            .unwrap_or_default()
            .into_iter()
            .map(|shaped| TypesetLine {
                line_height: if shaped.len() == 0 {
                    px(line_height)
                } else {
                    px(line_height).max(
                        shaped.unwrapped_layout.ascent + shaped.unwrapped_layout.descent + px(4.),
                    )
                },
                shaped,
                start: range.start,
                mixed: None,
            })
            .collect();
    }
    let mut fragments = Vec::new();
    let mut offset = range.start;
    for formula in &formulas {
        fragments.push(LineFragment::text(
            &display.text[offset..formula.display.start],
        ));
        fragments.push(LineFragment::element(
            px(formula.formula.width * font_size),
            formula.display.len(),
        ));
        offset = formula.display.end;
    }
    fragments.push(LineFragment::text(&display.text[offset..range.end]));
    let face = text_runs(display, runs, range.clone())
        .first()
        .map_or_else(|| font(".SystemUIFont"), |r| r.font.clone());
    let mut wrapper = window.text_system().line_wrapper(face, px(font_size));
    let mut boundaries = vec![range.start];
    boundaries.extend(
        wrapper
            .wrap_line(&fragments, width)
            .map(|b| display.snap(range.start + b.ix))
            .filter(|b| range.start < *b && *b < range.end),
    );
    boundaries.push(range.end);
    if !range.is_empty() {
        boundaries.dedup();
    }
    let mut output = Vec::new();
    for pair in boundaries.windows(2) {
        let row = pair[0]..pair[1];
        let shaped = window
            .text_system()
            .shape_text(
                display.text[row.clone()].to_owned().into(),
                px(font_size),
                &text_runs(display, runs, row.clone()),
                None,
                None,
            )
            .unwrap_or_default()
            .into_iter()
            .next()
            .unwrap_or_default();
        let mut pieces = Vec::new();
        let mut x = px(0.);
        let mut ascent = shaped.unwrapped_layout.ascent;
        let mut descent = shaped.unwrapped_layout.descent;
        let mut offset = row.start;
        let mut append_text =
            |range: Range<usize>, pieces: &mut Vec<InlinePiece>, x: &mut Pixels| {
                if range.is_empty() {
                    return;
                }
                let text = window.text_system().shape_line(
                    display.text[range.clone()].to_owned().into(),
                    px(font_size),
                    &text_runs(display, runs, range.clone()),
                    None,
                );
                ascent = ascent.max(text.ascent);
                descent = descent.max(text.descent);
                pieces.push(InlinePiece {
                    range: range.start - row.start..range.end - row.start,
                    x: *x,
                    width: text.width,
                    content: InlineContent::Text(std::sync::Arc::new(text.clone())),
                });
                *x += text.width;
            };
        for formula in formulas
            .iter()
            .filter(|f| row.start <= f.display.start && f.display.end <= row.end)
        {
            append_text(offset..formula.display.start, &mut pieces, &mut x);
            let width = px(formula.formula.width * font_size);
            pieces.push(InlinePiece {
                range: formula.display.start - row.start..formula.display.end - row.start,
                x,
                width,
                content: InlineContent::Math(formula.formula.clone()),
            });
            x += width;
            offset = formula.display.end;
        }
        append_text(offset..row.end, &mut pieces, &mut x);
        for piece in &pieces {
            if let InlineContent::Math(formula) = &piece.content {
                ascent = ascent.max(px(formula.ascent * font_size));
                descent = descent.max(px(formula.descent * font_size));
            }
        }
        let height = if row.is_empty() {
            px(line_height)
        } else {
            px(line_height).max(ascent + descent + px(4.))
        };
        output.push(TypesetLine {
            shaped,
            start: row.start,
            line_height: height,
            mixed: Some(MixedLine {
                pieces,
                width: x,
                baseline: (height - ascent - descent) / 2. + ascent,
                font_size,
            }),
        });
    }
    output
}

#[derive(Clone)]
pub(super) struct VisualRow {
    pub(super) range: Range<usize>,
    pub(super) line: usize,
    pub(super) top: Pixels,
    pub(super) start_x: Pixels,
    pub(super) align_offset: Pixels,
    pub(super) line_height: Pixels,
    pub(super) newline: bool,
}

#[derive(Clone)]
pub(super) struct Geometry {
    pub(super) snapshot: SnapshotId,
    pub(super) frame_epoch: u64,
    pub(super) display: std::sync::Arc<Display>,
    pub(super) rows: std::sync::Arc<Vec<VisualRow>>,
    pub(super) text_layout: std::sync::Arc<text_layout::TextLayout>,
    pub(super) bounds: Bounds<Pixels>,
    pub(super) page: theme::Page,
    pub(super) source_mode: bool,
    pub(super) lines: std::sync::Arc<Vec<Line>>,
    pub(super) scroll: Pixels,
    pub(super) height: Pixels,
    pub(super) math_painted: usize,
    pub(super) diagrams_painted: usize,
    pub(super) diagram_images: Vec<std::sync::Arc<RenderImage>>,
    pub(super) diagram_paint_error: Option<(usize, String)>,
    pub(super) math_error: Option<String>,
    pub(super) block_control: Option<Bounds<Pixels>>,
    pub(super) block_target: Option<crate::block::Block>,
    pub(super) block_menu: Option<Bounds<Pixels>>,
    pub(super) append_zone: Option<Bounds<Pixels>>,
    pub(super) append_control: Option<Bounds<Pixels>>,
    pub(super) append_menu: Option<Bounds<Pixels>>,
    pub(super) list_markers: Vec<(Range<usize>, Bounds<Pixels>)>,
    pub(super) render_regions: Vec<RenderRegion>,
    pub(super) render_buttons: Vec<Bounds<Pixels>>,
    pub(super) render_menu_bounds: Vec<Bounds<Pixels>>,
}

impl Geometry {
    pub(super) fn is_current(&self, editor: &Editor) -> bool {
        self.snapshot == editor.document.source.id()
            // A cancelled preedit can return to the exact same projection before
            // another draw. Its original geometry is safe to reuse in that case.
            && (self.frame_epoch == editor.frame_epoch || self.display.as_ref() == &editor.display)
            && self.source_mode == editor.document.source_mode
    }

    pub(super) fn block_origin(&self, block: &crate::block::Block) -> Option<Point<Pixels>> {
        let offset = self.display.position_at(if block.is_list() {
            block.anchor
        } else {
            block.range.start
        });
        let row = self.row_for(offset, Affinity::Downstream)?;
        let line = &self.lines[row.line];
        // Diagram previews have no text line; keep their control at the top edge.
        let mut first_line_height = if line.diagram.is_some() {
            px(BLOCK_CONTROL_HEIGHT)
        } else {
            row.line_height
        };
        let mut origin = self.bounds.origin
            + point(
                px(self.page.left) + line.inset,
                px(self.page.top) + row.top - self.scroll,
            );
        if let Some(index) = self
            .display
            .code_blocks
            .iter()
            .position(|b| b.source.range == block.range)
        {
            let first = self.lines.iter().find(|line| line.code == Some(index))?;
            first_line_height = first.line_height;
            origin = self.bounds.origin
                + point(
                    px(self.page.left) + first.inset - px(16.),
                    px(self.page.top) + first.top - self.scroll,
                );
        } else if let Some(table) = self
            .display
            .tables
            .iter()
            .find(|t| t.source.range == block.range)
        {
            let start = table.rows.first()?.first()?.start;
            let first = self
                .lines
                .iter()
                .find(|line| line.start == start && line.cell.is_some())?;
            let cell = first.cell?;
            first_line_height = first.line_height;
            origin = self.bounds.origin
                + point(
                    px(self.page.left) + cell.left(),
                    px(self.page.top) + first.top - self.scroll,
                );
        } else if let Some((_, marker)) = self
            .list_markers
            .iter()
            .find(|(range, _)| range.start == block.range.start)
        {
            // Use the measured label bounds so long ordered markers keep the same gap.
            origin = marker.origin;
            first_line_height = marker.size.height;
        }
        let mut origin = origin
            + point(
                px(-40.),
                (first_line_height - px(BLOCK_CONTROL_HEIGHT)) / 2.,
            );
        for region in &self.render_regions {
            let trigger = region.trigger;
            // Keep the whole-block type menu to the left of item controls,
            // using their measured bounds without changing the text layout.
            if origin.y < trigger.bottom()
                && trigger.top() < origin.y + px(BLOCK_CONTROL_HEIGHT)
                && origin.x < trigger.right()
                && trigger.left() < origin.x + px(32.)
            {
                origin.x = trigger.left() - px(36.);
            }
        }
        Some(origin)
    }
    pub(super) fn row_for(&self, offset: usize, affinity: Affinity) -> Option<&VisualRow> {
        let contains = |row: &&VisualRow| row.range.start <= offset && offset <= row.range.end;
        if affinity == Affinity::Upstream {
            self.rows.iter().find(contains)
        } else {
            self.rows.iter().rev().find(contains)
        }
    }
    pub(super) fn line_height(&self, offset: usize) -> Pixels {
        self.row_for(offset, Affinity::Downstream)
            .map_or(px(LINE_HEIGHT), |row| row.line_height)
    }
    pub(super) fn position_in_row(&self, row: &VisualRow, offset: usize) -> Point<Pixels> {
        let line = &self.lines[row.line];
        let x = line.x_for_index(offset.saturating_sub(line.start)) - row.start_x;
        self.bounds.origin
            + point(
                px(self.page.left) + line.inset + row.align_offset + x,
                px(self.page.top) + row.top - self.scroll,
            )
    }
    pub(super) fn position_with_affinity(
        &self,
        offset: usize,
        affinity: Affinity,
    ) -> Point<Pixels> {
        self.row_for(offset, affinity).map_or(
            self.bounds.origin + point(px(self.page.left), px(self.page.top) - self.scroll),
            |row| self.position_in_row(row, offset),
        )
    }
    pub(super) fn position(&self, offset: usize) -> Point<Pixels> {
        self.position_with_affinity(offset, Affinity::Downstream)
    }
    pub(super) fn caret_bounds(&self, offset: usize, affinity: Affinity) -> Bounds<Pixels> {
        let mut origin = self.position_with_affinity(offset, affinity);
        origin.x -= px(theme::CARET_WIDTH / 2.);
        let Some(row) = self.row_for(offset, affinity) else {
            return Bounds::new(origin, size(px(theme::CARET_WIDTH), px(theme::BODY.0)));
        };
        let line = &self.lines[row.line];
        let layout = &line.shaped.unwrapped_layout;
        let mut font_size = layout.font_size;
        let mut top = (row.line_height - font_size) / 2.;
        if let Some(mixed) = &line.mixed {
            // Formula height expands the row, but the caret follows the text baseline.
            let local = offset.saturating_sub(line.start);
            let text = mixed.pieces.iter().find_map(|piece| {
                if piece.range.start <= local
                    && local <= piece.range.end
                    && let InlineContent::Text(text) = &piece.content
                {
                    Some(text)
                } else {
                    None
                }
            });
            let (ascent, descent) = text.map_or((layout.ascent, layout.descent), |text| {
                (text.ascent, text.descent)
            });
            font_size = px(mixed.font_size);
            top = mixed.baseline - ascent + (ascent + descent - font_size) / 2.;
        }
        let height = font_size.max(px(1.)).min(row.line_height);
        origin.y += top.max(px(0.)).min(row.line_height - height);
        Bounds::new(origin, size(px(theme::CARET_WIDTH), height))
    }
    pub(super) fn hit_caret(&self, position: Point<Pixels>) -> (usize, Affinity) {
        let local = position
            - self.bounds.origin
            - point(px(self.page.left), px(self.page.top) - self.scroll);
        let Some(first) = self.rows.first() else {
            return (0, Affinity::Downstream);
        };
        if local.y < first.top
            && !self
                .lines
                .iter()
                .filter_map(|line| line.cell)
                .any(|cell| cell.top() <= local.y && local.y < cell.bottom())
        {
            return (0, Affinity::Downstream);
        }
        if self
            .rows
            .last()
            .is_some_and(|row| local.y >= row.top + row.line_height)
            && !self
                .lines
                .iter()
                .filter_map(|line| line.cell)
                .any(|cell| cell.top() <= local.y && local.y < cell.bottom())
        {
            return (self.display.text.len(), Affinity::Upstream);
        }
        let cell = self
            .lines
            .iter()
            .filter_map(|line| line.cell)
            .filter(|cell| cell.top() <= local.y && local.y < cell.bottom())
            .min_by(|a, b| {
                let distance = |cell: &Bounds<Pixels>| {
                    (cell.left() - local.x).max(px(0.)) + (local.x - cell.right()).max(px(0.))
                };
                distance(a)
                    .partial_cmp(&distance(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        let mut candidates = self
            .rows
            .iter()
            .filter(|row| self.lines[row.line].cell == cell);
        let row = if cell.is_some() {
            candidates
                .find(|row| local.y < row.top + row.line_height)
                .or_else(|| {
                    self.rows
                        .iter()
                        .rev()
                        .find(|row| self.lines[row.line].cell == cell)
                })
        } else {
            candidates.min_by(|a, b| {
                let distance = |row: &VisualRow| {
                    (row.top - local.y).max(px(0.))
                        + (local.y - row.top - row.line_height).max(px(0.))
                };
                let inside =
                    |row: &VisualRow| row.top <= local.y && local.y < row.top + row.line_height;
                inside(b).cmp(&inside(a)).then_with(|| {
                    distance(a)
                        .partial_cmp(&distance(b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            })
        };
        let Some(row) = row else {
            return (self.display.text.len(), Affinity::Upstream);
        };
        let line = &self.lines[row.line];
        // GPUI's closest_index_for_x jumps to the end immediately after the
        // last glyph's origin. Pixel rounding can therefore skip a final emoji.
        // Compare the actual grapheme boundaries, including the row end.
        let target = local.x - line.inset - row.align_offset + row.start_x;
        let offset = self.display.text[row.range.clone()]
            .grapheme_indices(true)
            .map(|(index, _)| row.range.start + index)
            .chain(std::iter::once(row.range.end))
            .min_by(|a, b| {
                let distance = |index: usize| (line.x_for_index(index - line.start) - target).abs();
                distance(*a)
                    .partial_cmp(&distance(*b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(row.range.start);
        (
            offset,
            if offset == row.range.end {
                Affinity::Upstream
            } else {
                Affinity::Downstream
            },
        )
    }
    pub(super) fn vertical_target(
        &self,
        offset: usize,
        affinity: Affinity,
        x: Pixels,
        down: bool,
    ) -> (usize, Affinity) {
        let Some(row) = self.row_for(offset, affinity) else {
            return (offset, affinity);
        };
        if let Some(cell) = self.lines[row.line].cell {
            let mut same = self
                .rows
                .iter()
                .filter(|candidate| self.lines[candidate.line].cell == Some(cell));
            let target = if down {
                same.find(|candidate| candidate.top > row.top)
            } else {
                same.rfind(|candidate| candidate.top < row.top)
            };
            let target = target.or_else(|| {
                let outside = self.rows.iter().filter(|candidate| {
                    if down {
                        candidate.top >= cell.bottom()
                    } else {
                        candidate.top + candidate.line_height <= cell.top()
                    }
                });
                if down {
                    outside.min_by(|a, b| {
                        a.top
                            .partial_cmp(&b.top)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                } else {
                    outside.max_by(|a, b| {
                        a.top
                            .partial_cmp(&b.top)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                }
            });
            return target.map_or((offset, affinity), |target| {
                self.hit_caret(point(
                    x,
                    self.bounds.top() + px(self.page.top) + target.top + target.line_height / 2.
                        - self.scroll,
                ))
            });
        }
        let Some(index) = self
            .rows
            .iter()
            .position(|candidate| std::ptr::eq(candidate, row))
        else {
            return (offset, affinity);
        };
        let target = if down {
            self.rows.get(index + 1)
        } else {
            index.checked_sub(1).and_then(|i| self.rows.get(i))
        };
        let Some(target) = target else {
            return (offset, affinity);
        };
        let y = target.top + target.line_height / 2.;
        self.hit_caret(point(
            x,
            self.bounds.origin.y + px(self.page.top) + y - self.scroll,
        ))
    }
    pub(super) fn selection_quads(&self, selection: Range<usize>, color: Hsla) -> Vec<PaintQuad> {
        let mut quads = Vec::new();
        for row in self.rows.iter() {
            let start = selection.start.max(row.range.start);
            let end = selection.end.min(row.range.end);
            let newline =
                row.newline && selection.start <= row.range.end && row.range.end < selection.end;
            let empty_cell = row.range.is_empty()
                && self.lines[row.line].cell.is_some()
                && selection.start <= row.range.start
                && row.range.start < selection.end;
            if start < end || newline || empty_cell {
                let a = self.position_in_row(row, start.min(row.range.end));
                let b = self.position_in_row(row, end.max(start).min(row.range.end));
                let width = (b.x - a.x + if newline { px(6.) } else { px(0.) }).max(px(2.));
                quads.push(fill(Bounds::new(a, size(width, row.line_height)), color));
            }
        }
        quads
    }
}
