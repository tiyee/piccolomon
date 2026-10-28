use super::*;
use crate::*;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::SeqCst};

struct Provider {
    id: &'static str,
    dependencies: Option<CacheDependencies>,
    calls: AtomicUsize,
    generation: AtomicU64,
    failure: AtomicUsize,
    enabled: AtomicBool,
    mutate_generation: AtomicBool,
    empty: bool,
}
impl Provider {
    fn new(id: &'static str, dependencies: Option<CacheDependencies>) -> Arc<Self> {
        Arc::new(Self {
            id,
            dependencies,
            calls: AtomicUsize::new(0),
            generation: AtomicU64::new(0),
            failure: AtomicUsize::new(0),
            enabled: AtomicBool::new(true),
            mutate_generation: AtomicBool::new(false),
            empty: false,
        })
    }
}
impl RenderActionProvider for Provider {
    fn id(&self) -> &'static str {
        self.id
    }
    fn commands(&self) -> &'static [&'static str] {
        match self.id {
            "stable" => &["stable.run"],
            _ => &["dynamic.run"],
        }
    }
    fn cache_dependencies(&self) -> Option<CacheDependencies> {
        assert_ne!(self.failure.load(SeqCst), 3, "key failure");
        self.dependencies.map(|mut d| {
            d.generation = self.generation.load(SeqCst);
            d
        })
    }
    fn controls(&self, s: &EditingState) -> Result<Vec<RenderControl>, String> {
        self.calls.fetch_add(1, SeqCst);
        match self.failure.load(SeqCst) {
            1 => return Err("temporary failure".into()),
            2 => panic!("controls failure"),
            _ => (),
        }
        if self.mutate_generation.load(SeqCst) {
            self.generation.fetch_add(1, SeqCst);
        }
        if self.empty {
            return Ok(Vec::new());
        }
        let dependencies = self.dependencies.unwrap_or(CacheDependencies {
            selection: true,
            visibility: true,
            closers: true,
            generation: 0,
        });
        let input = PresentationKey::for_state(s, dependencies);
        Ok(vec![RenderControl {
            provider: self.id,
            id: format!("{:?}:{}", input, self.generation.load(SeqCst)),
            target: RenderTarget {
                document: s.source.id(),
                role: "test",
                owner: 0..s.source.text().len(),
                part: vec![],
            },
            focus: 0..s.source.text().len(),
            placement: RenderControlPlacement::Trailing,
            trailing_space: if self.failure.load(SeqCst) == 4 {
                f32::NAN
            } else {
                24.
            },
            actions: vec![RenderAction {
                command: self.commands()[0],
                label: "run",
                fallback: "Run",
                icon: None,
                enabled: self.enabled.load(SeqCst),
                checked: false,
                group: 0,
                submenu: None,
                shortcut: None,
            }],
        }])
    }
    fn plan(&self, _: &EditingState, _: &RenderActionRequest) -> Result<CommandPlan, String> {
        Ok(CommandPlan::Consume)
    }
}
fn signature(controls: &[RenderControl]) -> Vec<(&str, &str, bool)> {
    controls
        .iter()
        .map(|c| (c.provider, c.id.as_str(), c.actions[0].enabled))
        .collect()
}

struct IdentityCallbacks {
    inner: Arc<Provider>,
    unavailable: AtomicBool,
    ids: AtomicUsize,
    commands: AtomicUsize,
}
impl IdentityCallbacks {
    fn new(id: &'static str) -> Arc<Self> {
        Arc::new(Self {
            inner: Provider::new(id, Some(Default::default())),
            unavailable: AtomicBool::new(false),
            ids: AtomicUsize::new(0),
            commands: AtomicUsize::new(0),
        })
    }
}
impl RenderActionProvider for IdentityCallbacks {
    fn id(&self) -> &'static str {
        self.ids.fetch_add(1, SeqCst);
        assert!(!self.unavailable.load(SeqCst), "identity unavailable");
        self.inner.id
    }
    fn commands(&self) -> &'static [&'static str] {
        self.commands.fetch_add(1, SeqCst);
        assert!(!self.unavailable.load(SeqCst), "commands unavailable");
        self.inner.commands()
    }
    fn cache_dependencies(&self) -> Option<CacheDependencies> {
        self.inner.cache_dependencies()
    }
    fn controls(&self, state: &EditingState) -> Result<Vec<RenderControl>, String> {
        self.inner.controls(state)
    }
    fn plan(
        &self,
        state: &EditingState,
        request: &RenderActionRequest,
    ) -> Result<CommandPlan, String> {
        self.inner.plan(state, request)
    }
}

#[test]
fn registered_identity_is_never_requeried_by_cache_diagnostics_execution_or_extension() {
    let healthy = IdentityCallbacks::new("stable");
    let registry = RenderActionRegistry::new(vec![healthy.clone()]).unwrap();
    healthy.unavailable.store(true, SeqCst);
    let faulty = IdentityCallbacks::new("dynamic");
    let registry = registry.with_provider(faulty.clone()).unwrap();
    faulty.unavailable.store(true, SeqCst);
    let state = EditingState::new(editable_markdown_parser::Source::new("body"));
    let mut cache = RenderControlCache::default();
    for failure in [0, 1, 2, 3, 0] {
        faulty.inner.failure.store(failure, SeqCst);
        faulty.inner.generation.fetch_add(1, SeqCst);
        for _ in 0..2 {
            let batch = registry.cached_control_batch(&state, &mut cache);
            let fresh = registry.control_batch(&state);
            for controls in [&*batch.controls, &fresh.controls] {
                let control = controls.iter().find(|c| c.provider == "stable").unwrap();
                assert!(registry
                    .plan(
                        &state,
                        &RenderActionRequest {
                            provider: "stable",
                            target: control.target.clone(),
                            command: "stable.run",
                        }
                    )
                    .is_ok());
            }
            for diagnostic in batch.diagnostics.iter().chain(&fresh.diagnostics) {
                assert_eq!(diagnostic.feature, "dynamic");
            }
        }
    }
    for provider in [&healthy, &faulty] {
        assert_eq!(provider.ids.load(SeqCst), 1);
        assert_eq!(provider.commands.load(SeqCst), 1);
    }
    // Invalid startup metadata is reported without damaging an existing registry.
    let rejected = IdentityCallbacks::new("dynamic");
    rejected.unavailable.store(true, SeqCst);
    assert!(registry.with_provider(rejected).is_err());
    assert_eq!(registry.control_batch(&state).controls.len(), 2);
}
#[test]
fn undeclared_controls_recompute_only_their_own_contribution_and_empty_results_reuse() {
    let stable = Provider::new("stable", Some(Default::default()));
    let dynamic = Provider::new("dynamic", None);
    let registry = RenderActionRegistry::new(vec![stable.clone(), dynamic.clone()]).unwrap();
    let mut cache = RenderControlCache::default();
    let mut state = EditingState::new(editable_markdown_parser::Source::new("中😀 tail"));
    for i in 0..10 {
        state.head = [0, 3, 7, 8][i % 4];
        registry.cached_control_batch(&state, &mut cache);
    }
    assert_eq!(stable.calls.load(SeqCst), 1);
    assert_eq!(dynamic.calls.load(SeqCst), 10);
    assert_eq!(cache.entries.len(), 1);
    let mut empty = Provider::new("stable", Some(Default::default()));
    Arc::get_mut(&mut empty).unwrap().empty = true;
    let registry = RenderActionRegistry::new(vec![empty.clone()]).unwrap();
    let a = registry.cached_control_batch(&state, &mut cache);
    let b = registry.cached_control_batch(&state, &mut cache);
    assert!(Arc::ptr_eq(&a.controls, &b.controls));
    assert_eq!(empty.calls.load(SeqCst), 1);
    assert_eq!(cache.entries.len(), 1);
}
#[test]
fn declared_inputs_and_provider_identity_match_uncached_controls() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw = format!("中😀{newline}tail");
        let provider = Provider::new(
            "stable",
            Some(CacheDependencies {
                selection: true,
                visibility: true,
                closers: true,
                generation: 0,
            }),
        );
        let registry = RenderActionRegistry::new(vec![provider.clone()]).unwrap();
        let mut cache = RenderControlCache::default();
        let mut s = EditingState::new(editable_markdown_parser::Source::new(&raw));
        let mut previous = None;
        for step in 0..10 {
            match step {
                1 => s.head = 3,
                2 => {
                    s.anchor = 3;
                    s.head = 0;
                }
                3 => {
                    s.reveal =
                        Some(editable_markdown_parser::Session::new(&s.source, 0..3).unwrap())
                }
                4 => s.heading_reveal = s.reveal.clone(),
                5 => s.closers = std::iter::once(3..7).collect(),
                6 => provider.generation.store(1, SeqCst),
                7 => s.block_source = s.reveal.clone(),
                8 => s.source_mode = true,
                9 => {
                    s.source_mode = false;
                    s.block_source = None;
                }
                _ => (),
            }
            let cached = registry.cached_control_batch(&s, &mut cache);
            let fresh = registry.control_batch(&s);
            assert!(cached.diagnostics.is_empty());
            assert_eq!(signature(&cached.controls), signature(&fresh.controls));
            if let Some(old) = previous {
                assert!(!Arc::ptr_eq(&old, &cached.controls), "step {step}");
            }
            let same = registry.cached_control_batch(&s, &mut cache);
            assert!(Arc::ptr_eq(&cached.controls, &same.controls));
            previous = Some(cached.controls);
        }
        // Replacing an instance under the same ID cannot reuse its predecessor.
        let replacement = Provider::new("stable", Some(Default::default()));
        let registry = RenderActionRegistry::new(vec![replacement.clone()]).unwrap();
        registry.cached_control_batch(&s, &mut cache);
        assert_eq!(replacement.calls.load(SeqCst), 1);
        s = EditingState::new(editable_markdown_parser::Source::new(&raw));
        registry.cached_control_batch(&s, &mut cache);
        assert_eq!(replacement.calls.load(SeqCst), 2);
    }
}
#[test]
fn faults_and_changing_generations_never_poison_healthy_contributors() {
    let stable = Provider::new("stable", Some(Default::default()));
    let dynamic = Provider::new("dynamic", Some(Default::default()));
    let registry = RenderActionRegistry::new(vec![stable.clone(), dynamic.clone()]).unwrap();
    let state = EditingState::new(editable_markdown_parser::Source::new("literal"));
    let mut cache = RenderControlCache::default();
    for failure in [1, 2, 3, 4] {
        dynamic.failure.store(failure, SeqCst);
        dynamic.generation.fetch_add(1, SeqCst);
        for _ in 0..2 {
            let batch = registry.cached_control_batch(&state, &mut cache);
            assert_eq!(batch.diagnostics.len(), 1);
            assert!(batch.controls.iter().any(|c| c.provider == "stable"));
        }
        dynamic.failure.store(0, SeqCst);
        let a = registry.cached_control_batch(&state, &mut cache);
        let b = registry.cached_control_batch(&state, &mut cache);
        assert!(a.diagnostics.is_empty());
        assert!(Arc::ptr_eq(&a.controls, &b.controls));
    }
    assert_eq!(stable.calls.load(SeqCst), 1);
    dynamic.mutate_generation.store(true, SeqCst);
    dynamic.generation.fetch_add(1, SeqCst);
    for _ in 0..2 {
        registry.cached_control_batch(&state, &mut cache);
        assert!(!cache.entries.contains_key("dynamic"));
    }
    dynamic.mutate_generation.store(false, SeqCst);
    let batch = registry.cached_control_batch(&state, &mut cache);
    let control = batch
        .controls
        .iter()
        .find(|c| c.provider == "dynamic")
        .unwrap();
    let request = RenderActionRequest {
        provider: "dynamic",
        target: control.target.clone(),
        command: "dynamic.run",
    };
    dynamic.enabled.store(false, SeqCst);
    assert!(
        registry.plan(&state, &request).is_err(),
        "execution must recheck enabled"
    );
}

struct Primary(bool);
impl FeatureModule for Primary {
    fn manifest(&self) -> FeatureManifest {
        FeatureManifest::builtin("primary")
    }
    fn register(&self, r: &mut Registrar) -> Result<(), String> {
        r.presentation(
            Contribution {
                id: "primary.display",
                slot: Slot::PrimaryPresentation("document"),
                exclusive: true,
                priority: 0,
            },
            Arc::new(Self(self.0)),
        )
    }
}
impl PresentationProvider for Primary {
    fn cache_key(&self, s: &EditingState, _: &FeatureServices<'_>) -> Option<PresentationKey> {
        self.0.then(|| {
            PresentationKey::for_state(
                s,
                CacheDependencies {
                    selection: true,
                    visibility: true,
                    closers: true,
                    generation: 42,
                },
            )
        })
    }
    fn display(
        &self,
        _: &EditingState,
        _: &FeatureServices<'_>,
        _: Option<&mut dyn SyntaxHighlighter>,
        _: &[ryne_projection::DiagramViewInput],
    ) -> ryne_projection::Display {
        Default::default()
    }
}
#[test]
fn primary_provider_is_conservative_and_can_declare_selection_and_input_dependencies() {
    struct Formula;
    impl FormulaRenderer for Formula {
        fn render(&self, _: &str) -> Option<Arc<ryne_projection::Formula>> {
            None
        }
    }
    let services = FeatureServices {
        formulas: &Formula,
        planning: Default::default(),
    };
    let mut state = EditingState::new(editable_markdown_parser::Source::new("abc"));
    assert!(Registry::build(&[&Primary(false)])
        .unwrap()
        .presentation_key(&state, &services)
        .is_none());
    let registry = Registry::build(&[&Primary(true)]).unwrap();
    let first = registry.presentation_key(&state, &services).unwrap();
    state.anchor = 3;
    let selected = registry.presentation_key(&state, &services).unwrap();
    assert_ne!(first, selected);
    state.head = 3;
    state.anchor = 0;
    assert_ne!(
        selected,
        registry.presentation_key(&state, &services).unwrap()
    );
    state.closers = std::iter::once(1..2).collect();
    assert_ne!(first, registry.presentation_key(&state, &services).unwrap());
}

#[test]
fn stale_semantics_cannot_bypass_validation_via_a_cached_control() {
    let provider = Provider::new("stable", Some(Default::default()));
    let registry = RenderActionRegistry::new(vec![provider.clone()]).unwrap();
    let mut cache = RenderControlCache::default();
    let mut state = EditingState::new(editable_markdown_parser::Source::new("same"));
    let original = state.parse.clone();
    registry.cached_control_batch(&state, &mut cache);
    state.parse = EditingState::new(editable_markdown_parser::Source::new("same")).parse;
    let invalid = registry.cached_control_batch(&state, &mut cache);
    assert!(invalid.controls.is_empty());
    assert_eq!(invalid.diagnostics.len(), 1);
    assert_eq!(provider.calls.load(SeqCst), 1);
    state.parse = original;
    registry.cached_control_batch(&state, &mut cache);
    assert_eq!(provider.calls.load(SeqCst), 2);
}
