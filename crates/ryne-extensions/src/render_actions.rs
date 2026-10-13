//! Render-mode controls and executable commands, independent of GPUI.
use crate::{CommandPlan, Diagnostic, EditingState};
use ryne_core::{DocumentKey, SourceRange};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderTarget {
    pub document: DocumentKey,
    pub role: &'static str,
    pub owner: Range<usize>,
    pub part: Vec<usize>,
}
impl RenderTarget {
    pub fn validate(&self, state: &EditingState) -> Result<(), String> {
        SourceRange::new(self.document, self.owner.clone())
            .validate(&state.source)
            .map_err(|e| format!("{e:?}"))?;
        if state.parse.source().id() != state.source.id() {
            return Err("stale semantics".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct RenderAction {
    pub command: &'static str,
    pub label: &'static str,
    pub fallback: &'static str,
    pub icon: Option<&'static str>,
    pub enabled: bool,
    pub checked: bool,
    pub group: u8,
    pub submenu: Option<RenderSubmenu>,
    pub shortcut: Option<RenderShortcut>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderSubmenu {
    pub label: &'static str,
    pub fallback: &'static str,
}
#[derive(Clone, Copy, Debug)]
pub struct RenderShortcut {
    pub key: &'static str,
    pub control: bool,
    pub platform: bool,
    pub alt: bool,
    pub shift: bool,
    pub hint: &'static str,
}
#[derive(Clone, Debug)]
pub struct RenderControl {
    pub provider: &'static str,
    pub id: String,
    pub target: RenderTarget,
    /// Source body used to determine whether the editor caret focuses this region.
    pub focus: Range<usize>,
    pub placement: RenderControlPlacement,
    /// Always reserve this logical width, including when the trigger is hidden.
    pub trailing_space: f32,
    pub actions: Vec<RenderAction>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderControlPlacement {
    Leading,
    Trailing,
    /// An always-visible icon in the structural marker slot, invoking its sole action.
    Marker {
        icon: &'static str,
    },
}
#[derive(Clone, Debug)]
pub struct RenderActionRequest {
    pub provider: &'static str,
    pub target: RenderTarget,
    pub command: &'static str,
}
pub trait RenderActionProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn commands(&self) -> &'static [&'static str];
    fn controls(&self, state: &EditingState) -> Result<Vec<RenderControl>, String>;
    fn plan(
        &self,
        state: &EditingState,
        request: &RenderActionRequest,
    ) -> Result<CommandPlan, String>;
}
#[derive(Default)]
pub struct RenderControlBatch {
    pub controls: Vec<RenderControl>,
    pub diagnostics: Vec<Diagnostic>,
}
#[derive(Default, Clone)]
pub struct RenderActionRegistry {
    providers: Vec<Arc<dyn RenderActionProvider>>,
    commands: BTreeMap<&'static str, &'static str>,
}
impl RenderActionRegistry {
    pub fn new(providers: Vec<Arc<dyn RenderActionProvider>>) -> Result<Self, String> {
        let mut commands = BTreeMap::new();
        let mut ids = BTreeSet::new();
        for provider in &providers {
            if !ids.insert(provider.id()) {
                return Err("duplicate render provider".into());
            }
            for &command in provider.commands() {
                if !command.starts_with(&format!("{}.", provider.id()))
                    || commands.insert(command, provider.id()).is_some()
                {
                    return Err("invalid or duplicate render command".into());
                }
            }
        }
        Ok(Self {
            providers,
            commands,
        })
    }
    pub fn with_provider(&self, provider: Arc<dyn RenderActionProvider>) -> Result<Self, String> {
        let mut providers = self.providers.clone();
        providers.push(provider);
        Self::new(providers)
    }
    pub fn controls(&self, state: &EditingState) -> Result<Vec<RenderControl>, String> {
        Ok(self.control_batch(state).controls)
    }
    /// Publish each provider atomically. One malformed or panicking provider
    /// cannot remove other providers' controls or their reserved layout space.
    pub fn control_batch(&self, state: &EditingState) -> RenderControlBatch {
        let mut batch = RenderControlBatch::default();
        if state.source_mode {
            return batch;
        }
        for provider in &self.providers {
            match self.provider_controls(provider.as_ref(), state) {
                Ok(controls) => batch.controls.extend(controls),
                Err(message) => batch.diagnostics.push(Diagnostic {
                    feature: provider.id().into(),
                    message,
                }),
            }
        }
        batch
    }
    fn provider_controls(
        &self,
        provider: &dyn RenderActionProvider,
        state: &EditingState,
    ) -> Result<Vec<RenderControl>, String> {
        if state.source_mode {
            return Ok(Vec::new());
        }
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut controls = provider.controls(state)?;
            let mut ids = BTreeSet::new();
            for control in &controls {
                control.target.validate(state)?;
                if control.provider != provider.id()
                    || !ids.insert(control.id.clone())
                    || control.actions.is_empty()
                    || (matches!(control.placement, RenderControlPlacement::Marker { .. })
                        && control.actions.len() != 1)
                    || !control.trailing_space.is_finite()
                    || control.trailing_space < 0.
                    || control.focus.start < control.target.owner.start
                    || control.focus.end > control.target.owner.end
                    || state.source.text().get(control.focus.clone()).is_none()
                    || control
                        .actions
                        .iter()
                        .any(|a| self.commands.get(a.command) != Some(&provider.id()))
                {
                    return Err("invalid render control".into());
                }
            }
            controls.retain(|control| {
                !state.block_source.as_ref().is_some_and(|s| {
                    s.range.start < control.target.owner.end
                        && control.target.owner.start < s.range.end
                })
            });
            Ok(controls)
        }))
        .unwrap_or_else(|_| Err("render control provider panicked".into()))
    }
    pub fn plan(
        &self,
        state: &EditingState,
        request: &RenderActionRequest,
    ) -> Result<CommandPlan, String> {
        request.target.validate(state)?;
        if self.commands.get(request.command) != Some(&request.provider) {
            return Err("unknown render command".into());
        }
        let provider = self
            .providers
            .iter()
            .find(|p| p.id() == request.provider)
            .ok_or("missing render provider")?;
        // Re-evaluate only the requested provider; an unrelated failure cannot
        // prevent a valid command. Never trust a cached enabled flag.
        if !self
            .provider_controls(provider.as_ref(), state)?
            .iter()
            .any(|c| {
                c.target == request.target
                    && c.actions
                        .iter()
                        .any(|a| a.command == request.command && a.enabled)
            })
        {
            return Err("render action unavailable".into());
        }
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            provider.plan(state, request)
        }))
        .unwrap_or_else(|_| Err("render action provider panicked".into()))
    }
}
