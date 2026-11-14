//! Opt-in stage timings shared by the source, semantic and presentation paths.
//! Disabled runs do not read the clock or allocate a sample.
use std::{sync::OnceLock, time::Instant};

pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("RYNE_PROFILE").is_some())
}
pub struct Scope {
    start: Option<Instant>,
    label: &'static str,
}
impl Scope {
    pub fn new(label: &'static str) -> Self {
        Self {
            start: enabled().then(Instant::now),
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
