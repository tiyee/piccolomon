use super::*;
impl EditorElement {
    pub(super) fn prepaint_frame(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Frame {
        let _profile = profile::Scope::new("layout_frame");
        let editor = self.editor.read(cx);
        let display = &editor.display;
        let page = theme::Page::new(f32::from(bounds.size.width), editor.document.source_mode);
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
                        let reserve = editor
                            .render_controls
                            .iter()
                            .filter(|c| {
                                c.target.owner == table.source.range
                                    && c.focus == table.source.rows[row_index].cells[column].content
                            })
                            .map(|c| c.trailing_space)
                            .fold(0., f32::max);
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
                            editor
                                .render_controls
                                .iter()
                                .filter(|c| {
                                    c.target.owner == table.source.range
                                        && c.focus
                                            == table.source.rows[row_index].cells[column].content
                                })
                                .cloned()
                                .collect::<Vec<_>>(),
                        ));
                    }
                    top += row_height;
                }
                start = table.range.end;
                continue;
            }
            let text = display.text[start..].split('\n').next().unwrap_or("");
            let end = start + text.len();
            let heading = display
                .spans
                .iter()
                .find(|span| {
                    span.display.start <= end && start < span.display.end && span.style.heading > 0
                })
                .map_or(0, |span| span.style.heading);
            let raw = editor.document.source_mode
                || display.spans.iter().any(|span| {
                    span.style.source
                        && span.display.start <= start
                        && (start < span.display.end || start == display.text.len())
                });
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
        let mut geometry = Geometry {
            snapshot: editor.document.source.id(),
            frame_epoch: editor.frame_epoch,
            display: display.clone(),
            rows: Vec::new(),
            bounds,
            page,
            source_mode: editor.document.source_mode,
            lines,
            scroll: editor.scroll,
            height: top
                + px(page.top + theme::BOTTOM)
                + if editor.document.source_mode {
                    px(0.)
                } else {
                    px(APPEND_HEIGHT)
                },
            math_painted: 0,
            diagrams_painted: 0,
            diagram_images: editor.diagram_images.clone(),
            diagram_paint_error: None,
            math_error: None,
            block_control: None,
            block_target: None,
            block_menu: None,
            append_zone: None,
            append_control: None,
            append_menu: None,
            list_markers: Vec::new(),
            render_regions: Vec::new(),
            render_buttons: Vec::new(),
            render_menu_bounds: Vec::new(),
        };
        for (line_index, line) in geometry.lines.iter().enumerate() {
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
                geometry.rows.push(VisualRow {
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
        if let Some(previous) = editor.geometry.as_ref().filter(|previous| {
            previous.snapshot == editor.document.source.id()
                && (previous.page != page || previous.source_mode != editor.document.source_mode)
        }) && let Some(row) = previous
            .rows
            .iter()
            .find(|row| px(previous.page.top) + row.top + row.line_height > previous.scroll)
        {
            let source = previous
                .display
                .source_at(row.range.start, Affinity::Downstream);
            let offset = display.position_at(source);
            if let Some(next) = geometry.row_for(offset, Affinity::Downstream) {
                geometry.scroll = (px(page.top) + next.top
                    - (px(previous.page.top) + row.top - previous.scroll))
                    .max(px(0.));
            }
        }
        let caret = display.snap(display.position_at(editor.caret()));
        if editor.ensure_caret {
            let position = geometry.position_with_affinity(caret, editor.caret_affinity);
            if position.y < bounds.top() + px(8.) {
                geometry.scroll =
                    (geometry.scroll - (bounds.top() + px(8.) - position.y)).max(px(0.));
            } else if position.y + geometry.line_height(caret) > bounds.bottom() - px(8.) {
                geometry.scroll +=
                    position.y + geometry.line_height(caret) - bounds.bottom() + px(8.);
            }
        }
        geometry.scroll = geometry
            .scroll
            .clamp(px(0.), (geometry.height - bounds.size.height).max(px(0.)));
        let mut code_panels = Vec::new();
        for (index, block) in display.code_blocks.iter().enumerate() {
            let mut block_lines = geometry
                .lines
                .iter()
                .filter(|line| line.code == Some(index));
            let Some(first) = block_lines.next() else {
                continue;
            };
            let last = block_lines.next_back().unwrap_or(first);
            let label = block
                .source
                .language
                .split_whitespace()
                .next()
                .unwrap_or("code");
            let label = block.diagnostic.as_ref().map_or_else(
                || label.to_owned(),
                |error| {
                    format!(
                        "mermaid · {}: {}",
                        editor.locale.language.text(Text::DiagramFailed),
                        error
                            .replace(['\n', '\r', '\t'], " ")
                            .chars()
                            .take(90)
                            .collect::<String>()
                    )
                },
            );
            let label = window.text_system().shape_line(
                label.clone().into(),
                px(12.),
                &[TextRun {
                    len: label.len(),
                    font: editor.fonts.code.clone(),
                    color: rgb(theme::MUTED).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            );
            let indent = px(block.source.indent as f32 * 18.);
            code_panels.push(CodePanel {
                bounds: Bounds::new(
                    bounds.origin
                        + point(
                            px(page.left) + indent,
                            px(page.top) + first.top - px(32.) - geometry.scroll,
                        ),
                    size(
                        (px(page.width) - indent).max(px(40.)),
                        last.top + last.shaped.size(last.line_height).height + px(12.) - first.top
                            + px(32.),
                    ),
                ),
                label,
            });
        }
        let mut table_quads = Vec::new();
        for (mut cell, header, contributions) in table_cells {
            cell.origin += bounds.origin + point(px(page.left), px(page.top) - geometry.scroll);
            if !contributions.is_empty() {
                geometry.render_regions.push(RenderRegion {
                    bounds: cell,
                    trigger: Bounds::new(
                        point(cell.right() - px(28.), cell.top() + px(10.)),
                        size(px(24.), px(24.)),
                    ),
                    controls: contributions,
                });
            }
            table_quads.push(fill(cell, rgb(if header { 0xeeefed } else { 0xffffff })));
            for edge in [
                Bounds::new(cell.origin, size(cell.size.width, px(1.))),
                Bounds::new(
                    point(cell.left(), cell.bottom() - px(1.)),
                    size(cell.size.width, px(1.)),
                ),
                Bounds::new(cell.origin, size(px(1.), cell.size.height)),
                Bounds::new(
                    point(cell.right() - px(1.), cell.top()),
                    size(px(1.), cell.size.height),
                ),
            ] {
                table_quads.push(fill(edge, rgb(theme::BORDER)));
            }
        }
        let mut list_labels = Vec::new();
        for row in &display.lists {
            if !row.source.first {
                continue;
            }
            let Some(line) = geometry.lines.iter().find(|line| line.start == row.start) else {
                continue;
            };
            let label = &row.source.item.label;
            let shaped = window.text_system().shape_line(
                label.clone().into(),
                px(18.),
                &[TextRun {
                    len: label.len(),
                    font: editor.fonts.body.clone(),
                    color: rgb(theme::MUTED).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            );
            let origin = bounds.origin
                + point(
                    px(page.left) + line.inset - shaped.width - px(10.),
                    px(page.top) + line.top - geometry.scroll,
                );
            geometry.list_markers.push((
                row.source.item.range.clone(),
                Bounds::new(origin, size(shaped.width, line.line_height)),
            ));
            let contributions = editor
                .render_controls
                .iter()
                .filter(|c| {
                    c.placement == ryne_extensions::RenderControlPlacement::Leading
                        && c.target.part == [row.source.item.marker.start]
                        && c.target.owner.start == row.source.item.range.start
                })
                .cloned()
                .collect::<Vec<_>>();
            if !contributions.is_empty() {
                let trigger = Bounds::new(
                    point(
                        origin.x - px(28.),
                        origin.y + (line.line_height - px(24.)) / 2.,
                    ),
                    size(px(24.), px(24.)),
                );
                let end = display
                    .lists
                    .iter()
                    .filter(|r| r.source.item.id == row.source.item.id)
                    .filter_map(|r| geometry.lines.iter().find(|l| l.start == r.start))
                    .map(|l| {
                        geometry.bounds.top()
                            + px(page.top)
                            + l.top
                            + l.shaped.size(l.line_height).height
                            - geometry.scroll
                    })
                    .fold(origin.y + line.line_height, Pixels::max);
                let bounds = Bounds::new(
                    point(trigger.left(), origin.y),
                    size(
                        (geometry.bounds.left() + px(page.left + page.width) - trigger.left())
                            .max(px(24.)),
                        end - origin.y,
                    ),
                );
                geometry.render_regions.push(RenderRegion {
                    bounds,
                    trigger,
                    controls: contributions,
                });
            }
            list_labels.push((origin, shaped, line.line_height));
        }
        let selected = editor.selected();
        // Items whose first child is a code/table/diagram panel have no first
        // ListView text row. Their source body still provides a layout anchor.
        for contribution in editor
            .render_controls
            .iter()
            .filter(|c| c.placement == ryne_extensions::RenderControlPlacement::Leading)
        {
            if geometry.render_regions.iter().any(|r| {
                r.controls
                    .iter()
                    .any(|c| c.provider == contribution.provider && c.id == contribution.id)
            }) {
                continue;
            }
            let start = display.position_at(contribution.focus.start);
            let end = display.position_at(contribution.focus.end);
            let Some(first) = geometry.row_for(start, Affinity::Downstream) else {
                continue;
            };
            let Some(last) = geometry.row_for(end, Affinity::Upstream) else {
                continue;
            };
            let left = geometry.bounds.left() + px(page.left) + geometry.lines[first.line].inset;
            let top = geometry.bounds.top() + px(page.top) + first.top - geometry.scroll;
            let bottom = geometry.bounds.top() + px(page.top) + last.top + last.line_height
                - geometry.scroll;
            let trigger = Bounds::new(
                point(left - px(44.), top + (first.line_height - px(24.)) / 2.),
                size(px(24.), px(24.)),
            );
            let bounds = Bounds::new(
                point(trigger.left(), top),
                size(
                    (geometry.bounds.left() + px(page.left + page.width) - trigger.left())
                        .max(px(24.)),
                    (bottom - top).max(first.line_height),
                ),
            );
            geometry.render_regions.push(RenderRegion {
                bounds,
                trigger,
                controls: vec![contribution.clone()],
            });
        }
        let selected = display.position_at(selected.start)..display.position_at(selected.end);
        let selections = geometry.selection_quads(selected, rgba(0x9abfe966).into());
        let cursor = fill(
            geometry.caret_bounds(caret, editor.caret_affinity),
            rgb(theme::LINK),
        );
        let focused_block = (!editor.document.source_mode
            && !editor.append_hovered
            && (editor.focus.is_focused(window) || editor.hovered_block.is_some())
            && !editor.dragging
            && editor.document.composition.is_none())
        .then(|| editor.active_block())
        .flatten();
        let menu_open = editor.block_menu.is_some();
        let source_scope = editor
            .document
            .block_source
            .as_ref()
            .map(|session| session.range.clone());
        let spec = focused_block.as_ref().map(|block| {
            editor
                .document
                .block_ui(block, source_scope.as_ref() == Some(&block.scope))
        });
        let language = editor.locale.language;
        let insert_templates = editor.document.insert_templates().to_vec();
        let snapshot = editor.document.source.id();
        let append_menu_open = editor.append_menu == Some(snapshot);
        let render_view = RenderViewState::new(editor, window);
        let mut controls = Vec::new();
        if !editor.document.source_mode && !editor.dragging && editor.document.composition.is_none()
        {
            let origin = bounds.origin
                + point(
                    px(page.left),
                    px(page.top) + top + px(12.) - geometry.scroll,
                );
            let zone = Bounds::new(
                point(bounds.left(), origin.y),
                size(bounds.size.width, px(APPEND_HEIGHT)),
            );
            geometry.append_zone = Some(zone);
            let control_origin = origin - point(px(40.), px(0.));
            let control_bounds = Bounds::new(control_origin, size(px(32.), px(28.)));
            if editor.append_hovered
                && control_origin.y >= bounds.top()
                && control_bounds.bottom() <= bounds.bottom()
            {
                geometry.append_control = Some(control_bounds);
                let entity = self.editor.clone();
                let mut control = div()
                    .id("append-control")
                    .w(px(32.))
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .text_size(px(22.))
                    .text_color(rgb(0x858b93))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(0xe8e7e2)).text_color(rgb(theme::LINK)))
                    .child("+")
                    .on_hover(move |hovered, _, cx| {
                        if *hovered {
                            entity.update(cx, |editor, cx| {
                                if !editor.smoke_mode {
                                    editor.open_append_menu(snapshot, cx);
                                }
                            });
                        }
                    })
                    .on_mouse_down(MouseButton::Left, {
                        let entity = self.editor.clone();
                        move |_, _, cx| {
                            cx.stop_propagation();
                            entity.update(cx, |editor, cx| editor.open_append_menu(snapshot, cx));
                        }
                    })
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .into_any_element();
                control.prepaint_as_root(
                    control_origin,
                    size(px(32.).into(), px(28.).into()),
                    window,
                    cx,
                );
                controls.push(control);
                if append_menu_open {
                    let mut menu = div()
                        .id("append-menu")
                        .w(px(380.).min(bounds.size.width))
                        .p_1()
                        .bg(rgb(0xfdfcf9))
                        .text_color(rgb(0x363c44))
                        .border_1()
                        .border_color(rgb(0xdeddd7))
                        .rounded_md()
                        .shadow_md()
                        .cursor_default()
                        .child(
                            div()
                                .px_3()
                                .py_2()
                                .text_size(px(12.))
                                .text_color(rgb(0x858b93))
                                .child(language.text(Text::AddContent)),
                        );
                    for items in insert_templates.chunks(2) {
                        let mut row = div().flex();
                        for &(label, kind) in items {
                            let entity = self.editor.clone();
                            row = row.child(
                                div()
                                    .id(label)
                                    .flex_1()
                                    .min_w_0()
                                    .px_3()
                                    .py_2()
                                    .text_size(px(13.))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(theme::CODE_BG)))
                                    .child(language.insert_label(kind))
                                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                        cx.stop_propagation();
                                        entity.update(cx, |editor, cx| {
                                            editor.append_block(snapshot, kind, window, cx)
                                        });
                                    })
                                    .on_mouse_up(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    }),
                            );
                        }
                        menu = menu.child(row);
                    }
                    let mut menu = menu
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .into_any_element();
                    let menu_size = menu.layout_as_root(
                        size(
                            px(380.).min(bounds.size.width).into(),
                            AvailableSpace::MinContent,
                        ),
                        window,
                        cx,
                    );
                    let menu_origin = point(
                        (control_bounds.right() + px(4.))
                            .min(bounds.right() - menu_size.width)
                            .max(bounds.left()),
                        control_origin
                            .y
                            .min(bounds.bottom() - menu_size.height)
                            .max(bounds.top()),
                    );
                    geometry.append_menu = Some(Bounds::new(menu_origin, menu_size));
                    menu.prepaint_at(menu_origin, window, cx);
                    controls.push(menu);
                }
            }
        }
        if let Some((block, spec)) = focused_block.zip(spec)
            && let Some(origin) = geometry.block_origin(&block)
        {
            let control_bounds = Bounds::new(origin, size(px(32.), px(BLOCK_CONTROL_HEIGHT)));
            // A scrolled-off block does not get a floating control over another block.
            if origin.y >= bounds.top() && control_bounds.bottom() <= bounds.bottom() {
                geometry.block_control = Some(control_bounds);
                geometry.block_target = Some(block.clone());
                let entity = self.editor.clone();
                let target = block.clone();
                let icon = block_type_icon(
                    &spec.metadata,
                    if menu_open { theme::LINK } else { 0x858b93 },
                );
                let mut control = div()
                    .id("block-control")
                    .group("block-control")
                    .w(px(32.))
                    .h(px(BLOCK_CONTROL_HEIGHT))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .text_color(rgb(0x858b93))
                    .cursor_pointer()
                    .when(menu_open, |style| {
                        style.bg(rgb(0xe8e7e2)).text_color(rgb(theme::LINK))
                    })
                    .hover(|style| style.bg(rgb(0xe8e7e2)).text_color(rgb(0x454b53)))
                    .child(icon)
                    .on_hover(move |hovered, _, cx| {
                        if *hovered {
                            entity.update(cx, |editor, cx| {
                                if !editor.smoke_mode {
                                    editor.open_block_menu(snapshot, target.clone(), cx);
                                }
                            });
                        }
                    })
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .into_any_element();
                control.prepaint_as_root(
                    origin,
                    size(px(32.).into(), px(BLOCK_CONTROL_HEIGHT).into()),
                    window,
                    cx,
                );
                controls.push(control);
                if menu_open {
                    let mut menu = div()
                        .id("block-menu")
                        .w(px(260.).min(bounds.size.width))
                        .p_1()
                        .bg(rgb(0xfdfcf9))
                        .text_color(rgb(0x363c44))
                        .border_1()
                        .border_color(rgb(0xdeddd7))
                        .rounded_md()
                        .shadow_md()
                        .cursor_default()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .px_3()
                                .py_2()
                                .text_size(px(12.))
                                .text_color(rgb(0x858b93))
                                .child(block_type_icon(&spec.metadata, 0x858b93))
                                .child(language.block_name(spec.metadata.icon, spec.metadata.name)),
                        )
                        .children(spec.menu.iter().map(|item| {
                            block_menu_item(
                                language.text(match item.command {
                                    BlockAction::Source
                                        if source_scope.as_ref() == Some(&block.scope) =>
                                    {
                                        Text::FoldSource
                                    }
                                    BlockAction::Source => Text::EditSource,
                                    BlockAction::Select => Text::SelectBlock,
                                    BlockAction::Copy => Text::CopyBlock,
                                }),
                                item.command,
                                self.editor.clone(),
                                snapshot,
                                block.clone(),
                            )
                        }))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .into_any_element();
                    let menu_size = menu.layout_as_root(
                        size(
                            px(260.).min(bounds.size.width).into(),
                            AvailableSpace::MinContent,
                        ),
                        window,
                        cx,
                    );
                    let menu_origin = point(
                        origin
                            .x
                            .min(bounds.right() - menu_size.width)
                            .max(bounds.left()),
                        (origin.y + px(28.))
                            .min(bounds.bottom() - menu_size.height)
                            .max(bounds.top()),
                    );
                    geometry.block_menu = Some(Bounds::new(menu_origin, menu_size));
                    menu.prepaint_at(menu_origin, window, cx);
                    controls.push(menu);
                }
            }
        }
        prepaint_render_controls(
            &render_view,
            &self.editor,
            &mut geometry,
            &mut controls,
            window,
            cx,
        );
        Frame {
            geometry,
            selections,
            cursor,
            code_panels,
            list_labels,
            table_quads,
            controls,
        }
    }
}
