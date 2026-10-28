//! Source candidates are cached separately from live visibility and renderer jobs.
use crate::{
    catch_callback, Diagnostic, EditingState, PreviewBatch, PreviewInput, PreviewProvider,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

const MAX_CANDIDATES: usize = 32;
type Candidates = Arc<Vec<(usize, PreviewInput)>>;
#[derive(PartialEq, Eq)]
struct Key {
    document: ryne_core::DocumentKey,
    generation: u64,
}
struct Entry {
    provider: Arc<dyn PreviewProvider>,
    key: Key,
    candidates: Candidates,
}
/// One source generation and at most 32 positive candidates per current provider.
/// Negative results are represented by the absence of candidates, not one entry per block.
#[derive(Default)]
pub struct PreviewCache {
    entries: BTreeMap<&'static str, Entry>,
}
impl PreviewCache {
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
#[derive(Clone, Default)]
pub(crate) struct PreviewRegistry {
    providers: Vec<(&'static str, Arc<dyn PreviewProvider>)>,
}
impl PreviewRegistry {
    pub fn new(providers: Vec<(&'static str, Arc<dyn PreviewProvider>)>) -> Self {
        Self { providers }
    }
    pub fn batch(&self, state: &EditingState) -> PreviewBatch {
        self.run(state, None)
    }
    pub fn cached_batch(&self, state: &EditingState, cache: &mut PreviewCache) -> PreviewBatch {
        self.run(state, Some(cache))
    }
    fn run(&self, state: &EditingState, mut cache: Option<&mut PreviewCache>) -> PreviewBatch {
        let mut batch = PreviewBatch::default();
        if state.parse.source().id() != state.source.id() {
            if let Some(cache) = cache {
                cache.clear();
            }
            batch.diagnostics.push(Diagnostic {
                feature: "preview".into(),
                message: "stale preview semantics".into(),
            });
            return batch;
        }
        if let Some(cache) = &mut cache {
            let current = self
                .providers
                .iter()
                .map(|(id, provider)| (*id, provider))
                .collect::<BTreeMap<_, _>>();
            cache.entries.retain(|id, entry| {
                current
                    .get(id)
                    .is_some_and(|provider| Arc::ptr_eq(provider, &entry.provider))
            });
        }
        if self.providers.is_empty() {
            return batch;
        }
        // Some = source-only candidate directory; None = live, ordered callback.
        let mut directories = Vec::with_capacity(self.providers.len());
        for (id, provider) in &self.providers {
            let directory = cache.as_deref_mut().and_then(|cache| {
                let generation = match catch_callback(id, "preview cache generation", || {
                    provider.source_cache_generation()
                }) {
                    Ok(generation) => generation,
                    Err(error) => {
                        batch.diagnostics.push(error);
                        None
                    }
                };
                let Some(generation) = generation else {
                    cache.entries.remove(id);
                    return None;
                };
                let key = Key {
                    document: state.source.id(),
                    generation,
                };
                if let Some(entry) = cache.entries.get(id).filter(|e| e.key == key) {
                    return Some(entry.candidates.clone());
                }
                cache.entries.remove(id);
                let (candidates, failed) =
                    collect(id, provider.as_ref(), state, &mut batch.diagnostics);
                let after = catch_callback(id, "preview cache generation", || {
                    provider.source_cache_generation()
                });
                let stable = match after {
                    Ok(after) => after == Some(generation),
                    Err(error) => {
                        batch.diagnostics.push(error);
                        false
                    }
                };
                if !failed && stable {
                    cache.entries.insert(
                        id,
                        Entry {
                            provider: provider.clone(),
                            key,
                            candidates: candidates.clone(),
                        },
                    );
                }
                Some(candidates)
            });
            directories.push(directory);
        }
        // With only stable providers, visit positive candidates instead of every
        // ordinary fence. Unknown providers must still see all unclaimed blocks.
        let blocks = ryne_syntax::SemanticQuery::new(&state.parse).code_blocks();
        let indices: Box<dyn Iterator<Item = usize>> = if directories.iter().all(Option::is_some) {
            Box::new(
                directories
                    .iter()
                    .flatten()
                    .flat_map(|c| c.iter().map(|(i, _)| *i))
                    .collect::<BTreeSet<_>>()
                    .into_iter(),
            )
        } else {
            Box::new(0..blocks.len())
        };
        let mut failed = BTreeSet::new();
        for index in indices {
            for ((id, provider), directory) in self.providers.iter().zip(&directories) {
                if failed.contains(id) {
                    continue;
                }
                let result = if let Some(candidates) = directory {
                    Ok(candidates
                        .binary_search_by_key(&index, |(i, _)| *i)
                        .ok()
                        .map(|i| candidates[i].1.clone()))
                } else {
                    input(id, provider.as_ref(), state, &blocks[index])
                };
                match result {
                    Ok(Some(input)) => {
                        batch.inputs.push(input);
                        break;
                    }
                    Ok(None) => (),
                    Err(error) => {
                        failed.insert(id);
                        batch.diagnostics.push(error);
                    }
                }
            }
            if batch.inputs.len() == MAX_CANDIDATES {
                break;
            }
        }
        batch
    }
}
fn input(
    id: &'static str,
    provider: &dyn PreviewProvider,
    state: &EditingState,
    block: &ryne_syntax::code_block::CodeBlock,
) -> Result<Option<PreviewInput>, Diagnostic> {
    catch_callback(id, "preview input", || provider.input(state, block)).map(|input| {
        input.map(|mut input| {
            // The registry owns resource identity, including for cached inputs.
            input.feature = id;
            input
        })
    })
}
fn collect(
    id: &'static str,
    provider: &dyn PreviewProvider,
    state: &EditingState,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Candidates, bool) {
    let mut candidates = Vec::new();
    let mut failed = false;
    for (index, block) in ryne_syntax::SemanticQuery::new(&state.parse)
        .code_blocks()
        .iter()
        .enumerate()
    {
        match input(id, provider, state, block) {
            Ok(Some(input)) => candidates.push((index, input)),
            Ok(None) => (),
            Err(error) => {
                diagnostics.push(error);
                failed = true;
                break;
            }
        }
        // Any later match has at least 32 earlier matching blocks. Regardless
        // of which provider wins those blocks, the global result is already full.
        if candidates.len() == MAX_CANDIDATES {
            break;
        }
    }
    (Arc::new(candidates), failed)
}

#[cfg(test)]
#[path = "preview_tests.rs"]
mod tests;
