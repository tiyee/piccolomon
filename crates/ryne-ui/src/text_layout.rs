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
pub(super) fn control_index(
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
#[derive(Clone, PartialEq)]
pub(super) struct LayoutKey {
    snapshot: SnapshotId,
    epoch: u64,
    page: theme::Page,
    source_mode: bool,
    fonts: theme::Fonts,
    scale: f32,
    reserved: Vec<(ControlKey, u32)>,
}
impl LayoutKey {
    pub(super) fn new(
        editor: &Editor,
        page: theme::Page,
        window: &Window,
        controls: &BTreeMap<ControlKey, Vec<ryne_extensions::RenderControl>>,
    ) -> Self {
        Self {
            snapshot: editor.document.source.id(),
            epoch: editor.frame_epoch,
            page,
            source_mode: editor.document.source_mode,
            fonts: editor.fonts.clone(),
            scale: window.scale_factor(),
            reserved: controls
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
        }
    }
}
pub(super) struct TextLayout {
    pub key: LayoutKey,
    pub display: Arc<Display>,
    pub lines: Arc<Vec<Line>>,
    pub rows: Arc<Vec<VisualRow>>,
    pub cells: Vec<(Bounds<Pixels>, bool, ControlKey)>,
    pub height: Pixels,
    pub body_height: Pixels,
}
impl TextLayout {
    pub(super) fn build(editor: &Editor, key: LayoutKey, window: &mut Window) -> Self {
        let _profile = profile::Scope::new("text_layout_build");
        let display = &editor.display;
        let page = key.page;
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
                    let header = display
                        .tables
                        .iter()
                        .filter_map(|table| table.rows.first())
                        .flatten()
                        .any(|cell| cell.start < span.display.end && span.display.start < cell.end);
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
        while start <= display.text.len() {
            let code = display.code_row(start);
            if previous_code.is_some() && code != previous_code {
                top += px(12.);
            }
            top += px(spacing.enter(display, start));
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
                let inset = px(diagram.source.indent as f32 * 18.);
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
                .iter()
                .find(|table| table.range.start == start)
            {
                let inset = px(table.source.inset as f32);
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
                            ) {
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
                line_height = spacing.empty_height(display, start);
            }
            let list = display.list_row(start);
            let inset = code.map_or(
                list.map_or(px(0.), |row| px(row.source.item.inset as f32)),
                |index| px(16. + display.code_blocks[index].source.indent as f32 * 18.),
            );
            for layout in shape_inline(
                display,
                &runs,
                start..end,
                (font_size, line_height),
                (px(page.width) - inset - if code.is_some() { px(16.) } else { px(0.) })
                    .max(px(40.)),
                window,
            ) {
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
        Self {
            body_height: top,
            height: top
                + px(page.top + theme::BOTTOM)
                + if editor.document.source_mode {
                    px(0.)
                } else {
                    px(APPEND_HEIGHT)
                },
            key,
            display: Arc::new(display.clone()),
            lines: Arc::new(lines),
            rows: Arc::new(rows),
            cells: table_cells,
        }
    }
}
