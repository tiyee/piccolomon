//! Opt-in same-thread query diagnostics; absent from default builds.
use std::cell::Cell;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Metrics {
    pub queries: usize,
    pub directories: usize,
    pub semantic_indexes: usize,
    pub physical_lines: usize,
    pub candidates: usize,
    pub comparisons: usize,
    pub records: usize,
    /// Map record payload only; excludes B-tree node overhead and existing indexes.
    pub record_payload_bytes: usize,
}
thread_local! { static COUNTERS: Cell<Metrics> = Cell::new(Metrics::default()); }
pub fn snapshot() -> Metrics {
    COUNTERS.with(Cell::get)
}
pub(crate) fn record(f: impl FnOnce(&mut Metrics)) {
    COUNTERS.with(|cell| {
        let mut value = cell.get();
        f(&mut value);
        cell.set(value);
    });
}
