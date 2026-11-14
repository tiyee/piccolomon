//! Source-owned semantic identities. This is a disposable full-parse directory,
//! not an incremental parser or an independently editable block document.
use crate::{ParseSnapshot, RangeIndex};
use ryne_parser::{Affinity, Error, NodeKind, PositionMap, SnapshotId, Source};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(test)]
#[path = "directory_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(u64);
impl BlockId {
    fn fresh() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BlockRole {
    Paragraph,
    Formula,
    Heading,
    Quote,
    Code,
    Html,
    List,
    Item,
    Table,
    TableHead,
    TableRow,
    TableCell,
    Rule,
    Source,
}
impl BlockRole {
    fn of(kind: &NodeKind) -> Option<Self> {
        Some(match kind {
            NodeKind::Paragraph => Self::Paragraph,
            NodeKind::DisplayMath(_) => Self::Formula,
            NodeKind::Heading(_) => Self::Heading,
            NodeKind::BlockQuote => Self::Quote,
            NodeKind::CodeBlock(_) => Self::Code,
            NodeKind::HtmlBlock => Self::Html,
            NodeKind::List(_) => Self::List,
            NodeKind::Item => Self::Item,
            NodeKind::Table(_) => Self::Table,
            NodeKind::TableHead => Self::TableHead,
            NodeKind::TableRow => Self::TableRow,
            NodeKind::TableCell => Self::TableCell,
            NodeKind::Rule => Self::Rule,
            NodeKind::SourceBlock => Self::Source,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct BlockEntry {
    pub id: BlockId,
    pub role: BlockRole,
    /// Raw-source byte envelope, including nested children. Do not sum these.
    pub range: Range<usize>,
    pub parent: Option<BlockId>,
    pub depth: usize,
    pub content_revision: u64,
    /// Conservative: every semantic rebuild invalidates contextual dependencies.
    pub semantic_revision: u64,
}

/// All raw bytes have exactly one owner in this partition, including gaps,
/// unsupported syntax and container prefixes. Zero-width entities live in entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourcePiece {
    pub range: Range<usize>,
}

pub struct BlockDirectory {
    source: Source,
    entries: Vec<BlockEntry>,
    ids: BTreeMap<BlockId, usize>,
    ranges: RangeIndex,
    pieces: Vec<SourcePiece>,
}
impl BlockDirectory {
    pub fn new(parse: &ParseSnapshot) -> Self {
        Self::build(parse, None)
    }
    /// No state is changed until the caller publishes the corresponding source.
    pub fn updated(&self, parse: &ParseSnapshot, positions: &PositionMap) -> Result<Self, Error> {
        self.source.check(positions.from())?;
        parse.source().check(positions.to())?;
        Ok(Self::build(parse, Some((self, positions))))
    }
    fn build(parse: &ParseSnapshot, previous: Option<(&Self, &PositionMap)>) -> Self {
        let _profile = ryne_parser::profile::Scope::new("block_directory_build");
        let source = parse.source().clone();
        // Bare-CR normalization preserves byte offsets, but not Source identity.
        let semantic = parse.semantic();
        let mut descriptors = Vec::new();
        let mut ancestors = vec![None; semantic.nodes.len()];
        let mut depths = Vec::new();
        for (node_id, node) in semantic.nodes.iter().enumerate() {
            let parent = node.parent.and_then(|p| ancestors[p]);
            ancestors[node_id] = parent;
            if matches!(node.kind, NodeKind::DisplayMath(_))
                && !crate::math::standalone(semantic, node)
            {
                continue;
            }
            if let Some(role) = BlockRole::of(&node.kind) {
                let depth = parent.map_or(0, |p: usize| depths[p] + 1);
                ancestors[node_id] = Some(descriptors.len());
                depths.push(depth);
                descriptors.push((role, node.envelope.clone(), parent, depth));
            }
        }
        let mut exact = BTreeMap::new();
        let mut starts: BTreeMap<_, Vec<usize>> = BTreeMap::new();
        if let Some((old, positions)) = previous {
            let unchanged: Vec<_> = positions.unchanged_ranges().collect();
            for (i, entry) in old.entries.iter().enumerate() {
                let slot = unchanged.partition_point(|(a, _)| a.end <= entry.range.start);
                if let Some((a, b)) = unchanged
                    .get(slot)
                    .filter(|(a, _)| a.start <= entry.range.start && entry.range.start < a.end)
                {
                    let start = b.start + entry.range.start - a.start;
                    starts
                        .entry((start, entry.role, entry.depth))
                        .or_default()
                        .push(i);
                    if entry.range.end <= a.end {
                        exact.insert(
                            (start, start + entry.range.len(), entry.role, entry.depth),
                            i,
                        );
                    }
                    // Reserve exact matches before considering an insertion
                    // at the start. A duplicate must not steal the old ID.
                    let (old_end, new_end) = slot
                        .checked_sub(1)
                        .map_or((0, 0), |p| (unchanged[p].0.end, unchanged[p].1.end));
                    if a.start == entry.range.start && old_end == a.start && new_end != start {
                        starts
                            .entry((new_end, entry.role, entry.depth))
                            .or_default()
                            .push(i);
                    }
                } else if let Some((a, _)) = unchanged.get(slot)
                    && a.start < entry.range.end
                    && old.source.text()[a.start..a.end.min(entry.range.end)]
                        .chars()
                        .any(|ch| !ch.is_whitespace())
                {
                    // Replacing the leading character is still an edit of the
                    // block if actual body bytes survive. A wholesale replacement
                    // with only a trailing newline left gets a fresh identity.
                    let start = slot.checked_sub(1).map_or(0, |p| unchanged[p].1.end);
                    starts
                        .entry((start, entry.role, entry.depth))
                        .or_default()
                        .push(i);
                }
            }
        }
        let reserved: BTreeSet<_> = descriptors
            .iter()
            .filter_map(|(role, range, _, depth)| {
                exact.get(&(range.start, range.end, *role, *depth)).copied()
            })
            .collect();
        let mut used = BTreeSet::new();
        let mut entries: Vec<BlockEntry> = Vec::with_capacity(descriptors.len());
        for (role, range, parent_index, depth) in descriptors {
            let parent = parent_index.map(|p| entries[p].id);
            let candidate = previous.and_then(|(old, _)| {
                let compatible = |i: &usize| !used.contains(i) && old.entries[*i].parent == parent;
                exact
                    .get(&(range.start, range.end, role, depth))
                    .filter(|i| compatible(i))
                    .copied()
                    .or_else(|| {
                        starts
                            .get(&(range.start, role, depth))?
                            .iter()
                            .find(|i| !reserved.contains(i) && compatible(i))
                            .copied()
                    })
                    .map(|i| {
                        used.insert(i);
                        &old.entries[i]
                    })
            });
            let unchanged = candidate.is_some_and(|old| {
                previous.is_some_and(|(directory, _)| {
                    exact
                        .get(&(range.start, range.end, role, depth))
                        .is_some_and(|&i| directory.entries[i].id == old.id)
                })
            });
            entries.push(BlockEntry {
                id: candidate.map_or_else(BlockId::fresh, |e| e.id),
                role,
                range,
                parent,
                depth,
                content_revision: candidate
                    .map_or(0, |e| e.content_revision + u64::from(!unchanged)),
                semantic_revision: source.revision(),
            });
        }
        let mut boundaries = vec![0, source.text().len()];
        boundaries.extend(entries.iter().flat_map(|e| [e.range.start, e.range.end]));
        // Envelopes may end between CR and LF; physical byte pieces may not.
        boundaries.retain(|&p| {
            !(p > 0
                && source.text().as_bytes().get(p - 1) == Some(&b'\r')
                && source.text().as_bytes().get(p) == Some(&b'\n'))
        });
        boundaries.sort_unstable();
        boundaries.dedup();
        let pieces = boundaries
            .windows(2)
            .map(|p| SourcePiece { range: p[0]..p[1] })
            .collect();
        Self {
            ranges: RangeIndex::new(
                entries
                    .iter()
                    .enumerate()
                    .map(|(i, e)| (e.range.clone(), i))
                    .collect(),
            ),
            ids: entries.iter().enumerate().map(|(i, e)| (e.id, i)).collect(),
            source,
            entries,
            pieces,
        }
    }
    pub fn snapshot(&self) -> SnapshotId {
        self.source.id()
    }
    pub fn entries(&self, snapshot: SnapshotId) -> Result<&[BlockEntry], Error> {
        self.source.check(snapshot)?;
        Ok(&self.entries)
    }
    pub fn pieces(&self, snapshot: SnapshotId) -> Result<&[SourcePiece], Error> {
        self.source.check(snapshot)?;
        Ok(&self.pieces)
    }
    pub fn get(&self, snapshot: SnapshotId, id: BlockId) -> Result<Option<&BlockEntry>, Error> {
        self.source.check(snapshot)?;
        Ok(self.ids.get(&id).map(|&i| &self.entries[i]))
    }
    pub fn at(
        &self,
        snapshot: SnapshotId,
        offset: usize,
        affinity: Affinity,
    ) -> Result<Vec<&BlockEntry>, Error> {
        self.source.check(snapshot)?;
        if !self.source.text().is_char_boundary(offset) {
            return Err(Error::InvalidRange);
        }
        Ok(self
            .ranges
            .at(offset)
            .into_iter()
            .map(|i| &self.entries[i])
            .filter(|entry| {
                entry.range.is_empty()
                    || match affinity {
                        Affinity::Upstream => {
                            entry.range.start < offset && offset <= entry.range.end
                        }
                        Affinity::Downstream => {
                            entry.range.start <= offset && offset < entry.range.end
                        }
                    }
            })
            .collect())
    }
}
