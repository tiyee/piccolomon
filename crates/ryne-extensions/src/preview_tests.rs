use super::*;
use crate::{CancelControl, DiagramRenderer};
use ryne_parser::{Session, Source};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

struct DocumentPreview {
    calls: AtomicUsize,
    malformed: bool,
    renderer: Arc<dyn DiagramRenderer>,
}
impl PreviewProvider for DocumentPreview {
    fn source_cache_generation(&self) -> Option<u64> {
        Some(0)
    }
    fn document_inputs(&self, _: &EditingState) -> Option<Vec<PreviewInput>> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Some(vec![PreviewInput {
            fold_errors: false,
            feature: "untrusted",
            block: ryne_projection::PreviewSource {
                range: 0..3,
                scope: 0..3,
                anchor: if self.malformed { 99 } else { 0 },
                indent: 0,
            },
            input: Ok("image".into()),
            renderer: self.renderer.clone(),
        }])
    }
}
#[test]
fn document_candidates_merge_with_fences_and_share_cache_validation_and_identity() {
    let state = state("one\n\n```diagram\nbody\n```\n");
    for malformed in [false, true] {
        let image = Arc::new(DocumentPreview {
            calls: AtomicUsize::new(0),
            malformed,
            renderer: Arc::new(Renderer),
        });
        let fence = Probe::new(true, "diagram");
        let registry =
            PreviewRegistry::new(vec![("image", image.clone()), ("fence", fence.clone())]);
        let mut cache = PreviewCache::default();
        for _ in 0..3 {
            let batch = registry.cached_batch(&state, &mut cache);
            if malformed {
                assert_eq!(batch.inputs.len(), 1);
                assert_eq!(batch.inputs[0].feature, "fence");
                assert_eq!(batch.diagnostics.len(), 1);
            } else {
                assert_eq!(batch.inputs.len(), 2);
                assert_eq!(batch.inputs[0].feature, "image");
                assert_eq!(batch.inputs[1].feature, "fence");
                assert!(batch.inputs[0].block.range.end < batch.inputs[1].block.range.start);
            }
        }
        assert_eq!(
            image.calls.load(Ordering::Relaxed),
            if malformed { 3 } else { 1 }
        );
        assert_eq!(fence.calls(), 1);
    }
}

struct Renderer;
impl DiagramRenderer for Renderer {
    fn generation(&self) -> u64 {
        0
    }
    fn render(&self, _: &str, _: CancelControl) -> ryne_projection::DiagramOutcome {
        Err("unused".into())
    }
}
struct Probe {
    calls: AtomicUsize,
    generation: AtomicU64,
    cacheable: bool,
    language: &'static str,
    enabled: AtomicBool,
    fail: AtomicBool,
    fail_at: AtomicUsize,
    fail_key: AtomicBool,
    change_key: AtomicBool,
    renderer: Arc<dyn DiagramRenderer>,
}
impl Probe {
    fn new(cacheable: bool, language: &'static str) -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            generation: AtomicU64::new(0),
            cacheable,
            language,
            enabled: AtomicBool::new(true),
            fail: AtomicBool::new(false),
            fail_at: AtomicUsize::new(usize::MAX),
            fail_key: AtomicBool::new(false),
            change_key: AtomicBool::new(false),
            renderer: Arc::new(Renderer),
        })
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}
impl PreviewProvider for Probe {
    fn source_cache_generation(&self) -> Option<u64> {
        assert!(!self.fail_key.load(Ordering::Relaxed), "key failure");
        self.cacheable
            .then(|| self.generation.load(Ordering::Relaxed))
    }
    fn input(
        &self,
        state: &EditingState,
        block: &ryne_syntax::code_block::CodeBlock,
    ) -> Option<PreviewInput> {
        let call = self.calls.fetch_add(1, Ordering::Relaxed);
        assert_ne!(
            call,
            self.fail_at.load(Ordering::Relaxed),
            "late input failure"
        );
        assert!(!self.fail.load(Ordering::Relaxed), "input failure");
        if self.change_key.load(Ordering::Relaxed) {
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
        if !self.enabled.load(Ordering::Relaxed)
            || (self.language != "*" && block.language != self.language)
        {
            return None;
        }
        Some(PreviewInput {
            fold_errors: false,
            feature: "untrusted-id",
            block: block.into(),
            input: Ok(state.source.text()[block.range.clone()].to_owned()),
            renderer: self.renderer.clone(),
        })
    }
}
fn state(raw: &str) -> EditingState {
    EditingState::new(Source::new(raw))
}
fn summary(
    batch: &PreviewBatch,
) -> Vec<(&'static str, std::ops::Range<usize>, Result<String, String>)> {
    batch
        .inputs
        .iter()
        .map(|p| (p.feature, p.block.range.clone(), p.input.clone()))
        .collect()
}

#[test]
fn source_candidates_and_negative_matches_survive_view_changes_without_scanning() {
    let raw = "```text\nplain\n```\n\n".repeat(1000) + "```diagram\nA-->B\n```\n\ntail";
    let mut state = state(&raw);
    let provider = Probe::new(true, "diagram");
    let registry = PreviewRegistry::new(vec![("preview", provider.clone())]);
    let mut cache = PreviewCache::default();
    let first = registry.cached_batch(&state, &mut cache);
    assert_eq!(provider.calls(), 1001);
    assert_eq!(first.inputs.len(), 1);
    for i in 0..10 {
        state.head = i;
        state.anchor = i / 2;
        state.source_mode = i % 2 == 0;
        state.block_source =
            (i % 2 == 0).then(|| Session::new(&state.source, 0..raw.len()).unwrap());
        state.reveal = Some(Session::new(&state.source, i..i + 1).unwrap());
        assert_eq!(
            summary(&registry.cached_batch(&state, &mut cache)),
            summary(&first)
        );
    }
    assert_eq!(
        provider.calls(),
        1001,
        "caret refreshes must not revisit ordinary fences"
    );
    println!("preview_candidates blocks=1001 cold_calls=1001 refreshes=10 additional_calls=0");
    assert_eq!(cache.entries.len(), 1);
    assert_eq!(cache.entries["preview"].candidates.len(), 1);
    let empty = EditingState::new(Source::new(&"```text\nplain\n```\n\n".repeat(1000)));
    assert!(registry.cached_batch(&empty, &mut cache).inputs.is_empty());
    let calls = provider.calls();
    for _ in 0..10 {
        assert!(registry.cached_batch(&empty, &mut cache).inputs.is_empty());
    }
    assert_eq!(provider.calls(), calls);
    assert!(cache.entries["preview"].candidates.is_empty());
}

#[test]
fn unknown_provider_recomputes_without_invalidating_stable_provider_or_precedence() {
    let state = state("```diagram\nbody\n```\n");
    let dynamic = Probe::new(false, "diagram");
    let stable = Probe::new(true, "diagram");
    let registry = PreviewRegistry::new(vec![
        ("dynamic", dynamic.clone()),
        ("stable", stable.clone()),
    ]);
    let mut cache = PreviewCache::default();
    for enabled in [true, false, true, false] {
        dynamic.enabled.store(enabled, Ordering::Relaxed);
        let result = registry.cached_batch(&state, &mut cache);
        assert_eq!(
            result.inputs[0].feature,
            if enabled { "dynamic" } else { "stable" }
        );
    }
    assert_eq!(dynamic.calls(), 4);
    assert_eq!(stable.calls(), 1);
    assert_eq!(cache.entries.len(), 1);
}

#[test]
fn candidates_are_bounded_and_merge_in_original_block_and_provider_order() {
    let raw = (0..80)
        .map(|i| format!("```{}\n{i}\n```\n\n", if i % 2 == 0 { "a" } else { "b" }))
        .collect::<String>();
    let state = state(&raw);
    let first = Probe::new(true, "a");
    let second = Probe::new(true, "*");
    let registry = PreviewRegistry::new(vec![("first", first), ("second", second)]);
    let mut cache = PreviewCache::default();
    let cached = registry.cached_batch(&state, &mut cache);
    assert_eq!(cached.inputs.len(), 32);
    assert_eq!(summary(&cached), summary(&registry.batch(&state)));
    assert!(cache.entries.values().all(|e| e.candidates.len() <= 32));
    assert!(cached
        .inputs
        .iter()
        .enumerate()
        .all(|(i, p)| p.feature == if i % 2 == 0 { "first" } else { "second" }));
}

#[test]
fn source_generation_and_provider_identity_invalidate_only_affected_entries() {
    let state = state("```diagram\none\n```\n");
    let provider = Probe::new(true, "diagram");
    let registry = PreviewRegistry::new(vec![("preview", provider.clone())]);
    let mut cache = PreviewCache::default();
    registry.cached_batch(&state, &mut cache);
    provider.generation.store(1, Ordering::Relaxed);
    registry.cached_batch(&state, &mut cache);
    assert_eq!(provider.calls(), 2);
    let replacement = Probe::new(true, "diagram");
    let replaced = PreviewRegistry::new(vec![("preview", replacement.clone())]);
    let batch = replaced.cached_batch(&state, &mut cache);
    assert_eq!(replacement.calls(), 1);
    assert!(Arc::ptr_eq(
        &batch.inputs[0].renderer,
        &replacement.renderer
    ));
    let next = EditingState::new(
        state
            .source
            .apply(
                state.source.id(),
                vec![ryne_parser::Edit {
                    range: 0..0,
                    replacement: "prefix\n\n".into(),
                }],
            )
            .unwrap()
            .source,
    );
    let batch = replaced.cached_batch(&next, &mut cache);
    assert_eq!(replacement.calls(), 2);
    assert_eq!(batch.inputs[0].block.range.start, 8);
    let foreign = EditingState::new(Source::new(next.source.text()));
    replaced.cached_batch(&foreign, &mut cache);
    assert_eq!(replacement.calls(), 3);
    PreviewRegistry::default().cached_batch(&foreign, &mut cache);
    assert!(cache.entries.is_empty());
}

#[test]
fn faults_and_unstable_generation_do_not_poison_healthy_candidates() {
    let state = state("```diagram\none\n```\n");
    let broken = Probe::new(true, "diagram");
    let healthy = Probe::new(true, "diagram");
    let registry = PreviewRegistry::new(vec![
        ("broken", broken.clone()),
        ("healthy", healthy.clone()),
    ]);
    let mut cache = PreviewCache::default();
    broken.fail.store(true, Ordering::Relaxed);
    for _ in 0..2 {
        let batch = registry.cached_batch(&state, &mut cache);
        assert_eq!(batch.inputs[0].feature, "healthy");
        assert_eq!(batch.diagnostics.len(), 1);
        assert!(!cache.entries.contains_key("broken"));
    }
    assert_eq!(healthy.calls(), 1);
    broken.fail.store(false, Ordering::Relaxed);
    broken.fail_key.store(true, Ordering::Relaxed);
    let batch = registry.cached_batch(&state, &mut cache);
    assert_eq!(batch.inputs[0].feature, "broken");
    assert_eq!(batch.diagnostics.len(), 1);
    assert!(!cache.entries.contains_key("broken"));
    broken.fail_key.store(false, Ordering::Relaxed);
    broken.change_key.store(true, Ordering::Relaxed);
    registry.cached_batch(&state, &mut cache);
    assert!(!cache.entries.contains_key("broken"));
    broken.change_key.store(false, Ordering::Relaxed);
    registry.cached_batch(&state, &mut cache);
    assert!(cache.entries.contains_key("broken"));
    assert_eq!(healthy.calls(), 1);
}

#[test]
fn stale_semantics_clear_the_cache_before_invoking_providers() {
    let mut state = state("```diagram\none\n```\n");
    let provider = Probe::new(true, "diagram");
    let registry = PreviewRegistry::new(vec![("preview", provider.clone())]);
    let mut cache = PreviewCache::default();
    registry.cached_batch(&state, &mut cache);
    state.source = Source::new("different");
    let batch = registry.cached_batch(&state, &mut cache);
    assert!(batch.inputs.is_empty());
    assert_eq!(batch.diagnostics.len(), 1);
    assert!(cache.entries.is_empty());
    assert_eq!(provider.calls(), 1);
}

#[test]
fn late_provider_failure_preserves_its_successful_prefix_and_healthy_fallbacks() {
    let state = state(&"```diagram\none\n```\n\n".repeat(3));
    let broken = Probe::new(true, "diagram");
    let healthy = Probe::new(true, "diagram");
    broken.fail_at.store(1, Ordering::Relaxed);
    let registry = PreviewRegistry::new(vec![
        ("broken", broken.clone()),
        ("healthy", healthy.clone()),
    ]);
    let mut cache = PreviewCache::default();
    let cached = registry.cached_batch(&state, &mut cache);
    assert_eq!(cached.inputs.len(), 3);
    assert_eq!(cached.inputs[0].feature, "broken");
    assert_eq!(cached.inputs[1].feature, "healthy");
    assert_eq!(cached.diagnostics.len(), 1);
    assert!(!cache.entries.contains_key("broken"));
    assert!(cache.entries.contains_key("healthy"));
    broken.calls.store(0, Ordering::Relaxed);
    assert_eq!(summary(&cached), summary(&registry.batch(&state)));
}
