//! Typed, read-only feature and service contracts for static built-ins.
use editable_markdown_parser::{Error, Session, Source};
use ryne_core::{DocumentKey, PreparedEdit, SourceRange};
use ryne_projection::{Class, DiagramOutcome, DiagramViewInput, Display, Formula};
use ryne_syntax::code_block::CodeBlock;
use ryne_syntax::ParseSnapshot;
use std::{
    ops::Range,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct EditingState {
    pub source: Source,
    pub parse: Arc<ParseSnapshot>,
    pub anchor: usize,
    pub head: usize,
    pub reveal: Option<Session>,
    pub heading_reveal: Option<Session>,
    pub block_source: Option<Session>,
    pub source_mode: bool,
    pub closers: Vec<Range<usize>>,
}
impl EditingState {
    pub fn new(source: Source) -> Self {
        Self {
            parse: Arc::new(ParseSnapshot::with_math(source.clone())),
            source,
            anchor: 0,
            head: 0,
            reveal: None,
            heading_reveal: None,
            block_source: None,
            source_mode: false,
            closers: Vec::new(),
        }
    }
    pub fn selection(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
}
#[derive(Clone)]
pub enum EditIntent {
    FormatSource,
    /// Semantic rules delegate in registration order; the final rule edits plain text.
    Delete {
        forward: bool,
    },
    Tab {
        backward: bool,
    },
    Normalize,
    RestoreHeading,
    Select {
        position: usize,
        extend: bool,
        frozen: bool,
    },
    RevealHeading,
    ToggleBlockSource,
    Insert {
        range: SourceRange,
        text: String,
        origin: InputOrigin,
    },
    Replace {
        range: SourceRange,
        text: String,
    },
    LineBreak {
        hard: bool,
    },
    NavigateTable {
        backward: bool,
        next_row: bool,
    },
    IndentList {
        outdent: bool,
    },
    RemoveCodeBoundary {
        forward: bool,
    },
    RemoveListBoundary {
        forward: bool,
    },
    JoinParagraphs {
        forward: bool,
    },
    InsertAdjacent {
        block: SourceRange,
        before: bool,
        template: InsertBlock,
        labels: TemplateLabels,
    },
    AppendLocalized {
        template: InsertBlock,
        labels: TemplateLabels,
    },
    Append {
        template: InsertBlock,
    },
}
// Templates are declarative data; no GPUI view crosses this boundary.
mod insert_block;
pub use insert_block::{InsertBlock, TemplateLabels};
pub enum CommandPlan {
    Edit {
        prepared: PreparedEdit,
        state: EditingState,
        handled: bool,
    },
    Select {
        base: DocumentKey,
        state: EditingState,
        handled: bool,
    },
    View {
        base: DocumentKey,
        state: EditingState,
        handled: bool,
    },
    Consume,
}
pub trait FeatureEngine: Send + Sync {
    /// Opt in only when these inputs fully describe presentation, excluding diagrams
    /// and highlighting (tracked by the host). Unknown extensions remain uncached.
    fn presentation_key(&self, _: &EditingState) -> Option<PresentationKey> {
        None
    }
    /// Opt in when controls depend only on source identity, source mode and
    /// the explicit local-source range. The host keys all three inputs.
    fn cache_source_controls(&self) -> bool {
        false
    }
    /// Per-contributor caching is optional; legacy engines keep their existing
    /// whole-batch opt-in. Unknown engines recompute by default.
    fn render_control_batch_cached(
        &self,
        state: &EditingState,
        cache: &mut RenderControlCache,
    ) -> Result<CachedRenderControlBatch, String> {
        cache.clear();
        self.render_control_batch(state)
            .map(|batch| CachedRenderControlBatch {
                controls: Arc::new(batch.controls),
                diagnostics: batch.diagnostics,
            })
    }
    /// Freeze presentation settings for a detached background export.
    fn snapshot(&self) -> Option<Arc<dyn FeatureEngine>> {
        None
    }
    fn plan(&self, intent: EditIntent, state: &EditingState) -> Result<CommandPlan, String>;
    fn display(
        &self,
        state: &EditingState,
        highlighter: Option<&mut dyn SyntaxHighlighter>,
        diagrams: &[DiagramViewInput],
    ) -> Display;
    fn block_window(&self, state: &EditingState) -> Option<Range<usize>>;
    fn protects_table_boundary(&self, state: &EditingState, forward: bool) -> bool;
    fn preview_inputs(&self, state: &EditingState) -> Vec<PreviewInput>;
    fn preview_batch(&self, state: &EditingState) -> PreviewBatch {
        PreviewBatch {
            inputs: self.preview_inputs(state),
            diagnostics: Vec::new(),
        }
    }
    /// Session-owned candidate cache; unknown engines retain uncached behavior.
    fn preview_batch_cached(&self, state: &EditingState, cache: &mut PreviewCache) -> PreviewBatch {
        cache.clear();
        self.preview_batch(state)
    }
    fn block_ui(&self, block: &ryne_syntax::block::Block, source_open: bool) -> AdornmentSpec;
    fn insert_templates(&self) -> &[(&'static str, InsertBlock)];
    fn render_controls(&self, _: &EditingState) -> Result<Vec<RenderControl>, String> {
        Ok(Vec::new())
    }
    fn render_control_batch(&self, state: &EditingState) -> Result<RenderControlBatch, String> {
        self.render_controls(state)
            .map(|controls| RenderControlBatch {
                controls,
                diagnostics: Vec::new(),
            })
    }
    fn plan_render_action(
        &self,
        _: &EditingState,
        _: &RenderActionRequest,
    ) -> Result<CommandPlan, String> {
        Err("render actions are not registered".into())
    }
}
mod math_font;
pub use math_font::MathFont;
pub trait FormulaRenderer: Send + Sync {
    /// Stable until any setting or availability affecting rendered output changes.
    fn presentation_generation(&self) -> Option<u64> {
        None
    }
    fn font(&self) -> MathFont {
        MathFont::default()
    }
    fn set_font(&self, _: MathFont) {}
    fn snapshot(&self) -> Option<Arc<dyn FormulaRenderer>> {
        None
    }
    /// Resolve a content-addressed asset independently of the current preference.
    fn render_asset(&self, path: &str) -> Option<Arc<Formula>> {
        self.render(path.strip_prefix("math:")?)
    }
    fn render(&self, latex: &str) -> Option<Arc<Formula>>;
}
pub struct HighlightSpan {
    pub range: Range<usize>,
    pub class: Class,
}
pub trait SyntaxHighlighter {
    /// None also requests another pass when a frame deferred work under its budget.
    fn presentation_generation(&self) -> Option<u64> {
        None
    }
    /// Explicitly request a later host pass. Unknown/non-cacheable services do
    /// not implicitly schedule work just because their generation is None.
    fn has_pending_work(&self) -> bool {
        false
    }
    fn begin_frame(&mut self);
    fn end_frame(&mut self);
    fn spans(&mut self, source: &str, block: &CodeBlock) -> Vec<HighlightSpan>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationKey {
    pub document: DocumentKey,
    pub source_mode: bool,
    pub inline: Option<Range<usize>>,
    pub heading: Option<Range<usize>>,
    pub block: Option<Range<usize>>,
    pub generation: u64,
    pub selection: Option<(usize, usize)>,
    pub closers: Option<Vec<Range<usize>>>,
}
/// An explicit promise that all undeclared inputs cannot affect this contribution.
/// Source identity, source mode and local-source filtering are always keyed.
#[derive(Clone, Copy, Debug, Default)]
pub struct CacheDependencies {
    pub selection: bool,
    pub visibility: bool,
    pub closers: bool,
    /// Must change whenever configuration/resources affecting output change.
    pub generation: u64,
}
impl PresentationKey {
    pub fn for_state(state: &EditingState, dependencies: CacheDependencies) -> Self {
        Self {
            document: state.source.id(),
            source_mode: state.source_mode,
            block: state.block_source.as_ref().map(|s| s.range.clone()),
            inline: dependencies
                .visibility
                .then(|| state.reveal.as_ref().map(|s| s.range.clone()))
                .flatten(),
            heading: dependencies
                .visibility
                .then(|| state.heading_reveal.as_ref().map(|s| s.range.clone()))
                .flatten(),
            selection: dependencies.selection.then_some((state.anchor, state.head)),
            closers: dependencies.closers.then(|| state.closers.clone()),
            generation: dependencies.generation,
        }
    }
}
/// Conservative language inference over literal code, without document ownership.
pub trait CodeLanguageDetector: Send + Sync {
    fn detect(&self, code: &str) -> Option<&'static str>;
}
#[derive(Clone)]
pub struct CancelControl {
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
    callbacks: Arc<Mutex<Vec<Weak<CancelCallback>>>>,
}
type CancelCallback = Mutex<Option<Box<dyn FnOnce() + Send>>>;

/// Keeps a backend cancellation relay registered until the operation finishes.
pub struct CancellationRegistration {
    _callback: Arc<CancelCallback>,
}

fn invoke_cancellation(callback: &CancelCallback) {
    let callback = callback.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(callback) = callback {
        // Cancellation may originate in a native event callback or Drop.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(callback));
    }
}
impl Default for CancelControl {
    fn default() -> Self {
        Self::with_timeout(Duration::from_secs(2))
    }
}
impl CancelControl {
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline: Instant::now() + timeout,
            callbacks: Default::default(),
        }
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        let callbacks = {
            let mut callbacks = self.callbacks.lock().unwrap_or_else(|e| e.into_inner());
            callbacks
                .drain(..)
                .filter_map(|c| c.upgrade())
                .collect::<Vec<_>>()
        };
        for callback in callbacks {
            invoke_cancellation(&callback);
        }
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    /// Registers an at-most-once relay, including cancellation before registration.
    /// Dropping the registration stops retaining the relay; panics are contained.
    pub fn on_cancel(&self, callback: impl FnOnce() + Send + 'static) -> CancellationRegistration {
        let callback: Arc<CancelCallback> = Arc::new(Mutex::new(Some(Box::new(callback))));
        {
            let mut callbacks = self.callbacks.lock().unwrap_or_else(|e| e.into_inner());
            callbacks.retain(|c| c.strong_count() > 0);
            callbacks.push(Arc::downgrade(&callback));
        }
        if self.is_cancelled() {
            invoke_cancellation(&callback);
        }
        CancellationRegistration {
            _callback: callback,
        }
    }
    pub fn remaining(&self) -> Result<Duration, &'static str> {
        self.checkpoint()?;
        self.deadline
            .checked_duration_since(Instant::now())
            .ok_or("图表处理超时，保留源码")
    }
    pub fn checkpoint(&self) -> Result<(), &'static str> {
        if self.is_cancelled() {
            Err("图表请求已取消")
        } else if Instant::now() > self.deadline {
            Err("图表处理超时，保留源码")
        } else {
            Ok(())
        }
    }
}
pub trait DiagramRenderer: Send + Sync {
    /// Return a renderer with frozen export settings, or None if unavailable.
    fn snapshot(&self) -> Option<Arc<dyn DiagramRenderer>> {
        None
    }
    fn render(&self, source: &str, control: CancelControl) -> DiagramOutcome;
    fn generation(&self) -> u64;
}
/// Validate renderer output consistently for interactive and detached consumers.
pub fn validate_diagram(
    resource: Arc<ryne_projection::Diagram>,
) -> ryne_projection::DiagramOutcome {
    let pixels = u64::from(resource.pixel_width) * u64::from(resource.pixel_height);
    if pixels == 0
        || pixels > 2_000_000
        || pixels * 4 != resource.pixels.len() as u64
        || resource.bytes != resource.pixels.len()
        || ![resource.width, resource.height]
            .iter()
            .all(|v| v.is_finite() && *v > 0. && *v <= 4096.)
    {
        Err("图表资源无效，保留源码".into())
    } else {
        Ok(resource)
    }
}
pub trait FileStore: Send + Sync {
    fn read(&self, path: &Path) -> std::io::Result<String>;
    fn write(&self, path: &Path, source: &Source) -> std::io::Result<()>;
}
pub trait Validator: Send + Sync {
    fn validate(&self, before: &EditingState, plan: &CommandPlan) -> Result<(), String>;
}
pub trait Observer: Send + Sync {
    fn id(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
    fn observe(&self, event: &SessionEvent, state: &EditingState);
}
#[derive(Clone, Debug)]
pub enum EventKind {
    DocumentChanged,
    SelectionChanged,
    ViewChanged,
    ResourcesUpdated,
    SaveCompleted,
    SaveFailed,
    FeatureFailed,
}
#[derive(Clone, Debug)]
pub struct SessionEvent {
    pub document: DocumentKey,
    pub sequence: u64,
    pub origin: &'static str,
    pub kind: EventKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub feature: String,
    pub message: String,
}
impl From<Error> for Diagnostic {
    fn from(e: Error) -> Self {
        Self {
            feature: "host".into(),
            message: format!("{e:?}"),
        }
    }
}
mod execution;
pub use execution::*;
pub mod registry;
pub use registry::*;
mod render_actions;
pub use render_actions::*;

#[derive(Clone)]
pub struct PreviewInput {
    pub feature: &'static str,
    pub block: CodeBlock,
    pub input: Result<String, String>,
    pub renderer: Arc<dyn DiagramRenderer>,
}
#[derive(Default)]
pub struct PreviewBatch {
    pub inputs: Vec<PreviewInput>,
    pub diagnostics: Vec<Diagnostic>,
}
mod preview;
pub use preview::PreviewCache;
mod host;
pub use host::{catch_callback, DiagnosticLog, GuardedFeatureEngine};
#[cfg(test)]
mod cancellation_tests;
#[cfg(test)]
mod registry_tests;

pub mod input;
pub use input::{InputDraft, InputOrigin};

/// Desktop operations consume immutable values, independently of document saves.
pub trait PdfExporter: Send + Sync {
    fn export(&self, display: &Display) -> Result<Vec<u8>, String>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateStatus {
    Current,
    NoRelease,
    Available { version: String, url: String },
}
pub trait UpdateChecker: Send + Sync {
    fn check(&self, current: &str) -> Result<UpdateStatus, String>;
}
#[derive(Clone)]
pub struct DesktopServices {
    pub exports: ExportRegistry,
    pub artifacts: Arc<dyn ArtifactStore>,
    pub updates: Arc<dyn UpdateChecker>,
}

mod block_metadata;

mod projection;
pub use projection::{project_checked, ProjectionBatch};

mod export;
pub use export::{ArtifactStore, ExportDescriptor, ExportRegistry};
