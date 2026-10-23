use super::regression_tests::new_document;
use editable_markdown_parser::Affinity;

#[test]
fn enter_keeps_an_empty_new_row_in_the_same_quote_and_one_undo_step() {
    for newline in ["\n", "\r\n", "\r"] {
        for (first, prefix) in [
            ("> 中😀", "> "),
            (">body", ">"),
            ("> > nested", "> > "),
            ("- > body", "  > "),
            ("- list\n  > body", "  > "),
        ] {
            let raw = first.replace('\n', newline);
            // Supply the local newline convention without adding a newline after the quote.
            let raw = format!("before{newline}{newline}{raw}");
            let mut doc = new_document(&raw);
            let caret = raw.len();
            doc.select(caret, false, false);
            doc.line_break(false).unwrap();
            let expected = format!("{raw}{newline}{prefix}");
            assert_eq!(doc.source.text(), expected);
            assert_eq!(doc.head, expected.len());
            assert_eq!(doc.history_len(false), 1);
            let display = doc.display();
            display.validate(&expected).unwrap();
            assert_eq!(
                display.quote_at(display.text.len()).unwrap().empty_tail,
                Some(expected.len())
            );
            assert_eq!(
                display.source_at(display.text.len(), Affinity::Downstream),
                expected.len()
            );
            assert_eq!(display.quotes.iter().filter(|q| q.depth == 1).count(), 1);
            doc.insert_text(doc.selection(), "new").unwrap();
            assert_eq!(doc.source.text(), format!("{expected}new"));
            assert!(doc
                .display()
                .quote_at(doc.display().text.find("new").unwrap())
                .is_some());
            doc.undo(false);
            doc.undo(false);
            assert_eq!(doc.source.text(), raw);
            assert_eq!(doc.head, caret);
            assert!(!doc.dirty());
            doc.undo(true);
            assert_eq!(doc.source.text(), expected);
        }
    }
}

#[test]
fn quote_enter_splits_content_and_empty_enter_exits_one_layer() {
    for newline in ["\n", "\r\n", "\r"] {
        for (raw, expected) in [
            ("> abcd", "> ab\n> cd"),
            ("> > abcd", "> > ab\n> > cd"),
            ("> first\nlazy abcd", "> first\nlazy ab\n> cd"),
        ] {
            let raw = format!("before{newline}{newline}{}", raw.replace('\n', newline));
            let mut doc = new_document(&raw);
            let caret = raw.find("abcd").unwrap() + 2;
            doc.select(caret, false, false);
            doc.line_break(false).unwrap();
            assert_eq!(
                doc.source.text(),
                format!(
                    "before{newline}{newline}{}",
                    expected.replace('\n', newline)
                )
            );
            doc.undo(false);
            assert_eq!(doc.source.text(), raw);
        }
        for (raw, expected) in [
            ("> first\n> ", "> first\n"),
            ("> > first\n> > ", "> > first\n> "),
            ("- > first\n  > ", "- > first\n  "),
            ("> ", ""),
        ] {
            let raw = format!("before{newline}{newline}{}", raw.replace('\n', newline));
            let mut doc = new_document(&raw);
            doc.select(raw.len(), false, false);
            doc.line_break(false).unwrap();
            assert_eq!(
                doc.source.text(),
                format!(
                    "before{newline}{newline}{}",
                    expected.replace('\n', newline)
                )
            );
            assert_eq!(doc.history_len(false), 1);
            doc.undo(false);
            assert_eq!(doc.source.text(), raw);
        }
    }
}

#[test]
fn quote_hard_break_source_modes_and_nested_code_list_precedence() {
    let mut doc = new_document("> abcd");
    doc.select(4, false, false);
    doc.line_break(true).unwrap();
    assert_eq!(doc.source.text(), "> ab  \n> cd");
    for local in [false, true] {
        let mut doc = new_document("> abcd");
        doc.select(4, false, false);
        if local {
            assert!(doc.toggle_block_source());
        } else {
            doc.set_source_mode(true);
        }
        doc.line_break(false).unwrap();
        assert_eq!(doc.source.text(), "> ab\ncd");
    }
    for (raw, expected) in [
        ("> - item", "> - item\n> - "),
        (
            "> ```text\n> literal\n> ```",
            "> ```text\n> literal\n> \n> ```",
        ),
    ] {
        let mut doc = new_document(raw);
        let caret = raw
            .find("literal")
            .map_or(raw.len(), |p| p + "literal".len());
        doc.select(caret, false, false);
        doc.line_break(false).unwrap();
        assert_eq!(doc.source.text(), expected);
    }
}
