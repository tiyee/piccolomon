//! Opt-in local profiling. Disabled runs allocate nothing and do not read the clock.
use std::{sync::OnceLock, time::Instant};
pub(super) struct Scope {
    start: Option<Instant>,
    label: &'static str,
}
impl Scope {
    pub(super) fn new(label: &'static str) -> Self {
        let enabled = enabled();
        Self {
            start: enabled.then(Instant::now),
            label,
        }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            eprintln!(
                "RYNE_PROFILE {{\"metric\":\"{}\",\"elapsed_ms\":{:.4}}}",
                self.label,
                start.elapsed().as_secs_f64() * 1000.
            );
        }
    }
}

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("RYNE_PROFILE").is_some())
}
pub(super) fn shape_work(work: crate::inline_layout_cache::ShapeWork) {
    if enabled() {
        eprintln!("RYNE_PROFILE {{\"metric\":\"shape_work\",\"hits\":{},\"misses\":{},\"shaped_bytes\":{}}}",
            work.hits, work.misses, work.shaped_bytes);
    }
}
/// Counters are per process; IDs distinguish successive documents in a window.
pub(super) fn layout(reused: bool) {
    if !enabled() {
        return;
    }
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
    static BUILDS: AtomicU64 = AtomicU64::new(0);
    static HITS: AtomicU64 = AtomicU64::new(0);
    let counter = if reused { &HITS } else { &BUILDS };
    let count = counter.fetch_add(1, Relaxed) + 1;
    let metric = if reused {
        "layout_cache_hit"
    } else {
        "layout_build_attempt"
    };
    eprintln!("RYNE_PROFILE {{\"metric\":\"{metric}\",\"count\":{count}}}");
}
/// Owned history and current published control batch only, not total process memory.
pub(super) fn memory(editor: &super::Editor) {
    if !enabled() {
        return;
    }
    use ryne_extensions::RenderControlPlacement;
    let controls = &editor.viewport.render_controls;
    let history = editor.document.history_memory();
    let mut actions = 0;
    let bytes = controls.capacity() * std::mem::size_of::<ryne_extensions::RenderControl>()
        + controls
            .iter()
            .map(|c| {
                actions += c.actions.len();
                c.id.capacity()
                    + c.target.part.capacity() * std::mem::size_of::<usize>()
                    + c.actions.capacity() * std::mem::size_of::<ryne_extensions::RenderAction>()
                    + match &c.placement {
                        RenderControlPlacement::Header { label } => label.capacity(),
                        _ => 0,
                    }
            })
            .sum::<usize>();
    eprintln!(
        "RYNE_PROFILE {}",
        serde_json::json!({
            "metric": "owned_memory_sample", "document": format!("{:?}", editor.document.source.id().document_id),
            "revision": editor.document.source.revision(), "source_bytes": editor.document.source.text().len(),
            "undo_entries": history.undo_entries, "redo_entries": history.redo_entries,
            "history_reserved_bytes": history.reserved_bytes,
            "published_controls": controls.len(), "published_actions": actions,
            "published_controls_reserved_bytes": bytes,
        })
    );
}
