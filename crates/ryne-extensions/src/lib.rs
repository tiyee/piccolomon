//! Typed, read-only feature and service contracts for static built-ins.
use editable_markdown_parser::{Error, ParseSnapshot, Session, Source};
use ryne_core::{DocumentKey, PreparedEdit, SourceRange};
use ryne_projection::{Class, DiagramOutcome, DiagramViewInput, Display, Formula};
use ryne_syntax::code_block::CodeBlock;
use std::{
    ops::Range,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
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
pub enum EditIntent {
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
    fn block_ui(&self, block: &ryne_syntax::block::Block, source_open: bool) -> AdornmentSpec;
    fn insert_templates(&self) -> &[(&'static str, InsertBlock)];
    fn render_controls(&self, _: &EditingState) -> Result<Vec<RenderControl>, String> {
        Ok(Vec::new())
    }
    fn plan_render_action(
        &self,
        _: &EditingState,
        _: &RenderActionRequest,
    ) -> Result<CommandPlan, String> {
        Err("render actions are not registered".into())
    }
}
pub trait FormulaRenderer: Send + Sync {
    fn render(&self, latex: &str) -> Option<Arc<Formula>>;
}
pub struct HighlightSpan {
    pub range: Range<usize>,
    pub class: Class,
}
pub trait SyntaxHighlighter {
    fn begin_frame(&mut self);
    fn end_frame(&mut self);
    fn spans(&mut self, source: &str, block: &CodeBlock) -> Vec<HighlightSpan>;
}
/// Conservative language inference over literal code, without document ownership.
pub trait CodeLanguageDetector: Send + Sync {
    fn detect(&self, code: &str) -> Option<&'static str>;
}
#[derive(Clone)]
pub struct CancelControl {
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
}
impl Default for CancelControl {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline: Instant::now() + Duration::from_secs(2),
        }
    }
}
impl CancelControl {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed)
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
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
    fn render(&self, source: &str, control: CancelControl) -> DiagramOutcome;
    fn generation(&self) -> u64;
}
pub trait FileStore: Send + Sync {
    fn read(&self, path: &Path) -> std::io::Result<String>;
    fn write(&self, path: &Path, source: &Source) -> std::io::Result<()>;
}
pub trait Validator: Send + Sync {
    fn validate(&self, before: &EditingState, plan: &CommandPlan) -> Result<(), String>;
}
pub trait Observer: Send + Sync {
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
#[derive(Debug)]
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
pub mod registry;
pub use registry::*;
mod render_actions;
pub use render_actions::*;

pub struct PreviewInput {
    pub feature: &'static str,
    pub block: CodeBlock,
    pub input: Result<String, String>,
    pub renderer: Arc<dyn DiagramRenderer>,
}
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
    pub pdf: Arc<dyn PdfExporter>,
    pub pdf_v2: Arc<dyn PdfExporter>,
    pub updates: Arc<dyn UpdateChecker>,
}
