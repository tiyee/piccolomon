//! Window-owned browser. No side panel action edits the document directly.
mod browser;
mod chrome;
mod filter;
mod preferences;
mod tasks;
#[cfg(test)]
mod tests;
mod view;

use super::*;
use browser::Browser;
use filter::Filter;
use preferences::Preferences;
use ryne_extensions::directory::*;
use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::SystemTime,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pane {
    Tree,
    List,
    Outline,
}
impl Pane {
    fn index(self) -> usize {
        match self {
            Self::Tree => 0,
            Self::List => 1,
            Self::Outline => 2,
        }
    }
}

struct Request {
    channel: async_channel::Receiver<tasks::Notice>,
    id: u64,
    path: PathBuf,
    cancel: Arc<AtomicBool>,
}
impl Drop for Request {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.channel.close();
    }
}
#[derive(Clone, PartialEq, Eq)]
struct SummaryKey {
    generation: u64,
    path: PathBuf,
    modified: Option<SystemTime>,
    bytes: Option<u64>,
    revision: u64,
}

struct SavedRevision {
    version: u64,
    reading_metadata: bool,
}

pub(super) struct SidebarState {
    prefs: Preferences,
    browser: Browser,
    focus: FocusHandle,
    filter: Entity<Filter>,
    heading_filter: Entity<Filter>,
    heading_search: bool,
    heading_needle: String,
    heading_matches: Vec<usize>,
    folder_menu: bool,
    scroll: [UniformListScrollHandle; 3],
    browse_focus: [Option<PathBuf>; 2],
    refresh_anchors: [Option<(PathBuf, f32)>; 2],
    outline_focus: Option<(SnapshotId, usize)>,
    pub(super) jump: Option<(SnapshotId, usize)>,
    active: Option<usize>,
    outline_id: Option<SnapshotId>,
    pub(super) opening: Option<PathBuf>,
    resizing: bool,
    pub(super) layout_generation: u64,
    dock: f32,
    ticket: u64,
    picker: u64,
    request: Option<Request>,
    choice: Option<PathBuf>,
    message: Option<String>,
    #[cfg(test)]
    queue_peak: Arc<std::sync::atomic::AtomicUsize>,
    summary_cache: VecDeque<(SummaryKey, String)>,
    summaries: Vec<SummaryKey>,
    summary_cancel: Arc<AtomicBool>,
    saved_revisions: HashMap<PathBuf, SavedRevision>,
    draft_preview: Option<(SnapshotId, String)>,
}
impl SidebarState {
    pub(super) fn new(path: Option<&Path>, cx: &mut Context<Editor>) -> Self {
        let mut browser = Browser::default();
        browser.installed(path);
        let filter = cx.new(Filter::new);
        cx.observe(&filter, |editor, input, cx| {
            let value = &input.read(cx).value;
            if &editor.sidebar.browser.filter != value {
                editor.sidebar.browser.filter = value.clone();
                editor.sidebar.browser.dirty_rows = true;
                editor.sidebar.scroll[1].scroll_to_item_strict(0, ScrollStrategy::Top);
                cx.notify();
            }
        })
        .detach();
        let heading_filter = cx.new(Filter::new);
        cx.observe(&heading_filter, |e, input, cx| {
            if e.sidebar.heading_needle != input.read(cx).value.to_lowercase() {
                cx.notify();
            }
        })
        .detach();
        Self {
            browser,
            filter,
            heading_filter,
            heading_search: false,
            heading_needle: String::new(),
            heading_matches: vec![],
            folder_menu: false,
            prefs: Default::default(),
            focus: cx.focus_handle(),
            scroll: Default::default(),
            browse_focus: Default::default(),
            refresh_anchors: Default::default(),
            outline_focus: None,
            jump: None,
            active: None,
            outline_id: None,
            opening: None,
            resizing: false,
            layout_generation: 0,
            dock: 0.,
            ticket: 0,
            picker: 0,
            request: None,
            choice: None,
            message: None,
            #[cfg(test)]
            queue_peak: Default::default(),
            summary_cache: Default::default(),
            summaries: vec![],
            summary_cancel: Default::default(),
            saved_revisions: Default::default(),
            draft_preview: None,
        }
    }
    pub(super) fn load(&mut self, path: Option<PathBuf>) {
        self.prefs = Preferences::load(path);
    }
    fn pane(&self) -> Pane {
        if !self.prefs.directory.unwrap_or(self.browser.root.is_some()) {
            Pane::Outline
        } else if self.prefs.list {
            Pane::List
        } else {
            Pane::Tree
        }
    }
    fn overlay(&self, window: &Window) -> bool {
        f32::from(window.viewport_size().width) - self.prefs.width < 460.
    }
    fn cancel_request(&mut self) {
        if let Some(req) = &self.request {
            req.cancel.store(true, Ordering::Relaxed);
        }
    }
    fn invalidate_choice(&mut self) {
        if self.choice.is_some() {
            self.cancel_request();
        }
        self.picker += 1;
        self.choice = None;
    }
    fn reset_root(&mut self, refresh: bool, cx: &mut Context<Editor>) {
        self.cancel_request();
        self.folder_menu = false;
        self.summary_cache.clear();
        self.summary_cancel.store(true, Ordering::Relaxed);
        self.summary_cancel = Default::default();
        self.saved_revisions.clear();
        self.message = None;
        if !refresh {
            self.scroll[0] = Default::default();
            self.scroll[1] = Default::default();
            self.browse_focus = Default::default();
            self.refresh_anchors = Default::default();
            self.filter.update(cx, |f, cx| f.reset(cx));
        }
    }
}

impl Editor {
    fn persist_sidebar(&mut self) {
        if let Err(e) = self.sidebar.prefs.save() {
            self.status = self.locale.language.error(Text::PreferenceFailed, e)
        }
    }
    pub(super) fn sidebar_installed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar.invalidate_choice();
        self.sidebar.heading_search = false;
        self.sidebar.heading_filter.update(cx, |f, cx| f.reset(cx));
        if self
            .sidebar
            .browser
            .installed(self.document.files.path.as_deref())
        {
            self.sidebar.reset_root(false, cx);
        }
        if let (Some(path), Some(rel)) = (
            self.sidebar.browser.current.clone(),
            self.sidebar.browser.relative_current().map(Path::to_owned),
        ) && self.sidebar.browser.done_parent(&rel)
        {
            self.refresh_saved_metadata(path, rel, cx);
        }
        self.sidebar.outline_focus = None;
        self.sidebar.scroll[2] = Default::default();
        self.sidebar.jump = None;
        self.sidebar.active = None;
        if self.sidebar.prefs.visible && self.sidebar.overlay(window) {
            self.sidebar.prefs.visible = false;
            self.persist_sidebar();
        }
    }
    pub(super) fn sidebar_saved(
        &mut self,
        path: &Path,
        intent: ryne_session::SaveIntent,
        cx: &mut Context<Self>,
    ) {
        let Ok(absolute) = std::path::absolute(path) else {
            return;
        };
        if self.sidebar.browser.current.as_ref() != Some(&absolute)
            || intent == ryne_session::SaveIntent::SaveAs
        {
            self.sidebar.invalidate_choice();
            if self.sidebar.browser.installed(Some(&absolute)) {
                self.sidebar.reset_root(false, cx);
            }
        }
        let Some(relative) = self.sidebar.browser.relative_current().map(Path::to_owned) else {
            return;
        };
        if !self.sidebar.browser.done_parent(&relative)
            && !self.sidebar.browser.entries.contains_key(&relative)
        {
            return;
        }
        self.refresh_saved_metadata(absolute, relative, cx);
    }
    fn sidebar_protected(&self) -> bool {
        self.viewport.dragging || self.document.composition().is_some()
    }
    pub(super) fn sidebar_toggle(
        &mut self,
        _: &ToggleSidebar,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sidebar_protected() {
            return;
        }
        self.sidebar.folder_menu = false;
        self.sidebar.prefs.visible = !self.sidebar.prefs.visible;
        if self.sidebar.prefs.visible {
            window.focus(&self.sidebar.focus, cx)
        } else {
            window.focus(&self.focus, cx)
        }
        self.sidebar_geometry(window);
        self.persist_sidebar();
        cx.notify();
    }
    pub(super) fn sidebar_geometry(&mut self, window: &Window) -> f32 {
        let dock = if self.sidebar.prefs.visible && !self.sidebar.overlay(window) {
            self.sidebar.prefs.width
        } else {
            0.
        };
        if self.sidebar.dock != dock {
            self.sidebar.dock = dock;
            self.sidebar.layout_generation += 1;
            self.menus.close_all();
            self.viewport.ensure_caret = false;
        }
        dock
    }
    fn select_pane(&mut self, pane: Pane, window: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar_protected() {
            return;
        }
        self.sidebar.folder_menu = false;
        self.sidebar.prefs.directory = Some(pane != Pane::Outline);
        if pane != Pane::Outline {
            self.sidebar.prefs.list = pane == Pane::List
        }
        window.focus(&self.sidebar.focus, cx);
        self.persist_sidebar();
        cx.notify();
    }
    pub(super) fn sidebar_resize_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.sidebar.resizing {
            return;
        }
        if event.pressed_button != Some(MouseButton::Left) {
            self.sidebar_resize_end(cx);
            return;
        }
        self.sidebar.prefs.width = f32::from(event.position.x).clamp(200., 360.);
        self.sidebar_geometry(window);
        cx.notify();
    }
    pub(super) fn sidebar_resize_end(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.sidebar.resizing) {
            self.persist_sidebar();
            cx.notify();
        }
    }
    fn activate_path(
        &mut self,
        generation: u64,
        pane: Pane,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if generation != self.sidebar.browser.generation
            || !self.sidebar.prefs.visible
            || self.sidebar.pane() != pane
            || self.viewport.dragging
        {
            return;
        }
        let Some(entry) = self.sidebar.browser.entries.get(&path) else {
            return;
        };
        let kind = entry.kind;
        self.sidebar.folder_menu = false;
        self.sidebar.browse_focus[pane.index()] = Some(path.clone());
        if kind != EntryKind::Document {
            if kind == EntryKind::Directory && !self.sidebar_protected() {
                window.focus(&self.sidebar.focus, cx);
                if !self.sidebar.browser.expanded.insert(path.clone()) {
                    self.sidebar.browser.expanded.remove(&path);
                    if self
                        .sidebar
                        .request
                        .as_ref()
                        .is_some_and(|r| r.path.starts_with(&path))
                    {
                        self.sidebar.cancel_request();
                    }
                }
                self.sidebar.browser.dirty_rows = true;
                cx.notify();
            }
            return;
        }
        let Some(absolute) = self.sidebar.browser.root.as_ref().map(|p| p.join(&path)) else {
            return;
        };
        if Some(&absolute) == self.sidebar.browser.current.as_ref() {
            if self.sidebar_protected() {
                return;
            }
            if self.sidebar.overlay(window) {
                self.sidebar_toggle(&ToggleSidebar, window, cx)
            }
            window.focus(&self.focus, cx);
            return;
        }
        if !self.operations.blocks_document_replacement() {
            self.open_external_path(absolute.clone(), window, cx);
            if self.operations.file_operation_pending.is_pending() {
                self.sidebar.opening = Some(absolute);
            }
        }
    }
    fn activate_heading(
        &mut self,
        id: SnapshotId,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sidebar_protected()
            || self.document.source.id() != id
            || self.sidebar.pane() != Pane::Outline
            || !self.sidebar.prefs.visible
        {
            return;
        }
        let Some(h) = ryne_syntax::SemanticQuery::new(&self.document.parse)
            .outline()
            .get(index)
        else {
            return;
        };
        self.sidebar.jump = Some((id, h.anchor));
        self.sidebar.outline_focus = Some((id, index));
        if self.sidebar.overlay(window) {
            self.sidebar_toggle(&ToggleSidebar, window, cx)
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }
    pub(super) fn follow_outline(&mut self, geometry: &Geometry, cx: &mut Context<Self>) {
        self.sidebar.jump = None;
        if !self.sidebar.prefs.visible
            || self.sidebar.pane() != Pane::Outline
            || self.sidebar_protected()
        {
            return;
        }
        let y = (geometry.scroll - px(geometry.page.top)).max(px(0.));
        let visible = geometry
            .text_layout
            .visibility
            .rows
            .visible(y, y + geometry.bounds.size.height);
        let source = visible
            .into_iter()
            .filter_map(|i| geometry.rows.get(i))
            .filter(|row| row.top + row.line_height > y)
            .min_by_key(|row| f32::from(row.top.max(y)).to_bits())
            .map(|row| {
                let line = &geometry.lines[row.line];
                line.diagram
                    .map(|i| geometry.display.diagrams[i].source.anchor)
                    .or_else(|| {
                        geometry
                            .display
                            .image_failure_at(row.range.start)
                            .map(|v| v.source.anchor)
                    })
                    .unwrap_or_else(|| {
                        geometry
                            .display
                            .source_at(row.range.start, Affinity::Downstream)
                    })
            })
            .unwrap_or(self.document.source.text().len());
        let headings = ryne_syntax::SemanticQuery::new(&self.document.parse).outline();
        let active = headings
            .partition_point(|h| h.range.start <= source)
            .checked_sub(1);
        if active != self.sidebar.active {
            self.sidebar.active = active;
            cx.notify();
        }
    }
}

impl Drop for SidebarState {
    fn drop(&mut self) {
        self.summary_cancel.store(true, Ordering::Relaxed);
    }
}
