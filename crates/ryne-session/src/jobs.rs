//! Bounded serial content jobs; cache keys include feature and renderer generation.
use ryne_core::DocumentKey;
use ryne_extensions::{CancelControl, DiagramRenderer, EditingState, PreviewInput};
use ryne_projection::{DiagramOutcome, DiagramViewInput};
use std::{collections::HashMap, ops::Range, sync::Arc};
const MAX_BLOCKS: usize = 32;
const MAX_BYTES: usize = 32 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InputKey {
    pub feature: &'static str,
    pub generation: u64,
    pub source: String,
}
pub struct JobRequest {
    pub id: u64,
    pub document: DocumentKey,
    pub key: InputKey,
    pub control: CancelControl,
    renderer: Arc<dyn DiagramRenderer>,
}
pub struct JobResult {
    id: u64,
    document: DocumentKey,
    key: InputKey,
    control: CancelControl,
    result: DiagramOutcome,
    renderer: Arc<dyn DiagramRenderer>,
}
impl JobRequest {
    pub fn run(self) -> JobResult {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.renderer.render(&self.key.source, self.control.clone())
        }))
        .unwrap_or_else(|_| Err("图表渲染失败，保留源码".into()));
        JobResult {
            id: self.id,
            document: self.document,
            key: self.key,
            control: self.control,
            result,
            renderer: self.renderer,
        }
    }
}
#[derive(Default)]
pub struct JobHost {
    cache: HashMap<InputKey, DiagramOutcome>,
    active: Option<(u64, DocumentKey, InputKey, CancelControl)>,
    next: u64,
}
impl Drop for JobHost {
    fn drop(&mut self) {
        if let Some((_, _, _, c)) = &self.active {
            c.cancel()
        }
    }
}
impl JobHost {
    pub fn pending(&self) -> bool {
        self.active.is_some()
    }
    pub fn refresh(
        &mut self,
        state: &EditingState,
        inputs: Vec<PreviewInput>,
        frozen: bool,
        folded: &[Range<usize>],
    ) -> (Vec<DiagramViewInput>, Option<JobRequest>) {
        let mut views = Vec::new();
        let mut sources = Vec::new();
        let mut requested = Vec::new();
        for preview in inputs.into_iter().take(MAX_BLOCKS) {
            let block = preview.block;
            let source = match preview.input {
                Ok(s) => s,
                Err(e) => {
                    views.push((block.range, Err(e)));
                    continue;
                }
            };
            let key = InputKey {
                feature: preview.feature,
                generation: preview.renderer.generation(),
                source,
            };
            sources.push(key.clone());
            let visible = !state.source_mode
                && !state.block_source.as_ref().is_some_and(|s| {
                    s.range.start < block.range.end && block.range.start < s.range.end
                });
            let active = block.range.contains(&state.head);
            if visible && ((!frozen && !active) || (frozen && folded.contains(&block.range))) {
                if let Some(result) = self.cache.get(&key) {
                    views.push((block.range, result.clone()))
                }
                if !frozen {
                    requested.push((key, preview.renderer))
                }
            } else if let Some(Err(e)) = self.cache.get(&key) {
                views.push((block.range, Err(e.clone())))
            }
        }
        self.cache.retain(|key, _| sources.contains(key));
        if let Some((_, document, key, control)) = &self.active
            && (document.document_id != state.source.id().document_id
                || !requested.iter().any(|(input, _)| input == key))
        {
            control.cancel()
        }
        let request = if self.active.is_none() {
            requested
                .into_iter()
                .find(|(key, _)| !self.cache.contains_key(key))
                .map(|(key, renderer)| {
                    self.next += 1;
                    let control = CancelControl::default();
                    let document = state.source.id();
                    self.active = Some((self.next, document, key.clone(), control.clone()));
                    JobRequest {
                        id: self.next,
                        document,
                        key,
                        control,
                        renderer,
                    }
                })
        } else {
            None
        };
        (views, request)
    }
    pub fn complete(&mut self, result: JobResult, current: DocumentKey) -> bool {
        if !self.active.as_ref().is_some_and(|(id, doc, key, _)| {
            *id == result.id && *doc == result.document && *key == result.key
        }) {
            return false;
        }
        self.active = None;
        if result.control.is_cancelled()
            || result.document.document_id != current.document_id
            || result.key.generation != result.renderer.generation()
        {
            return false;
        }
        if result.control.checkpoint().is_err() {
            return false;
        }
        let result_value = result.result.and_then(|resource| {
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
        });
        // A resource is content-addressed; a later refresh binds it to current ranges.
        let used: usize = self
            .cache
            .values()
            .filter_map(|v| v.as_ref().ok())
            .map(|d| d.bytes)
            .sum();
        let value = if result_value
            .as_ref()
            .is_ok_and(|d| used + d.bytes > MAX_BYTES)
        {
            Err("图表缓存已达上限，保留源码".into())
        } else {
            result_value
        };
        if self.cache.len() < MAX_BLOCKS || self.cache.contains_key(&result.key) {
            self.cache.insert(result.key, value);
            true
        } else {
            false
        }
    }
    pub fn invalidate(&mut self, input: &str, error: String) {
        for (key, result) in &mut self.cache {
            if key.source == input {
                *result = Err(error.clone())
            }
        }
    }
}
