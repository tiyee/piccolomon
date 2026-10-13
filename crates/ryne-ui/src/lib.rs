//! GPUI facade; input, geometry, elements, smoke and asset upload have separate modules.
pub use ryne_syntax::{block, code_block, list, table};
mod assets;
mod element;
mod export;
mod files;
mod i18n;
mod input;
mod layout;
mod layout_frame;
mod math_fonts;
mod menus;
mod paint;
mod profile;
mod recent;
mod render_controls;
mod smoke;
mod text_layout;
mod theme;
mod welcome;
use editable_markdown_parser::{Affinity, ParseSnapshot, Session, SnapshotId, Source};
use element::*;
use gpui::{prelude::*, *};
use i18n::{Language, Localization, Preference, Status, Text};
use layout::*;
use menus::{OpenRecent, SelectLanguage, SelectMathFont};
use render_controls::*;
pub use ryne_extensions::DesktopServices;
use ryne_extensions::{FormulaRenderer, MathFont, SyntaxHighlighter};
use ryne_projection::{self as presentation, Display};
use ryne_session::{Composition, EditorSession as Document, SessionServices};
use std::{ops::Range, path::PathBuf};
use unicode_segmentation::UnicodeSegmentation;

actions!(
    ryne,
    [
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        Home,
        End,
        Backspace,
        Delete,
        Enter,
        HardBreak,
        SelectHome,
        SelectEnd,
        Tab,
        Outdent,
        SelectAll,
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        About,
        CheckUpdates,
        Hide,
        New,
        Reload,
        ExportPdf,
        ExportPdfV2,
        ClearRecent,
        NoRecent,
        Open,
        Save,
        SaveAs,
        FormatSource,
        ToggleSource,
        ToggleBlockSource,
        Escape,
        Quit
    ]
);

fn utf16_to_utf8(text: &str, units: usize) -> Result<usize, editable_markdown_parser::Error> {
    ryne_core::utf16_to_source(text, ryne_core::Utf16Offset(units)).map(|byte| byte.0)
}
const LINE_HEIGHT: f32 = theme::BODY.1;
const APPEND_HEIGHT: f32 = 64.;
fn heading_metrics(level: u8) -> (f32, f32) {
    theme::heading(level)
}

struct Editor {
    document: Document,
    desktop: DesktopServices,
    formulas: std::sync::Arc<dyn FormulaRenderer>,
    math_font: math_fonts::MathFontPreference,
    recent: recent::RecentFiles,
    file_operation_pending: bool,
    exporting: bool,
    checking_updates: bool,
    default_document: Option<welcome::DefaultDocument>,
    highlighter: Box<dyn SyntaxHighlighter>,
    fonts: theme::Fonts,
    diagram_images: Vec<std::sync::Arc<RenderImage>>,
    display: Display,
    focus: FocusHandle,
    geometry: Option<Geometry>,
    frame_epoch: u64,
    scroll: Pixels,
    ensure_caret: bool,
    dragging: bool,
    status: Status,
    locale: Localization,
    top_menu: Option<usize>,
    top_submenu: Option<usize>,
    allow_close: bool,
    close_prompt: bool,
    close_after_save: bool,
    smoke_mode: bool,
    preferred_x: Option<Pixels>,
    caret_affinity: Affinity,
    block_menu: Option<(SnapshotId, crate::block::Block)>,
    block_insert: Option<bool>,
    hovered_block: Option<(SnapshotId, crate::block::Block)>,
    append_hovered: bool,
    append_menu: Option<SnapshotId>,
    focus_observed: bool,
    caret_visible: bool,
    caret_blink_task: Option<Task<()>>,
    render_controls: Vec<ryne_extensions::RenderControl>,
    hovered_render: Option<String>,
    render_menu: Option<RenderMenu>,
}

impl Editor {
    fn select_language(&mut self, preference: Preference, cx: &mut Context<Self>) {
        self.top_menu = None;
        self.top_submenu = None;
        self.block_menu = None;
        self.append_menu = None;
        self.render_menu = None;
        if let Err(error) = self.locale.select(preference) {
            self.status = self.locale.language.error(Text::PreferenceFailed, error);
        }
        if !self.dragging
            && self
                .default_document
                .as_mut()
                .is_some_and(|guide| guide.switch(&mut self.document, self.locale.language))
        {
            self.scroll = px(0.);
            self.geometry = None;
            self.preferred_x = None;
            self.caret_affinity = Affinity::Downstream;
            self.ensure_caret = true;
            self.hovered_render = None;
            self.refresh(cx);
        }
        self.sync_application_menus(cx);
        // User-owned documents and pending IME input retain their exact state.
        cx.notify();
    }

    fn reset_caret_blink(&mut self) {
        self.caret_blink_task = None;
        self.caret_visible = true;
    }

    fn sync_caret_blink(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window)
            || !window.is_window_active()
            || !self.selected().is_empty()
            || self.dragging
            || self.document.composition.is_some()
        {
            self.reset_caret_blink();
        } else if self.caret_blink_task.is_none() {
            self.caret_blink_task = Some(cx.spawn_in(window, async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(500))
                    .await;
                let _ = this.update_in(cx, |editor, _, cx| {
                    editor.caret_visible = !editor.caret_visible;
                    editor.caret_blink_task = None;
                    // Repaint only: blinking must never reparse or change the document.
                    cx.notify();
                });
            }));
        }
    }

    fn open_append_menu(&mut self, snapshot: SnapshotId, cx: &mut Context<Self>) {
        if snapshot == self.document.source.id()
            && self.append_hovered
            && !self.document.source_mode
            && !self.dragging
            && self.document.composition.is_none()
            && self.append_menu != Some(snapshot)
        {
            self.block_menu = None;
            self.append_menu = Some(snapshot);
            cx.notify();
        }
    }

    fn append_block(
        &mut self,
        snapshot: SnapshotId,
        kind: ryne_extensions::InsertBlock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if snapshot != self.document.source.id()
            || self.document.source_mode
            || self.dragging
            || self.document.composition.is_some()
        {
            self.append_menu = None;
            cx.notify();
            return;
        }
        match self
            .document
            .dispatch(ryne_extensions::EditIntent::AppendLocalized {
                template: kind,
                labels: self.locale.language.template_labels(),
            }) {
            Ok(_) => {
                self.status = Text::Added.into();
                self.preferred_x = None;
                self.caret_affinity = Affinity::Downstream;
                self.ensure_caret = true;
                window.focus(&self.focus);
            }
            Err(_) => self.status = Text::AppendFailed.into(),
        }
        self.refresh(cx);
    }

    fn insert_adjacent_block(
        &mut self,
        snapshot: SnapshotId,
        block: &crate::block::Block,
        before: bool,
        kind: ryne_extensions::InsertBlock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if snapshot != self.document.source.id()
            || self.document.source_mode
            || self.dragging
            || self.document.composition.is_some()
            || self.block_menu.as_ref() != Some(&(snapshot, block.clone()))
            || self.block_insert != Some(before)
        {
            self.block_menu = None;
            self.block_insert = None;
            cx.notify();
            return;
        }
        match self
            .document
            .dispatch(ryne_extensions::EditIntent::InsertAdjacent {
                block: ryne_core::SourceRange::new(snapshot, block.range.clone()),
                before,
                template: kind,
                labels: self.locale.language.template_labels(),
            }) {
            Ok(_) => {
                self.status = Text::Added.into();
                self.preferred_x = None;
                self.caret_affinity = Affinity::Downstream;
                self.ensure_caret = true;
                window.focus(&self.focus);
            }
            Err(_) => self.status = Text::InsertFailed.into(),
        }
        self.refresh(cx);
    }

    fn active_block(&self) -> Option<crate::block::Block> {
        self.hovered_block
            .as_ref()
            .filter(|(snapshot, _)| *snapshot == self.document.source.id())
            .map(|(_, block)| block.clone())
            .or_else(|| self.focused_block())
    }

    fn focused_block(&self) -> Option<crate::block::Block> {
        // At a forward selection's end, use its last selected character.
        let head = self.document.head;
        let caret = if self.document.anchor < head {
            self.document.source.text()[..head]
                .char_indices()
                .next_back()
                .map_or(head, |(index, _)| index)
        } else {
            head
        };
        crate::block::at(&self.document.parse, caret)
    }

    fn open_block_menu(
        &mut self,
        snapshot: SnapshotId,
        block: crate::block::Block,
        cx: &mut Context<Self>,
    ) {
        if snapshot != self.document.source.id()
            || self.document.source_mode
            || self.document.composition.is_some()
            || self.dragging
        {
            return;
        }
        if self
            .block_menu
            .as_ref()
            .is_some_and(|target| target == &(snapshot, block.clone()))
        {
            return;
        }
        if self.active_block().as_ref() == Some(&block) {
            self.block_insert = None;
            self.block_menu = Some((snapshot, block));
            cx.notify();
        }
    }

    fn block_action(
        &mut self,
        snapshot: SnapshotId,
        block: &crate::block::Block,
        action: BlockAction,
        cx: &mut Context<Self>,
    ) {
        // A menu captured from an older source can never operate on new offsets.
        if snapshot != self.document.source.id() || self.document.composition.is_some() {
            self.block_menu = None;
            cx.notify();
            return;
        }
        match action {
            BlockAction::AddBelow | BlockAction::AddAbove => {
                if self.document.source_mode
                    || self.dragging
                    || self.block_menu.as_ref() != Some(&(snapshot, block.clone()))
                {
                    return;
                }
                let before = matches!(action, BlockAction::AddAbove);
                if before && !block::can_insert_above(&self.document.parse, block) {
                    return;
                }
                self.block_insert = Some(before);
                cx.notify();
                return;
            }
            BlockAction::Source => {
                if self
                    .document
                    .block_source
                    .as_ref()
                    .is_some_and(|session| session.range == block.scope)
                {
                    self.document.set_block_source(None);
                } else {
                    self.document.set_block_source(Some(block.scope.clone()));
                    self.document.clear_heading_reveal();
                    if !block.range.contains(&self.document.head) {
                        self.document.select(block.anchor, false, false);
                    }
                }
            }
            BlockAction::Select => {
                self.document.select(block.range.start, false, true);
                self.document.select(block.range.end, true, true);
            }
            BlockAction::Copy => {
                cx.write_to_clipboard(ClipboardItem::new_string(
                    self.document.source.text()[block.range.clone()].into(),
                ));
                self.status = Text::Copied.into();
            }
        }
        self.preferred_x = None;
        self.caret_affinity = Affinity::Downstream;
        self.refresh(cx);
    }
    fn new(
        mut document: Document,
        desktop: DesktopServices,
        formulas: std::sync::Arc<dyn FormulaRenderer>,
        math_font: math_fonts::MathFontPreference,
        mut highlighter: Box<dyn SyntaxHighlighter>,
        locale: Localization,
        cx: &mut Context<Self>,
    ) -> Self {
        document.normalize_heading_caret();
        let display = document.highlighted_display(highlighter.as_mut());
        let mut editor = Self {
            document,
            desktop,
            formulas,
            math_font,
            recent: recent::RecentFiles::default(),
            file_operation_pending: false,
            exporting: false,
            checking_updates: false,
            default_document: None,
            highlighter,
            fonts: theme::Fonts::new(cx.text_system()),
            diagram_images: Vec::new(),
            display,
            focus: cx.focus_handle(),
            geometry: None,
            frame_epoch: 0,
            scroll: px(0.),
            ensure_caret: true,
            dragging: false,
            status: Text::Help.into(),
            locale,
            top_menu: None,
            top_submenu: None,
            allow_close: false,
            close_prompt: false,
            close_after_save: false,
            smoke_mode: false,
            preferred_x: None,
            caret_affinity: Affinity::Downstream,
            block_menu: None,
            block_insert: None,
            hovered_block: None,
            append_hovered: false,
            append_menu: None,
            focus_observed: false,
            caret_visible: true,
            caret_blink_task: None,
            render_controls: Vec::new(),
            hovered_render: None,
            render_menu: None,
        };
        editor.refresh(cx);
        editor
    }

    fn editable_text(&self) -> String {
        let mut text = self.document.source.text().to_owned();
        if let Some(composition) = &self.document.composition {
            text.replace_range(composition.range.clone(), &composition.text);
        }
        text
    }
    fn selected(&self) -> Range<usize> {
        self.document
            .composition
            .as_ref()
            .map_or_else(|| self.document.selection(), |c| c.selection.clone())
    }
    fn caret(&self) -> usize {
        self.document
            .composition
            .as_ref()
            .map_or(self.document.head, |c| c.selection.end)
    }
    fn accept_render_controls(
        &mut self,
        batch: Result<ryne_extensions::RenderControlBatch, String>,
    ) {
        match batch {
            Ok(batch) => {
                if let Some(diagnostic) = batch.diagnostics.first() {
                    self.status = self.locale.language.error(
                        Text::RenderActionFailed,
                        format!("{}: {}", diagnostic.feature, diagnostic.message),
                    );
                }
                self.document.record_diagnostics(batch.diagnostics);
                self.render_controls = batch.controls;
            }
            Err(error) => {
                self.status = self.locale.language.error(Text::RenderActionFailed, error);
                self.render_controls.clear();
            }
        }
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let _profile = profile::Scope::new("refresh");
        // Keep preedit and drag geometry stable; apply the saved choice when
        // the normal input lifecycle next refreshes a settled document.
        if !self.dragging && self.document.composition.is_none() {
            self.formulas.set_font(self.math_font.font);
        }
        self.reset_caret_blink();
        self.render_menu = None;
        self.block_menu = None;
        self.block_insert = None;
        self.append_menu = None;
        self.append_hovered = false;
        let batch = if self.document.composition.is_some() {
            Ok(ryne_extensions::RenderControlBatch::default())
        } else {
            self.document.project_render_control_batch(&self.document)
        };
        self.accept_render_controls(batch);
        if self.dragging || self.document.composition.is_some() || self.document.source_mode {
            self.hovered_render = None;
        }
        if self.dragging
            || self.document.composition.is_some()
            || self.document.source_mode
            || self
                .hovered_block
                .as_ref()
                .is_some_and(|(snapshot, _)| *snapshot != self.document.source.id())
        {
            self.hovered_block = None;
        }
        if self.document.composition.is_none() {
            self.document.normalize_heading_caret();
        }
        let frozen = self.dragging
            || !self.document.selection().is_empty()
            || self.document.composition.is_some();
        let folded = self
            .display
            .diagrams
            .iter()
            .map(|v| v.source.range.clone())
            .collect::<Vec<_>>();
        let (views, request) = self.document.diagram_views(frozen, &folded);
        if let Some(request) = request {
            let task = cx.background_executor().spawn(async move { request.run() });
            cx.spawn(async move |this, cx| {
                let result = task.await;
                let _ = this.update(cx, |editor, cx| {
                    editor.document.complete_job(result);
                    if !editor.dragging && editor.document.composition.is_none() {
                        editor.refresh(cx);
                    }
                });
            })
            .detach();
        }
        let block_window = self.document.block_window();
        let next_display = if let Some(composition) = &self.document.composition {
            // This temporary parse never becomes the committed document parse.
            let parse = ParseSnapshot::with_math(Source::new(&self.editable_text()));
            let mut window =
                composition.range.start..composition.range.start + composition.text.len();
            if let Some(session) = &self.document.reveal {
                let delta = composition.text.len() as isize - composition.range.len() as isize;
                window.start = window.start.min(session.range.start);
                window.end = window
                    .end
                    .max(session.range.end.saturating_add_signed(delta));
            }
            let map = |position: usize, end: bool| {
                if position < composition.range.start {
                    position
                } else if position > composition.range.end {
                    position.saturating_add_signed(
                        composition.text.len() as isize - composition.range.len() as isize,
                    )
                } else {
                    composition.range.start + if end { composition.text.len() } else { 0 }
                }
            };
            let map_range =
                |session: &Session| map(session.range.start, false)..map(session.range.end, true);
            let views = views
                .iter()
                .map(|(range, result)| {
                    (
                        map(range.start, false)..map(range.end, true),
                        result.clone(),
                    )
                })
                .collect::<Vec<_>>();
            let heading = self.document.heading_reveal.as_ref().map(map_range);
            let block = block_window
                .as_ref()
                .map(|range| map(range.start, false)..map(range.end, true));
            let mut temporary = (*self.document).clone();
            temporary.source = parse.source().clone();
            temporary.parse = std::sync::Arc::new(parse);
            temporary.reveal = Session::new(&temporary.source, window).ok();
            temporary.heading_reveal =
                heading.and_then(|r| Session::new(&temporary.source, r).ok());
            temporary.block_source = block.and_then(|r| Session::new(&temporary.source, r).ok());
            let batch = self.document.project_render_control_batch(&temporary);
            self.accept_render_controls(batch);
            self.document
                .project(&temporary, Some(self.highlighter.as_mut()), &views)
        } else {
            self.document
                .project(&self.document, Some(self.highlighter.as_mut()), &views)
        };
        // Caret-only refreshes may reuse geometry. Text, mappings, structure or
        // resource changes invalidate it before any subsequent native callback.
        if self.display != next_display {
            self.frame_epoch = self.frame_epoch.wrapping_add(1);
        }
        self.display = next_display;
        self.diagram_images = crate::assets::diagram_images(&self.display, &self.geometry);
        cx.notify();
    }
    fn commit_composition(&mut self, cx: &mut Context<Self>) {
        if let Some(composition) = self.document.composition.take() {
            self.preferred_x = None;
            self.caret_affinity = Affinity::Downstream;
            if let Err(error) = self.document.insert_text_from(
                composition.range,
                &composition.text,
                ryne_extensions::InputOrigin::Ime,
            ) {
                self.status = self
                    .locale
                    .language
                    .error(Text::InputFailed, format!("{:?}", error));
            }
            self.refresh(cx);
        }
    }
    fn insert(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        self.insert_from(range, text, ryne_extensions::InputOrigin::Keyboard, cx)
    }
    fn insert_from(
        &mut self,
        range: Range<usize>,
        text: &str,
        origin: ryne_extensions::InputOrigin,
        cx: &mut Context<Self>,
    ) {
        self.preferred_x = None;
        self.caret_affinity = Affinity::Downstream;
        if let Err(error) = self.document.insert_text_from(range, text, origin) {
            self.status = self
                .locale
                .language
                .error(Text::InputFailed, format!("{:?}", error));
        }
        self.ensure_caret = true;
        self.refresh(cx);
    }
    fn move_to(&mut self, offset: usize, extend: bool, cx: &mut Context<Self>) {
        self.hovered_block = None;
        self.preferred_x = None;
        self.caret_affinity = Affinity::Downstream;
        self.document
            .select(offset, extend, self.dragging || extend);
        self.ensure_caret = true;
        self.refresh(cx);
    }
    fn horizontal(&mut self, forward: bool, extend: bool, cx: &mut Context<Self>) {
        self.commit_composition(cx);
        let selection = self.document.selection();
        if !extend && !selection.is_empty() {
            self.move_to(
                if forward {
                    selection.end
                } else {
                    selection.start
                },
                false,
                cx,
            );
            return;
        }
        let position = self.display.position_at(self.document.head);
        if !extend
            && let Some(diagram) = self.display.diagrams.iter().find(|view| {
                if forward {
                    position == view.display.start
                } else {
                    position == view.display.end
                }
            })
        {
            self.move_to(diagram.source.anchor, false, cx);
            return;
        }
        if !extend
            && let Some(formula) = self.display.formulas.iter().find(|formula| {
                if forward {
                    position == formula.display.start
                } else {
                    position == formula.display.end
                }
            })
        {
            let source = if forward {
                formula.source.start + 1
            } else {
                formula.source.end - 1
            };
            self.move_to(source, false, cx);
            return;
        }
        let next = if forward {
            self.display
                .text
                .grapheme_indices(true)
                .map(|(p, _)| p)
                .chain(std::iter::once(self.display.text.len()))
                .find(|&p| p > position)
                .unwrap_or(position)
        } else {
            self.display
                .text
                .grapheme_indices(true)
                .map(|(p, _)| p)
                .take_while(|&p| p < position)
                .last()
                .unwrap_or(0)
        };
        let affinity = if forward {
            Affinity::Upstream
        } else {
            Affinity::Downstream
        };
        self.move_to(self.display.source_at(next, affinity), extend, cx);
    }
    fn vertical(&mut self, down: bool, extend: bool, cx: &mut Context<Self>) {
        self.commit_composition(cx);
        if let Some(geometry) = &self.geometry {
            if !geometry.is_current(self) {
                return;
            }
            let offset = geometry.display.position_at(self.document.head);
            let x = self.preferred_x.unwrap_or_else(|| {
                geometry
                    .position_with_affinity(offset, self.caret_affinity)
                    .x
            });
            let (hit, affinity) = geometry.vertical_target(offset, self.caret_affinity, x, down);
            let source = geometry
                .display
                .source_byte_at(ryne_core::DisplayByte(hit), affinity)
                .0;
            self.move_to(source, extend, cx);
            self.preferred_x = Some(x);
            self.caret_affinity = affinity;
        }
    }
    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.render_menu_horizontal(false, cx) {
            return;
        }
        self.horizontal(false, false, cx);
    }
    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.render_menu_horizontal(true, cx) {
            return;
        }
        self.horizontal(true, false, cx);
    }
    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.horizontal(false, true, cx);
    }
    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.horizontal(true, true, cx);
    }
    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        if self.render_menu_vertical(false, cx) {
            return;
        }
        self.vertical(false, false, cx);
    }
    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        if self.render_menu_vertical(true, cx) {
            return;
        }
        self.vertical(true, false, cx);
    }
    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.vertical(false, true, cx);
    }
    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.vertical(true, true, cx);
    }
    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.line_edge(false, false, cx);
    }
    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.line_edge(true, false, cx);
    }
    fn select_home(&mut self, _: &SelectHome, _: &mut Window, cx: &mut Context<Self>) {
        self.line_edge(false, true, cx);
    }
    fn select_end(&mut self, _: &SelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.line_edge(true, true, cx);
    }
    fn line_edge(&mut self, end: bool, extend: bool, cx: &mut Context<Self>) {
        self.commit_composition(cx);
        if let Some(geometry) = &self.geometry {
            if !geometry.is_current(self) {
                return;
            }
            let position = geometry.display.position_at(self.document.head);
            if let Some(row) = geometry.row_for(position, self.caret_affinity) {
                let affinity = if end {
                    Affinity::Upstream
                } else {
                    Affinity::Downstream
                };
                let offset = if end { row.range.end } else { row.range.start };
                let source = geometry.display.source_at(offset, affinity);
                self.move_to(source, extend, cx);
                self.caret_affinity = affinity;
            }
        }
    }
    fn remove(&mut self, forward: bool, cx: &mut Context<Self>) {
        self.commit_composition(cx);
        let selection = self.document.selection();
        if !selection.is_empty() {
            self.insert(selection, "", cx);
            return;
        }
        let position = self.display.position_at(self.document.head);
        if let Some(view) = self.display.diagrams.iter().find(|view| {
            if forward {
                position == view.display.start
            } else {
                position == view.display.end
            }
        }) {
            self.move_to(view.source.anchor, false, cx);
            return;
        }
        if self.document.remove_table_boundary(forward) {
            return;
        }
        let removed = self
            .document
            .remove_code_boundary(forward)
            .and_then(|handled| {
                if handled {
                    Ok(true)
                } else {
                    self.document
                        .remove_list_boundary(forward)
                        .and_then(|handled| {
                            if handled {
                                Ok(true)
                            } else {
                                self.document.join_paragraphs(forward)
                            }
                        })
                }
            });
        match removed {
            Ok(true) => {
                self.preferred_x = None;
                self.caret_affinity = Affinity::Downstream;
                self.ensure_caret = true;
                self.refresh(cx);
                return;
            }
            Err(error) => {
                self.status = self
                    .locale
                    .language
                    .error(Text::DeleteFailed, format!("{:?}", error));
                return;
            }
            Ok(false) => {}
        }
        let position = self.display.position_at(self.document.head);
        let adjacent = if forward {
            self.display
                .text
                .grapheme_indices(true)
                .map(|(p, _)| p)
                .chain(std::iter::once(self.display.text.len()))
                .find(|&p| p > position)
        } else {
            self.display
                .text
                .grapheme_indices(true)
                .map(|(p, _)| p)
                .take_while(|&p| p < position)
                .last()
        };
        if let Some(adjacent) = adjacent {
            // Delete the visible grapheme, excluding hidden markers at its edges.
            let start = self
                .display
                .source_at(position.min(adjacent), Affinity::Downstream);
            let end = self
                .display
                .source_at(position.max(adjacent), Affinity::Upstream);
            if start < end {
                self.insert(start..end, "", cx);
            }
        }
    }
    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        self.remove(false, cx);
    }
    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        self.remove(true, cx);
    }
    fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        if self.activate_render_menu(window, cx) {
            return;
        }
        self.line_break(false, cx);
    }
    fn hard_break(&mut self, _: &HardBreak, _: &mut Window, cx: &mut Context<Self>) {
        self.line_break(true, cx);
    }
    fn line_break(&mut self, hard: bool, cx: &mut Context<Self>) {
        // The IME consumes Enter while choosing candidates before this action.
        if self.document.composition.is_some() {
            self.commit_composition(cx);
            return;
        }
        if let Err(error) = self.document.line_break(hard) {
            self.status = self
                .locale
                .language
                .error(Text::BreakFailed, format!("{:?}", error));
        }
        self.preferred_x = None;
        self.caret_affinity = Affinity::Downstream;
        self.ensure_caret = true;
        self.refresh(cx);
    }
    fn tab(&mut self, _: &Tab, _: &mut Window, cx: &mut Context<Self>) {
        self.commit_composition(cx);
        let handled = self
            .document
            .navigate_table(false, false)
            .and_then(|handled| {
                if handled {
                    Ok(true)
                } else {
                    self.document.indent_list(false)
                }
            });
        match handled {
            Ok(true) => {
                self.preferred_x = None;
                self.caret_affinity = Affinity::Downstream;
                self.ensure_caret = true;
                self.refresh(cx);
            }
            Ok(false) => self.insert(self.document.selection(), "\t", cx),
            Err(error) => {
                self.status = self
                    .locale
                    .language
                    .error(Text::IndentFailed, format!("{:?}", error));
                cx.notify();
            }
        }
    }
    fn outdent(&mut self, _: &Outdent, _: &mut Window, cx: &mut Context<Self>) {
        self.commit_composition(cx);
        self.preferred_x = None;
        self.caret_affinity = Affinity::Downstream;
        if let Err(error) = self
            .document
            .navigate_table(true, false)
            .and_then(|handled| {
                if handled {
                    Ok(true)
                } else {
                    self.document.indent_list(true)
                }
            })
        {
            self.status = self
                .locale
                .language
                .error(Text::OutdentFailed, format!("{:?}", error));
        }
        self.ensure_caret = true;
        self.refresh(cx);
    }
    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.commit_composition(cx);
        self.preferred_x = None;
        self.caret_affinity = Affinity::Downstream;
        self.document.select(0, false, true);
        self.document
            .select(self.document.source.text().len(), true, true);
        self.refresh(cx);
    }
    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let selection = self.selected();
        if !selection.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.editable_text()[selection].into(),
            ));
        }
    }
    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_composition(cx);
        self.copy(&Copy, window, cx);
        self.insert(self.document.selection(), "", cx);
    }
    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        self.commit_composition(cx);
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.insert_from(
                self.document.selection(),
                &text,
                ryne_extensions::InputOrigin::Paste,
                cx,
            );
        }
    }
    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.caret_affinity = Affinity::Downstream;
        if self.document.composition.take().is_none() {
            self.document.undo(false);
        }
        self.ensure_caret = true;
        self.refresh(cx);
    }
    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.commit_composition(cx);
        self.document.undo(true);
        self.preferred_x = None;
        self.caret_affinity = Affinity::Downstream;
        self.ensure_caret = true;
        self.refresh(cx);
    }
    fn format_source(&mut self, _: &FormatSource, window: &mut Window, cx: &mut Context<Self>) {
        if self.dragging || self.document.composition.is_some() {
            self.status = Text::OperationWait.into();
            cx.notify();
            return;
        }
        let before = self.document.source.id();
        match self
            .document
            .dispatch(ryne_extensions::EditIntent::FormatSource)
        {
            Ok(_) => {
                self.status = if self.document.source.id() == before {
                    Text::SourceUnchanged
                } else {
                    Text::SourceFormatted
                }
                .into();
                self.preferred_x = None;
                self.caret_affinity = Affinity::Downstream;
                self.ensure_caret = true;
                self.refresh(cx);
            }
            Err(error) => {
                self.status = self.locale.language.error(Text::FormatFailed, error);
                cx.notify();
            }
        }
        self.focus.focus(window);
    }
    fn toggle_source(&mut self, _: &ToggleSource, _: &mut Window, cx: &mut Context<Self>) {
        self.commit_composition(cx);
        self.document.clear_heading_reveal();
        self.document.set_block_source(None);
        self.document.set_source_mode(!self.document.source_mode);
        self.preferred_x = None;
        self.caret_affinity = Affinity::Downstream;
        self.ensure_caret = true;
        self.refresh(cx);
    }
    fn toggle_block_source(
        &mut self,
        _: &ToggleBlockSource,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_composition(cx);
        if self.document.toggle_block_source() {
            self.preferred_x = None;
            self.caret_affinity = Affinity::Downstream;
            self.ensure_caret = true;
            self.refresh(cx);
        } else {
            self.status = Text::PlaceCaret.into();
            cx.notify();
        }
    }
    fn escape(&mut self, _: &Escape, _: &mut Window, cx: &mut Context<Self>) {
        if self.top_menu.take().is_some() {
            self.top_submenu = None;
            cx.notify();
            return;
        }
        if let Some(menu) = &mut self.render_menu {
            if menu.submenu.take().is_none() {
                self.render_menu = None;
            }
            cx.notify();
            return;
        }
        if self.append_menu.take().is_some() {
            cx.notify();
            return;
        }
        if self.block_menu.is_some() {
            if self.block_insert.take().is_none() {
                self.block_menu = None;
            }
            cx.notify();
            return;
        }
        self.preferred_x = None;
        self.caret_affinity = Affinity::Downstream;
        self.document.composition = None;
        self.document.clear_inline_reveal();
        self.document.clear_heading_reveal();
        self.document.set_block_source(None);
        self.document.set_source_mode(false);
        if let Some(range) = self.document.preview_at_caret() {
            self.document.select(range.end, false, false);
        }
        self.refresh(cx);
    }
    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.render_menu = None;
        self.commit_composition(cx);
        window.focus(&self.focus);
        if let Some(geometry) = &self.geometry {
            if !geometry.is_current(self) {
                return;
            }
            // Resolve once against the displayed layout, then retain source anchors.
            let (hit, affinity) = geometry.hit_caret(event.position);
            let source = if !event.modifiers.shift {
                geometry
                    .display
                    .formulas
                    .iter()
                    .find(|formula| {
                        let a = geometry
                            .position_with_affinity(formula.display.start, Affinity::Downstream);
                        let b = geometry
                            .position_with_affinity(formula.display.end, Affinity::Upstream);
                        event.position.x >= a.x
                            && event.position.x < b.x
                            && event.position.y >= a.y
                            && event.position.y < a.y + geometry.line_height(formula.display.start)
                    })
                    .map_or_else(
                        || {
                            geometry
                                .display
                                .source_byte_at(ryne_core::DisplayByte(hit), affinity)
                                .0
                        },
                        |formula| formula.source.start + 1,
                    )
            } else {
                geometry
                    .display
                    .source_byte_at(ryne_core::DisplayByte(hit), affinity)
                    .0
            };
            let source = if !event.modifiers.shift {
                geometry
                    .lines
                    .iter()
                    .find_map(|line| {
                        let index = line.diagram?;
                        let top = geometry.bounds.top() + px(geometry.page.top) + line.top
                            - geometry.scroll;
                        (event.position.y >= top && event.position.y < top + line.line_height)
                            .then_some(geometry.display.diagrams[index].source.anchor)
                    })
                    .unwrap_or(source)
            } else {
                source
            };
            self.document.select(source, event.modifiers.shift, false);
            if !event.modifiers.shift {
                self.document.reveal_clicked_heading();
            }
            self.preferred_x = None;
            self.caret_affinity = affinity;
            self.dragging = true;
            self.refresh(cx);
        }
    }
    fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.dragging
            && let Some(geometry) = &self.geometry
        {
            if !geometry.is_current(self) {
                return;
            }
            let (hit, affinity) = geometry.hit_caret(event.position);
            self.document.select(
                geometry
                    .display
                    .source_byte_at(ryne_core::DisplayByte(hit), affinity)
                    .0,
                true,
                true,
            );
            self.caret_affinity = affinity;
            self.refresh(cx);
        } else if !self.dragging {
            if self.update_render_hover(event.position, cx) {
                return;
            }
            self.update_block_hover(event.position, cx);
        }
    }
    fn update_block_hover(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.document.composition.is_some() || self.document.source_mode || self.dragging {
            return;
        }
        let Some(geometry) = &self.geometry else {
            return;
        };
        if !geometry.is_current(self) {
            return;
        }
        if self.append_menu.is_some()
            && let (Some(control), Some(menu)) = (geometry.append_control, geometry.append_menu)
            && position.x >= control.left().min(menu.left())
            && position.x < control.right().max(menu.right())
            && position.y >= control.top().min(menu.top())
            && position.y < control.bottom().max(menu.bottom())
        {
            return;
        }
        if self.block_menu.is_some()
            && let (Some(control), Some(menu)) = (geometry.block_control, geometry.block_menu)
            && position.x >= control.left().min(menu.left())
            && position.x < control.right().max(menu.right())
            && position.y >= control.top().min(menu.top())
            && position.y < control.bottom().max(menu.bottom())
        {
            // Keep the menu open across the small gap between it and its control.
            return;
        }
        if geometry
            .append_zone
            .is_some_and(|zone| zone.contains(&position))
        {
            let changed =
                !self.append_hovered || self.block_menu.is_some() || self.hovered_block.is_some();
            self.append_hovered = true;
            self.block_menu = None;
            self.hovered_block = None;
            if geometry
                .append_control
                .is_some_and(|control| control.contains(&position))
            {
                self.open_append_menu(geometry.snapshot, cx);
            } else if changed {
                cx.notify();
            }
            return;
        }
        if self.append_hovered || self.append_menu.is_some() {
            self.append_hovered = false;
            self.append_menu = None;
            cx.notify();
        }
        if geometry
            .block_control
            .is_some_and(|control| control.contains(&position))
        {
            if let Some(block) = geometry.block_target.clone() {
                let snapshot = geometry.snapshot;
                self.hovered_block = Some((snapshot, block.clone()));
                self.open_block_menu(snapshot, block, cx);
            }
            return;
        }
        let block =
            geometry
                .bounds
                .contains(&position)
                .then(|| {
                    let y = position.y - geometry.bounds.top() - px(geometry.page.top)
                        + geometry.scroll;
                    // Code panels include a language header and padding outside their
                    // text rows; these belong to the same hover target as the body.
                    if let Some(source) = geometry.display.code_blocks.iter().enumerate().find_map(
                        |(index, block)| {
                            let mut lines = geometry
                                .lines
                                .iter()
                                .filter(|line| line.code == Some(index));
                            let first = lines.next()?;
                            let last = lines.next_back().unwrap_or(first);
                            (first.top - px(32.) <= y
                                && y < last.top
                                    + last.shaped.size(last.line_height).height
                                    + px(12.))
                            .then_some(block.source.anchor)
                        },
                    ) {
                        return crate::block::at(&self.document.parse, source);
                    }
                    let line = geometry.lines.iter().find(|line| {
                        line.top <= y && y < line.top + line.shaped.size(line.line_height).height
                    })?;
                    let source = if let Some(index) = line.diagram {
                        geometry.display.diagrams[index].source.anchor
                    } else if let Some(index) = line.code {
                        geometry.display.code_blocks[index].source.anchor
                    } else {
                        let (hit, affinity) = geometry.hit_caret(position);
                        geometry
                            .display
                            .source_byte_at(ryne_core::DisplayByte(hit), affinity)
                            .0
                    };
                    crate::block::at(&self.document.parse, source)
                })
                .flatten()
                .map(|block| (geometry.snapshot, block));
        let closed_menu = self.block_menu.take().is_some();
        if self.hovered_block != block || closed_menu {
            self.hovered_block = block;
            cx.notify();
        }
    }
    fn editor_hover(&mut self, hovered: &bool, _: &mut Window, cx: &mut Context<Self>) {
        if !hovered && !self.dragging {
            let render = self.hovered_render.take().is_some() || self.render_menu.is_some();
            self.render_menu = None;
            let append = self.append_hovered || self.append_menu.is_some();
            self.append_hovered = false;
            self.append_menu = None;
            let block = self.hovered_block.take().is_some();
            let menu = self.block_menu.take().is_some();
            if block || menu || append || render {
                self.refresh(cx);
            }
        }
    }
    fn mouse_up(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.dragging = false;
        self.refresh(cx);
        self.update_block_hover(event.position, cx);
    }
    fn scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.render_menu = None;
        self.hovered_render = None;
        self.append_hovered = false;
        self.append_menu = None;
        self.block_menu = None;
        self.hovered_block = None;
        let max = self
            .geometry
            .as_ref()
            .map_or(px(0.), |g| (g.height - g.bounds.size.height).max(px(0.)));
        self.scroll = (self.scroll - event.delta.pixel_delta(px(LINE_HEIGHT)).y).clamp(px(0.), max);
        self.ensure_caret = false;
        cx.notify();
    }
}

pub fn run(
    services: SessionServices,
    desktop: DesktopServices,
    formulas: std::sync::Arc<dyn FormulaRenderer>,
    highlighter: Box<dyn SyntaxHighlighter>,
) {
    let argument = std::env::args_os().nth(1);
    let force_failure = argument.as_deref() == Some(std::ffi::OsStr::new("--smoke-test-failure"));
    let smoke = force_failure || argument.as_deref() == Some(std::ffi::OsStr::new("--smoke-test"));
    let locale = if smoke {
        Localization::for_smoke()
    } else {
        Localization::load()
    };
    let font_storage = if smoke {
        Some(std::env::temp_dir().join(format!("ryne-math-font-smoke-{}", std::process::id())))
    } else {
        i18n::preference_path().map(|p| p.with_file_name("math-font"))
    };
    let mut math_font = math_fonts::MathFontPreference::load(font_storage);
    if smoke {
        math_font.font = MathFont::default();
    }
    formulas.set_font(math_font.font);
    let path = argument.filter(|_| !smoke).map(PathBuf::from);
    Application::new()
        .with_assets(crate::assets::Assets {
            formulas: formulas.clone(),
        })
        .run(move |cx: &mut App| {
            cx.bind_keys([
                KeyBinding::new("left", Left, Some("MarkdownEditor")),
                KeyBinding::new("right", Right, Some("MarkdownEditor")),
                KeyBinding::new("up", Up, Some("MarkdownEditor")),
                KeyBinding::new("down", Down, Some("MarkdownEditor")),
                KeyBinding::new("shift-left", SelectLeft, Some("MarkdownEditor")),
                KeyBinding::new("shift-right", SelectRight, Some("MarkdownEditor")),
                KeyBinding::new("shift-up", SelectUp, Some("MarkdownEditor")),
                KeyBinding::new("shift-down", SelectDown, Some("MarkdownEditor")),
                KeyBinding::new("home", Home, Some("MarkdownEditor")),
                KeyBinding::new("end", End, Some("MarkdownEditor")),
                KeyBinding::new("shift-home", SelectHome, Some("MarkdownEditor")),
                KeyBinding::new("shift-end", SelectEnd, Some("MarkdownEditor")),
                KeyBinding::new("cmd-left", Home, Some("MarkdownEditor")),
                KeyBinding::new("cmd-right", End, Some("MarkdownEditor")),
                KeyBinding::new("cmd-shift-left", SelectHome, Some("MarkdownEditor")),
                KeyBinding::new("cmd-shift-right", SelectEnd, Some("MarkdownEditor")),
                KeyBinding::new("backspace", Backspace, Some("MarkdownEditor")),
                KeyBinding::new("delete", Delete, Some("MarkdownEditor")),
                KeyBinding::new("enter", Enter, Some("MarkdownEditor")),
                KeyBinding::new("shift-enter", HardBreak, Some("MarkdownEditor")),
                KeyBinding::new("tab", Tab, Some("MarkdownEditor")),
                KeyBinding::new("shift-tab", Outdent, Some("MarkdownEditor")),
                KeyBinding::new("cmd-a", SelectAll, Some("MarkdownEditor")),
                KeyBinding::new("cmd-c", Copy, Some("MarkdownEditor")),
                KeyBinding::new("cmd-x", Cut, Some("MarkdownEditor")),
                KeyBinding::new("cmd-v", Paste, Some("MarkdownEditor")),
                KeyBinding::new("cmd-z", Undo, Some("MarkdownEditor")),
                KeyBinding::new("cmd-shift-z", Redo, Some("MarkdownEditor")),
                KeyBinding::new("cmd-n", New, Some("MarkdownEditor")),
                KeyBinding::new("cmd-h", Hide, Some("MarkdownEditor")),
                KeyBinding::new("cmd-shift-r", Reload, Some("MarkdownEditor")),
                KeyBinding::new("cmd-shift-e", ExportPdf, Some("MarkdownEditor")),
                KeyBinding::new("cmd-o", Open, Some("MarkdownEditor")),
                KeyBinding::new("cmd-s", Save, Some("MarkdownEditor")),
                KeyBinding::new("cmd-shift-s", SaveAs, Some("MarkdownEditor")),
                KeyBinding::new("cmd-shift-m", ToggleSource, Some("MarkdownEditor")),
                KeyBinding::new("cmd-shift-b", ToggleBlockSource, Some("MarkdownEditor")),
                KeyBinding::new("escape", Escape, Some("MarkdownEditor")),
                KeyBinding::new("cmd-q", Quit, Some("MarkdownEditor")),
            ]);
            let bounds = Bounds::centered(None, size(px(920.), px(680.)), cx);
            let opened = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(520.), px(360.))),
                    titlebar: Some(TitlebarOptions {
                        appears_transparent: cfg!(target_os = "macos"),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    let sample = if smoke {
                        smoke::SAMPLE
                    } else {
                        locale.language.sample()
                    };
                    let (document, error) =
                        match path.as_ref().map(|p| Document::open(p, services.clone())) {
                            Some(Ok(document)) => (document, None),
                            Some(Err(error)) => (
                                Document::new(sample, services.clone()),
                                Some(error.to_string()),
                            ),
                            None => (Document::new(sample, services.clone()), None),
                        };
                    let editor = cx.new(|cx| {
                        let mut editor = Editor::new(
                            document,
                            desktop,
                            formulas,
                            math_font,
                            highlighter,
                            locale,
                            cx,
                        );
                        if !smoke {
                            editor.recent = recent::RecentFiles::load(
                                i18n::preference_path().map(|p| p.with_file_name("recent.json")),
                            );
                            if let Some(path) = editor.document.files.path.clone() {
                                editor.remember_file(&path, cx);
                            }
                        }
                        editor.sync_application_menus(cx);
                        if !smoke && editor.document.files.path.is_none() {
                            editor.default_document = Some(welcome::DefaultDocument::new(
                                &editor.document,
                                editor.locale.language,
                            ));
                        }
                        if let Some(error) = error {
                            editor.status = editor.locale.language.error(Text::OpenFailed, error);
                        }
                        editor
                    });
                    window.focus(&editor.read(cx).focus);
                    let weak = editor.downgrade();
                    window.on_window_should_close(cx, move |window, cx| {
                        weak.update(cx, |editor, cx| editor.request_close(window, cx))
                            .unwrap_or(true)
                    });
                    if smoke {
                        editor.update(cx, |editor, cx| {
                            editor.start_smoke(force_failure, window, cx)
                        });
                    }
                    editor
                },
            );
            if let Err(error) = opened {
                eprintln!("打开窗口失败：{error}");
                std::process::exit(1)
            }
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
}
