//! Parser-backed semantic indexes. No rendering or editor state.
pub mod block;
pub mod code_block;
pub mod list;
pub mod math;
pub mod parser;
mod query;
#[cfg(any(test, feature = "query-metrics"))]
pub mod query_metrics;
pub mod quote;
pub mod table;
pub use query::SemanticQuery;
mod snapshot;
pub use snapshot::{ParseSnapshot, RangeIndex, SemanticIndex};

mod utf16;
pub use utf16::Utf16Index;
