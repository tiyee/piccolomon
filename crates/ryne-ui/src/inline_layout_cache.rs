//! Bounded block-local shaping reuse. Entries contain relative display offsets;
//! source mappings, Y coordinates and event tokens are always rebuilt for the
//! current frame. CRC is only a bucket selector, never proof of equality.
use super::*;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

const MAX_ENTRIES: usize = 2048;
const MAX_CHARGE: usize = 16 * 1024 * 1024;

#[derive(Default, Clone, Copy)]
pub(super) struct ShapeWork {
    pub hits: usize,
    pub misses: usize,
    pub shaped_bytes: usize,
}
struct Entry {
    text: String,
    runs: Vec<TextRun>,
    metrics: (f32, f32),
    width: Pixels,
    formulas: Vec<(Range<usize>, Arc<ryne_projection::Formula>)>,
    lines: Vec<TypesetLine>,
    charge: usize,
}
#[derive(Default)]
pub(super) struct InlineLayoutCache {
    document: Option<ryne_parser::DocumentId>,
    scale: f32,
    next: u64,
    entries: BTreeMap<u64, Entry>,
    buckets: BTreeMap<(u64, usize), Vec<u64>>,
    order: VecDeque<((u64, usize), u64)>,
    charge: usize,
    pub work: ShapeWork,
    /// Used by the real-window differential probe, never enabled by editing.
    pub bypass: bool,
}
impl InlineLayoutCache {
    pub fn begin(&mut self, document: ryne_parser::DocumentId, scale: f32) {
        if self.document != Some(document) || self.scale != scale {
            *self = Self {
                document: Some(document),
                scale,
                ..Self::default()
            };
        }
        self.work = ShapeWork::default();
    }
    pub fn shape(
        &mut self,
        display: &Display,
        runs: &[TextRun],
        range: Range<usize>,
        metrics: (f32, f32),
        width: Pixels,
        window: &Window,
    ) -> Result<Vec<TypesetLine>, String> {
        display.validate_inline_layout(&range)?;
        if self.bypass {
            self.work.misses += 1;
            self.work.shaped_bytes += range.len();
            return shape_inline(display, runs, range, metrics, width, window);
        }
        let text = &display.text[range.clone()];
        let bucket = (crc64(text.as_bytes()), text.len());
        let local_runs = text_runs(display, runs, range.clone());
        let first = display
            .formulas
            .partition_point(|f| f.display.start < range.start);
        let formulas: Vec<_> = display.formulas[first..]
            .iter()
            .take_while(|f| f.display.end <= range.end)
            .map(|f| {
                (
                    f.display.start - range.start..f.display.end - range.start,
                    f.formula.clone(),
                )
            })
            .collect();
        if let Some(entry) = self
            .buckets
            .get(&bucket)
            .into_iter()
            .flatten()
            .filter_map(|id| self.entries.get(id))
            .find(|e| e.matches(text, &local_runs, metrics, width, &formulas))
        {
            self.work.hits += 1;
            return Ok(rebase(&entry.lines, range.start));
        }
        self.work.misses += 1;
        self.work.shaped_bytes += text.len();
        let lines = shape_inline(display, runs, range.clone(), metrics, width, window)?;
        // The legacy shaper may return an empty/default result after a platform
        // failure. Do not turn that transient result into a reusable success.
        if lines.iter().map(|line| line.shaped.len()).sum::<usize>() != text.len() {
            return Ok(lines);
        }
        // Conservative charge for retained text, runs, glyphs and formula data.
        // Oversized single lines still use the existing complete shaping path.
        let charge = 512usize
            .saturating_add(text.len().saturating_mul(128))
            .saturating_add(local_runs.len().saturating_mul(256))
            .saturating_add(
                formulas
                    .iter()
                    .map(|(_, f)| f.svg.len() + f.path.len() + 128)
                    .sum::<usize>(),
            );
        if charge <= MAX_CHARGE {
            let mut local = lines.clone();
            for line in &mut local {
                line.start -= range.start;
            }
            self.insert(
                bucket,
                Entry {
                    text: text.into(),
                    runs: local_runs,
                    metrics,
                    width,
                    formulas,
                    lines: local,
                    charge,
                },
            );
        }
        Ok(lines)
    }
    fn insert(&mut self, bucket: (u64, usize), entry: Entry) {
        while self.entries.len() >= MAX_ENTRIES || self.charge + entry.charge > MAX_CHARGE {
            let Some((key, id)) = self.order.pop_front() else {
                break;
            };
            if let Some(old) = self.entries.remove(&id) {
                self.charge -= old.charge;
            }
            if let Some(ids) = self.buckets.get_mut(&key) {
                ids.retain(|&old| old != id);
                if ids.is_empty() {
                    self.buckets.remove(&key);
                }
            }
        }
        self.next += 1;
        self.charge += entry.charge;
        self.entries.insert(self.next, entry);
        self.buckets.entry(bucket).or_default().push(self.next);
        self.order.push_back((bucket, self.next));
    }
}
impl Entry {
    fn matches(
        &self,
        text: &str,
        runs: &[TextRun],
        metrics: (f32, f32),
        width: Pixels,
        formulas: &[(Range<usize>, Arc<ryne_projection::Formula>)],
    ) -> bool {
        self.text == text
            && self.runs == runs
            && self.metrics == metrics
            && self.width == width
            && self.formulas.len() == formulas.len()
            && self
                .formulas
                .iter()
                .zip(formulas)
                .all(|((ar, a), (br, b))| ar == br && Arc::ptr_eq(a, b))
    }
}
fn rebase(lines: &[TypesetLine], start: usize) -> Vec<TypesetLine> {
    lines
        .iter()
        .cloned()
        .map(|mut line| {
            line.start += start;
            line
        })
        .collect()
}

// CRC-64/ECMA-182: poly 0x42F0E1EBA9EA3693, init=0, xorout=0, refin/refout=false.
fn crc64(bytes: &[u8]) -> u64 {
    const TABLE: [u64; 256] = crc_table();
    bytes.iter().fold(0, |crc, &b| {
        (crc << 8) ^ TABLE[((crc >> 56) as u8 ^ b) as usize]
    })
}
const fn crc_table() -> [u64; 256] {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut value = (i as u64) << 56;
        let mut bit = 0;
        while bit < 8 {
            value = if value & (1 << 63) != 0 {
                (value << 1) ^ 0x42F0E1EBA9EA3693
            } else {
                value << 1
            };
            bit += 1;
        }
        table[i] = value;
        i += 1;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(text: &str) -> Entry {
        Entry {
            text: text.into(),
            runs: vec![],
            metrics: (18., 30.),
            width: px(400.),
            formulas: vec![],
            lines: vec![],
            charge: 1024,
        }
    }
    #[::core::prelude::v1::test]
    fn collisions_dependencies_and_budget_are_checked() {
        assert_eq!(crc64(b"123456789"), 0x6c40df5f0b497347);
        let mut cache = InlineLayoutCache::default();
        let collision = (42, 3);
        cache.insert(collision, entry("one"));
        cache.insert(collision, entry("two"));
        let matches = |text, metrics, width| {
            cache.buckets[&collision]
                .iter()
                .filter(|id| cache.entries[id].matches(text, &[], metrics, width, &[]))
                .count()
        };
        assert_eq!(matches("one", (18., 30.), px(400.)), 1);
        assert_eq!(matches("six", (18., 30.), px(400.)), 0);
        assert_eq!(matches("one", (15., 24.), px(400.)), 0);
        assert_eq!(matches("one", (18., 30.), px(300.)), 0);
        for i in 0..MAX_ENTRIES + 4 {
            cache.insert((i as u64, 0), entry(""));
        }
        assert_eq!(cache.entries.len(), MAX_ENTRIES);
        assert!(cache.charge <= MAX_CHARGE);
        cache.begin(ryne_parser::Source::new("").id().document_id, 2.);
        assert!(cache.entries.is_empty());
    }
}
