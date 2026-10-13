//! A single-range input stays in original-document coordinates through every rule.
use ryne_core::{DocumentKey, SourceByte, SourceRange};
use std::ops::Range;
#[derive(Clone, Copy, Debug)]
pub enum InputOrigin {
    Keyboard,
    Ime,
    Paste,
}
pub struct InputDraft {
    pub base: DocumentKey,
    pub range: SourceRange,
    pub text: String,
    pub caret: usize,
    pub closers: Vec<Range<usize>>,
    pub origin: InputOrigin,
}
impl InputDraft {
    pub fn new(base: DocumentKey, range: Range<usize>, text: &str, origin: InputOrigin) -> Self {
        Self {
            base,
            range: SourceRange {
                document: base,
                bytes: SourceByte(range.start)..SourceByte(range.end),
            },
            text: text.into(),
            caret: text.len(),
            closers: Vec::new(),
            origin,
        }
    }
    pub fn normalize(&mut self, text: String) {
        self.caret = text.len();
        self.text = text;
        self.closers.clear();
    }
    pub fn complete(
        &mut self,
        text: String,
        caret: usize,
        closing: Range<usize>,
    ) -> Result<(), String> {
        if caret > text.len()
            || !text.is_char_boundary(caret)
            || text.get(closing.clone()).is_none()
        {
            return Err("invalid completion coordinates".into());
        }
        self.text = text;
        self.caret = caret;
        self.closers = vec![closing];
        Ok(())
    }
    pub fn bytes(&self) -> Range<usize> {
        self.range.bytes.start.0..self.range.bytes.end.0
    }
}
