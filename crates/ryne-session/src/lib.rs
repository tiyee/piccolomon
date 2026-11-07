//! The only publisher of document content, session state and post-commit facts.
use ryne_core::DocumentCore;
use ryne_extensions::*;
use ryne_parser::Source;
use ryne_projection::{DiagramViewInput, Display};
#[cfg(test)]
mod image_tests;
use ryne_syntax::ParseSnapshot;
use std::{
    ops::{Deref, Range},
    path::Path,
    sync::Arc,
};
pub mod basic;
pub mod composition;
pub use composition::Composition;
mod export;
mod presentation_cache;
pub use presentation_cache::{CachedControls, CachedProjection};
pub mod jobs;
pub use export::ExportSnapshot;

#[derive(Clone)]
struct Checkpoint {
    anchor: usize,
    head: usize,
    closers: Vec<Range<usize>>,
}
impl From<&EditingState> for Checkpoint {
    fn from(s: &EditingState) -> Self {
        Self {
            anchor: s.anchor,
            head: s.head,
            closers: s.closers.clone(),
        }
    }
}
#[derive(Clone)]
pub struct SessionServices {
    pub features: Arc<dyn FeatureEngine>,
    pub files: Arc<dyn FileStore>,
}
pub mod files;
pub use files::{FileSession, SaveCompletion, SaveIntent, SaveReceipt, SaveRequest};
pub struct EditorSession {
    presentation_cache: std::cell::RefCell<presentation_cache::PresentationCache>,
    core: DocumentCore<Checkpoint>,
    composition: Option<Composition>,
    state: EditingState,
    services: SessionServices,
    features: Arc<dyn FeatureEngine>,
    pub files: FileSession,
    pub jobs: jobs::JobHost,
    validators: Vec<Arc<dyn Validator>>,
    observers: Vec<RegisteredObserver>,
    sequence: u64,
    pub diagnostics: DiagnosticLog,
}
struct RegisteredObserver {
    id: String,
    observer: Arc<dyn Observer>,
    enabled: bool,
}
// Read-only snapshot access. Only host commands and explicit view operations can
// mutate session state; the GUI cannot replace source or parse fields.
impl Deref for EditorSession {
    type Target = EditingState;
    fn deref(&self) -> &EditingState {
        &self.state
    }
}
impl EditorSession {
    pub fn new(text: &str, services: SessionServices) -> Self {
        Self::from_core(DocumentCore::new(text), services)
    }
    pub fn recovered(text: &str, services: SessionServices) -> Self {
        Self::from_core(DocumentCore::recovered(text), services)
    }
    fn from_core(core: DocumentCore<Checkpoint>, services: SessionServices) -> Self {
        let state = EditingState::new(core.source().clone());
        let diagnostics = DiagnosticLog::default();
        let features = Arc::new(GuardedFeatureEngine::new(
            services.features.clone(),
            diagnostics.clone(),
        ));
        let jobs = jobs::JobHost::new(diagnostics.clone());
        Self {
            presentation_cache: Default::default(),
            core,
            composition: None,
            state,
            services,
            features,
            files: FileSession::default(),
            jobs,
            validators: Vec::new(),
            observers: Vec::new(),
            sequence: 0,
            diagnostics,
        }
    }
    pub fn set_source_mode(&mut self, mode: bool) {
        self.state.source_mode = mode;
        self.notify(EventKind::ViewChanged, "view");
    }
    pub fn clear_heading_reveal(&mut self) {
        self.state.heading_reveal = None;
    }
    pub fn clear_inline_reveal(&mut self) {
        self.state.reveal = None;
    }
    pub fn set_block_source(&mut self, range: Option<Range<usize>>) {
        self.state.block_source =
            range.and_then(|r| ryne_parser::Session::new(&self.state.source, r).ok());
    }
    pub fn services(&self) -> SessionServices {
        self.services.clone()
    }
    pub fn fork(&self, text: &str) -> Self {
        Self::new(text, self.services.clone())
    }
    pub fn open_with(&self, path: &Path) -> std::io::Result<Self> {
        Self::open(path, self.services.clone())
    }
    pub fn open(path: &Path, services: SessionServices) -> std::io::Result<Self> {
        let target = services.files.resolve_path(path)?;
        let text = services.files.read(&target)?;
        if services.files.resolve_path(path)? != target {
            return Err(std::io::Error::other("file target changed during open"));
        }
        let mut doc = Self::new(&text, services);
        doc.files.path = Some(path.into());
        doc.files.binding = Some((path.into(), target));
        Ok(doc)
    }
    pub fn dirty(&self) -> bool {
        self.core.dirty()
    }
    pub fn history_len(&self, redo: bool) -> usize {
        self.core.history_len(redo)
    }
    /// Diagnostic query only; no history policy or document state changes.
    pub fn history_memory(&self) -> ryne_core::HistoryMemory {
        self.core.history_memory(|checkpoint| {
            checkpoint.closers.capacity() * std::mem::size_of::<Range<usize>>()
        })
    }
    pub fn add_validator(&mut self, v: Arc<dyn Validator>) {
        self.validators.push(v)
    }
    pub fn add_observer(&mut self, o: Arc<dyn Observer>) {
        let name = catch_callback("observer", "identity", || o.id());
        let name = match name {
            Ok(name) => name,
            Err(diagnostic) => {
                self.diagnostics.record(diagnostic);
                return;
            }
        };
        let mut name_end = name.len().min(192);
        while !name.is_char_boundary(name_end) {
            name_end -= 1;
        }
        self.observers.push(RegisteredObserver {
            id: format!("observer:{}:{}", self.observers.len(), &name[..name_end]),
            observer: o,
            enabled: true,
        });
    }
    fn notify(&mut self, kind: EventKind, origin: &'static str) {
        self.sequence += 1;
        let event = SessionEvent {
            document: self.core.source().id(),
            sequence: self.sequence,
            origin,
            kind,
        };
        for registration in &mut self.observers {
            if registration.enabled
                && let Err(diagnostic) = catch_callback(&registration.id, "observe", || {
                    registration.observer.observe(&event, &self.state)
                })
            {
                registration.enabled = false;
                self.diagnostics.record(diagnostic);
            }
        }
    }
    pub fn dispatch(&mut self, intent: EditIntent) -> Result<bool, String> {
        if self.composition.is_some()
            && matches!(
                intent,
                EditIntent::InsertAdjacent { .. } | EditIntent::FormatSource
            )
        {
            return Err("structural edits are unavailable during composition".into());
        }
        if self.state.source.id() != self.core.source().id()
            || self.state.parse.source().id() != self.core.source().id()
        {
            return Err("stale session snapshot".into());
        }
        let plan = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.features.plan(intent, &self.state)
        }))
        .map_err(|_| "feature planning failed".to_string())??;
        self.publish_plan(plan)
    }
    pub fn publish_plan(&mut self, plan: CommandPlan) -> Result<bool, String> {
        self.publish_plan_inner(plan, false)
    }
    fn publish_plan_inner(
        &mut self,
        plan: CommandPlan,
        completing_ime: bool,
    ) -> Result<bool, String> {
        if self.composition.is_some() && !completing_ime && matches!(plan, CommandPlan::Edit { .. })
        {
            return Err("confirm or cancel composition before editing".into());
        }
        self.validate_plan(&plan, None)?;
        let (handled, kind) = match plan {
            CommandPlan::Edit {
                prepared,
                state,
                handled,
            } => {
                self.core
                    .publish(prepared, Checkpoint::from(&self.state))
                    .map_err(|e| format!("{e:?}"))?;
                self.state = state;
                (handled, EventKind::DocumentChanged)
            }
            CommandPlan::Select { state, handled, .. } => {
                self.state = state;
                (handled, EventKind::SelectionChanged)
            }
            CommandPlan::View { state, handled, .. } => {
                self.state = state;
                (handled, EventKind::ViewChanged)
            }
            CommandPlan::Consume => {
                if completing_ime {
                    self.composition = None;
                }
                return Ok(true);
            }
        };
        if completing_ime {
            self.composition = None;
        }
        self.notify(kind, "command");
        Ok(handled)
    }
    fn validate_plan(
        &self,
        plan: &CommandPlan,
        history: Option<HistoryDirection>,
    ) -> Result<(), String> {
        let (base, state) = match plan {
            CommandPlan::Edit {
                prepared, state, ..
            } => (prepared.base(), Some(state)),
            CommandPlan::Select { base, state, .. } | CommandPlan::View { base, state, .. } => {
                (*base, Some(state))
            }
            CommandPlan::Consume => (self.core.source().id(), None),
        };
        self.core
            .source()
            .check(base)
            .map_err(|e| format!("{e:?}"))?;
        if let Some(state) = state {
            let candidate = match plan {
                CommandPlan::Edit { prepared, .. } => &prepared.applied().source,
                _ => self.core.source(),
            };
            if state.source.id() != candidate.id()
                || state.parse.source().id() != candidate.id()
                || [state.anchor, state.head]
                    .into_iter()
                    .any(|p| p > candidate.text().len() || !candidate.text().is_char_boundary(p))
                || state
                    .closers
                    .iter()
                    .any(|r| r.is_empty() || candidate.text().get(r.clone()).is_none())
                || [&state.reveal, &state.heading_reveal, &state.block_source]
                    .into_iter()
                    .flatten()
                    .any(|session| session.visible(candidate).is_err())
            {
                return Err("invalid candidate state".into());
            }
        }
        let failures = self
            .validators
            .iter()
            .filter_map(|v| {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match history {
                    Some(direction) => v.validate_history(&self.state, plan, direction),
                    None => v.validate(&self.state, plan),
                }))
                .unwrap_or_else(|_| Err("validator failed".into()))
                .err()
            })
            .collect::<Vec<_>>();
        if !failures.is_empty() {
            return Err(failures.join("; "));
        }
        Ok(())
    }
    pub fn render_controls(&self) -> Result<Vec<RenderControl>, String> {
        if self.composition.is_some() {
            return Ok(Vec::new());
        }
        self.project_render_controls(&self.state)
    }
    /// Temporary preedit projections retain control space but never activate controls.
    pub fn project_render_controls(
        &self,
        state: &EditingState,
    ) -> Result<Vec<RenderControl>, String> {
        self.project_render_control_batch(state)
            .map(|batch| batch.controls)
    }
    pub fn project_render_control_batch(
        &self,
        state: &EditingState,
    ) -> Result<RenderControlBatch, String> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.features.render_control_batch(state)
        }))
        .map_err(|_| "render control provider failed".to_string())?
    }
    pub fn record_diagnostics(&mut self, diagnostics: Vec<Diagnostic>) {
        for diagnostic in diagnostics {
            self.diagnostics.record(diagnostic);
        }
    }
    pub fn execute_render_action(&mut self, request: &RenderActionRequest) -> Result<bool, String> {
        if self.composition.is_some() || self.state.source_mode {
            return Err("render actions unavailable".into());
        }
        request.target.validate(&self.state)?;
        let plan = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.features.plan_render_action(&self.state, request)
        }))
        .map_err(|_| "render action planning failed".to_string())??;
        self.publish_plan(plan)
    }
    pub fn normalize_heading_caret(&mut self) {
        let _ = self.dispatch(EditIntent::Normalize);
    }
    pub fn selection(&self) -> Range<usize> {
        self.state.selection()
    }
    pub fn select(&mut self, position: usize, extend: bool, frozen: bool) {
        let _ = self.dispatch(EditIntent::Select {
            position,
            extend,
            frozen,
        });
    }
    pub fn reveal_clicked_heading(&mut self) {
        let _ = self.dispatch(EditIntent::RevealHeading);
    }
    pub fn toggle_block_source(&mut self) -> bool {
        self.dispatch(EditIntent::ToggleBlockSource)
            .unwrap_or(false)
    }
    pub fn block_window(&self) -> Option<Range<usize>> {
        self.features.block_window(&self.state)
    }
    pub fn display(&self) -> Display {
        self.project(&self.state, None, &[])
    }
    pub fn highlighted_display(&self, highlighter: &mut dyn SyntaxHighlighter) -> Display {
        self.project(&self.state, Some(highlighter), &[])
    }
    pub fn project(
        &self,
        state: &EditingState,
        highlighter: Option<&mut dyn SyntaxHighlighter>,
        diagrams: &[DiagramViewInput],
    ) -> Display {
        self.project_batch(state, highlighter, diagrams).display
    }
    pub fn project_batch(
        &self,
        state: &EditingState,
        highlighter: Option<&mut dyn SyntaxHighlighter>,
        diagrams: &[DiagramViewInput],
    ) -> ProjectionBatch {
        let batch = project_checked(self.features.as_ref(), state, highlighter, diagrams);
        for diagnostic in &batch.diagnostics {
            self.diagnostics.record(diagnostic.clone());
        }
        batch
    }
    pub fn diagram_views(
        &mut self,
        frozen: bool,
        folded: &[Range<usize>],
    ) -> (Vec<DiagramViewInput>, Option<jobs::JobRequest>) {
        let inputs = self.cached_preview_inputs();
        self.jobs.refresh(&self.state, inputs, frozen, folded)
    }
    pub fn complete_job(&mut self, result: jobs::JobResult) {
        if self.jobs.complete(result, self.core.source().id()) {
            self.notify(EventKind::ResourcesUpdated, "renderer");
        }
    }
    pub fn block_ui(&self, block: &ryne_syntax::block::Block, source_open: bool) -> AdornmentSpec {
        self.features.block_ui(block, source_open)
    }
    pub fn insert_templates(&self) -> &[(&'static str, InsertBlock)] {
        self.features.insert_templates()
    }
    pub fn invalidate_preview(&mut self, range: &Range<usize>, error: String) {
        for input in self.cached_preview_inputs() {
            if input.block.range == *range
                && let Ok(source) = input.input
            {
                self.jobs.invalidate(&source, error.clone());
            }
        }
    }
    pub fn preview_at_caret(&self) -> Option<Range<usize>> {
        self.cached_preview_inputs()
            .into_iter()
            .find(|input| input.block.range.contains(&self.head))
            .map(|input| input.block.range)
    }
    pub fn insert_text(&mut self, range: Range<usize>, text: &str) -> Result<(), String> {
        self.insert_text_from(range, text, InputOrigin::Keyboard)
    }
    pub fn insert_text_from(
        &mut self,
        range: Range<usize>,
        text: &str,
        origin: InputOrigin,
    ) -> Result<(), String> {
        self.dispatch(EditIntent::Insert {
            range: ryne_core::SourceRange::new(self.core.source().id(), range),
            text: text.into(),
            origin,
        })
        .map(|_| ())
    }
    pub fn replace(&mut self, range: Range<usize>, text: &str) -> Result<(), String> {
        self.dispatch(EditIntent::Replace {
            range: ryne_core::SourceRange::new(self.core.source().id(), range),
            text: text.into(),
        })
        .map(|_| ())
    }
    pub fn line_break(&mut self, hard: bool) -> Result<(), String> {
        self.dispatch(EditIntent::LineBreak { hard }).map(|_| ())
    }
    pub fn delete(&mut self, forward: bool) -> Result<bool, String> {
        self.dispatch(EditIntent::Delete { forward })
    }
    pub fn tab(&mut self, backward: bool) -> Result<bool, String> {
        self.dispatch(EditIntent::Tab { backward })
    }
    pub fn navigate_table(&mut self, backward: bool, next_row: bool) -> Result<bool, String> {
        self.dispatch(EditIntent::NavigateTable { backward, next_row })
    }
    pub fn indent_list(&mut self, outdent: bool) -> Result<bool, String> {
        self.dispatch(EditIntent::IndentList { outdent })
    }
    pub fn remove_code_boundary(&mut self, forward: bool) -> Result<bool, String> {
        self.dispatch(EditIntent::RemoveCodeBoundary { forward })
    }
    pub fn remove_list_boundary(&mut self, forward: bool) -> Result<bool, String> {
        self.dispatch(EditIntent::RemoveListBoundary { forward })
    }
    pub fn join_paragraphs(&mut self, forward: bool) -> Result<bool, String> {
        self.dispatch(EditIntent::JoinParagraphs { forward })
    }
    pub fn remove_table_boundary(&self, forward: bool) -> bool {
        self.features.protects_table_boundary(&self.state, forward)
    }
    pub fn append_block(&mut self, template: InsertBlock) -> Result<(), String> {
        self.dispatch(EditIntent::Append { template }).map(|_| ())
    }
    /// Compatibility entry point; failures remain observable through diagnostics.
    pub fn undo(&mut self, redo: bool) {
        if let Err(message) = self.try_undo(redo) {
            self.diagnostics.record(Diagnostic {
                feature: "history".into(),
                message,
            });
        }
    }
    pub fn try_undo(&mut self, redo: bool) -> Result<bool, String> {
        if self.composition.is_some() {
            return Err("confirm or cancel composition before restoring history".into());
        }
        let Some((prepared, checkpoint)) = self
            .core
            .prepare_history(redo)
            .map_err(|e| format!("{e:?}"))?
        else {
            return Ok(false);
        };
        let mut state = self.state.clone();
        state.block_source = state
            .block_source
            .take()
            .and_then(|mut s| s.transform(&prepared.applied().positions).ok().map(|_| s));
        state.source = prepared.applied().source.clone();
        state.parse = Arc::new(ParseSnapshot::with_math(state.source.clone()));
        state.anchor = checkpoint.anchor;
        state.head = checkpoint.head;
        state.closers = checkpoint.closers;
        state.heading_reveal = None;
        // Resolve the final visibility/selection checkpoint before either history
        // stack or the authoritative source changes. History never runs input rules.
        for intent in [
            EditIntent::Select {
                position: state.anchor,
                extend: false,
                frozen: false,
            },
            EditIntent::Select {
                position: state.head,
                extend: true,
                frozen: false,
            },
            EditIntent::RestoreHeading,
        ] {
            let planned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.features.plan(intent, &state)
            }));
            match planned {
                Ok(Ok(
                    CommandPlan::Select { state: next, .. } | CommandPlan::View { state: next, .. },
                )) if next.source.id() == state.source.id()
                    && next.parse.source().id() == state.source.id()
                    && [next.anchor, next.head]
                        .into_iter()
                        .all(|p| next.source.text().is_char_boundary(p)) =>
                {
                    state = next
                }
                Ok(Ok(CommandPlan::Consume)) => {}
                _ => return Err("history visibility planning failed".into()),
            }
        }
        let direction = if redo {
            HistoryDirection::Redo
        } else {
            HistoryDirection::Undo
        };
        let plan = CommandPlan::Edit {
            prepared,
            state,
            handled: true,
        };
        self.validate_plan(&plan, Some(direction))?;
        let CommandPlan::Edit {
            prepared, state, ..
        } = plan
        else {
            return Err("invalid history plan".into());
        };
        self.core
            .publish_history(prepared, redo, Checkpoint::from(&self.state))
            .map_err(|e| format!("{e:?}"))?;
        self.state = state;
        self.notify(
            EventKind::DocumentChanged,
            if redo { "redo" } else { "undo" },
        );
        Ok(true)
    }
    pub fn queue_save(&mut self, path: std::path::PathBuf) -> Option<SaveRequest> {
        self.queue_save_with_intent(path, SaveIntent::Save)
    }
    pub fn queue_save_with_intent(
        &mut self,
        path: std::path::PathBuf,
        intent: SaveIntent,
    ) -> Option<SaveRequest> {
        self.files.next += 1;
        let request = SaveRequest {
            intent,
            id: self.files.next,
            snapshot: self.core.source().clone(),
            target: self
                .files
                .binding
                .as_ref()
                .filter(|(display, _)| *display == path)
                .map(|(_, target)| target.clone()),
            path,
            store: self.services.files.clone(),
        };
        if self.files.active.is_some() {
            self.files.queued = Some(request);
            None
        } else {
            self.files.active = Some(request.active_write());
            self.files.saving = true;
            Some(request)
        }
    }
    pub fn complete_save(&mut self, receipt: SaveReceipt) -> SaveCompletion {
        if self.files.active.as_ref().map(|(id, _)| *id) != Some(receipt.id)
            || receipt.snapshot.id().document_id != self.core.source().id().document_id
        {
            return SaveCompletion {
                intent: receipt.intent,
                accepted: false,
                error: None,
                next: None,
                clean: !self.dirty(),
            };
        }
        self.files.active = None;
        let error = match receipt.result {
            Ok(target) => {
                self.finish_save(&receipt.path, &receipt.snapshot);
                self.files.binding = Some((receipt.path.clone(), target));
                None
            }
            Err(e) => {
                self.notify(EventKind::SaveFailed, "file");
                Some(e.to_string())
            }
        };
        let next = self.files.queued.take();
        self.files.active = next.as_ref().map(SaveRequest::active_write);
        self.files.saving = next.is_some();
        SaveCompletion {
            intent: receipt.intent,
            accepted: true,
            error,
            next,
            clean: !self.dirty(),
        }
    }
    pub fn save(&mut self, path: &Path) -> std::io::Result<()> {
        if self.files.saving {
            return Err(std::io::Error::other("save already in progress"));
        }
        let snapshot = self.core.source().clone();
        let expected = self
            .files
            .binding
            .as_ref()
            .filter(|(display, _)| display == path)
            .map(|(_, target)| target.as_path());
        let target = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            files::write_snapshot(self.services.files.as_ref(), path, expected, &snapshot)
        }))
        .unwrap_or_else(|_| Err(std::io::Error::other("file write callback panicked")))?;
        self.finish_save(path, &snapshot);
        self.files.binding = Some((path.into(), target));
        Ok(())
    }
    pub fn save_snapshot(&self) -> (Source, Arc<dyn FileStore>) {
        (self.core.source().clone(), self.services.files.clone())
    }
    pub fn finish_save(&mut self, path: &Path, snapshot: &Source) -> bool {
        if self.core.mark_saved(snapshot).is_err() {
            return false;
        }
        if self
            .files
            .binding
            .as_ref()
            .is_some_and(|(display, _)| display != path)
        {
            self.files.binding = None;
        }
        self.files.path = Some(path.into());
        self.notify(EventKind::SaveCompleted, "file");
        true
    }
}

#[cfg(test)]
mod quote_tests;
#[cfg(test)]
mod regression_tests;

#[cfg(test)]
mod code_render_action_tests;
#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod job_tests;
#[cfg(test)]
mod list_render_action_tests;
#[cfg(test)]
mod preview_cache_tests;
#[cfg(test)]
mod render_action_tests;

#[cfg(test)]
mod adjacent_block_tests;

#[cfg(test)]
mod format_tests;

#[cfg(test)]
mod architecture_tests;
#[cfg(test)]
mod first_batch_tests;
#[cfg(test)]
mod second_batch_tests;

#[cfg(test)]
mod third_batch_tests;

#[cfg(test)]
mod p1_tests;

#[cfg(test)]
mod file_binding_tests;
#[cfg(test)]
mod history_policy_tests;
