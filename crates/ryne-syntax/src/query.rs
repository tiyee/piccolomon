//! Read-only semantic facade. Keeps backend parsing and container lookup here.
use crate::ParseSnapshot;
use crate::{block, code_block, list, table};
use ryne_parser::SnapshotId;

pub struct SemanticQuery<'a> {
    parse: &'a ParseSnapshot,
}
impl<'a> SemanticQuery<'a> {
    pub fn new(parse: &'a ParseSnapshot) -> Self {
        Self { parse }
    }
    pub fn outline(&self) -> &'a [crate::OutlineHeading] {
        self.parse
            .outline
            .get_or_init(|| crate::outline::collect(self.parse))
    }
    pub fn block_at(&self, caret: usize) -> Option<block::Block> {
        block::at(self.parse, caret)
    }
    pub fn insertion_context(
        &self,
        block: &block::Block,
    ) -> Option<(std::ops::Range<usize>, String)> {
        block::insertion_context(self.parse, block)
    }
    pub fn code_blocks(&self) -> &'a [code_block::CodeBlock] {
        code_block::cached(self.parse)
    }
    pub fn code_language_range(
        &self,
        block: &code_block::CodeBlock,
    ) -> Option<std::ops::Range<usize>> {
        code_block::language_range(self.parse, block)
    }
    pub fn tables(&self) -> &'a [table::Table] {
        table::cached(self.parse)
    }
    pub fn table_containing(&self, position: usize) -> Option<&'a table::Table> {
        table::containing(self.parse, position)
    }
    pub fn cell_containing(&self, position: usize) -> Option<&'a table::Cell> {
        table::cell_containing(self.parse, position)
    }
    pub fn list_items(&self) -> &'a [list::Item] {
        list::cached_items(self.parse)
    }
    pub fn list_rows(&self) -> &'a [list::Row] {
        list::cached_rows(self.parse)
    }
    pub fn source_text(
        &self,
        document: SnapshotId,
        range: std::ops::Range<usize>,
    ) -> Option<&'a str> {
        self.parse.source().check(document).ok()?;
        self.parse.source().text().get(range)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ryne_parser::Source;
    #[test]
    fn queries_preserve_container_bytes_and_reject_foreign_ranges() {
        for newline in ["\n", "\r\n", "\r"] {
            let raw = format!("> - first{newline}>   second{newline}{newline}");
            let parse = ParseSnapshot::new(Source::new(&raw));
            let query = SemanticQuery::new(&parse);
            let block = query.block_at(raw.find("first").unwrap()).unwrap();
            let (boundary, _) = query.insertion_context(&block).unwrap();
            assert!(query.source_text(parse.source().id(), boundary).is_some());
            assert!(query.source_text(Source::new(&raw).id(), 0..1).is_none());
            assert!(query
                .source_text(parse.source().id(), 0..raw.len() + 1)
                .is_none());
            assert_eq!(query.list_items().len(), 1);
            assert_eq!(parse.source().text(), raw);
        }
    }
}
