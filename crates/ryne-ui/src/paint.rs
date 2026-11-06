use super::*;
impl EditorElement {
    pub(super) fn paint_frame(
        &mut self,
        bounds: Bounds<Pixels>,
        frame: &mut Frame,
        window: &mut Window,
        cx: &mut App,
    ) {
        let _profile = profile::Scope::new("paint");
        let editor = self.editor.read(cx);
        let focus = editor.focus.clone();
        let show_cursor = editor.selected().is_empty() && editor.caret_state.caret_visible;
        let marked = editor.document.composition().map(|c| {
            let start = editor.viewport.display.position_at(c.range.start);
            let end = editor
                .viewport
                .display
                .position_at(c.range.start + c.text.len());
            start..end
        });
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            let quote_origin = bounds.origin
                + point(
                    px(frame.geometry.page.left),
                    px(frame.geometry.page.top) - frame.geometry.scroll,
                );
            let top = frame.geometry.scroll - px(frame.geometry.page.top);
            let bottom = top + bounds.size.height;
            for i in frame
                .geometry
                .text_layout
                .visibility
                .quotes
                .visible(top, bottom)
            {
                let panel = &frame.geometry.text_layout.quote_panels[i];
                let mut background = panel.bounds;
                background.origin += quote_origin;
                window.paint_quad(fill(background, rgb(theme::QUOTE_BG)).corner_radii(px(6.)));
            }
            for quad in &frame.table_quads {
                window.paint_quad(quad.clone());
            }
            for panel in &frame.code_panels {
                window.paint_quad(fill(panel.bounds, rgb(theme::CODE_BG)).corner_radii(px(6.)));
                if let Some(label) = &panel.label
                    && panel.bounds.top() + px(6.) < bounds.bottom()
                    && panel.bounds.top() + px(26.) > bounds.top()
                {
                    let _ = label.paint(
                        panel.bounds.origin + point(px(16.), px(6.)),
                        px(20.),
                        window,
                        cx,
                    );
                }
            }
            for i in frame
                .geometry
                .text_layout
                .visibility
                .quotes
                .visible(top, bottom)
            {
                let panel = &frame.geometry.text_layout.quote_panels[i];
                let mut icon = panel.icon;
                icon.origin += quote_origin;
                if !icon.intersects(&bounds) {
                    continue;
                }
                let _ = window.paint_svg(
                    icon,
                    "icon:quoter".into(),
                    TransformationMatrix::default(),
                    rgb(theme::MUTED).into(),
                    cx,
                );
            }
            for (origin, label, height) in &frame.list_labels {
                if origin.y + *height < bounds.top() || origin.y > bounds.bottom() {
                    continue;
                }
                let _ = label.paint(*origin, *height, window, cx);
            }
            for selection in frame.selections.drain(..) {
                window.paint_quad(selection);
            }
            for i in frame
                .geometry
                .text_layout
                .visibility
                .lines
                .visible(top, bottom)
            {
                let line = &frame.geometry.lines[i];
                let mut origin = bounds.origin
                    + point(
                        px(frame.geometry.page.left) + line.inset,
                        px(frame.geometry.page.top) + line.top - frame.geometry.scroll,
                    );
                if origin.y + line.shaped.size(line.line_height).height < bounds.top()
                    || origin.y > bounds.bottom()
                {
                    continue;
                }
                if let Some(index) = line.diagram {
                    let result = window.paint_image(
                        Bounds::new(
                            origin + point(px(0.), px(12.)),
                            size(line.width, line.line_height - px(24.)),
                        ),
                        Corners::default(),
                        frame.geometry.diagram_images[index].clone(),
                        0,
                        false,
                    );
                    match result {
                        Ok(()) => frame.geometry.diagrams_painted += 1,
                        Err(error) => {
                            frame.geometry.diagram_paint_error = Some((index, error.to_string()))
                        }
                    }
                    continue;
                }
                if let Some(mixed) = &line.mixed {
                    origin.x += match line.align {
                        TextAlign::Center => (line.width - mixed.width) / 2.,
                        TextAlign::Right => line.width - mixed.width,
                        _ => px(0.),
                    };
                    for piece in &mixed.pieces {
                        if let InlineContent::Text(text) = &piece.content {
                            let at = origin + point(piece.x, mixed.baseline - text.ascent);
                            let _ =
                                text.paint_background(at, text.ascent + text.descent, window, cx);
                        }
                    }
                    for piece in &mixed.pieces {
                        match &piece.content {
                            InlineContent::Text(text) => {
                                let at = origin + point(piece.x, mixed.baseline - text.ascent);
                                let height = text.ascent + text.descent;
                                let _ = text.paint(at, height, window, cx);
                            }
                            InlineContent::Math(formula) => {
                                let at = origin
                                    + point(
                                        piece.x,
                                        mixed.baseline - px(formula.ascent * mixed.font_size),
                                    );
                                match window.paint_svg(
                                    Bounds::new(
                                        at,
                                        size(
                                            piece.width,
                                            px((formula.ascent + formula.descent)
                                                * mixed.font_size),
                                        ),
                                    ),
                                    formula.path.clone().into(),
                                    TransformationMatrix::default(),
                                    rgb(0x282b30).into(),
                                    cx,
                                ) {
                                    Ok(()) => frame.geometry.math_painted += 1,
                                    Err(error) => {
                                        frame.geometry.math_error = Some(error.to_string())
                                    }
                                }
                            }
                        }
                    }
                    continue;
                }
                let _ = line.shaped.paint_background(
                    origin,
                    line.line_height,
                    line.align,
                    Some(Bounds::new(origin, size(line.width, line.line_height))),
                    window,
                    cx,
                );
                let _ = line.shaped.paint(
                    origin,
                    line.line_height,
                    line.align,
                    Some(Bounds::new(origin, size(line.width, line.line_height))),
                    window,
                    cx,
                );
            }
            if let Some(marked) = marked {
                for quad in frame
                    .geometry
                    .selection_quads(marked, rgb(theme::LINK).into())
                {
                    window.paint_quad(fill(
                        Bounds::new(
                            quad.bounds.origin + point(px(0.), quad.bounds.size.height - px(3.)),
                            size(quad.bounds.size.width, px(1.)),
                        ),
                        rgb(theme::LINK),
                    ));
                }
            }
            if focus.is_focused(window) && window.is_window_active() && show_cursor {
                window.paint_quad(frame.cursor.clone());
            }
            for control in &mut frame.controls {
                control.paint(window, cx);
            }
        });
        self.editor.update(cx, |editor, cx| {
            if let Some(error) = &frame.geometry.math_error {
                editor.status = editor.locale.language.error(Text::FormulaFailed, error);
            }
            editor.follow_outline(&frame.geometry, cx);
            editor.viewport.scroll = frame.geometry.scroll;
            editor.viewport.geometry = Some(frame.geometry.clone());
            editor.viewport.ensure_caret = false;
            if let Some((index, error)) = &frame.geometry.diagram_paint_error {
                let view = &frame.geometry.display.diagrams[*index];
                let source = view
                    .source
                    .rows
                    .iter()
                    .map(|r| &editor.document.source.text()[r.content.clone()])
                    .collect::<Vec<_>>()
                    .join("\n");
                editor.document.jobs.invalidate(&source, error.clone());
                editor.status = editor.locale.language.error(Text::DiagramFailed, error);
                editor.refresh(cx);
            }
        });
    }
}
