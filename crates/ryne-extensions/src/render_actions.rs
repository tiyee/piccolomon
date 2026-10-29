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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderControlPlacement {
    /// Always-visible text trigger in a panel header; does not reserve body width.
    Header {
        label: String,
    },
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
    /// Opt in only when these dependencies fully describe controls and actions.
    /// The host also keys source mode and applies local-source filtering.
    fn cache_dependencies(&self) -> Option<crate::CacheDependencies> {
        None
    }
    /// Read once at registration; the registry owns this stable identity thereafter.
    fn id(&self) -> &'static str;
    /// Read once at registration and validated before the registry is changed.
    fn commands(&self) -> &'static [&'static str];
    fn controls(&self, state: &EditingState) -> Result<Vec<RenderControl>, String>;
    /// Return current controls for this target, or None to use full-batch validation.
    /// The host validates every returned control and rechecks the requested action.
    fn controls_for_target(
        &self,
        _: &EditingState,
        _: &RenderTarget,
    ) -> Result<Option<Vec<RenderControl>>, String> {
        Ok(None)
    }

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
/// Session-owned cache: at most one validated result for each current provider.
#[derive(Default)]
pub struct RenderControlCache {
    entries: BTreeMap<&'static str, ControlEntry>,
    aggregate: Option<ControlAggregate>,
}
struct ControlEntry {
    provider: Arc<dyn RenderActionProvider>,
    key: crate::PresentationKey,
    controls: Arc<Vec<RenderControl>>,
}
struct ControlAggregate {
    parts: Vec<Arc<Vec<RenderControl>>>,
    controls: Arc<Vec<RenderControl>>,
}
pub struct CachedRenderControlBatch {
    pub controls: Arc<Vec<RenderControl>>,
    pub diagnostics: Vec<Diagnostic>,
}
impl RenderControlCache {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.aggregate = None;
    }
    fn aggregate(&mut self, parts: Vec<Arc<Vec<RenderControl>>>) -> Arc<Vec<RenderControl>> {
        if let Some(old) = &self.aggregate
            && old.parts.len() == parts.len()
            && old.parts.iter().zip(&parts).all(|(a, b)| Arc::ptr_eq(a, b))
        {
            return old.controls.clone();
        }
        let controls = Arc::new(
            parts
                .iter()
                .flat_map(|p| p.iter().cloned())
                .collect::<Vec<_>>(),
        );
        self.aggregate = Some(ControlAggregate {
            parts,
            controls: controls.clone(),
        });
        controls
    }
}
#[derive(Clone)]
struct RegisteredProvider {
    id: &'static str,
    instance: Arc<dyn RenderActionProvider>,
}
#[derive(Default, Clone)]
pub struct RenderActionRegistry {
    providers: Vec<RegisteredProvider>,
    commands: BTreeMap<&'static str, &'static str>,
}
impl RenderActionRegistry {
    pub fn new(providers: Vec<Arc<dyn RenderActionProvider>>) -> Result<Self, String> {
        let mut registry = Self::default();
        for provider in providers {
            registry.register(provider)?;
        }
        Ok(registry)
    }
    fn register(&mut self, provider: Arc<dyn RenderActionProvider>) -> Result<(), String> {
        // Identity and the command directory are startup metadata. Never call
        // them again during generation, diagnostics, cache lookup or execution.
        let (id, declared) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (provider.id(), provider.commands().to_vec())
        }))
        .map_err(|_| "render provider registration panicked".to_string())?;
        if self.providers.iter().any(|p| p.id == id) {
            return Err("duplicate render provider".into());
        }
        let mut unique = BTreeSet::new();
        for &command in &declared {
            if !command.starts_with(&format!("{id}."))
                || !unique.insert(command)
                || self.commands.contains_key(command)
            {
                return Err("invalid or duplicate render command".into());
            }
        }
        for command in declared {
            self.commands.insert(command, id);
        }
        self.providers.push(RegisteredProvider {
            id,
            instance: provider,
        });
        Ok(())
    }
    pub fn with_provider(&self, provider: Arc<dyn RenderActionProvider>) -> Result<Self, String> {
        let mut registry = self.clone();
        registry.register(provider)?;
        Ok(registry)
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
            match self.provider_controls(provider, state) {
                Ok(controls) => batch.controls.extend(controls),
                Err(message) => batch.diagnostics.push(Diagnostic {
                    feature: provider.id.into(),
                    message,
                }),
            }
        }
        batch
    }
    /// Unknown/failing contributions recompute independently of healthy entries.
    pub fn cached_control_batch(
        &self,
        state: &EditingState,
        cache: &mut RenderControlCache,
    ) -> CachedRenderControlBatch {
        if state.parse.source().id() != state.source.id() {
            cache.clear();
            return CachedRenderControlBatch {
                controls: Arc::new(Vec::new()),
                diagnostics: vec![Diagnostic {
                    feature: "render-controls".into(),
                    message: "stale semantics".into(),
                }],
            };
        }
        let current = self
            .providers
            .iter()
            .map(|p| (p.id, p))
            .collect::<BTreeMap<_, _>>();
        cache.entries.retain(|id, entry| {
            current
                .get(id)
                .is_some_and(|p| Arc::ptr_eq(&p.instance, &entry.provider))
        });
        let mut diagnostics = Vec::new();
        let mut parts = Vec::new();
        if !state.source_mode {
            for provider in &self.providers {
                let mut key = || match crate::catch_callback(
                    provider.id,
                    "control cache dependencies",
                    || {
                        provider
                            .instance
                            .cache_dependencies()
                            .map(|deps| crate::PresentationKey::for_state(state, deps))
                    },
                ) {
                    Ok(key) => key,
                    Err(error) => {
                        diagnostics.push(error);
                        None
                    }
                };
                let before = key();
                if let Some(before) = &before
                    && let Some(entry) = cache.entries.get(provider.id)
                    && entry.key == *before
                {
                    parts.push(entry.controls.clone());
                    continue;
                }
                cache.entries.remove(provider.id);
                match self.provider_controls(provider, state) {
                    Ok(controls) => {
                        let controls = Arc::new(controls);
                        if let Some(before) = before
                            && key().as_ref() == Some(&before)
                        {
                            cache.entries.insert(
                                provider.id,
                                ControlEntry {
                                    provider: provider.instance.clone(),
                                    key: before,
                                    controls: controls.clone(),
                                },
                            );
                        }
                        parts.push(controls);
                    }
                    Err(message) => diagnostics.push(Diagnostic {
                        feature: provider.id.into(),
                        message,
                    }),
                }
            }
        } else {
            cache.entries.clear();
        }
        CachedRenderControlBatch {
            controls: cache.aggregate(parts),
            diagnostics,
        }
    }
    fn provider_controls(
        &self,
        provider: &RegisteredProvider,
        state: &EditingState,
    ) -> Result<Vec<RenderControl>, String> {
        self.provider_controls_for_target(provider, state, None)
    }
    fn provider_controls_for_target(
        &self,
        provider: &RegisteredProvider,
        state: &EditingState,
        target: Option<&RenderTarget>,
    ) -> Result<Vec<RenderControl>, String> {
        if state.source_mode {
            return Ok(Vec::new());
        }
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut controls = match target {
                Some(target) => match provider.instance.controls_for_target(state, target)? {
                    Some(controls) => controls,
                    None => provider.instance.controls(state)?,
                },
                None => provider.instance.controls(state)?,
            };
            let mut ids = BTreeSet::new();
            for control in &controls {
                control.target.validate(state)?;
                if control.provider != provider.id
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
                        .any(|a| self.commands.get(a.command) != Some(&provider.id))
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
            .find(|p| p.id == request.provider)
            .ok_or("missing render provider")?;
        // Re-evaluate only the requested provider; an unrelated failure cannot
        // prevent a valid command. Never trust a cached enabled flag.
        if !self
            .provider_controls_for_target(provider, state, Some(&request.target))?
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
            provider.instance.plan(state, request)
        }))
        .unwrap_or_else(|_| Err("render action provider panicked".into()))
    }
}

#[cfg(test)]
#[path = "render_cache_tests.rs"]
mod cache_tests;
