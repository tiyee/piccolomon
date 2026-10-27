//! Native-window regression for guarded layouts and repeated frame overlays.
use super::*;
#[derive(Default)]
pub(super) struct State {
    source: Option<editable_markdown_parser::Source>,
    composition: Option<ryne_session::Composition>,
    layout: Option<std::sync::Arc<text_layout::TextLayout>>,
    display: Option<std::sync::Arc<ryne_projection::IndexedDisplay>>,
    controls: Option<std::sync::Arc<Vec<ryne_extensions::RenderControl>>>,
    selection: Range<usize>,
    #[cfg(feature = "p1-metrics")]
    metrics: Option<ryne_syntax::query_metrics::Metrics>,
}
impl State {
    fn capture(&mut self, editor: &Editor, geometry: &Geometry) {
        self.source = Some(editor.document.source.clone());
        self.selection = editor.document.selection();
        self.layout = Some(geometry.text_layout.clone());
        self.display = Some(editor.viewport.display.clone());
        self.controls = Some(editor.viewport.render_controls.clone());
        #[cfg(feature = "p1-metrics")]
        {
            self.metrics = Some(ryne_syntax::query_metrics::snapshot());
        }
    }
    fn unchanged(&self, editor: &Editor, geometry: &Geometry) -> Result<(), String> {
        if self.source.as_ref().map(|s| s.id()) != Some(editor.document.source.id())
            || self.selection != editor.document.selection()
            || editor.document.dirty()
            || editor.document.history_len(false) != 0
            || editor.document.history_len(true) != 0
            || !self
                .layout
                .as_ref()
                .is_some_and(|p| std::sync::Arc::ptr_eq(p, &geometry.text_layout))
            || !self
                .display
                .as_ref()
                .is_some_and(|p| std::sync::Arc::ptr_eq(p, &editor.viewport.display))
            || !self
                .controls
                .as_ref()
                .is_some_and(|p| std::sync::Arc::ptr_eq(p, &editor.viewport.render_controls))
        {
            return Err("P1 stable redraw rebuilt content or changed the document".into());
        }
        #[cfg(feature = "p1-metrics")]
        if self.metrics != Some(ryne_syntax::query_metrics::snapshot()) {
            return Err("P1 stable redraw ran a semantic query or rebuilt indexes".into());
        }
        Ok(())
    }
    pub fn step(
        &mut self,
        stage: u16,
        editor: &mut Editor,
        geometry: &Geometry,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Result<(), String> {
        match stage {
            294 => {
                editor.document = editor.document.fork(
                    "| $x$ | b |\r\n| --- | --- |\r\n| | $y$ |\r\n\r\n$z$\r\n\r\nplain 中😀\r\n",
                );
                editor.viewport.scroll = px(0.);
                editor.move_to(editor.document.source.text().len(), false, cx);
                self.source = Some(editor.document.source.clone());
                let mut d = editor.document.display();
                d.tables
                    .get_mut(0)
                    .ok_or("P1 expected a normal table before fault injection")?
                    .range = 0..0;
                editor.viewport.display =
                    std::sync::Arc::new(ryne_projection::IndexedDisplay::new(d));
                editor.viewport.frame_epoch = editor.viewport.frame_epoch.wrapping_add(1);
                let page = theme::Page::new(f32::from(geometry.bounds.size.width), false);
                let key = text_layout::LayoutKey::new(
                    editor,
                    page,
                    window,
                    &editor.viewport.control_index.cells,
                );
                if text_layout::TextLayout::build(editor, key, window).is_ok() {
                    return Err("zero-progress table entered layout".into());
                }
                cx.notify();
            }
            295 | 297 => {
                if *editor.viewport.display != Display::plain_source(editor.document.source.text())
                    || !editor.viewport.render_controls.is_empty()
                    || !editor.viewport.diagram_images.is_empty()
                    || !geometry.is_current(editor)
                    || editor.document.dirty()
                    || editor.document.history_len(false) != 0
                {
                    return Err("P1 native source fallback is inconsistent".into());
                }
                if stage == 295 {
                    editor.refresh(cx);
                } else {
                    editor.replace_and_mark_text_in_range(None, "临时😀", Some(2..2), window, cx);
                    self.composition = editor.document.composition().cloned();
                    let mut d = Display::clone(&editor.viewport.display);
                    d.tables
                        .get_mut(0)
                        .ok_or("P1 expected a preedit table before fault injection")?
                        .range = 0..0;
                    editor.viewport.display =
                        std::sync::Arc::new(ryne_projection::IndexedDisplay::new(d));
                    editor.viewport.frame_epoch = editor.viewport.frame_epoch.wrapping_add(1);
                    cx.notify();
                }
            }
            296 => {
                if editor.viewport.display.formulas.len() != 3 {
                    return Err("P1 healthy projection did not recover".into());
                }
                let mut d = editor.document.display();
                if d.formulas.len() < 3 {
                    return Err("P1 expected three formula objects before fault injection".into());
                }
                d.formulas.swap(1, 2);
                editor.viewport.display =
                    std::sync::Arc::new(ryne_projection::IndexedDisplay::new(d));
                editor.viewport.frame_epoch = editor.viewport.frame_epoch.wrapping_add(1);
                cx.notify();
            }
            298 => {
                if editor.document.composition() != self.composition.as_ref()
                    || self.source.as_ref().map(|s| s.id()) != Some(editor.document.source.id())
                    || *editor.viewport.display
                        != Display::plain_source(&editor.document.editable_text())
                {
                    return Err(
                        "P1 layout recovery changed preedit or used committed offsets".into(),
                    );
                }
                let marked = editor
                    .marked_text_range(window, cx)
                    .ok_or("P1 marked text missing")?;
                if editor
                    .bounds_for_range(marked, Bounds::default(), window, cx)
                    .is_none()
                {
                    return Err("P1 fallback lost IME geometry".into());
                }
                editor.escape(&Escape, window, cx);
            }
            299 => {
                let pos = editor
                    .document
                    .source
                    .text()
                    .find("plain")
                    .ok_or("P1 plain text missing")?;
                editor.move_to(pos, false, cx);
            }
            300 => {
                let block = editor.active_block().ok_or("P1 focused block missing")?;
                editor.open_block_menu(editor.document.source.id(), block, cx);
            }
            301 | 348 => {
                self.capture(editor, geometry);
            }
            322 => {
                self.unchanged(editor, geometry)?;
                editor.menus.close_all();
                self.capture(editor, geometry);
                cx.notify();
            }
            302..=321 | 323..=342 | 349..=368 => {
                self.unchanged(editor, geometry)?;
                editor.caret_state.caret_visible = !editor.caret_state.caret_visible;
                cx.notify();
            }
            343 => {
                self.unchanged(editor, geometry)?;
                editor.document.set_source_mode(true);
                editor.refresh(cx);
            }
            344 => {
                if !editor.viewport.render_controls.is_empty() {
                    return Err("P1 source controls remained active".into());
                }
                editor.document.set_source_mode(false);
                editor
                    .document
                    .set_block_source(Some(0..editor.document.source.text().len()));
                editor.refresh(cx);
            }
            345 => {
                editor.document.set_block_source(None);
                editor.refresh(cx);
            }
            346 => {
                let offset = editor.document.head;
                editor.document = editor
                    .document
                    .fork(&format!("## {}", "h".repeat(offset + 3)));
                editor.move_to(offset, false, cx);
                if !editor
                    .focused_block()
                    .is_some_and(|b| b.kind == block::BlockKind::Heading(2))
                {
                    return Err("P1 new-document cache was stale".into());
                }
            }
            347 => {
                editor.document = editor.document.fork("# x\n\nparagraph");
                editor.move_to(4, false, cx);
            }
            369 => {
                self.unchanged(editor, geometry)?;
            }
            _ => {}
        }
        Ok(())
    }
}
