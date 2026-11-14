//! Source-preserving parsing and editing for hybrid Markdown editors.
//!
//! All positions are UTF-8 byte offsets. Editing APIs require a snapshot identity.
//!
//! ```
//! use ryne_parser::{Edit, Source};
//! let source = Source::new("**world**");
//! let applied = source.apply(source.id(), vec![Edit {
//!     range: 2..7, replacement: "Rust".into(),
//! }]).unwrap();
//! assert_eq!(applied.source.text(), "**Rust**");
//! let restored = applied.source.commit(applied.inverse).unwrap();
//! assert_eq!(restored.source.text(), source.text());
//! ```
use pulldown_cmark::{Options, Parser};
use std::{
    ops::Range,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

mod planner;
pub mod profile;
mod projection;
mod render;
mod syntax;
pub use planner::*;
pub use projection::*;
pub use render::HtmlPolicy;
pub use syntax::*;

pub fn options() -> Options {
    // ENABLE_GFM also enables non-baseline alerts; deliberately excluded.
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
}

/// Diagnostic backend output, without the compatibility renderer.
pub fn backend_html(source: &str) -> String {
    let mut output = String::new();
    pulldown_cmark::html::push_html(&mut output, Parser::new_ext(source, options()));
    output
}

/// GFM export through the owned semantic model, with tagfilter enabled.
pub fn html(source: &str) -> String {
    ParseSnapshot::new(Source::new(source)).html(HtmlPolicy::Export)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    StaleSnapshot,
    StaleProjection,
    InvalidRange,
    Overlap,
    SplitCrlf,
    SplitGrapheme,
    NoChange,
}

/// Process-local document identity. Cloning a Source preserves its document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DocumentId(u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SnapshotId {
    pub document_id: DocumentId,
    pub revision: u64,
}
static NEXT_DOCUMENT: AtomicU64 = AtomicU64::new(1);

/// Immutable source snapshot. Revisions are unique even when old snapshots branch.
#[derive(Clone, Debug)]
pub struct Source {
    id: SnapshotId,
    text: Arc<str>,
    next_revision: Arc<AtomicU64>,
}
impl Source {
    pub fn new(text: &str) -> Self {
        Self {
            id: SnapshotId {
                document_id: DocumentId(NEXT_DOCUMENT.fetch_add(1, Ordering::Relaxed)),
                revision: 0,
            },
            text: Arc::from(text),
            next_revision: Arc::new(AtomicU64::new(1)),
        }
    }
    pub fn id(&self) -> SnapshotId {
        self.id
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn revision(&self) -> u64 {
        self.id.revision
    }
    pub fn check(&self, base: SnapshotId) -> Result<(), Error> {
        if base == self.id {
            Ok(())
        } else {
            Err(Error::StaleSnapshot)
        }
    }
    pub fn commit(&self, transaction: Transaction) -> Result<Applied, Error> {
        self.apply(transaction.base, transaction.edits)
    }
    /// Apply ranges in one old snapshot atomically; reject invalid UTF-8/CRLF
    /// endpoints, overlaps, stale identities, and byte-identical changes.
    pub fn apply(&self, base: SnapshotId, mut edits: Vec<Edit>) -> Result<Applied, Error> {
        let _profile = profile::Scope::new("source_apply");
        self.check(base)?;
        edits.sort_by_key(|e| (e.range.start, e.range.end));
        for (i, edit) in edits.iter().enumerate() {
            validate(&self.text, &edit.range)?;
            if i > 0 {
                let previous = &edits[i - 1].range;
                if previous.end > edit.range.start
                    || (previous.end == edit.range.start
                        && (previous.is_empty() || edit.range.is_empty()))
                {
                    return Err(Error::Overlap);
                }
            }
        }
        let mut next = self.text.to_string();
        for edit in edits.iter().rev() {
            next.replace_range(edit.range.clone(), &edit.replacement);
        }
        if next == *self.text {
            return Err(Error::NoChange);
        }
        // Pair new ranges with old ranges, then absorb unchanged CR/LF bytes.
        // Merging before and after expansion prevents overlapping inverse edits.
        let mut pairs: Vec<(Range<usize>, Range<usize>)> = Vec::new();
        let mut delta: isize = 0;
        for edit in &edits {
            let start = edit
                .range
                .start
                .checked_add_signed(delta)
                .ok_or(Error::InvalidRange)?;
            let new = start..start + edit.replacement.len();
            if let Some((previous, old)) = pairs.last_mut().filter(|(p, _)| p.end == new.start) {
                previous.end = new.end;
                old.end = edit.range.end;
            } else {
                pairs.push((new, edit.range.clone()));
            }
            delta += edit.replacement.len() as isize - edit.range.len() as isize;
        }
        for (new, old) in &mut pairs {
            if split_crlf(&next, new.start) {
                new.start -= 1;
                old.start -= 1;
            }
            if split_crlf(&next, new.end) {
                new.end += 1;
                old.end += 1;
            }
        }
        let mut merged: Vec<(Range<usize>, Range<usize>)> = Vec::new();
        for (new, old) in pairs {
            if let Some((previous, previous_old)) =
                merged.last_mut().filter(|(p, _)| p.end >= new.start)
            {
                previous.end = previous.end.max(new.end);
                previous_old.end = previous_old.end.max(old.end);
            } else {
                merged.push((new, old));
            }
        }
        let inverse = merged
            .into_iter()
            .map(|(range, old)| {
                validate(&next, &range)?;
                Ok(Edit {
                    range,
                    replacement: self.text[old].to_owned(),
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let source = Self {
            id: SnapshotId {
                document_id: self.id.document_id,
                revision: self.next_revision.fetch_add(1, Ordering::Relaxed),
            },
            text: Arc::from(next),
            next_revision: self.next_revision.clone(),
        };
        Ok(Applied {
            inverse: Transaction {
                base: source.id,
                edits: inverse,
            },
            positions: PositionMap {
                from: self.id,
                to: source.id,
                old: self.text.clone(),
                new: source.text.clone(),
                edits,
            },
            source,
        })
    }
}
fn split_crlf(text: &str, p: usize) -> bool {
    p > 0 && p < text.len() && &text.as_bytes()[p - 1..=p] == b"\r\n"
}
pub(crate) fn validate(text: &str, range: &Range<usize>) -> Result<(), Error> {
    if range.start > range.end
        || range.end > text.len()
        || !text.is_char_boundary(range.start)
        || !text.is_char_boundary(range.end)
    {
        return Err(Error::InvalidRange);
    }
    if split_crlf(text, range.start) || split_crlf(text, range.end) {
        return Err(Error::SplitCrlf);
    }
    Ok(())
}
fn cursor_boundary(text: &str, position: usize, affinity: Affinity) -> Result<usize, Error> {
    if position > text.len() || !text.is_char_boundary(position) {
        return Err(Error::InvalidRange);
    }
    Ok(if split_crlf(text, position) {
        match affinity {
            Affinity::Upstream => position - 1,
            Affinity::Downstream => position + 1,
        }
    } else {
        position
    })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub replacement: String,
}
#[derive(Clone, Debug)]
pub struct Transaction {
    pub base: SnapshotId,
    pub edits: Vec<Edit>,
}
pub struct Applied {
    pub source: Source,
    pub inverse: Transaction,
    pub positions: PositionMap,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Affinity {
    Upstream,
    Downstream,
}

/// Old-to-new byte mapping; map_cursor additionally repairs newly formed CRLF.
pub struct PositionMap {
    from: SnapshotId,
    to: SnapshotId,
    old: Arc<str>,
    new: Arc<str>,
    edits: Vec<Edit>,
}
impl PositionMap {
    /// Transaction-proven unchanged byte intervals, in source order. Inserted
    /// text is excluded even when it happens to equal the original bytes.
    pub fn unchanged_ranges(&self) -> impl Iterator<Item = (Range<usize>, Range<usize>)> + '_ {
        let mut old = 0;
        let mut new = 0;
        self.edits
            .iter()
            .map(Some)
            .chain(std::iter::once(None))
            .filter_map(move |edit| {
                let end = edit.map_or(self.old.len(), |e| e.range.start);
                let length = end - old;
                let pair = (old..end, new..new + length);
                if let Some(edit) = edit {
                    old = edit.range.end;
                    new += length + edit.replacement.len();
                }
                (length > 0).then_some(pair)
            })
    }
    pub fn from(&self) -> SnapshotId {
        self.from
    }
    pub fn to(&self) -> SnapshotId {
        self.to
    }
    pub fn map_bytes(
        &self,
        base: SnapshotId,
        offset: usize,
        affinity: Affinity,
    ) -> Result<usize, Error> {
        if base != self.from {
            return Err(Error::StaleSnapshot);
        }
        if offset > self.old.len() || !self.old.is_char_boundary(offset) {
            return Err(Error::InvalidRange);
        }
        Ok(self.map_raw(offset, affinity))
    }
    fn map_raw(&self, offset: usize, affinity: Affinity) -> usize {
        let mut delta: isize = 0;
        for e in &self.edits {
            if offset < e.range.start {
                break;
            }
            if offset <= e.range.end {
                if offset == e.range.end && !e.range.is_empty() {
                    delta += e.replacement.len() as isize - e.range.len() as isize;
                    continue;
                }
                return e.range.start.checked_add_signed(delta).unwrap()
                    + match affinity {
                        Affinity::Upstream => 0,
                        Affinity::Downstream => e.replacement.len(),
                    };
            }
            delta += e.replacement.len() as isize - e.range.len() as isize;
        }
        offset.checked_add_signed(delta).unwrap()
    }
    pub fn map_cursor(
        &self,
        source: &Source,
        target: &Source,
        offset: usize,
        affinity: Affinity,
    ) -> Result<usize, Error> {
        source.check(self.from)?;
        target.check(self.to)?;
        validate(source.text(), &(offset..offset))?;
        cursor_boundary(&self.new, self.map_raw(offset, affinity), affinity)
    }
}

/// Legacy raw byte partition. Typed syntax is available in ParseSnapshot.
pub fn partition(source: &str) -> Vec<Range<usize>> {
    let mut boundaries = vec![0, source.len()];
    for (_, range) in Parser::new_ext(source, options()).into_offset_iter() {
        boundaries.extend([range.start, range.end]);
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries
        .windows(2)
        .filter(|w| w[0] < w[1])
        .map(|w| w[0]..w[1])
        .collect()
}
pub fn serialize(source: &str, tokens: &[Range<usize>]) -> String {
    let mut result = String::new();
    let mut end = 0;
    for token in tokens {
        assert_eq!(token.start, end);
        result.push_str(&source[token.clone()]);
        end = token.end;
    }
    assert_eq!(end, source.len());
    result
}
pub fn utf16_to_utf8(text: &str, units: usize) -> Result<usize, Error> {
    let mut position = 0;
    for (byte, c) in text.char_indices() {
        if units == position {
            return Ok(byte);
        }
        position += c.len_utf16();
        if position > units {
            return Err(Error::InvalidRange);
        }
    }
    if units == position {
        Ok(text.len())
    } else {
        Err(Error::InvalidRange)
    }
}
#[derive(Clone, Debug)]
pub struct Session {
    pub base: SnapshotId,
    pub range: Range<usize>,
}
impl Session {
    pub fn new(source: &Source, range: Range<usize>) -> Result<Self, Error> {
        validate(source.text(), &range)?;
        Ok(Self {
            base: source.id(),
            range,
        })
    }
    pub fn transform(&mut self, map: &PositionMap) -> Result<(), Error> {
        if self.base != map.from {
            return Err(Error::StaleSnapshot);
        }
        validate(&map.old, &self.range)?;
        let start = cursor_boundary(
            &map.new,
            map.map_raw(self.range.start, Affinity::Upstream),
            Affinity::Upstream,
        )?;
        let end = cursor_boundary(
            &map.new,
            map.map_raw(self.range.end, Affinity::Downstream),
            Affinity::Downstream,
        )?;
        self.range = start..end;
        self.base = map.to;
        Ok(())
    }
    pub fn visible<'a>(&self, source: &'a Source) -> Result<&'a str, Error> {
        source.check(self.base)?;
        validate(source.text(), &self.range)?;
        Ok(&source.text()[self.range.clone()])
    }
}
/// Preedit is separate from source; only commit creates a transaction.
pub struct Composition {
    pub base: SnapshotId,
    pub range: Range<usize>,
    pub preedit: String,
}
impl Composition {
    pub fn new(source: &Source, range: Range<usize>) -> Result<Self, Error> {
        validate(source.text(), &range)?;
        Ok(Self {
            base: source.id(),
            range,
            preedit: String::new(),
        })
    }
    pub fn update(&mut self, value: &str) {
        self.preedit = value.to_owned();
    }
    pub fn commit(self, source: &Source, final_text: &str) -> Result<Applied, Error> {
        source.apply(
            self.base,
            vec![Edit {
                range: self.range,
                replacement: final_text.to_owned(),
            }],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_segmentation::UnicodeSegmentation;
    fn edit(range: Range<usize>, text: &str) -> Edit {
        Edit {
            range,
            replacement: text.into(),
        }
    }

    fn apply_new(text: &str, edits: Vec<Edit>) -> Result<Applied, Error> {
        let source = Source::new(text);
        source.apply(source.id(), edits)
    }
    #[test]
    fn source_bytes_survive_partition() {
        for s in [
            "",
            "\u{feff}> **中**\r\n\t末尾  \r\0",
            "| a | b |\n| - | - |\n| c | d | ignored |",
            "[x]: /one\n\n[x]",
        ] {
            assert_eq!(serialize(s, &partition(s)).as_bytes(), s.as_bytes());
        }
    }
    #[test]
    fn atomic_multi_edit_and_inverse() {
        let s = Source::new("甲 abc 乙 def");
        let a = s
            .apply(s.id(), vec![edit(4..7, "中文"), edit(12..15, "Z")])
            .unwrap();
        assert_eq!(&*a.source.text, "甲 中文 乙 Z");
        let b = a.source.commit(a.inverse).unwrap();
        assert_eq!(b.source.text, s.text);
        assert_eq!(b.source.revision(), 2);
    }
    #[test]
    fn invalid_utf8_boundary_is_rejected() {
        let s = Source::new("中x");
        assert!(matches!(
            s.apply(s.id(), vec![edit(1..2, "")]),
            Err(Error::InvalidRange)
        ));
        assert_eq!(&*s.text, "中x");
    }
    #[test]
    fn stale_source_is_rejected() {
        assert!(matches!(
            Source::new("abc").apply(Source::new("abc").id(), vec![edit(0..1, "x")]),
            Err(Error::StaleSnapshot)
        ));
    }
    #[test]
    fn overlapping_edits_are_rejected() {
        assert!(matches!(
            apply_new("abcd", vec![edit(0..2, "x"), edit(1..3, "y")]),
            Err(Error::Overlap)
        ));
    }
    #[test]
    fn crlf_is_atomic() {
        let s = Source::new("a\r\nb");
        assert!(matches!(
            s.apply(s.id(), vec![edit(1..2, "")]),
            Err(Error::SplitCrlf)
        ));
        assert_eq!(
            &*s.apply(s.id(), vec![edit(1..3, "")]).unwrap().source.text,
            "ab"
        );
    }
    #[test]
    fn insertion_affinity() {
        let a = apply_new("ab", vec![edit(1..1, "中")]).unwrap();
        assert_eq!(
            a.positions
                .map_bytes(a.positions.from, 1, Affinity::Upstream)
                .unwrap(),
            1
        );
        assert_eq!(
            a.positions
                .map_bytes(a.positions.from, 1, Affinity::Downstream)
                .unwrap(),
            4
        );
        assert_eq!(
            a.positions
                .map_bytes(a.positions.from, 2, Affinity::Downstream)
                .unwrap(),
            5
        );
    }
    #[test]
    fn adjacent_replacement_boundary_maps_once() {
        let a = apply_new("abcd", vec![edit(0..2, "X"), edit(2..4, "YZ")]).unwrap();
        assert_eq!(
            a.positions
                .map_bytes(a.positions.from, 2, Affinity::Upstream)
                .unwrap(),
            1
        );
        assert_eq!(
            a.positions
                .map_bytes(a.positions.from, 2, Affinity::Downstream)
                .unwrap(),
            3
        );
    }
    #[test]
    fn adjacent_deletions_have_unambiguous_inverse() {
        let s = Source::new("abcd");
        let a = s
            .apply(s.id(), vec![edit(0..2, ""), edit(2..4, "")])
            .unwrap();
        assert_eq!(a.inverse.edits.len(), 1);
        assert_eq!(a.source.commit(a.inverse).unwrap().source.text, s.text);
    }
    #[test]
    fn no_op_does_not_create_a_revision() {
        let s = Source::new("ab");
        assert!(matches!(
            s.apply(s.id(), vec![edit(0..1, "a")]),
            Err(Error::NoChange)
        ));
        assert_eq!(s.revision(), 0);
    }
    #[test]
    fn direct_projection_mapping() {
        let p = preview_inline("a **中** c");
        assert_eq!(p.text, "a 中 c");
        assert_eq!(p.resolve(2, Affinity::Downstream), Some(4));
        assert_eq!(p.resolve(5, Affinity::Upstream), Some(7));
    }
    #[test]
    fn entity_mapping_is_atomic() {
        let p = preview_inline("&fjlig;");
        assert_eq!(p.text, "fj");
        assert_eq!(p.resolve(1, Affinity::Upstream), Some(0));
        assert_eq!(p.resolve(1, Affinity::Downstream), Some(7));
    }
    #[test]
    fn semantic_entity_partial_delete_with_inverse() {
        let s = Source::new("&fjlig;");
        let decoded = preview_inline(&s.text).text;
        let remainder = &decoded[1..];
        let a = s
            .apply(s.id(), vec![edit(0..s.text.len(), remainder)])
            .unwrap();
        assert_eq!(html(&a.source.text), "<p>j</p>\n");
        assert_eq!(a.source.commit(a.inverse).unwrap().source.text, s.text);
    }
    #[test]
    fn session_survives_strong_to_rule_to_strong() {
        let s = Source::new("**b**");
        let mut session = Session::new(&s, 0..5).unwrap();
        let a = s.apply(s.id(), vec![edit(2..3, "")]).unwrap();
        session.transform(&a.positions).unwrap();
        assert_eq!(html(&a.source.text), "<hr />\n");
        assert_eq!(session.visible(&a.source).unwrap(), "****");
        let b = a
            .source
            .apply(a.source.id(), vec![edit(2..2, "x")])
            .unwrap();
        session.transform(&b.positions).unwrap();
        assert_eq!(session.visible(&b.source).unwrap(), "**x**");
        assert_eq!(html(&b.source.text), "<p><strong>x</strong></p>\n");
    }
    #[test]
    fn composed_grapheme_crosses_nodes() {
        let p = preview_inline("*e*\u{301}");
        assert_eq!(p.text.graphemes(true).count(), 1);
    }
    #[test]
    fn utf16_surrogate_middle_is_rejected() {
        assert_eq!(utf16_to_utf8("a😀中", 3), Ok(5));
        assert_eq!(utf16_to_utf8("a😀中", 2), Err(Error::InvalidRange));
    }
    #[test]
    fn composition_commit_once_cancel_no_change() {
        let s = Source::new("ab");
        let mut c = Composition {
            base: s.id(),
            range: 1..1,
            preedit: String::new(),
        };
        c.update("n");
        c.update("ni");
        c.update("你");
        assert_eq!(s.revision(), 0);
        assert_eq!(&*s.text, "ab");
        let a = c.commit(&s, "你").unwrap();
        assert_eq!(a.source.revision(), 1);
        assert_eq!(&*a.source.text, "a你b");
        assert_eq!(a.source.commit(a.inverse).unwrap().source.text, s.text);
        let cancelled = Composition {
            base: s.id(),
            range: 0..0,
            preedit: "中".into(),
        };
        drop(cancelled);
        assert_eq!(s.revision(), 0);
        assert_eq!(&*s.text, "ab");
    }
    #[test]
    fn long_code_delimiter_is_not_truncated() {
        let ticks = "`".repeat(256);
        assert_eq!(
            preview_inline(&format!("before {ticks}x{ticks}")).text,
            "before x"
        );
    }
    #[test]
    fn task_state_changes_only_one_byte() {
        let s = Source::new("- [ ] 中文\r\n");
        let a = s.apply(s.id(), vec![edit(3..4, "x")]).unwrap();
        assert_eq!(&*a.source.text, "- [x] 中文\r\n");
        assert_eq!(a.source.commit(a.inverse).unwrap().source.text, s.text);
    }
    #[test]
    fn unresolved_reference_reparses() {
        let s = Source::new("[entry][key]\n");
        assert!(!html(&s.text).contains("<a "));
        let a = s
            .apply(
                s.id(),
                vec![edit(s.text.len()..s.text.len(), "\n[key]: /url\n")],
            )
            .unwrap();
        assert!(html(&a.source.text).contains("href=\"/url\""));
        assert_eq!(
            html(&a.source.commit(a.inverse).unwrap().source.text),
            html(&s.text)
        );
    }
    #[test]
    fn deterministic_edit_sequences_roundtrip() {
        let mut s = Source::new("中 **ab**\n");
        let mut seed: u64 = 42;
        for i in 0..500 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let mut boundaries: Vec<usize> = s.text.char_indices().map(|(i, _)| i).collect();
            boundaries.push(s.text.len());
            let start = (seed as usize) % boundaries.len();
            let end = (start + ((seed >> 32) as usize % 3)).min(boundaries.len() - 1);
            let value = ["", "中文", "**", "&amp;", "\n", "e\u{301}", "😀"][i % 7];
            let a = match s.apply(
                s.id(),
                vec![edit(boundaries[start]..boundaries[end], value)],
            ) {
                Ok(a) => a,
                Err(Error::NoChange) => continue,
                Err(e) => panic!("unexpected edit error: {e:?}"),
            };
            assert_eq!(a.source.commit(a.inverse).unwrap().source.text, s.text);
            assert_eq!(
                serialize(&a.source.text, &partition(&a.source.text)),
                &*a.source.text
            );
            s = a.source;
        }
    }
}
