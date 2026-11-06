//! Provenance for the untouched, unsaved built-in guide, independent of its text.
use crate::i18n::Language;
use ryne_parser::SnapshotId;
use ryne_session::EditorSession;

pub(super) struct DefaultDocument {
    snapshot: SnapshotId,
    language: Language,
}

impl DefaultDocument {
    pub fn new(document: &EditorSession, language: Language) -> Self {
        Self {
            snapshot: document.source.id(),
            language,
        }
    }

    pub fn switch(&mut self, document: &mut EditorSession, language: Language) -> bool {
        // Never infer ownership from matching contents, or discard a user's
        // history, in-flight save or temporary IME input. The caller guards drags.
        if self.language == language
            || document.source.id() != self.snapshot
            || document.files.path.is_some()
            || document.files.saving
            || document.composition().is_some()
            || document.dirty()
            || document.history_len(false) != 0
            || document.history_len(true) != 0
        {
            return false;
        }
        let source_mode = document.source_mode;
        *document = document.fork(language.sample());
        document.set_source_mode(source_mode);
        *self = Self::new(document, language);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ryne_extensions::FileStore;
    use ryne_parser::{Alignment, NodeKind, Source};
    use ryne_session::{basic::SourceEditing, SessionServices};
    use ryne_syntax::ParseSnapshot;
    use std::{path::Path, sync::Arc};

    struct Files;
    impl FileStore for Files {
        fn read(&self, _: &Path) -> std::io::Result<String> {
            Ok(Language::En.sample().into())
        }
        fn write(&self, _: &Path, _: &Source) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn document(language: Language) -> EditorSession {
        EditorSession::new(
            language.sample(),
            SessionServices {
                features: Arc::new(SourceEditing),
                files: Arc::new(Files),
            },
        )
    }

    #[test]
    fn every_guide_parses_all_documented_formats() {
        for language in Language::ALL {
            let text = language.sample();
            let parse = ParseSnapshot::with_math(Source::new(text));
            let has = |kind: NodeKind| parse.nodes.iter().any(|node| node.kind == kind);
            for level in 1..=6 {
                assert!(has(NodeKind::Heading(level)), "{language:?}: H{level}");
            }
            assert_eq!(
                parse
                    .nodes
                    .iter()
                    .filter(|n| n.kind == NodeKind::Heading(1))
                    .count(),
                2,
                "{language:?}: ATX and Setext H1"
            );
            for kind in [
                NodeKind::Strong,
                NodeKind::Emphasis,
                NodeKind::Strikethrough,
                NodeKind::List(None),
                NodeKind::List(Some(1)),
                NodeKind::Rule,
                NodeKind::HtmlBlock,
            ] {
                assert!(has(kind.clone()), "{language:?}: {kind:?}");
            }
            assert!(has(NodeKind::BlockQuote));
            assert!(parse
                .nodes
                .iter()
                .any(|n| matches!(n.kind, NodeKind::Task(_))));
            assert!(parse
                .nodes
                .iter()
                .any(|n| matches!(n.kind, NodeKind::Link { .. })));
            assert!(parse
                .nodes
                .iter()
                .any(|n| matches!(n.kind, NodeKind::Image { .. })));
            assert!(parse
                .nodes
                .iter()
                .any(|n| matches!(n.kind, NodeKind::InlineMath(_))));
            assert!(parse
                .nodes
                .iter()
                .any(|n| matches!(n.kind, NodeKind::Code(_))));
            let items = ryne_syntax::list::items(&parse);
            assert!(
                items.iter().any(|item| item.parent.is_some()),
                "{language:?}: nested list"
            );
            let tables = ryne_syntax::table::tables(&parse);
            assert_eq!(tables.len(), 1, "{language:?}: editable table");
            assert_eq!(
                tables[0].align,
                [Alignment::Left, Alignment::Center, Alignment::Right]
            );
            assert!(tables[0]
                .rows
                .iter()
                .flat_map(|row| &row.cells)
                .any(|cell| cell.content.is_empty()));
            let blocks = ryne_syntax::code_block::blocks(&parse);
            assert_eq!(
                blocks.len(),
                5,
                "{language:?}: fences, indentation and nesting"
            );
            for language in ["rust", "python", "json", "mermaid"] {
                assert!(blocks.iter().any(|block| block.language == language));
            }
            assert!(text.contains("  \n"));
            assert!(text.contains("<br>"));
            assert!(text.contains("a\\|b"));
        }
    }

    #[test]
    fn pristine_guide_switches_repeatedly_without_dirty_or_history() {
        let mut doc = document(Language::En);
        let mut guide = DefaultDocument::new(&doc, Language::En);
        let unchanged = doc.source.id();
        assert!(!guide.switch(&mut doc, Language::En));
        assert_eq!(doc.source.id(), unchanged);
        doc.set_source_mode(true);
        for language in Language::ALL {
            doc.select(10, false, false);
            assert!(guide.switch(&mut doc, language));
            assert_eq!(doc.source.text(), language.sample());
            assert!(doc.source_mode);
            assert!(!doc.dirty());
            assert_eq!((doc.history_len(false), doc.history_len(true)), (0, 0));
            assert_eq!(doc.selection(), 0..0);
            assert_eq!(doc.parse.source().id(), doc.source.id());
        }
    }

    #[test]
    fn edits_undo_redo_and_identical_opened_documents_are_preserved() {
        let mut doc = document(Language::En);
        let mut guide = DefaultDocument::new(&doc, Language::En);
        doc.insert_text(0..0, "中😀\r\n").unwrap();
        let edited = doc.source.id();
        let selection = doc.selection();
        assert!(!guide.switch(&mut doc, Language::Ja));
        assert_eq!(doc.source.id(), edited);
        assert_eq!(doc.selection(), selection);
        doc.undo(false);
        assert_eq!(doc.source.text(), Language::En.sample());
        assert!(!guide.switch(&mut doc, Language::Ja));
        assert_eq!(doc.history_len(true), 1);
        doc.undo(true);
        assert!(doc.source.text().starts_with("中😀\r\n"));

        doc = doc.open_with(Path::new("user.md")).unwrap();
        assert!(!guide.switch(&mut doc, Language::Ja));
        assert_eq!(doc.source.text(), Language::En.sample());
        // Even another untitled session with the exact guide text is user-owned.
        doc = doc.fork(Language::En.sample());
        assert!(!guide.switch(&mut doc, Language::Ja));
    }

    #[test]
    fn saved_saving_and_preedit_guides_are_preserved() {
        let mut doc = document(Language::En);
        let mut guide = DefaultDocument::new(&doc, Language::En);
        doc.update_composition(Some(0..0), "未確定😀", Some(0..0))
            .unwrap();
        let before = doc.source.id();
        assert!(!guide.switch(&mut doc, Language::Ja));
        assert_eq!(doc.source.id(), before);
        assert_eq!(doc.composition().unwrap().text, "未確定😀");
        doc.cancel_composition();
        let request = doc.queue_save("guide.md".into()).unwrap();
        assert!(!guide.switch(&mut doc, Language::Ja));
        assert!(doc.complete_save(request.run()).accepted);
        assert!(!guide.switch(&mut doc, Language::Ja));
        assert_eq!(doc.source.id(), before);
        assert!(!doc.dirty());
    }
}
