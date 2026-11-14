//! Source ownership and atomic history. This crate knows no syntax, projection or GUI.
pub use ryne_parser::{Affinity, SnapshotId as DocumentKey};
use ryne_parser::{Applied, Edit, Error, Source, Transaction};
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceByte(pub usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayByte(pub usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Utf16Offset(pub usize);
#[derive(Clone, Debug)]
pub struct SourceRange {
    pub document: DocumentKey,
    pub bytes: Range<SourceByte>,
}

impl SourceRange {
    pub fn new(document: DocumentKey, bytes: Range<usize>) -> Self {
        Self {
            document,
            bytes: SourceByte(bytes.start)..SourceByte(bytes.end),
        }
    }
    pub fn validate(&self, source: &Source) -> Result<Range<usize>, Error> {
        source.check(self.document)?;
        let bytes = self.bytes.start.0..self.bytes.end.0;
        ryne_parser::Session::new(source, bytes.clone())?;
        Ok(bytes)
    }
}
pub struct SourceEdit {
    pub range: SourceRange,
    pub replacement: String,
}
impl SourceEdit {
    pub fn from_parser(document: DocumentKey, edit: Edit) -> Self {
        Self {
            range: SourceRange {
                document,
                bytes: SourceByte(edit.range.start)..SourceByte(edit.range.end),
            },
            replacement: edit.replacement,
        }
    }
}
pub fn utf16_to_source(text: &str, offset: Utf16Offset) -> Result<SourceByte, Error> {
    ryne_parser::utf16_to_utf8(text, offset.0).map(SourceByte)
}

pub struct PreparedEdit {
    base: DocumentKey,
    applied: Applied,
    // Only the history stack that prepared this candidate may publish it.
    history: Option<(bool, Arc<()>)>,
}
impl PreparedEdit {
    pub fn prepare_plan(source: &Source, edits: Vec<SourceEdit>) -> Result<Self, Error> {
        let edits = edits
            .into_iter()
            .map(|edit| {
                Ok(Edit {
                    range: edit.range.validate(source)?,
                    replacement: edit.replacement,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Self::prepare(source, edits)
    }
    pub fn prepare(source: &Source, edits: Vec<Edit>) -> Result<Self, Error> {
        Ok(Self {
            base: source.id(),
            applied: source.apply(source.id(), edits)?,
            history: None,
        })
    }
    pub fn base(&self) -> DocumentKey {
        self.base
    }
    pub fn applied(&self) -> &Applied {
        &self.applied
    }
}
struct History<C> {
    transaction: Transaction,
    checkpoint: C,
    identity: Arc<()>,
}
/// Owned history allocations, excluding allocator metadata, identity tokens and shared source snapshots.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HistoryMemory {
    pub undo_entries: usize,
    pub redo_entries: usize,
    pub reserved_bytes: usize,
}
pub struct DocumentCore<C> {
    source: Source,
    saved: Option<Source>,
    dirty: bool,
    undo: Vec<History<C>>,
    redo: Vec<History<C>>,
}
impl<C: Clone> DocumentCore<C> {
    pub fn new(text: &str) -> Self {
        let source = Source::new(text);
        Self {
            saved: Some(source.clone()),
            source,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }
    /// A recovered document has no saved baseline, even when the draft is empty.
    pub fn recovered(text: &str) -> Self {
        let mut core = Self::new(text);
        core.saved = None;
        core.dirty = true;
        core
    }
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn dirty(&self) -> bool {
        self.dirty
    }
    fn update_dirty(&mut self) {
        self.dirty = self.saved.as_ref().is_none_or(|saved| {
            self.source.id() != saved.id() && self.source.text() != saved.text()
        });
    }
    pub fn history_len(&self, redo: bool) -> usize {
        if redo {
            self.redo.len()
        } else {
            self.undo.len()
        }
    }
    /// On-demand accounting. The owner supplies checkpoint heap allocations.
    pub fn history_memory(&self, checkpoint_heap: impl Fn(&C) -> usize) -> HistoryMemory {
        let reserved_bytes = (self.undo.capacity() + self.redo.capacity())
            * std::mem::size_of::<History<C>>()
            + self
                .undo
                .iter()
                .chain(&self.redo)
                .map(|entry| {
                    entry.transaction.edits.capacity() * std::mem::size_of::<Edit>()
                        + entry
                            .transaction
                            .edits
                            .iter()
                            .map(|edit| edit.replacement.capacity())
                            .sum::<usize>()
                        + checkpoint_heap(&entry.checkpoint)
                })
                .sum::<usize>();
        HistoryMemory {
            undo_entries: self.undo.len(),
            redo_entries: self.redo.len(),
            reserved_bytes,
        }
    }
    pub fn publish(&mut self, edit: PreparedEdit, checkpoint: C) -> Result<(), Error> {
        self.source.check(edit.base)?;
        if edit.history.is_some() {
            return Err(Error::StaleSnapshot);
        }
        self.undo.push(History {
            transaction: edit.applied.inverse,
            checkpoint,
            identity: Arc::new(()),
        });
        self.redo.clear();
        self.source = edit.applied.source;
        self.update_dirty();
        Ok(())
    }
    /// Prepare first; a failed inverse leaves both stacks and the current source intact.
    pub fn prepare_history(&self, redo: bool) -> Result<Option<(PreparedEdit, C)>, Error> {
        let Some(entry) = (if redo { &self.redo } else { &self.undo }).last() else {
            return Ok(None);
        };
        let mut prepared = PreparedEdit::prepare(&self.source, entry.transaction.edits.clone())?;
        prepared.history = Some((redo, Arc::clone(&entry.identity)));
        Ok(Some((prepared, entry.checkpoint.clone())))
    }
    pub fn publish_history(
        &mut self,
        edit: PreparedEdit,
        redo: bool,
        checkpoint: C,
    ) -> Result<(), Error> {
        self.source.check(edit.base)?;
        let stack = if redo { &mut self.redo } else { &mut self.undo };
        if !stack.last().is_some_and(|entry| {
            edit.history.as_ref().is_some_and(|(direction, identity)| {
                *direction == redo && Arc::ptr_eq(identity, &entry.identity)
            })
        }) {
            return Err(Error::StaleSnapshot);
        }
        stack.pop();
        (if redo { &mut self.undo } else { &mut self.redo }).push(History {
            transaction: edit.applied.inverse,
            checkpoint,
            identity: Arc::new(()),
        });
        self.source = edit.applied.source;
        self.update_dirty();
        Ok(())
    }
    /// The receipt binds the actual written snapshot, even when new editing has happened.
    pub fn mark_saved(&mut self, snapshot: &Source) -> Result<(), Error> {
        if snapshot.id().document_id != self.source.id().document_id {
            return Err(Error::StaleSnapshot);
        }
        self.saved = Some(snapshot.clone());
        self.update_dirty();
        Ok(())
    }
}
#[cfg(test)]
mod tests;
