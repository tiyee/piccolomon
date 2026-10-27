//! Content layout is immutable; caret, selection, scrolling and menus are frame overlays.
use super::*;
use std::{collections::BTreeMap, sync::Arc};
pub(super) type ControlKey = (usize, usize, usize, usize);
pub(super) fn control_key(control: &ryne_extensions::RenderControl) -> ControlKey {
    (
        control.target.owner.start,
        control.target.owner.end,
        control.focus.start,
        control.focus.end,
    )
}
fn cell_key(table: &ryne_projection::TableView, row: usize, column: usize) -> ControlKey {
    let cell = &table.source.rows[row].cells[column];
    (
        table.source.range.start,
        table.source.range.end,
        cell.content.start,
        cell.content.end,
    )
}
fn control_index(
    controls: &[ryne_extensions::RenderControl],
) -> BTreeMap<ControlKey, Vec<ryne_extensions::RenderControl>> {
    let mut index: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for control in controls {
        index
            .entry(control_key(control))
            .or_default()
            .push(control.clone());
    }
    index
}
#[derive(Default)]
pub(super) struct ControlIndex {
    pub cells: BTreeMap<ControlKey, Vec<ryne_extensions::RenderControl>>,
    reserved: Arc<Vec<(ControlKey, u32)>>,
    generation: u64,
    pub headers: BTreeMap<(usize, usize), ryne_extensions::RenderControl>,
    pub leading: BTreeMap<(usize, usize), Vec<ryne_extensions::RenderControl>>,
    pub markers: BTreeMap<(usize, usize), Vec<ryne_extensions::RenderControl>>,
}
impl ControlIndex {
    pub fn new(controls: &[ryne_extensions::RenderControl]) -> Self {
        use ryne_extensions::RenderControlPlacement;
        let mut index = Self {
            cells: control_index(controls),
            ..Self::default()
        };
        // Preserve the previous sorted-cell precedence for competing headers.
        for control in index.cells.values().flatten() {
            if matches!(control.placement, RenderControlPlacement::Header { .. }) {
                let owner = &control.target.owner;
                index
                    .headers
                    .entry((owner.start, owner.end))
                    .or_insert_with(|| control.clone());
            }
        }
        for control in controls {
            let owner = &control.target.owner;
            match control.placement {
                RenderControlPlacement::Leading | RenderControlPlacement::Marker { .. } => {
                    if let [marker] = control.target.part.as_slice() {
                        let target = if control.placement == RenderControlPlacement::Leading {
                            &mut index.leading
                        } else {
                            &mut index.markers
                        };
                        target
                            .entry((owner.start, *marker))
                            .or_default()
                            .push(control.clone());
                    }
                }
                _ => {}
            }
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        index.generation = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        index.reserved = Arc::new(
            index
                .cells
                .iter()
                .map(|(key, controls)| {
                    (
                        *key,
                        controls
                            .iter()
                            .map(|c| c.trailing_space)
                            .fold(0., f32::max)
                            .to_bits(),
                    )
                })
                .collect(),
        );
        index
    }
}
#[derive(Clone, PartialEq)]
pub(super) struct LayoutKey {
    snapshot: SnapshotId,
    control_generation: u64,
    epoch: u64,
    page: theme::Page,
    source_mode: bool,
    fonts: theme::Fonts,
    scale: f32,
    reserved: Arc<Vec<(ControlKey, u32)>>,
}
impl LayoutKey {
    pub(super) fn new(
        editor: &Editor,
        page: theme::Page,
        window: &Window,
        _controls: &BTreeMap<ControlKey, Vec<ryne_extensions::RenderControl>>,
    ) -> Self {
        Self {
            snapshot: editor.document.source.id(),
            control_generation: editor.viewport.control_index.generation,
            epoch: editor.viewport.frame_epoch,
            page,
            source_mode: editor.document.source_mode,
            fonts: editor.fonts.clone(),
            scale: window.scale_factor(),
            reserved: editor.viewport.control_index.reserved.clone(),
        }
    }
}
pub(super) struct TextLayout {
    pub hit: crate::hit_index::HitIndex,
    pub index: crate::frame_index::FrameIndex,
    pub visibility: crate::visibility::Visibility,
    pub key: LayoutKey,
    pub display: Arc<ryne_projection::IndexedDisplay>,
    pub lines: Arc<Vec<Line>>,
    pub rows: Arc<Vec<VisualRow>>,
    pub cells: Vec<(Bounds<Pixels>, bool, ControlKey)>,
    pub height: Pixels,
    pub body_height: Pixels,
    pub quote_panels: Vec<QuotePanel>,
}
pub(super) struct QuotePanel {
    pub bounds: Bounds<Pixels>,
    pub icon: Bounds<Pixels>,
}
impl TextLayout {
    pub(super) fn build(
        editor: &Editor,
        key: LayoutKey,
        window: &mut Window,
    ) -> Result<Self, String> {
        let _profile = profile::Scope::new("text_layout_build");
        let display = &editor.viewport.display;
        display.validate_layout()?;
        let page = key.page;
        let mut header_ranges: Vec<_> = display
            .tables
            .iter()
            .filter_map(|t| t.rows.first())
            .flatten()
            .filter(|r| !r.is_empty())
            .cloned()
            .collect();
        header_ranges.sort_by_key(|r| r.start);
        let runs = display
            .spans
            .iter()
            .map(|span| {
                let style = span.style;
                let mut face = if style.source || style.code || style.code_block {
                    editor.fonts.code.clone()
                } else {
                    editor.fonts.body.clone()
                };
                if style.bold {
                    let header = header_ranges
                        .get(header_ranges.partition_point(|r| r.end <= span.display.start))
                        .is_some_and(|r| r.start < span.display.end);
                    face.weight = if header {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::BOLD
                    };
                } else if style.heading > 0 {
                    face.weight = FontWeight::SEMIBOLD;
                }
                if style.italic {
                    face = face.italic();
                }
                let color = rgb(if style.marker {
                    theme::MARKER
                } else if style.link {
                    theme::LINK
                } else if let Some(syntax) = style.syntax {
                    syntax.color()
                } else if style.heading > 0 {
                    theme::HEADING_INK
                } else {
                    theme::INK
                })
                .into();
                TextRun {
                    len: span.display.len(),
                    font: face,
                    color,
                    background_color: (style.code && !style.source)
                        .then(|| rgb(theme::CODE_BG).into()),
                    underline: style.link.then_some(UnderlineStyle {
                        color: None,
                        thickness: px(1.),
                        wavy: false,
                    }),
                    strikethrough: style.strike.then_some(StrikethroughStyle {
                        color: None,
                        thickness: px(1.),
                    }),
                }
            })
            .collect::<Vec<_>>();
        let mut top = px(0.);
        let mut start = 0;
        let mut lines = Vec::new();
        let mut previous_code = None;
        let mut table_cells = Vec::new();
        let mut spacing = presentation::FlowSpacing::default();
        let code_rows = display.code_row_index();
        while start <= display.text.len() {
            let code = code_rows.code_row(start);
            if previous_code.is_some() && code != previous_code {
                top += px(12.);
            }
            top += px(spacing.enter_indexed(display, start));
            if code.is_some() && code != previous_code {
                top += px(32.);
            }
            previous_code = code;
            if let Some((index, diagram)) = display
                .diagrams
                .iter()
                .enumerate()
                .find(|(_, view)| view.display.start == start)
            {
                display.checked_layout_advance(start, diagram.display.end)?;
                let quote = display.quote_at(start);
                let inset =
                    px((diagram.source.indent * 18 + quote.map_or(0, |q| q.depth * 14)) as f32)
                        .max(px(quote.map_or(0, |q| q.inset) as f32));
                let available = (px(page.width) - inset).max(px(40.));
                let scale = (f32::from(available) / diagram.diagram.width).min(1.);
                let height = px(diagram.diagram.height * scale + 24.);
                let shaped = window
                    .text_system()
                    .shape_text(
                        ryne_projection::OBJECT.into(),
                        px(1.),
                        &[TextRun {
                            len: ryne_projection::OBJECT.len(),
                            font: editor.fonts.body.clone(),
                            color: transparent_black(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                        None,
                    )
                    .unwrap_or_default()
                    .into_iter()
                    .next()
                    .unwrap_or_default();
                lines.push(Line {
                    shaped,
                    start,
                    top,
                    line_height: height,
                    diagram: Some(index),
                    code: None,
                    inset,
                    cell: None,
                    align: TextAlign::Left,
                    width: px(diagram.diagram.width * scale),
                    mixed: None,
                });
                top += height;
                start = diagram.display.end
                    + usize::from(display.text.as_bytes().get(diagram.display.end) == Some(&b'\n'));
                continue;
            }
            if let Some(table) = display
                .tables
                .binary_search_by_key(&start, |table| table.range.start)
                .ok()
                .map(|i| &display.tables[i])
            {
                display.checked_layout_advance(start, table.range.end)?;
                let quote = display.quote_at(start);
                let inset = px((table.source.inset + quote.map_or(0, |q| q.depth * 14)) as f32)
                    .max(px(quote.map_or(0, |q| q.inset) as f32));
                let cell_width =
                    ((px(page.width) - inset) / table.source.align.len() as f32).max(px(24.));
                for (row_index, row) in table.rows.iter().enumerate() {
                    let row_start = lines.len();
                    let mut row_height = px(theme::TABLE.1 + 16.);
                    for (column, cell) in row.iter().enumerate() {
                        let control_key = cell_key(table, row_index, column);
                        let reserve = key
                            .reserved
                            .binary_search_by_key(&control_key, |(key, _)| *key)
                            .ok()
                            .map_or(0., |i| f32::from_bits(key.reserved[i].1));
                        let rectangle = Bounds::new(
                            point(inset + cell_width * column, top),
                            size(cell_width, px(0.)),
                        );
                        let align = match table.source.align[column] {
                            editable_markdown_parser::Alignment::Center => TextAlign::Center,
                            editable_markdown_parser::Alignment::Right => TextAlign::Right,
                            _ => TextAlign::Left,
                        };
                        let mut segment_start = cell.start;
                        let mut used = px(0.);
                        for segment in display.text[cell.clone()].split('\n') {
                            let segment_end = segment_start + segment.len();
                            for layout in shape_inline(
                                display,
                                &runs,
                                segment_start..segment_end,
                                theme::TABLE,
                                (cell_width - px(24. + reserve)).max(px(4.)),
                                window,
                            )? {
                                let height = layout.shaped.size(layout.line_height).height;
                                lines.push(Line {
                                    shaped: layout.shaped,
                                    start: layout.start,
                                    top: top + px(8.) + used,
                                    line_height: layout.line_height,
                                    diagram: None,
                                    code: None,
                                    inset: rectangle.left() + px(12.),
                                    cell: Some(rectangle),
                                    align,
                                    width: (cell_width - px(24. + reserve)).max(px(4.)),
                                    mixed: layout.mixed,
                                });
                                used += height;
                            }
                            segment_start = segment_end + 1;
                        }
                        row_height = row_height.max(used + px(16.));
                    }
                    for line in &mut lines[row_start..] {
                        if let Some(cell) = &mut line.cell {
                            cell.size.height = row_height;
                        }
                    }
                    for column in 0..row.len() {
                        table_cells.push((
                            Bounds::new(
                                point(inset + cell_width * column, top),
                                size(cell_width, row_height),
                            ),
                            row_index == 0,
                            cell_key(table, row_index, column),
                        ));
                    }
                    top += row_height;
                }
                start = table.range.end;
                continue;
            }
            let text = display.text[start..].split('\n').next().unwrap_or("");
            let end = start + text.len();
            let first_span = display
                .spans
                .partition_point(|span| span.display.end <= start);
            let heading = display.spans[first_span..]
                .iter()
                .take_while(|span| span.display.start <= end)
                .find(|span| span.style.heading > 0)
                .map_or(0, |span| span.style.heading);
            let raw = editor.document.source_mode
                || display
                    .spans
                    .get(first_span)
                    .or_else(|| {
                        (start == display.text.len())
                            .then(|| display.spans.last())
                            .flatten()
                    })
                    .is_some_and(|span| span.style.source && span.display.start <= start);
            let (font_size, mut line_height) = if raw || code.is_some() {
                theme::CODE
            } else {
                heading_metrics(heading)
            };
            if text.is_empty() && code.is_none() && !raw {
                line_height = spacing.empty_height_indexed(display, start);
            }
            let list = display.list_row(start);
            let quote = display.quote_at(start);
            let inset = code
                .map_or(
                    list.map_or(px(0.), |row| {
                        px(row.source.item.inset as f32 + quote.map_or(0, |q| q.depth * 32) as f32)
                    }),
                    |index| {
                        px(16.
                            + display.code_blocks[index].source.indent as f32 * 18.
                            + quote.map_or(0, |q| q.depth * 14) as f32)
                    },
                )
                .max(
                    px(quote.map_or(0, |q| q.inset) as f32)
                        + if code.is_some() { px(16.) } else { px(0.) },
                );
            for layout in shape_inline(
                display,
                &runs,
                start..end,
                (font_size, line_height),
                (px(page.width)
                    - inset
                    - if code.is_some() {
                        px(16.)
                    } else if quote.is_some() {
                        px(12.)
                    } else {
                        px(0.)
                    })
                .max(px(40.)),
                window,
            )? {
                let height = layout.shaped.size(layout.line_height).height;
                lines.push(Line {
                    shaped: layout.shaped,
                    start: layout.start,
                    top,
                    line_height: layout.line_height,
                    diagram: None,
                    code,
                    inset,
                    cell: None,
                    align: TextAlign::Left,
                    width: px(0.),
                    mixed: layout.mixed,
                });
                top += height;
            }
            start = end + 1;
        }
        if previous_code.is_some() {
            top += px(12.);
        }
        let mut rows = Vec::new();
        for (line_index, line) in lines.iter().enumerate() {
            let mut starts = vec![0];
            starts.extend(line.shaped.wrap_boundaries.iter().map(|boundary| {
                line.shaped.unwrapped_layout.runs[boundary.run_ix].glyphs[boundary.glyph_ix].index
            }));
            let ends = starts
                .iter()
                .copied()
                .skip(1)
                .chain(std::iter::once(line.shaped.len()));
            for (row_index, (start, end)) in starts.iter().copied().zip(ends).enumerate() {
                rows.push(VisualRow {
                    range: line.start + start..line.start + end,
                    line: line_index,
                    top: line.top + line.line_height * row_index,
                    start_x: if line.mixed.is_some() {
                        px(0.)
                    } else {
                        line.x_for_index(start)
                    },
                    align_offset: {
                        let width = line.mixed.as_ref().map_or_else(
                            || line.x_for_index(end) - line.x_for_index(start),
                            |mixed| mixed.width,
                        );
                        match line.align {
                            TextAlign::Center => (line.width - width) / 2.,
                            TextAlign::Right => line.width - width,
                            _ => px(0.),
                        }
                    },
                    line_height: line.line_height,
                    newline: end == line.shaped.len()
                        && display.text.as_bytes().get(line.start + end) == Some(&b'\n'),
                });
            }
        }
        let quote_panels: Vec<QuotePanel> = display
            .quotes
            .iter()
            // One panel and leading icon for the complete outer quote. Nested
            // quote views still provide their indentation and source mapping.
            .filter(|quote| quote.depth == 1)
            .filter_map(|quote| {
                let first = lines.partition_point(|line| line.start < quote.display.start);
                let last = lines.partition_point(|line| {
                    line.start < quote.display.end
                        || (quote.empty_tail.is_some() && line.start == quote.display.end)
                });
                let content = lines.get(first..last)?;
                let first = content.first()?;
                let last = content.last()?;
                let left = px(quote.inset as f32 - 32.);
                let top = first.cell.map_or(first.top, |cell| cell.top())
                    - if first.code.is_some() {
                        px(32.)
                    } else {
                        px(0.)
                    };
                let bottom = (last.top
                    + last.shaped.size(last.line_height).height
                    + if last.code.is_some() { px(12.) } else { px(0.) })
                .max(last.cell.map_or(px(0.), |cell| cell.bottom()));
                Some(QuotePanel {
                    bounds: Bounds::new(
                        point(left, top),
                        size((px(page.width) - left).max(px(40.)), bottom - top),
                    ),
                    icon: Bounds::new(
                        point(
                            left + px(8.),
                            first.top
                                + (if first.diagram.is_some() {
                                    px(30.)
                                } else {
                                    first.line_height
                                } - px(18.))
                                    / 2.,
                        ),
                        size(px(18.), px(18.)),
                    ),
                })
            })
            .collect();
        let mut index = crate::frame_index::FrameIndex::build(
            lines.iter().map(|line| {
                (
                    line.start,
                    line.code,
                    line.top + line.shaped.size(line.line_height).height,
                )
            }),
            display
                .lists
                .iter()
                .map(|row| (row.source.item.id, row.start)),
        );
        index.visual_rows = ryne_syntax::RangeIndex::new(
            rows.iter()
                .enumerate()
                .map(|(i, r)| (r.range.clone(), i))
                .collect(),
        );
        let mut visibility = crate::visibility::Visibility::default();
        visibility.lines = crate::visibility::YIndex::new(
            lines
                .iter()
                .enumerate()
                .map(|(i, l)| (i, l.top, l.top + l.shaped.size(l.line_height).height)),
        );
        visibility.rows = crate::visibility::YIndex::new(
            rows.iter()
                .enumerate()
                .map(|(i, r)| (i, r.top, r.top + r.line_height)),
        );
        visibility.cells = crate::visibility::YIndex::new(
            table_cells
                .iter()
                .enumerate()
                .map(|(i, (b, _, _))| (i, b.top(), b.bottom())),
        );
        visibility.codes =
            crate::visibility::YIndex::new(index.code_lines.iter().map(|(&i, &(first, last))| {
                (
                    i,
                    lines[first].top - px(32.),
                    lines[last].top
                        + lines[last].shaped.size(lines[last].line_height).height
                        + px(12.),
                )
            }));
        visibility.lists = crate::visibility::YIndex::new(
            display
                .lists
                .iter()
                .enumerate()
                .filter(|(_, r)| r.source.first)
                .filter_map(|(i, r)| {
                    index.first_line.get(&r.start).map(|&l| {
                        (
                            i,
                            lines[l].top,
                            *index
                                .item_bottom
                                .get(&r.source.item.id)
                                .unwrap_or(&(lines[l].top + lines[l].line_height)),
                        )
                    })
                }),
        );
        visibility.quotes = crate::visibility::YIndex::new(
            quote_panels
                .iter()
                .enumerate()
                .map(|(i, p)| (i, p.bounds.top(), p.bounds.bottom())),
        );
        visibility.leading = crate::visibility::YIndex::new(
            editor
                .viewport
                .render_controls
                .iter()
                .enumerate()
                .filter(|(_, c)| c.placement == ryne_extensions::RenderControlPlacement::Leading)
                .filter_map(|(i, c)| {
                    let start = display.position_at(c.focus.start);
                    let end = display.position_at(c.focus.end);
                    let starts = index.visual_rows.at(start);
                    let ends = index.visual_rows.at(end);
                    let first = rows.get(*starts.last()?)?;
                    let last = rows.get(*ends.first()?)?;
                    Some((i, first.top, last.top + last.line_height))
                }),
        );
        Ok(Self {
            hit: crate::hit_index::HitIndex::new(
                rows.iter()
                    .enumerate()
                    .map(|(i, r)| (i, r.top, r.top + r.line_height, lines[r.line].cell)),
            ),
            index,
            visibility,
            body_height: top,
            height: top
                + px(page.top + theme::BOTTOM)
                + if editor.document.source_mode {
                    px(0.)
                } else {
                    px(APPEND_HEIGHT)
                },
            key,
            display: display.clone(),
            lines: Arc::new(lines),
            rows: Arc::new(rows),
            cells: table_cells,
            quote_panels,
        })
    }

    /// A failed recovery never publishes partial geometry. No text can be hit
    /// until a later successful frame replaces this inert placeholder.
    pub(super) fn unavailable(editor: &Editor, key: LayoutKey) -> Self {
        Self {
            key,
            display: editor.viewport.display.clone(),
            hit: Default::default(),
            index: Default::default(),
            visibility: Default::default(),
            lines: Default::default(),
            rows: Default::default(),
            cells: Vec::new(),
            height: px(0.),
            body_height: px(0.),
            quote_panels: Vec::new(),
        }
    }
}
