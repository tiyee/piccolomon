use super::regression_tests::new_document;
use ryne_parser::Affinity;

#[test]
fn alert_projection_and_header_reveal_keep_exact_source_and_history() {
    for nl in ["\n", "\r\n", "\r"] {
        for kind in ryne_syntax::alert::Kind::ALL {
            let raw = format!(
                "before{nl}{nl}> [!{}]{nl}> 中文 **body**{nl}{nl}after",
                kind.label().to_uppercase()
            );
            let mut doc = new_document(&raw);
            let body = raw.find("中文").unwrap();
            doc.select(body, false, false);
            let display = doc.display();
            display.validate(&raw).unwrap();
            let quote = display.quotes.iter().find(|q| q.alert.is_some()).unwrap();
            assert_eq!(quote.alert.as_ref().unwrap().source.kind, kind);
            assert!(quote.alert.as_ref().unwrap().folded);
            assert!(!display.text.contains("[!"));
            let pos = display.text.find("中文").unwrap();
            assert_eq!(display.source_at(pos, Affinity::Downstream), body);
            assert_eq!(doc.history_len(false), 0);
            assert!(!doc.dirty());
            doc.delete(false).unwrap();
            assert_eq!(doc.source.text(), raw);
            assert!(doc.display().text.contains("[!"));
            assert_eq!(doc.history_len(false), 0);
            doc.set_block_source(None);
            doc.select(raw.find("body").unwrap() + 4, false, false);
            doc.insert_text(doc.selection(), "!").unwrap();
            assert!(doc.source.text().contains("body!"));
            doc.undo(false);
            assert_eq!(doc.source.text(), raw);
            doc.set_source_mode(true);
            assert!(doc.display().quotes.is_empty());
            assert!(doc.display().text.contains("[!"));
        }
    }
}
#[test]
fn alert_typing_empty_body_and_nested_blocks_remain_editable() {
    let mut doc = new_document("> [!NOTE]");
    doc.select(doc.source.text().len(), false, false);
    assert!(doc.display().text.contains("[!NOTE]"));
    doc.line_break(false).unwrap();
    assert_eq!(doc.source.text(), "> [!NOTE]\n> ");
    doc.display().validate(doc.source.text()).unwrap();
    assert!(doc.display().quotes.iter().any(|q| q.alert.is_some()));
    doc.insert_text(doc.selection(), "body").unwrap();
    assert!(doc.display().text.contains("body"));
    doc.undo(false);
    doc.undo(false);
    assert_eq!(doc.source.text(), "> [!NOTE]");
    let raw = "before\n\n> [!TIP]\n> **bold**\n>\n> - item\n>\n> ```rust\n> [!WARNING]\n> ```\n>\n> | x | y |\n> |---|---|\n> | a | b |\n\nafter";
    let doc = new_document(raw);
    let display = doc.display();
    display.validate(raw).unwrap();
    assert_eq!(
        display.quotes.iter().filter(|q| q.alert.is_some()).count(),
        1
    );
    assert!(display.text.contains("[!WARNING]"));
    assert_eq!(display.code_blocks.len(), 1);
    assert_eq!(display.tables.len(), 1);
}
