use super::*;
impl EditorElement {
    pub(super) fn prepaint_frame(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Frame {
        let _profile = profile::Scope::new("layout_frame");
        let attempt = {
            let editor = self.editor.read(cx);
            let page = theme::Page::new(f32::from(bounds.size.width), editor.document.source_mode);
            let key = text_layout::LayoutKey::new(
                editor,
                page,
                window,
                &editor.viewport.control_index.cells,
            );
            if let Some(layout) = editor
                .viewport
                .geometry
                .as_ref()
                .map(|g| &g.text_layout)
                .filter(|l| l.key == key)
            {
                profile::layout(true);
                Ok(layout.clone())
            } else {
                text_layout::TextLayout::build(editor, key, window).map(std::sync::Arc::new)
            }
        };
        let layout = match attempt {
            Ok(layout) => layout,
            Err(message) => {
                self.editor.update(cx, |editor, cx| {
                    editor.document.invalidate_projection_cache();
                    let (state, _) = editor.document.projection_state(&[]);
                    editor.viewport.display =
                        std::sync::Arc::new(ryne_projection::IndexedDisplay::new(
                            Display::plain_source(state.source.text()),
                        ));
                    editor.viewport.frame_epoch = editor.viewport.frame_epoch.wrapping_add(1);
                    editor.viewport.geometry = None;
                    editor.viewport.render_controls = Default::default();
                    editor.viewport.control_index = Default::default();
                    editor.viewport.diagram_images.clear();
                    editor.menus.close_all();
                    editor.menus.hovered_block = None;
                    editor.menus.hovered_render = None;
                    editor
                        .document
                        .record_diagnostics(vec![ryne_extensions::Diagnostic {
                            feature: "layout".into(),
                            message,
                        }]);
                    cx.notify();
                });
                let editor = self.editor.read(cx);
                let page =
                    theme::Page::new(f32::from(bounds.size.width), editor.document.source_mode);
                let key = text_layout::LayoutKey::new(
                    editor,
                    page,
                    window,
                    &editor.viewport.control_index.cells,
                );
                std::sync::Arc::new(
                    text_layout::TextLayout::build(editor, key.clone(), window)
                        .unwrap_or_else(|_| text_layout::TextLayout::unavailable(editor, key)),
                )
            }
        };
        let editor = self.editor.read(cx);
        let display = &editor.viewport.display;
        let page = theme::Page::new(f32::from(bounds.size.width), editor.document.source_mode);
        let controls = &editor.viewport.control_index;
        let control_index = &controls.cells;
        let mut geometry = Geometry {
            snapshot: editor.document.source.id(),
            frame_epoch: editor.viewport.frame_epoch,
            display: layout.display.clone(),
            rows: layout.rows.clone(),
            text_layout: layout.clone(),
            bounds,
            page,
            source_mode: editor.document.source_mode,
            lines: layout.lines.clone(),
            scroll: editor.viewport.scroll,
            height: layout.height,
            math_painted: 0,
            diagrams_painted: 0,
            diagram_images: editor.viewport.diagram_images.clone(),
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
        if let Some(previous) = editor.viewport.geometry.as_ref().filter(|previous| {
            previous.snapshot == editor.document.source.id()
                && (previous.page != page
                    || previous.source_mode != editor.document.source_mode
                    || previous.display.formulas != display.formulas)
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
        if editor.viewport.ensure_caret {
            let position =
                geometry.position_with_affinity(caret, editor.caret_state.caret_affinity);
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
        let visible_top = geometry.scroll - px(page.top);
        let visible_bottom = visible_top + bounds.size.height;
        let mut code_panels = Vec::new();
        for index in layout.visibility.codes.visible(visible_top, visible_bottom) {
            let block = &display.code_blocks[index];
            let Some(&(first, last)) = layout.index.code_lines.get(&index) else {
                continue;
            };
            let first = &geometry.lines[first];
            let last = &geometry.lines[last];
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
            let indent = first.inset - px(16.);
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
                label: if let Some(control) = controls
                    .headers
                    .get(&(block.source.range.start, block.source.range.end))
                {
                    let mut control = control.clone();
                    if block.diagnostic.is_some() {
                        control.placement = ryne_extensions::RenderControlPlacement::Header {
                            label: label.text.to_string(),
                        };
                    }
                    let ryne_extensions::RenderControlPlacement::Header { label: title } =
                        &control.placement
                    else {
                        continue;
                    };
                    let width = (menu_text_width(title, 12., window) + px(32.))
                        .min((px(page.width) - indent - px(24.)).max(px(24.)));
                    let trigger = Bounds::new(
                        bounds.origin
                            + point(
                                px(page.left) + indent + px(8.),
                                px(page.top) + first.top - px(28.) - geometry.scroll,
                            ),
                        size(width, px(24.)),
                    );
                    geometry.render_regions.push(RenderRegion {
                        bounds: trigger,
                        trigger,
                        controls: vec![control],
                    });
                    if editor.viewport.dragging || editor.document.composition().is_some() {
                        Some(label)
                    } else {
                        None
                    }
                } else {
                    Some(label)
                },
            });
        }
        let mut table_quads = Vec::new();
        for i in layout.visibility.cells.visible(visible_top, visible_bottom) {
            let (mut cell, header, control_key) = layout.cells[i];
            let contributions = control_index.get(&control_key).cloned().unwrap_or_default();
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
        for i in layout.visibility.lists.visible(visible_top, visible_bottom) {
            let row = &display.lists[i];
            if !row.source.first {
                continue;
            }
            let Some(&line) = layout.index.first_line.get(&row.start) else {
                continue;
            };
            let line = &geometry.lines[line];
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
            let task = row.source.item.task.is_some();
            let numbered = row.source.item.label != "•";
            let marker_width = if task {
                px(24.)
                    + if numbered {
                        shaped.width + px(10.)
                    } else {
                        px(0.)
                    }
            } else {
                shaped.width
            };
            let origin = bounds.origin
                + point(
                    px(page.left) + line.inset - marker_width - px(10.),
                    px(page.top) + line.top - geometry.scroll,
                );
            geometry.list_markers.push((
                row.source.item.range.clone(),
                Bounds::new(origin, size(marker_width, line.line_height)),
            ));
            let contributions = controls
                .leading
                .get(&(row.source.item.range.start, row.source.item.marker.start))
                .cloned()
                .unwrap_or_default();
            if !contributions.is_empty() {
                let trigger = Bounds::new(
                    point(
                        origin.x - px(28.),
                        origin.y + (line.line_height - px(24.)) / 2.,
                    ),
                    size(px(24.), px(24.)),
                );
                let end = layout.index.item_bottom.get(&row.source.item.id).map_or(
                    origin.y + line.line_height,
                    |bottom| {
                        (geometry.bounds.top() + px(page.top) + *bottom - geometry.scroll)
                            .max(origin.y + line.line_height)
                    },
                );
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
            if row.source.item.task.is_some() {
                let contributions = controls
                    .markers
                    .get(&(row.source.item.range.start, row.source.item.marker.start))
                    .cloned()
                    .unwrap_or_default();
                if !contributions.is_empty() {
                    let trigger = Bounds::new(
                        point(
                            origin.x + marker_width - px(24.),
                            origin.y + (line.line_height - px(24.)) / 2.,
                        ),
                        size(px(24.), px(24.)),
                    );
                    geometry.render_regions.push(RenderRegion {
                        bounds: trigger,
                        trigger,
                        controls: contributions,
                    });
                }
            }
            if !task || numbered {
                list_labels.push((origin, shaped, line.line_height));
            }
        }
        let selected = editor.selected();
        let placed = geometry
            .render_regions
            .iter()
            .flat_map(|r| r.controls.iter().map(|c| (c.provider, c.id.clone())))
            .collect::<std::collections::BTreeSet<_>>();
        // Items whose first child is a code/table/diagram panel have no first
        // ListView text row. Their source body still provides a layout anchor.
        for i in layout
            .visibility
            .leading
            .visible(visible_top, visible_bottom)
        {
            let contribution = &editor.viewport.render_controls[i];
            if placed.contains(&(contribution.provider, contribution.id.clone())) {
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
            geometry.caret_bounds(caret, editor.caret_state.caret_affinity),
            rgb(theme::LINK),
        );
        let focused_block = (editor.menus.render_menu.is_none()
            && !editor.document.source_mode
            && !editor.menus.append_hovered
            && (editor.focus.is_focused(window) || editor.menus.hovered_block.is_some())
            && !editor.viewport.dragging
            && editor.document.composition().is_none())
        .then(|| editor.active_block())
        .flatten();
        let menu_open = editor.menus.block_menu.is_some();
        let block_insert = editor.menus.block_insert;
        let can_insert_above = menu_open
            && focused_block.as_ref().is_some_and(|block| {
                editor
                    .viewport
                    .block_cache
                    .can_insert_above(&editor.document.parse, block)
            });
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
        let append_menu_open = editor.menus.append_menu == Some(snapshot);
        let append_available = !editor.document.source_mode
            && !editor.viewport.dragging
            && editor.document.composition().is_none();
        let append_hovered = editor.menus.append_hovered;
        let render_view = RenderViewState::new(editor, window);
        let mut controls = Vec::new();
        prepaint_render_controls(
            &render_view,
            &self.editor,
            &mut geometry,
            &mut controls,
            window,
            cx,
        );
        if append_available {
            let origin = bounds.origin
                + point(
                    px(page.left),
                    px(page.top) + layout.body_height + px(12.) - geometry.scroll,
                );
            let zone = Bounds::new(
                point(bounds.left(), origin.y),
                size(bounds.size.width, px(APPEND_HEIGHT)),
            );
            geometry.append_zone = Some(zone);
            let control_origin = origin - point(px(40.), px(0.));
            let control_bounds = Bounds::new(control_origin, size(px(32.), px(28.)));
            if append_hovered
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
                    menu = menu.child(insert_type_picker(
                        self.editor.clone(),
                        snapshot,
                        None,
                        language,
                        &insert_templates,
                    ));
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
                    let source_open = source_scope.as_ref() == Some(&block.scope);
                    let title = language.block_name(spec.metadata.kind, spec.metadata.name);
                    let items: Vec<_> = spec
                        .menu
                        .iter()
                        .filter(|item| {
                            !matches!(item.command, BlockAction::AddAbove) || can_insert_above
                        })
                        .collect();
                    let mut menu_width = items
                        .iter()
                        .fold(
                            px(260.).max(menu_text_width(title, 12., window) + px(58.)),
                            |width, item| {
                                width.max(
                                    menu_text_width(
                                        language.block_action(item.command, source_open),
                                        13.,
                                        window,
                                    ) + px(34.),
                                )
                            },
                        )
                        .min(bounds.size.width);
                    if block_insert.is_some() {
                        let column_width = insert_templates
                            .iter()
                            .map(|(_, kind)| {
                                menu_text_width(&language.insert_label(*kind), 13., window)
                                    + px(34.)
                            })
                            .fold(px(0.), Pixels::max);
                        menu_width = menu_width.max(column_width * 2.).min(bounds.size.width);
                    }
                    let mut menu = div()
                        .id("block-menu")
                        .w(menu_width)
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
                                .child(title),
                        );
                    if let Some(before) = block_insert {
                        let entity = self.editor.clone();
                        menu = menu
                            .child(
                                div()
                                    .id("block-insert-back")
                                    .px_3()
                                    .py_2()
                                    .text_size(px(13.))
                                    .cursor_pointer()
                                    .rounded_md()
                                    .hover(|style| style.bg(rgb(theme::CODE_BG)))
                                    .child(format!(
                                        "‹  {}",
                                        language.text(if before {
                                            Text::AddAbove
                                        } else {
                                            Text::AddBelow
                                        })
                                    ))
                                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                        cx.stop_propagation();
                                        entity.update(cx, |editor, cx| {
                                            editor.menus.block_insert = None;
                                            cx.notify();
                                        });
                                    })
                                    .on_mouse_up(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    }),
                            )
                            .child(insert_type_picker(
                                self.editor.clone(),
                                snapshot,
                                Some((block.clone(), before)),
                                language,
                                &insert_templates,
                            ));
                    } else {
                        menu = menu.children(items.iter().map(|item| {
                            block_menu_item(
                                language.block_action(item.command, source_open),
                                item.command,
                                self.editor.clone(),
                                snapshot,
                                block.clone(),
                            )
                        }));
                    }
                    let mut menu = menu
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .into_any_element();
                    let menu_size = menu.layout_as_root(
                        size(menu_width.into(), AvailableSpace::MinContent),
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
        prepaint_render_menu(
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
