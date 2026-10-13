//! Opt-in local profiling. Disabled runs allocate nothing and do not read the clock.
use std::{sync::OnceLock, time::Instant};
pub(super) struct Scope {
    start: Option<Instant>,
    label: &'static str,
}
impl Scope {
    pub(super) fn new(label: &'static str) -> Self {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        let enabled = *ENABLED.get_or_init(|| std::env::var_os("RYNE_PROFILE").is_some());
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
