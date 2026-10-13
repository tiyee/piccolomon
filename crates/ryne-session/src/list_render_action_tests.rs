use super::regression_tests::new_document;
use super::*;
fn request(doc: &EditorSession, needle: &str, command: &'static str) -> RenderActionRequest {
    let at = doc.source.text().find(needle).unwrap();
    let c = doc
        .render_controls()
        .unwrap()
        .into_iter()
        .filter(|c| c.provider == "list" && c.focus.start <= at && at < c.target.owner.end)
        .min_by_key(|c| c.focus.len())
        .unwrap();
    RenderActionRequest {
        provider: c.provider,
        target: c.target,
        command,
    }
}
#[test]
fn ordered_unordered_nested_empty_and_task_items_contribute_leading_controls() {
    for raw in [
        "- one\n  - child\n- two\n",
        "3. one\n   1. child\n4. two\n",
        "- [x] one\n- [ ] two\n",
        "- \n",
        "- ",
    ] {
        let doc = new_document(raw);
        let controls = doc.render_controls().unwrap();
        assert!(!controls.is_empty(), "{raw:?}");
        assert!(controls
            .iter()
            .all(|c| c.placement == RenderControlPlacement::Leading
                && c.trailing_space == 0.
                && c.actions.len() == 7));
        assert!(!doc.dirty());
        assert_eq!(doc.history_len(false), 0);
    }
    assert!(new_document("```\n- literal\n1. literal\n```\n")
        .render_controls()
        .unwrap()
        .is_empty());
}
#[test]
fn insert_siblings_preserves_prefix_newlines_subtree_and_missing_final_newline() {
    for newline in ["\n", "\r\n", "\r"] {
        for prefix in ["", "> "] {
            for marker in ["- ", "9. "] {
                for last in [true, false] {
                    let raw = format!(
                        "{prefix}{marker}first{newline}{prefix}   - child{}",
                        if last { newline } else { "" }
                    );
                    for command in ["list.insert-before", "list.insert-after"] {
                        let mut doc = new_document(&raw);
                        let req = request(&doc, "first", command);
                        doc.execute_render_action(&req)
                            .unwrap_or_else(|e| panic!("{raw:?} {command}: {e}"));
                        let next = if marker == "9. " { "10. " } else { marker };
                        let expected = if command == "list.insert-before" {
                            format!("{prefix}{marker}{newline}{raw}")
                        } else if last {
                            format!("{raw}{prefix}{next}{newline}")
                        } else {
                            format!("{raw}{newline}{prefix}{next}")
                        };
                        assert_eq!(doc.source.text(), expected, "{command}");
                        assert_eq!(doc.history_len(false), 1);
                        assert!(ryne_syntax::list::items(&doc.parse)
                            .iter()
                            .any(|i| i.body == doc.head));
                        doc.undo(false);
                        assert_eq!(doc.source.text(), raw);
                        assert!(!doc.dirty());
                    }
                }
            }
        }
    }
}
#[test]
fn moving_items_keeps_subtrees_and_source_marker_positions() {
    for (raw, expected) in [
        (
            "- first\n  - child\n- second\n  - other\n- third",
            "- second\n  - other\n- first\n  - child\n- third",
        ),
        (
            "1. first\n   - child\n2. second\n   - other\n3. third",
            "1. second\n   - other\n2. first\n   - child\n3. third",
        ),
        (
            "9. first\n   - child\n10. second\n    - other\n11. third",
            "9. second\n   - other\n10. first\n    - child\n11. third",
        ),
    ] {
        for newline in ["\n", "\r\n", "\r"] {
            let raw = raw.replace('\n', newline);
            let expected = expected.replace('\n', newline);
            let mut doc = new_document(&raw);
            doc.execute_render_action(&request(&doc, "first", "list.move-down"))
                .unwrap();
            assert_eq!(doc.source.text(), expected);
            assert_eq!(doc.history_len(false), 1);
            doc.undo(false);
            assert_eq!(doc.source.text(), raw);
            doc.execute_render_action(&request(&doc, "second", "list.move-up"))
                .unwrap();
            assert_eq!(doc.source.text(), expected);
        }
    }
}
#[test]
fn moving_last_item_preserves_eof_newline_and_moving_literal_code_preserves_its_body() {
    let raw = "9. first\r\n   ```rust\r\n   let x = \"**literal**\";\r\n   ```\r\n10. second";
    let mut doc = new_document(raw);
    let original_code = ryne_syntax::code_block::blocks(&doc.parse)[0]
        .rows
        .iter()
        .map(|r| &doc.source.text()[r.content.clone()])
        .collect::<String>();
    doc.execute_render_action(&request(&doc, "first", "list.move-down"))
        .unwrap();
    assert!(!doc.source.text().ends_with(['\r', '\n']));
    let code = ryne_syntax::code_block::blocks(&doc.parse);
    assert_eq!(code.len(), 1);
    assert_eq!(
        code[0]
            .rows
            .iter()
            .map(|r| &doc.source.text()[r.content.clone()])
            .collect::<String>(),
        original_code
    );
    doc.undo(false);
    assert_eq!(doc.source.text(), raw);
}
#[test]
fn quoted_number_width_move_keeps_container_only_blank_lines_unchanged() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw =
            "> 9. first\n>\n>    - child\n> 10. second\n>     - other\n".replace('\n', newline);
        let expected =
            "> 9. second\n>    - other\n> 10. first\n>\n>     - child\n".replace('\n', newline);
        let mut doc = new_document(&raw);
        doc.execute_render_action(&request(&doc, "first", "list.move-down"))
            .unwrap();
        assert_eq!(doc.source.text(), expected);
        doc.undo(false);
        assert_eq!(doc.source.text(), raw);
    }
}
#[test]
fn list_move_keeps_external_paragraph_separator_at_list_end() {
    let raw = "9. parent\r\n   - child\r\n10. second\r\n    - other\r\n\r\noutside\r\n";
    let mut doc = new_document(raw);
    doc.execute_render_action(&request(&doc, "second", "list.move-up"))
        .unwrap();
    assert_eq!(
        doc.source.text(),
        "9. second\r\n   - other\r\n10. parent\r\n    - child\r\n\r\noutside\r\n"
    );
    assert!(!ryne_syntax::list::items(&doc.parse).iter().any(|i| i
        .range
        .contains(&doc.source.text().find("outside").unwrap())));
    doc.undo(false);
    assert_eq!(doc.source.text(), raw);
}
#[test]
fn clicked_item_indent_and_outdent_reuse_subtree_rules_independently_of_caret() {
    let raw = "> - first\r\n> - second\r\n>   - child\r\n> - third\r\n";
    let mut doc = new_document(raw);
    let at = raw.find("third").unwrap();
    doc.select(at, false, false);
    doc.execute_render_action(&request(&doc, "second", "list.indent"))
        .unwrap();
    assert_eq!(
        doc.source.text(),
        "> - first\r\n>   - second\r\n>     - child\r\n> - third\r\n"
    );
    assert_eq!(&doc.source.text()[doc.head..doc.head + 5], "third");
    assert_eq!(doc.history_len(false), 1);
    doc.execute_render_action(&request(&doc, "second", "list.outdent"))
        .unwrap();
    assert_eq!(doc.source.text(), raw);
    doc.undo(false);
    doc.undo(false);
    assert_eq!(doc.source.text(), raw);
    assert_eq!(doc.head, at);
    assert!(!doc.dirty());
}
#[test]
fn nested_delete_removes_only_selected_subtree_and_is_atomic() {
    let raw = "before\n\n- parent\n  - child\n    - grandchild\n  - sibling\n- other\n\nafter\n";
    let mut doc = new_document(raw);
    doc.execute_render_action(&request(&doc, "child", "list.delete"))
        .unwrap();
    assert_eq!(
        doc.source.text(),
        "before\n\n- parent\n  - sibling\n- other\n\nafter\n"
    );
    assert_eq!(doc.history_len(false), 1);
    doc.undo(false);
    assert_eq!(doc.source.text(), raw);
    assert!(!doc.dirty());
    let mut doc = new_document("- only");
    doc.execute_render_action(&request(&doc, "only", "list.delete"))
        .unwrap();
    assert_eq!(doc.source.text(), "");
    doc.undo(false);
    assert_eq!(doc.source.text(), "- only");
}
#[test]
fn move_follows_unicode_caret_and_cross_line_selection_without_splitting_graphemes() {
    let raw = "9. 中😀\n   continuation\n10. other\n";
    let mut doc = new_document(raw);
    let a = raw.find("中😀").unwrap();
    let b = raw.find("continuation").unwrap() + "continuation".len();
    doc.select(a, false, false);
    doc.select(b, true, false);
    let selected = raw[a..b].to_owned();
    doc.execute_render_action(&request(&doc, "中😀", "list.move-down"))
        .unwrap();
    // One extra indentation column is required at marker-width transition 9 -> 10.
    assert_eq!(
        doc.source.text()[doc.selection()],
        selected.replace("\n   ", "\n    ")
    );
    assert!(
        doc.source.text().is_char_boundary(doc.anchor)
            && doc.source.text().is_char_boundary(doc.head)
    );
    doc.undo(false);
    assert_eq!(doc.selection(), a..b);
    assert_eq!(doc.source.text(), raw);
}
#[test]
fn loose_lists_and_nested_tables_move_without_losing_separator_or_container_bytes() {
    for raw in ["- first\n\n  continuation\n\n- second\n\n- third\n", "> - first\r\n>\r\n>   | A | B |\r\n>   | --- | --- |\r\n>   | 中😀 | x |\r\n> - second\r\n"] {
        let mut doc=new_document(raw);let table_count=doc.display().tables.len();
        doc.execute_render_action(&request(&doc,"first","list.move-down")).unwrap();
        assert!(doc.source.text().find("second").unwrap()<doc.source.text().find("first").unwrap());
        assert_eq!(doc.display().tables.len(),table_count);
        assert_eq!(doc.source.text().bytes().filter(|b|*b==b'\n').count(),raw.bytes().filter(|b|*b==b'\n').count());
        doc.undo(false);assert_eq!(doc.source.text(),raw);
    }
}
#[test]
fn task_insertion_uses_unchecked_marker_and_never_duplicates_existing_task_content() {
    let raw = "- [x] done\n- [ ] later\n";
    let mut doc = new_document(raw);
    doc.execute_render_action(&request(&doc, "done", "list.insert-after"))
        .unwrap();
    assert_eq!(doc.source.text(), "- [x] done\n- [ ] \n- [ ] later\n");
    doc.undo(false);
    assert_eq!(doc.source.text(), raw);
}
#[test]
fn empty_insert_before_first_item_cannot_turn_prior_paragraph_into_setext_heading() {
    for raw in [
        "paragraph\n- first\n- second\n",
        "- parent\n  - first\n  - next\n",
    ] {
        let mut doc = new_document(raw);
        let count = ryne_syntax::list::items(&doc.parse).len();
        doc.execute_render_action(&request(&doc, "first", "list.insert-before"))
            .unwrap();
        assert_eq!(ryne_syntax::list::items(&doc.parse).len(), count + 1);
        assert!(!doc
            .parse
            .nodes
            .iter()
            .any(|n| matches!(n.kind, editable_markdown_parser::NodeKind::Heading(_))));
        doc.undo(false);
        assert_eq!(doc.source.text(), raw);
    }
}
#[test]
fn boundary_commands_and_old_or_source_ime_targets_are_rejected() {
    for (needle, command) in [
        ("first", "list.move-up"),
        ("first", "list.indent"),
        ("first", "list.outdent"),
        ("second", "list.move-down"),
    ] {
        let mut doc = new_document("- first\n- second\n");
        let req = request(&doc, needle, command);
        assert!(doc.execute_render_action(&req).is_err());
        assert!(!doc.dirty());
        assert_eq!(doc.history_len(false), 0);
    }
    for mode in 0..5 {
        let mut doc = new_document("- first\n- second\n");
        let req = request(&doc, "second", "list.delete");
        match mode {
            0 => doc.set_source_mode(true),
            1 => doc.set_block_source(Some(req.target.owner.clone())),
            2 => {
                doc.composition = Some(Composition {
                    range: 0..0,
                    text: "候选".into(),
                    selection: 0..0,
                })
            }
            3 => {
                doc.insert_text(0..0, "x\n\n").unwrap();
            }
            _ => doc = new_document("- first\n- second\n"),
        }
        let source = doc.source.id();
        let undo = doc.history_len(false);
        assert!(doc.execute_render_action(&req).is_err());
        assert_eq!(doc.source.id(), source);
        assert_eq!(doc.history_len(false), undo);
    }
}
