//! UI-owned interaction components; document and IME state remain in the session.
use super::*;
pub(super) struct MenuState {
    pub top_menu: Option<usize>,
    pub top_submenu: Option<usize>,
    pub block_menu: Option<(SnapshotId, crate::block::Block)>,
    pub block_insert: Option<bool>,
    pub hovered_block: Option<(SnapshotId, crate::block::Block)>,
    pub append_hovered: bool,
    pub append_menu: Option<SnapshotId>,
    pub hovered_render: Option<String>,
    pub render_menu: Option<RenderMenu>,
}
pub(super) struct CaretController {
    pub preferred_x: Option<Pixels>,
    pub caret_affinity: Affinity,
    pub focus_observed: bool,
    pub caret_visible: bool,
    pub caret_blink_task: Option<Task<()>>,
}
pub(super) struct FileOperationState {
    pub file_operation_pending: bool,
    pub exporting: bool,
    pub checking_updates: bool,
    pub allow_close: bool,
    pub close_prompt: bool,
    pub close_after_save: bool,
}
pub(super) struct ViewportState {
    pub block_cache: crate::block_query_cache::BlockQueryCache,
    pub window_title: Option<String>,
    pub highlight_task: Option<Task<()>>,
    pub highlight_generation: u64,
    pub diagram_images: Vec<std::sync::Arc<RenderImage>>,
    pub display: std::sync::Arc<ryne_projection::IndexedDisplay>,
    pub geometry: Option<Geometry>,
    pub frame_epoch: u64,
    pub scroll: Pixels,
    pub ensure_caret: bool,
    pub dragging: bool,
    pub render_controls: std::sync::Arc<Vec<ryne_extensions::RenderControl>>,
    pub control_index: text_layout::ControlIndex,
}

impl MenuState {
    pub fn close_all(&mut self) {
        self.top_menu = None;
        self.top_submenu = None;
        self.block_menu = None;
        self.block_insert = None;
        self.append_menu = None;
        self.render_menu = None;
    }
}
impl CaretController {
    pub fn reset_blink(&mut self) {
        self.caret_blink_task = None;
        self.caret_visible = true;
    }
}
impl FileOperationState {
    pub fn blocks_document_replacement(&self) -> bool {
        self.file_operation_pending || self.close_prompt || self.exporting
    }
}
impl ViewportState {
    pub fn invalidate_geometry(&mut self) {
        self.geometry = None;
    }
}
