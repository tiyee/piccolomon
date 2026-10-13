//! Executable, startup-registered contributions. Implementations only prepare plans.
use crate::*;
use ryne_projection::{DiagramViewInput, Display};

pub struct FeatureServices<'a> {
    pub formulas: &'a dyn FormulaRenderer,
    pub language_detector: Option<&'a dyn CodeLanguageDetector>,
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
    fn display(
        &self,
        state: &EditingState,
        services: &FeatureServices<'_>,
        highlighter: Option<&mut dyn SyntaxHighlighter>,
        diagrams: &[DiagramViewInput],
    ) -> Display;
}
