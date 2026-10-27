//! Executable, startup-registered contributions. Implementations only prepare plans.
use crate::*;
use ryne_projection::{DiagramViewInput, Display};

pub struct FeatureServices<'a> {
    pub formulas: &'a dyn FormulaRenderer,
    pub planning: CommandDisplayCache,
}
/// Disposable mapping shared only by handlers in a single registry dispatch.
#[derive(Default)]
pub struct CommandDisplayCache(std::cell::RefCell<Option<CommandDisplayEntry>>);
struct CommandDisplayEntry {
    key: PresentationKey,
    display: std::sync::Arc<ryne_projection::IndexedDisplay>,
}
impl CommandDisplayCache {
    pub fn clear(&self) {
        self.0.borrow_mut().take();
    }
    pub fn get_or_build(
        &self,
        key: PresentationKey,
        build: impl FnOnce() -> Display,
    ) -> std::sync::Arc<ryne_projection::IndexedDisplay> {
        if let Some(entry) = &*self.0.borrow()
            && entry.key == key
        {
            return entry.display.clone();
        }
        let display = std::sync::Arc::new(ryne_projection::IndexedDisplay::new(build()));
        *self.0.borrow_mut() = Some(CommandDisplayEntry {
            key,
            display: display.clone(),
        });
        display
    }
}
pub trait CommandHandler: Send + Sync {
    /// None delegates to the next registered handler. An error aborts planning.
    fn plan(
        &self,
        intent: &EditIntent,
        state: &EditingState,
        services: &FeatureServices<'_>,
    ) -> Result<Option<CommandPlan>, String>;
}
pub trait PresentationProvider: Send + Sync {
    /// No declaration means recompute. The key must describe every input used
    /// by this provider; highlighter and diagram identities are tracked by session.
    fn cache_key(&self, _: &EditingState, _: &FeatureServices<'_>) -> Option<PresentationKey> {
        None
    }
    fn display(
        &self,
        state: &EditingState,
        services: &FeatureServices<'_>,
        highlighter: Option<&mut dyn SyntaxHighlighter>,
        diagrams: &[DiagramViewInput],
    ) -> Display;
}
