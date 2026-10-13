//! Temporary platform text, never a committed document revision or history entry.
use std::ops::Range;
pub struct Composition {
    pub range: Range<usize>,
    pub text: String,
    pub selection: Range<usize>,
}
