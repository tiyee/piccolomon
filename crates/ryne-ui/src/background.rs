//! Service panics become terminal receipts before crossing the executor boundary.
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct OperationToken(u64);
pub(super) struct Receipt<T> {
    token: OperationToken,
    result: Result<T, String>,
}
#[derive(Default)]
pub(super) struct OperationSlot {
    active: Option<OperationToken>,
}
impl OperationSlot {
    pub fn is_pending(&self) -> bool {
        self.active.is_some()
    }
    pub fn begin(&mut self) -> Option<OperationToken> {
        if self.is_pending() {
            return None;
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let token = OperationToken(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        self.active = Some(token);
        Some(token)
    }
    pub fn finish(&mut self, token: OperationToken) -> bool {
        if self.active != Some(token) {
            return false;
        }
        self.active = None;
        true
    }
    pub fn complete<T>(&mut self, receipt: Receipt<T>) -> Option<Result<T, String>> {
        self.finish(receipt.token).then_some(receipt.result)
    }
}
impl OperationToken {
    fn run<T>(self, name: &str, work: impl FnOnce() -> Result<T, String>) -> Receipt<T> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
            .unwrap_or_else(|_| Err(format!("{name} service panicked")));
        Receipt {
            token: self,
            result,
        }
    }
    pub fn open(
        self,
        path: Option<PathBuf>,
        services: ryne_session::SessionServices,
    ) -> Receipt<ryne_session::EditorSession> {
        self.run("open", || match path {
            Some(path) => {
                ryne_session::EditorSession::open(&path, services).map_err(|e| e.to_string())
            }
            None => Ok(ryne_session::EditorSession::new("", services)),
        })
    }
    pub fn export(
        self,
        snapshot: ryne_session::ExportSnapshot,
        exporter: Arc<dyn ryne_extensions::PdfExporter>,
        artifacts: Arc<dyn ryne_extensions::ArtifactStore>,
        path: PathBuf,
    ) -> Receipt<()> {
        self.run("export", || {
            let display = snapshot.render()?;
            let bytes = exporter.export(&display)?;
            artifacts.write(&path, &bytes).map_err(|e| e.to_string())
        })
    }
    pub fn check_updates(
        self,
        checker: Arc<dyn ryne_extensions::UpdateChecker>,
        version: &str,
    ) -> Receipt<ryne_extensions::UpdateStatus> {
        self.run("update check", || checker.check(version))
    }
}

#[cfg(test)]
#[path = "background_tests.rs"]
mod tests;
