//! GPUI facade; input, geometry, elements, smoke and asset upload have separate modules.
pub use ryne_syntax::{block, code_block, list, table};
mod alert_smoke;
mod assets;
mod block_math_smoke;
mod block_query_cache;
mod block_tree_smoke;
mod interaction_state;
use interaction_state::*;
mod background;
mod element;
mod export;
mod files;
mod frame_index;
mod hit_index;
mod i18n;
mod image_smoke;
mod input;
pub use image_smoke::smoke_image_renderer;
mod inline_layout_cache;
mod layout;
mod layout_frame;
#[cfg(test)]
mod layout_guard_tests;
mod low_risk_smoke;
mod math_fonts;
mod menus;
mod navigation_index;
mod p1_smoke;
mod paint;
mod profile;
mod recent;
mod recovery;
mod render_controls;
mod sidebar;
mod smoke;
#[cfg(windows)]
mod windows_activation;
pub use smoke::smoke_features;
#[cfg(test)]
mod gpui_upgrade_tests;
mod text_layout;
mod theme;
#[cfg(test)]
mod theme_tests;
mod visibility;
mod welcome;
#[cfg(test)]
mod window_controls_tests;
use element::*;
use gpui::{prelude::*, *};
use i18n::{Language, Localization, Preference, Status, Text};
use layout::*;
use menus::{OpenRecent, SelectLanguage, SelectMathFont};
use render_controls::*;
pub use ryne_extensions::{DesktopServices, ExportDescriptor, ExportRegistry};
use ryne_extensions::{FormulaRenderer, MathFont, SyntaxHighlighter};
use ryne_parser::{Affinity, SnapshotId};
use ryne_projection::{self as presentation, Display};
use ryne_session::{EditorSession as Document, SessionServices};
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
        ClearRecent,
        NoRecent,
        Open,
        OpenFolder,
        ToggleSidebar,
        Save,
        SaveAs,
        FormatSource,
        ToggleSource,
        ToggleBlockSource,
        Escape,
        Quit
    ]
);

fn utf16_to_utf8(text: &str, units: usize) -> Result<usize, ryne_parser::Error> {
    ryne_core::utf16_to_source(text, ryne_core::Utf16Offset(units)).map(|byte| byte.0)
}
const LINE_HEIGHT: f32 = theme::BODY.1;
const APPEND_HEIGHT: f32 = 64.;
fn heading_metrics(level: u8) -> (f32, f32) {
    theme::heading(level)
}

struct Editor {
    sidebar: sidebar::SidebarState,
    document: Document,
    desktop: DesktopServices,
    formulas: std::sync::Arc<dyn FormulaRenderer>,
    math_font: math_fonts::MathFontPreference,
    recent: recent::RecentFiles,
    recovery: recovery::Recovery,
    default_document: Option<welcome::DefaultDocument>,
    highlighter: Box<dyn SyntaxHighlighter>,
    fonts: theme::Fonts,
    focus: FocusHandle,
    status: Status,
    locale: Localization,
    smoke_mode: bool,
    menus: MenuState,
    caret_state: CaretController,
    operations: FileOperationState,
    viewport: ViewportState,
}

impl Editor {
    fn select_language(&mut self, preference: Preference, cx: &mut Context<Self>) {
        self.menus.close_all();
        if let Err(error) = self.locale.select(preference) {
            self.status = self.locale.language.error(Text::PreferenceFailed, error);
        }
        if !self.viewport.dragging
            && self
                .default_document
                .as_mut()
                .is_some_and(|guide| guide.switch(&mut self.document, self.locale.language))
        {
            self.viewport.scroll = px(0.);
            self.viewport.invalidate_geometry();
            self.caret_state.preferred_x = None;
            self.caret_state.caret_affinity = Affinity::Downstream;
            self.viewport.ensure_caret = true;
            self.menus.hovered_render = None;
            self.refresh(cx);
        }
        self.sync_application_menus(cx);
        // User-owned documents and pending IME input retain their exact state.
        cx.notify();
    }

    fn reset_caret_blink(&mut self) {
        self.caret_state.reset_blink();
    }

    fn sync_caret_blink(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window)
            || !window.is_window_active()
            || !self.selected().is_empty()
            || self.viewport.dragging
            || self.document.composition().is_some()
        {
            self.reset_caret_blink();
        } else if self.caret_state.caret_blink_task.is_none() {
            self.caret_state.caret_blink_task = Some(cx.spawn_in(window, async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(500))
                    .await;
                let _ = this.update_in(cx, |editor, _, cx| {
                    editor.caret_state.caret_visible = !editor.caret_state.caret_visible;
                    editor.caret_state.caret_blink_task = None;
                    // Repaint only: blinking must never reparse or change the document.
                    cx.notify();
                });
            }));
        }
    }

    fn open_append_menu(&mut self, snapshot: SnapshotId, cx: &mut Context<Self>) {
        if snapshot == self.document.source.id()
            && self.menus.append_hovered
            && !self.document.source_mode
            && !self.viewport.dragging
            && self.document.composition().is_none()
            && self.menus.append_menu != Some(snapshot)
        {
            self.menus.block_menu = None;
            self.menus.append_menu = Some(snapshot);
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
            || self.viewport.dragging
            || self.document.composition().is_some()
        {
            self.menus.append_menu = None;
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
                self.caret_state.preferred_x = None;
                self.caret_state.caret_affinity = Affinity::Downstream;
                self.viewport.ensure_caret = true;
                window.focus(&self.focus, cx);
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
            || self.viewport.dragging
            || self.document.composition().is_some()
            || self.menus.block_menu.as_ref() != Some(&(snapshot, block.clone()))
            || self.menus.block_insert != Some(before)
        {
            self.menus.block_menu = None;
            self.menus.block_insert = None;
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
                self.caret_state.preferred_x = None;
                self.caret_state.caret_affinity = Affinity::Downstream;
                self.viewport.ensure_caret = true;
                window.focus(&self.focus, cx);
            }
            Err(_) => self.status = Text::InsertFailed.into(),
        }
        self.refresh(cx);
    }

    fn active_block(&self) -> Option<crate::block::Block> {
        self.menus
            .hovered_block
            .as_ref()
            .filter(|(snapshot, _)| *snapshot == self.document.source.id())
            .map(|(_, block)| block.clone())
            .or_else(|| self.focused_block())
    }

    fn focused_block(&self) -> Option<crate::block::Block> {
        self.viewport.block_cache.focused(
            &self.document.parse,
            self.document.anchor,
            self.document.head,
        )
    }

    fn open_block_menu(
        &mut self,
        snapshot: SnapshotId,
        block: crate::block::Block,
        cx: &mut Context<Self>,
    ) {
        if snapshot != self.document.source.id()
            || self.document.source_mode
            || self.document.composition().is_some()
            || self.viewport.dragging
        {
            return;
        }
        if self
            .menus
            .block_menu
            .as_ref()
            .is_some_and(|target| target == &(snapshot, block.clone()))
        {
            return;
        }
        if self.active_block().as_ref() == Some(&block) {
            self.menus.block_insert = None;
            self.menus.block_menu = Some((snapshot, block));
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
        if snapshot != self.document.source.id() || self.document.composition().is_some() {
            self.menus.block_menu = None;
            cx.notify();
            return;
        }
        match action {
            BlockAction::AddBelow | BlockAction::AddAbove => {
                if self.document.source_mode
                    || self.viewport.dragging
                    || self.menus.block_menu.as_ref() != Some(&(snapshot, block.clone()))
                {
                    return;
                }
                let before = matches!(action, BlockAction::AddAbove);
                if before && !block::can_insert_above(&self.document.parse, block) {
                    return;
                }
                self.menus.block_insert = Some(before);
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
        self.caret_state.preferred_x = None;
        self.caret_state.caret_affinity = Affinity::Downstream;
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
        if cx.try_global::<theme::Settings>().is_none() {
            cx.set_global(theme::Settings::load(None, cx.window_appearance()));
        }
        document.normalize_heading_caret();
        let display = document
            .cached_project_batch(&document, Some(highlighter.as_mut()), &[])
            .display;
        let mut editor = Self {
            sidebar: sidebar::SidebarState::new(document.files.path.as_deref(), cx),
            document,
            desktop,
            formulas,
            math_font,
            recent: recent::RecentFiles::default(),
            recovery: Default::default(),
            default_document: None,
            highlighter,
            fonts: theme::Fonts::new(cx.text_system()),
            focus: cx.focus_handle(),
            status: Text::Help.into(),
            locale,
            smoke_mode: false,
            menus: MenuState {
                top_menu: None,
                top_submenu: None,
                block_menu: None,
                block_insert: None,
                hovered_block: None,
                append_hovered: false,
                append_menu: None,
                hovered_render: None,
                render_menu: None,
            },
            caret_state: CaretController {
                preferred_x: None,
                caret_affinity: Affinity::Downstream,
                focus_observed: false,
                caret_visible: true,
                caret_blink_task: None,
            },
            operations: FileOperationState::default(),
            viewport: ViewportState {
                block_cache: Default::default(),
                window_title: None,
                highlight_task: None,
                highlight_generation: 0,
                diagram_images: Vec::new(),
                display,
                geometry: None,
                frame_epoch: 0,
                scroll: px(0.),
                ensure_caret: true,
                dragging: false,
                render_controls: Default::default(),
                control_index: Default::default(),
                inline_layouts: Default::default(),
            },
        };
        editor.refresh(cx);
        editor
    }

    fn editable_text(&self) -> String {
        self.document.editable_text()
    }
    fn selected(&self) -> Range<usize> {
        self.document.editable_selection()
    }
    fn caret(&self) -> usize {
        self.document.editable_caret()
    }
    fn accept_render_controls(&mut self, batch: Result<ryne_session::CachedControls, String>) {
        match batch {
            Ok(batch) => {
                if let Some(diagnostic) = batch.diagnostics.first() {
                    self.status = self.locale.language.error(
                        Text::RenderActionFailed,
                        format!("{}: {}", diagnostic.feature, diagnostic.message),
                    );
                }
                self.document.record_diagnostics(batch.diagnostics);
                if !std::sync::Arc::ptr_eq(&self.viewport.render_controls, &batch.controls) {
                    self.viewport.control_index = text_layout::ControlIndex::new(&batch.controls);
                    self.viewport.render_controls = batch.controls;
                }
            }
            Err(error) => {
                self.status = self.locale.language.error(Text::RenderActionFailed, error);
                self.viewport.render_controls = Default::default();
                self.viewport.control_index = Default::default();
            }
        }
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.refresh_presentation(true, cx);
    }
    fn refresh_presentation(&mut self, interaction: bool, cx: &mut Context<Self>) {
        let _profile = profile::Scope::new("refresh");
        self.track_draft(cx);
        // Keep preedit and drag geometry stable; apply the saved choice when
        // the normal input lifecycle next refreshes a settled document.
        if !self.viewport.dragging && self.document.composition().is_none() {
            self.formulas.set_font(self.math_font.font);
        }
        if interaction {
            self.reset_caret_blink();
            self.menus.render_menu = None;
            self.menus.block_menu = None;
            self.menus.block_insert = None;
            self.menus.append_menu = None;
            self.menus.append_hovered = false;
        }
        let batch = if self.document.composition().is_some() {
            Ok(ryne_session::CachedControls {
                controls: Default::default(),
                diagnostics: Vec::new(),
            })
        } else {
            self.document.cached_render_control_batch(&self.document)
        };
        self.accept_render_controls(batch);
        if self.viewport.dragging
            || self.document.composition().is_some()
            || self.document.source_mode
        {
            self.menus.hovered_render = None;
        }
        if self.viewport.dragging
            || self.document.composition().is_some()
            || self.document.source_mode
            || self
                .menus
                .hovered_block
                .as_ref()
                .is_some_and(|(snapshot, _)| *snapshot != self.document.source.id())
        {
            self.menus.hovered_block = None;
        }
        if interaction && self.document.composition().is_none() {
            self.document.normalize_heading_caret();
        }
        let frozen = self.viewport.dragging
            || !self.document.selection().is_empty()
            || self.document.composition().is_some();
        let folded = self
            .viewport
            .display
            .diagrams
            .iter()
            .map(|v| v.source.range.clone())
            .chain(
                self.viewport
                    .display
                    .image_failures
                    .iter()
                    .map(|v| v.source.range.clone()),
            )
            .collect::<Vec<_>>();
        let (views, request) = self.document.diagram_views(frozen, &folded);
        if let Some(request) = request {
            let task = cx.background_executor().spawn(async move { request.run() });
            cx.spawn(async move |this, cx| {
                let result = task.await;
                let _ = this.update(cx, |editor, cx| {
                    editor.document.complete_job(result);
                    if !editor.viewport.dragging && editor.document.composition().is_none() {
                        editor.refresh(cx);
                    }
                });
            })
            .detach();
        }
        let (temporary, views) = self.document.projection_state(&views);
        if self.document.composition().is_some() {
            let batch = self.document.cached_render_control_batch(&temporary);
            self.accept_render_controls(batch);
        }
        let batch =
            self.document
                .cached_project_batch(&temporary, Some(self.highlighter.as_mut()), &views);
        self.document.record_diagnostics(batch.diagnostics);
        let next_display = batch.display;
        // Caret-only refreshes may reuse geometry. Text, mappings, structure or
        // resource changes invalidate it before any subsequent native callback.
        if !std::sync::Arc::ptr_eq(&self.viewport.display, &next_display)
            && self.viewport.display != next_display
        {
            self.viewport.frame_epoch = self.viewport.frame_epoch.wrapping_add(1);
            self.menus.render_menu = None;
        }
        self.viewport.display = next_display;
        self.viewport.diagram_images =
            crate::assets::diagram_images(&self.viewport.display, &self.viewport.geometry);
        self.schedule_highlight_refresh(cx);
        profile::memory(self);
        cx.notify();
    }
    fn schedule_highlight_refresh(&mut self, cx: &mut Context<Self>) {
        // Replacing this task cancels an older scheduled pass. Never let a timer
        // for another source/preedit reinterpret the current frame.
        self.viewport.highlight_task = None;
        self.viewport.highlight_generation = self.viewport.highlight_generation.wrapping_add(1);
        let pending = match ryne_extensions::catch_callback("highlight", "pending work", || {
            self.highlighter.has_pending_work()
        }) {
            Ok(pending) => pending,
            Err(diagnostic) => {
                self.document.record_diagnostics(vec![diagnostic]);
                false
            }
        };
        if !pending || self.viewport.dragging || self.document.composition().is_some() {
            return;
        }
        let source = self.document.source.id();
        let generation = self.viewport.highlight_generation;
        self.viewport.highlight_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(16))
                .await;
            let _ = this.update(cx, |editor, cx| {
                if editor.viewport.highlight_generation != generation
                    || editor.document.source.id() != source
                {
                    return;
                }
                editor.viewport.highlight_task = None;
                if !editor.viewport.dragging && editor.document.composition().is_none() {
                    editor.refresh_presentation(false, cx);
                }
            });
        }));
    }
    fn commit_composition(&mut self, cx: &mut Context<Self>) -> bool {
        match self.document.commit_composition() {
            Ok(false) => true,
            Ok(true) => {
                self.caret_state.preferred_x = None;
                self.caret_state.caret_affinity = Affinity::Downstream;
                self.refresh(cx);
                true
            }
            Err(error) => {
                self.status = self.locale.language.error(Text::InputFailed, error);
                cx.notify();
                false
            }
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
        self.caret_state.preferred_x = None;
        self.caret_state.caret_affinity = Affinity::Downstream;
        if let Err(error) = self.document.insert_text_from(range, text, origin) {
            self.status = self
                .locale
                .language
                .error(Text::InputFailed, format!("{:?}", error));
        }
        self.viewport.ensure_caret = true;
        self.refresh(cx);
    }
    fn move_to(&mut self, offset: usize, extend: bool, cx: &mut Context<Self>) {
        self.menus.hovered_block = None;
        self.caret_state.preferred_x = None;
        self.caret_state.caret_affinity = Affinity::Downstream;
        self.document
            .select(offset, extend, self.viewport.dragging || extend);
        self.viewport.ensure_caret = true;
        self.refresh(cx);
    }
    fn horizontal(&mut self, forward: bool, extend: bool, cx: &mut Context<Self>) {
        if !self.commit_composition(cx) {
            return;
        }
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
        let position = self.viewport.display.position_at(self.document.head);
        if !extend
            && let Some(anchor) = self
                .viewport
                .display
                .diagrams
                .iter()
                .map(|v| (&v.display, v.source.anchor))
                .chain(
                    self.viewport
                        .display
                        .image_failures
                        .iter()
                        .map(|v| (&v.display, v.source.anchor)),
                )
                .find_map(|(range, anchor)| {
                    (if forward {
                        position == range.start
                    } else {
                        position == range.end
                    })
                    .then_some(anchor)
                })
        {
            self.move_to(anchor, false, cx);
            return;
        }
        if !extend
            && let Some(formula) = self.viewport.display.formulas.iter().find(|formula| {
                if forward {
                    position == formula.display.start
                } else {
                    position == formula.display.end
                }
            })
        {
            let source = if forward {
                formula.source.start + if formula.block { 2 } else { 1 }
            } else {
                formula.source.end - if formula.block { 2 } else { 1 }
            };
            self.move_to(source, false, cx);
            return;
        }
        let next = if forward {
            self.viewport
                .display
                .next_grapheme(position)
                .unwrap_or(position)
        } else {
            self.viewport
                .display
                .previous_grapheme(position)
                .unwrap_or(0)
        };
        let affinity = if forward {
            Affinity::Upstream
        } else {
            Affinity::Downstream
        };
        self.move_to(self.viewport.display.source_at(next, affinity), extend, cx);
    }
    fn vertical(&mut self, down: bool, extend: bool, cx: &mut Context<Self>) {
        if !self.commit_composition(cx) {
            return;
        }
        if let Some(geometry) = &self.viewport.geometry {
            if !geometry.is_current(self) {
                return;
            }
            let offset = geometry.display.position_at(self.document.head);
            let x = self.caret_state.preferred_x.unwrap_or_else(|| {
                geometry
                    .position_with_affinity(offset, self.caret_state.caret_affinity)
                    .x
            });
            let (hit, affinity) =
                geometry.vertical_target(offset, self.caret_state.caret_affinity, x, down);
            let source = geometry
                .display
                .source_byte_at(ryne_core::DisplayByte(hit), affinity)
                .0;
            self.move_to(source, extend, cx);
            self.caret_state.preferred_x = Some(x);
            self.caret_state.caret_affinity = affinity;
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
        if !self.commit_composition(cx) {
            return;
        }
        if let Some(geometry) = &self.viewport.geometry {
            if !geometry.is_current(self) {
                return;
            }
            let position = geometry.display.position_at(self.document.head);
            if let Some(row) = geometry.row_for(position, self.caret_state.caret_affinity) {
                let affinity = if end {
                    Affinity::Upstream
                } else {
                    Affinity::Downstream
                };
                let offset = if end { row.range.end } else { row.range.start };
                let source = geometry.display.source_at(offset, affinity);
                self.move_to(source, extend, cx);
                self.caret_state.caret_affinity = affinity;
            }
        }
    }
    fn remove(&mut self, forward: bool, cx: &mut Context<Self>) {
        if !self.commit_composition(cx) {
            return;
        }
        let selection = self.document.selection();
        let position = self.viewport.display.position_at(self.document.head);
        if selection.is_empty()
            && let Some(anchor) = self
                .viewport
                .display
                .diagrams
                .iter()
                .map(|v| (&v.display, v.source.anchor))
                .chain(
                    self.viewport
                        .display
                        .image_failures
                        .iter()
                        .map(|v| (&v.display, v.source.anchor)),
                )
                .find_map(|(range, anchor)| {
                    (if forward {
                        position == range.start
                    } else {
                        position == range.end
                    })
                    .then_some(anchor)
                })
        {
            self.move_to(anchor, false, cx);
            return;
        }
        let removed = self.document.delete(forward);
        match removed {
            Ok(true) => {
                self.caret_state.preferred_x = None;
                self.caret_state.caret_affinity = Affinity::Downstream;
                self.viewport.ensure_caret = true;
                self.refresh(cx);
            }
            Err(error) => {
                self.status = self
                    .locale
                    .language
                    .error(Text::DeleteFailed, format!("{:?}", error));
            }
            Ok(false) => {}
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
        if self.document.composition().is_some() {
            if !self.commit_composition(cx) {
                return;
            }
            return;
        }
        if let Err(error) = self.document.line_break(hard) {
            self.status = self
                .locale
                .language
                .error(Text::BreakFailed, format!("{:?}", error));
        }
        self.caret_state.preferred_x = None;
        self.caret_state.caret_affinity = Affinity::Downstream;
        self.viewport.ensure_caret = true;
        self.refresh(cx);
    }
    fn tab(&mut self, _: &Tab, _: &mut Window, cx: &mut Context<Self>) {
        if !self.commit_composition(cx) {
            return;
        }
        let handled = self.document.tab(false);
        match handled {
            Ok(true) => {
                self.caret_state.preferred_x = None;
                self.caret_state.caret_affinity = Affinity::Downstream;
                self.viewport.ensure_caret = true;
                self.refresh(cx);
            }
            Ok(false) => {}
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
        if !self.commit_composition(cx) {
            return;
        }
        self.caret_state.preferred_x = None;
        self.caret_state.caret_affinity = Affinity::Downstream;
        if let Err(error) = self.document.tab(true) {
            self.status = self
                .locale
                .language
                .error(Text::OutdentFailed, format!("{:?}", error));
        }
        self.viewport.ensure_caret = true;
        self.refresh(cx);
    }
    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        if !self.commit_composition(cx) {
            return;
        }
        self.caret_state.preferred_x = None;
        self.caret_state.caret_affinity = Affinity::Downstream;
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
        if !self.commit_composition(cx) {
            return;
        }
        self.copy(&Copy, window, cx);
        self.insert(self.document.selection(), "", cx);
    }
    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let text = cx.read_from_clipboard().and_then(|item| item.text());
        self.paste_content(text, cx);
    }
    // Both action and platform paste use the same Markdown transaction path.
    fn paste_content(&mut self, text: Option<String>, cx: &mut Context<Self>) {
        if !self.commit_composition(cx) {
            return;
        }
        if let Some(text) = text {
            self.insert_from(
                self.document.selection(),
                &text,
                ryne_extensions::InputOrigin::Paste,
                cx,
            );
        }
    }
    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.caret_state.preferred_x = None;
        self.caret_state.caret_affinity = Affinity::Downstream;
        if !self.document.cancel_composition()
            && let Err(error) = self.document.try_undo(false)
        {
            self.status = self.locale.language.error(Text::InputFailed, error);
        }
        self.viewport.ensure_caret = true;
        self.refresh(cx);
    }
    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if !self.commit_composition(cx) {
            return;
        }
        if let Err(error) = self.document.try_undo(true) {
            self.status = self.locale.language.error(Text::InputFailed, error);
        }
        self.caret_state.preferred_x = None;
        self.caret_state.caret_affinity = Affinity::Downstream;
        self.viewport.ensure_caret = true;
        self.refresh(cx);
    }
    fn format_source(&mut self, _: &FormatSource, window: &mut Window, cx: &mut Context<Self>) {
        if self.viewport.dragging || self.document.composition().is_some() {
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
                self.caret_state.preferred_x = None;
                self.caret_state.caret_affinity = Affinity::Downstream;
                self.viewport.ensure_caret = true;
                self.refresh(cx);
            }
            Err(error) => {
                self.status = self.locale.language.error(Text::FormatFailed, error);
                cx.notify();
            }
        }
        self.focus.focus(window, cx);
    }
    fn toggle_source(&mut self, _: &ToggleSource, _: &mut Window, cx: &mut Context<Self>) {
        if !self.commit_composition(cx) {
            return;
        }
        self.document.clear_heading_reveal();
        self.document.set_block_source(None);
        self.document.set_source_mode(!self.document.source_mode);
        self.caret_state.preferred_x = None;
        self.caret_state.caret_affinity = Affinity::Downstream;
        self.viewport.ensure_caret = true;
        self.refresh(cx);
    }
    fn toggle_block_source(
        &mut self,
        _: &ToggleBlockSource,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.commit_composition(cx) {
            return;
        }
        if self.document.toggle_block_source() {
            self.caret_state.preferred_x = None;
            self.caret_state.caret_affinity = Affinity::Downstream;
            self.viewport.ensure_caret = true;
            self.refresh(cx);
        } else {
            self.status = Text::PlaceCaret.into();
            cx.notify();
        }
    }
    fn escape(&mut self, _: &Escape, _: &mut Window, cx: &mut Context<Self>) {
        if self.menus.top_menu.take().is_some() {
            self.menus.top_submenu = None;
            cx.notify();
            return;
        }
        if let Some(menu) = &mut self.menus.render_menu {
            if menu.submenu.take().is_none() {
                self.menus.render_menu = None;
            }
            cx.notify();
            return;
        }
        if self.menus.append_menu.take().is_some() {
            cx.notify();
            return;
        }
        if self.menus.block_menu.is_some() {
            if self.menus.block_insert.take().is_none() {
                self.menus.block_menu = None;
            }
            cx.notify();
            return;
        }
        self.caret_state.preferred_x = None;
        self.caret_state.caret_affinity = Affinity::Downstream;
        self.document.cancel_composition();
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
        self.menus.render_menu = None;
        if !self.commit_composition(cx) {
            return;
        }
        window.focus(&self.focus, cx);
        if let Some(geometry) = &self.viewport.geometry {
            if !geometry.is_current(self) {
                return;
            }
            if !event.modifiers.shift {
                let local = event.position
                    - geometry.bounds.origin
                    - point(
                        px(geometry.page.left),
                        px(geometry.page.top) - geometry.scroll,
                    );
                let header = geometry.text_layout.quote_panels.iter().find_map(|panel| {
                    let alert = panel.alert.as_ref().filter(|a| a.folded)?;
                    panel
                        .header_bounds
                        .contains(&local)
                        .then(|| alert.source.clone())
                });
                if let Some(header) = header {
                    self.document.set_block_source(Some(header.source));
                    self.document.select(header.marker.start + 2, false, false);
                    self.caret_state.preferred_x = None;
                    self.viewport.dragging = false;
                    self.refresh(cx);
                    return;
                }
            }
            // Resolve once against the displayed layout, then retain source anchors.
            let (hit, affinity) = geometry.hit_caret(event.position);
            let source = if !event.modifiers.shift {
                geometry.formula_at_point(event.position).map_or_else(
                    || {
                        geometry
                            .display
                            .source_byte_at(ryne_core::DisplayByte(hit), affinity)
                            .0
                    },
                    |formula| formula.source.start + if formula.block { 2 } else { 1 },
                )
            } else {
                geometry
                    .display
                    .source_byte_at(ryne_core::DisplayByte(hit), affinity)
                    .0
            };
            let source = if !event.modifiers.shift {
                geometry
                    .text_layout
                    .visibility
                    .lines
                    .visible(
                        event.position.y - geometry.bounds.top() - px(geometry.page.top)
                            + geometry.scroll,
                        event.position.y - geometry.bounds.top() - px(geometry.page.top)
                            + geometry.scroll,
                    )
                    .into_iter()
                    .map(|i| &geometry.lines[i])
                    .find_map(|line| {
                        let anchor = line
                            .diagram
                            .map(|i| geometry.display.diagrams[i].source.anchor)
                            .or_else(|| {
                                geometry
                                    .display
                                    .image_failure_at(line.start)
                                    .map(|v| v.source.anchor)
                            })?;
                        let top = geometry.bounds.top() + px(geometry.page.top) + line.top
                            - geometry.scroll;
                        (event.position.y >= top
                            && event.position.y < top + line.shaped.size(line.line_height).height)
                            .then_some(anchor)
                    })
                    .unwrap_or(source)
            } else {
                source
            };
            self.document.select(source, event.modifiers.shift, false);
            if !event.modifiers.shift {
                self.document.reveal_clicked_heading();
            }
            self.caret_state.preferred_x = None;
            self.caret_state.caret_affinity = affinity;
            self.viewport.dragging = true;
            self.refresh(cx);
        }
    }
    fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.viewport.dragging
            && let Some(geometry) = &self.viewport.geometry
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
            self.caret_state.caret_affinity = affinity;
            self.refresh(cx);
        } else if !self.viewport.dragging {
            if self.update_render_hover(event.position, cx) {
                return;
            }
            self.update_block_hover(event.position, cx);
        }
    }
    fn update_block_hover(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.document.composition().is_some()
            || self.document.source_mode
            || self.viewport.dragging
        {
            return;
        }
        let Some(geometry) = &self.viewport.geometry else {
            return;
        };
        if !geometry.is_current(self) {
            return;
        }
        if self.menus.append_menu.is_some()
            && let (Some(control), Some(menu)) = (geometry.append_control, geometry.append_menu)
            && position.x >= control.left().min(menu.left())
            && position.x < control.right().max(menu.right())
            && position.y >= control.top().min(menu.top())
            && position.y < control.bottom().max(menu.bottom())
        {
            return;
        }
        if self.menus.block_menu.is_some()
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
            let changed = !self.menus.append_hovered
                || self.menus.block_menu.is_some()
                || self.menus.hovered_block.is_some();
            self.menus.append_hovered = true;
            self.menus.block_menu = None;
            self.menus.hovered_block = None;
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
        if self.menus.append_hovered || self.menus.append_menu.is_some() {
            self.menus.append_hovered = false;
            self.menus.append_menu = None;
            cx.notify();
        }
        if geometry
            .block_control
            .is_some_and(|control| control.contains(&position))
        {
            if let Some(block) = geometry.block_target.clone() {
                let snapshot = geometry.snapshot;
                self.menus.hovered_block = Some((snapshot, block.clone()));
                self.open_block_menu(snapshot, block, cx);
            }
            return;
        }
        let block = geometry
            .bounds
            .contains(&position)
            .then(|| geometry.hover_source(position))
            .flatten()
            .and_then(|source| self.viewport.block_cache.at(&self.document.parse, source))
            .map(|block| (geometry.snapshot, block));
        let closed_menu = self.menus.block_menu.take().is_some();
        if self.menus.hovered_block != block || closed_menu {
            self.menus.hovered_block = block;
            cx.notify();
        }
    }
    fn editor_hover(&mut self, hovered: &bool, _: &mut Window, cx: &mut Context<Self>) {
        if !hovered && !self.viewport.dragging {
            let render =
                self.menus.hovered_render.take().is_some() || self.menus.render_menu.is_some();
            self.menus.render_menu = None;
            let append = self.menus.append_hovered || self.menus.append_menu.is_some();
            self.menus.append_hovered = false;
            self.menus.append_menu = None;
            let block = self.menus.hovered_block.take().is_some();
            let menu = self.menus.block_menu.take().is_some();
            if block || menu || append || render {
                self.refresh(cx);
            }
        }
    }
    fn mouse_up(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.viewport.dragging = false;
        self.refresh(cx);
        self.update_block_hover(event.position, cx);
    }
    fn scroll(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.menus.render_menu = None;
        self.menus.hovered_render = None;
        self.menus.append_hovered = false;
        self.menus.append_menu = None;
        self.menus.block_menu = None;
        self.menus.hovered_block = None;
        let max = self
            .viewport
            .geometry
            .as_ref()
            .map_or(px(0.), |g| (g.height - g.bounds.size.height).max(px(0.)));
        self.viewport.scroll =
            (self.viewport.scroll - event.delta.pixel_delta(px(LINE_HEIGHT)).y).clamp(px(0.), max);
        self.viewport.ensure_caret = false;
        cx.notify();
    }
}

fn bind_window_keys(cx: &mut App) {
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
        KeyBinding::new("cmd-\\", ToggleSidebar, Some("RyneWindow")),
        KeyBinding::new("cmd-n", New, Some("RyneWindow")),
        KeyBinding::new("cmd-h", Hide, Some("RyneWindow")),
        KeyBinding::new("cmd-shift-r", Reload, Some("RyneWindow")),
        KeyBinding::new("cmd-shift-e", ExportPdf, Some("RyneWindow")),
        KeyBinding::new("cmd-o", Open, Some("RyneWindow")),
        KeyBinding::new("cmd-s", Save, Some("RyneWindow")),
        KeyBinding::new("cmd-shift-s", SaveAs, Some("RyneWindow")),
        KeyBinding::new("cmd-shift-m", ToggleSource, Some("MarkdownEditor")),
        KeyBinding::new("cmd-shift-b", ToggleBlockSource, Some("MarkdownEditor")),
        KeyBinding::new("escape", Escape, Some("MarkdownEditor")),
        KeyBinding::new("cmd-q", Quit, Some("RyneWindow")),
    ]);
    #[cfg(windows)]
    cx.bind_keys([
        KeyBinding::new("ctrl-\\", ToggleSidebar, Some("RyneWindow")),
        KeyBinding::new("ctrl-o", Open, Some("RyneWindow")),
        KeyBinding::new("ctrl-s", Save, Some("RyneWindow")),
        KeyBinding::new("ctrl-shift-s", SaveAs, Some("RyneWindow")),
    ]);
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
    let path = argument
        .filter(|_| !smoke)
        .map(PathBuf::from)
        .map(|p| std::path::absolute(&p).unwrap_or(p));
    let (open_url_sender, open_url_receiver) = async_channel::unbounded::<PathBuf>();
    #[cfg(windows)]
    if !smoke
        && let Some(dir) =
            i18n::preference_path().and_then(|p| p.parent().map(std::path::Path::to_owned))
    {
        match windows_activation::start(&dir, path.as_deref(), open_url_sender.clone()) {
            Ok(true) => return,
            Ok(false) => (),
            Err(e) => eprintln!("Ryne file activation: {e}"),
        }
    }
    let application = gpui_platform::application().with_assets(crate::assets::Assets {
        formulas: formulas.clone(),
    });
    application.on_open_urls(move |urls| {
        if let Some(path) = urls.iter().find_map(|url| files::file_url_to_path(url)) {
            let _ = open_url_sender.try_send(path);
        }
    });
    application.run(move |cx: &mut App| {
        let theme_storage = if smoke {
            Some(std::env::temp_dir().join(format!("ryne-theme-smoke-{}", std::process::id())))
        } else {
            i18n::preference_path().map(|p| p.with_file_name("theme"))
        };
        let mut appearance = theme::Settings::load(theme_storage, cx.window_appearance());
        if smoke {
            appearance.mode = theme::Mode::Light;
        }
        cx.set_global(appearance);
        bind_window_keys(cx);
        let bounds = Bounds::centered(None, size(px(920.), px(680.)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(520.), px(360.))),
                titlebar: Some(TitlebarOptions {
                    appears_transparent: cfg!(any(target_os = "macos", target_os = "windows")),
                    // GPUI takes the top-left position of the native 16 px button frames.
                    traffic_light_position: cfg!(target_os = "macos")
                        .then(|| point(px(8.), px((element::WINDOW_HEADER_HEIGHT - 16.) / 2.))),
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
                        editor.sidebar.load(
                            i18n::preference_path().map(|p| p.with_file_name("sidebar.json")),
                        );
                        editor.recovery.directory =
                            i18n::preference_path().map(|p| p.with_file_name("drafts"));
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
                let focus = editor.read(cx).focus.clone();
                window.focus(&focus, cx);
                let weak = editor.downgrade();
                window.on_window_should_close(cx, move |window, cx| {
                    weak.update(cx, |editor, cx| editor.request_close(window, cx))
                        .unwrap_or(true)
                });
                if !smoke && path.is_none() {
                    editor.update(cx, |editor, cx| editor.offer_drafts(window, cx));
                }
                if smoke {
                    editor.update(cx, |editor, cx| {
                        editor.start_smoke(force_failure, window, cx)
                    });
                }
                editor
            },
        );
        let editor_window = match opened {
            Ok(editor_window) => editor_window,
            Err(error) => {
                eprintln!("打开窗口失败：{error}");
                std::process::exit(1)
            }
        };
        cx.spawn(async move |cx| {
            while let Ok(path) = open_url_receiver.recv().await {
                if editor_window
                    .update(cx, |editor, window, cx| {
                        editor.open_external_path(path, window, cx)
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        cx.on_window_closed(|cx, _window_id| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}
