//! One focus result and one menu availability result; both bind the full snapshot.
use editable_markdown_parser::SnapshotId;
use ryne_syntax::{
    block::{self, Block},
    ParseSnapshot,
};
use std::cell::RefCell;

type FocusEntry = (SnapshotId, usize, Option<Block>);
type MenuEntry = (SnapshotId, Block, bool);
#[derive(Default)]
pub(super) struct BlockQueryCache {
    focus: RefCell<Option<FocusEntry>>,
    menu: RefCell<Option<MenuEntry>>,
}
impl BlockQueryCache {
    pub fn focused(&self, parse: &ParseSnapshot, anchor: usize, head: usize) -> Option<Block> {
        let caret = if anchor < head {
            parse
                .source()
                .text()
                .get(..head)?
                .char_indices()
                .next_back()
                .map_or(head, |(p, _)| p)
        } else {
            head
        };
        self.at(parse, caret)
    }
    pub fn at(&self, parse: &ParseSnapshot, caret: usize) -> Option<Block> {
        let id = parse.source().id();
        if let Some((old, position, result)) = &*self.focus.borrow()
            && *old == id
            && *position == caret
        {
            return result.clone();
        }
        let result = block::at(parse, caret);
        *self.focus.borrow_mut() = Some((id, caret, result.clone()));
        result
    }
    pub fn can_insert_above(&self, parse: &ParseSnapshot, block: &Block) -> bool {
        let id = parse.source().id();
        if let Some((old, target, result)) = &*self.menu.borrow()
            && *old == id
            && target == block
        {
            return *result;
        }
        let result = block::can_insert_above(parse, block);
        *self.menu.borrow_mut() = Some((id, block.clone(), result));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use editable_markdown_parser::Source;
    #[test]
    fn direction_changes_with_the_same_head_use_the_actual_selected_character() {
        let cache = BlockQueryCache::default();
        let parse = ParseSnapshot::with_math(Source::new("first\n\nsecond"));
        assert!(cache.focused(&parse, 0, 7).is_none());
        assert_eq!(cache.focused(&parse, 8, 7).unwrap().anchor, 7);
        assert!(cache.focused(&parse, 0, 7).is_none());
        assert_eq!(cache.focused(&parse, 6, 0).unwrap().anchor, 0);
        assert_eq!(cache.focused(&parse, 7, 7).unwrap().anchor, 7);
    }
    #[test]
    fn caches_none_and_keys_new_documents_and_new_versions() {
        let cache = BlockQueryCache::default();
        let parse = ParseSnapshot::with_math(Source::new("# a\n\ntext"));
        assert!(cache.at(&parse, 4).is_none());
        for _ in 0..20 {
            assert!(cache.at(&parse, 4).is_none());
        }
        let original = cache.at(&parse, 2).unwrap();
        assert_eq!(original.kind, block::BlockKind::Heading(1));
        let fresh = ParseSnapshot::with_math(Source::new("plain"));
        assert_eq!(
            cache.at(&fresh, 2).unwrap().kind,
            block::BlockKind::Paragraph
        );
        let edited = parse
            .source()
            .apply(
                parse.source().id(),
                vec![editable_markdown_parser::Edit {
                    range: 0..2,
                    replacement: String::new(),
                }],
            )
            .unwrap();
        let edited = ParseSnapshot::with_math(edited.source);
        assert_eq!(
            cache.at(&edited, 0).unwrap().kind,
            block::BlockKind::Paragraph
        );
        assert_eq!(cache.at(&parse, 2), Some(original));
        let target = cache.at(&parse, 5).unwrap();
        assert!(cache.can_insert_above(&parse, &target));
        assert!(!cache.can_insert_above(&fresh, &target));
    }
}
