use super::*;
use crate::regression_tests::new_document;

fn format(doc: &mut EditorSession) -> Result<bool, String> {
    doc.dispatch(EditIntent::FormatSource)
}

#[test]
fn format_source_maps_unicode_selection_and_is_one_idempotent_undoable_transaction() {
    for newline in ["\n", "\r\n", "\r"] {
        for source_mode in [false, true] {
            let raw = "##   标题😀\n\n\n\n* __中文😀__ and _em_\n    + nested\n* [X] task\n\n~~~rust\n\n**literal**\n\n~~~\n".replace('\n', newline);
            let expected = "## 标题😀\n\n- **中文😀** and *em*\n    - nested\n- [X] task\n\n```rust\n\n**literal**\n\n```\n".replace('\n', newline);
            let mut doc = new_document(&raw);
            doc.set_source_mode(source_mode);
            let start = raw.find("中文😀").unwrap();
            doc.select(start + "中文😀".len(), false, false);
            doc.select(start, true, false);
            let selection = (doc.anchor, doc.head);
            format(&mut doc).unwrap();
            assert_eq!(doc.source.text(), expected);
            assert_eq!(&doc.source.text()[doc.selection()], "中文😀");
            assert!(doc.anchor > doc.head);
            assert_eq!(doc.source_mode, source_mode);
            assert!(doc.dirty());
            assert_eq!(doc.history_len(false), 1);
            let selected = (doc.anchor, doc.head);
            let id = doc.source.id();
            format(&mut doc).unwrap();
            assert_eq!(doc.source.id(), id);
            assert_eq!(doc.history_len(false), 1);
            doc.undo(false);
            assert_eq!(doc.source.text(), raw);
            assert_eq!((doc.anchor, doc.head), selection);
            assert!(!doc.dirty());
            doc.undo(true);
            assert_eq!(doc.source.text(), expected);
            assert_eq!((doc.anchor, doc.head), selected);
        }
    }
}

#[test]
fn format_tables_keeps_cell_content_alignments_containers_and_newline_bytes() {
    for newline in ["\n", "\r\n", "\r"] {
        for prefix in ["", "> ", "> - "] {
            let continuation = if prefix == "> - " { ">   " } else { prefix };
            let raw = format!("{prefix}|a| B|c|\n{continuation}|:-|:-:|-:|\n{continuation}|中😀| `x\\|y` | **z**<br>t|\n").replace('\n', newline);
            let mut doc = new_document(&raw);
            let at = raw.find("中😀").unwrap();
            doc.select(at, false, false);
            doc.select(at + "中😀".len(), true, false);
            let align = ryne_syntax::table::tables(&doc.parse)[0].align.clone();
            format(&mut doc).unwrap();
            assert_eq!(&doc.source.text()[doc.selection()], "中😀");
            assert_eq!(ryne_syntax::table::tables(&doc.parse)[0].align, align);
            assert!(doc.source.text().starts_with(&format!("{prefix}| a")));
            assert!(doc.source.text().contains("`x\\|y`"));
            assert!(doc.source.text().contains("**z**<br>t"));
            let result = doc.source.text().to_owned();
            format(&mut doc).unwrap();
            assert_eq!(doc.source.text(), result);
            assert_eq!(doc.history_len(false), 1);
            doc.undo(false);
            assert_eq!(doc.source.text(), raw);
        }
    }
}

#[test]
fn format_preserves_literal_unsupported_incomplete_and_sensitive_source() {
    for raw in [
        "#标题\n\n#####\n\n- [x]done\n",
        "```rust\n__literal__\n\n\n* item\n",
        "    __literal__\n\n    * item\n",
        "<script>\n__literal__\n\n\n* item\n</script>\n",
        "<!-- __comment__\n\n\n -->\n",
        "---\nname: _yaml_\n\n\nitems:\n  * value\n---\n",
        "+++\nname = '_toml_'\n\n\n+++\n",
        "---\nname: __unfinished__\n\n\n",
        "one  \ntwo\\\nthree\nfour\n",
        "[ref]: https://example.org \"__title__\"\n\n[link][ref]\n",
        "| a | b |\n|---|---|\n| c | d | extra |\n",
        "***nested*** `__code__` $x_i$ $$x_2$$\n",
        "1) ordered\n   continuation\n2) second\n",
        "> quote\n>\n>\n> __bold__\n", // Only emphasis inside the quote may change.
    ] {
        let mut doc = new_document(raw);
        format(&mut doc).unwrap();
        let expected = raw
            .replace("> __bold__", "> **bold**")
            .replace("#标题", "# 标题")
            .replace("- [x]done", "- [x] done");
        assert_eq!(doc.source.text(), expected, "{raw:?}");
    }
}

#[test]
fn format_fences_keep_literal_blank_lines_and_upgrade_for_backticks() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw = "> - ~~~~mermaid\n>   \n>   ```\n>   __literal__\n>   \n>   ~~~~~\n"
            .replace('\n', newline);
        let mut doc = new_document(&raw);
        format(&mut doc).unwrap();
        assert_eq!(
            doc.source.text(),
            raw.replace("~~~~~", "````").replace("~~~~", "````")
        );
        doc.undo(false);
        assert_eq!(doc.source.text(), raw);
    }
    let raw = "~~~lang`info\n__literal__\n~~~\n";
    let mut doc = new_document(raw);
    format(&mut doc).unwrap();
    assert_eq!(doc.source.text(), raw);
}

#[test]
fn distinct_list_boundaries_are_preserved_by_rule() {
    for (raw, expected) in [
        ("* first\n\n- second\n", "* first\n\n- second\n"),
        (
            "a _b_ c\n\n* first\n- second\n",
            "a *b* c\n\n* first\n- second\n",
        ),
    ] {
        let mut doc = new_document(raw);
        format(&mut doc).unwrap();
        assert_eq!(doc.source.text(), expected);
    }
}

#[test]
fn unsafe_delimiter_changes_reject_the_entire_plan() {
    let raw = "##   title\n\n_a_*b*\n";
    let mut doc = new_document(raw);
    let id = doc.source.id();
    assert!(format(&mut doc).is_err());
    assert_eq!(doc.source.id(), id);
    assert_eq!(doc.source.text(), raw);
    assert_eq!(doc.history_len(false), 0);
    assert!(!doc.dirty());
}

#[test]
fn unchanged_formatting_keeps_clean_history_and_redo() {
    let mut doc = new_document("# title\n\n- item\n");
    let id = doc.source.id();
    format(&mut doc).unwrap();
    assert_eq!(doc.source.id(), id);
    assert!(!doc.dirty());
    assert_eq!(doc.history_len(false), 0);
    doc.insert_text(0..0, "x").unwrap();
    doc.undo(false);
    format(&mut doc).unwrap();
    assert_eq!(doc.history_len(true), 1);
}

#[test]
fn formatting_is_unavailable_during_preedit_and_over_size_limit() {
    let mut doc = new_document("* item");
    doc.composition = Some(Composition {
        range: 2..2,
        text: "未".into(),
        selection: 0..3,
    });
    assert!(format(&mut doc).is_err());
    assert!(doc.composition.is_some());
    assert_eq!(doc.source.text(), "* item");
    assert_eq!(doc.history_len(false), 0);
    let raw = "x".repeat(1024 * 1024 + 1);
    let mut doc = new_document(&raw);
    assert!(format(&mut doc).is_err());
    assert_eq!(doc.source.text(), raw);
    assert!(!doc.dirty());
}

#[test]
fn missing_heading_space_is_an_explicit_conservative_repair_rule() {
    for newline in ["\n", "\r\n", "\r"] {
        for (raw, expected) in [
            ("#标题😀\n", "# 标题😀\n"),
            ("######Title\n", "###### Title\n"),
            ("> ##标题\n", "> ## 标题\n"),
            ("- item\n\n  ###标题\n", "- item\n\n  ### 标题\n"),
            ("#标题\nsecond line\n", "#标题\nsecond line\n"),
            ("before #标题\n", "before #标题\n"),
            ("#######标题\n", "#######标题\n"),
            ("#####\n", "#####\n"),
            ("#!/usr/bin/env sh\n", "#!/usr/bin/env sh\n"),
            ("```\n#标题\n```\n", "```\n#标题\n```\n"),
        ] {
            let raw = raw.replace('\n', newline);
            let expected = expected.replace('\n', newline);
            let mut doc = new_document(&raw);
            format(&mut doc).unwrap();
            assert_eq!(doc.source.text(), expected);
            let id = doc.source.id();
            format(&mut doc).unwrap();
            assert_eq!(doc.source.id(), id);
            if expected != raw {
                assert_eq!(doc.history_len(false), 1);
                doc.undo(false);
                assert_eq!(doc.source.text(), raw);
            }
        }
    }
}

#[test]
fn task_space_repair_requires_a_list_marker_and_keeps_code_literal() {
    for newline in ["\n", "\r\n", "\r"] {
        for (raw, expected) in [
            ("- [x]done\n", "- [x] done\n"),
            ("+ [ ]todo\n", "- [ ] todo\n"),
            ("> - [X]**done**\n", "> - [X] **done**\n"),
            ("1. [ ]todo\n", "1. [ ] todo\n"),
            ("[x]word\n", "[x]word\n"),
            ("- `[x]word`\n", "- `[x]word`\n"),
            ("- item\n  [x]word\n", "- item\n  [x]word\n"),
            ("```\n- [x]word\n```\n", "```\n- [x]word\n```\n"),
        ] {
            let raw = raw.replace('\n', newline);
            let expected = expected.replace('\n', newline);
            let mut doc = new_document(&raw);
            format(&mut doc).unwrap();
            assert_eq!(doc.source.text(), expected);
            let id = doc.source.id();
            format(&mut doc).unwrap();
            assert_eq!(doc.source.id(), id);
            if raw != expected {
                doc.undo(false);
                assert_eq!(doc.source.text(), raw);
            }
        }
    }
}

#[test]
fn tables_without_outer_pipes_empty_cells_and_cell_end_carets_stay_stable() {
    for raw in [
        "A | B\n--- | :---:\nx | y\n",
        "|a|b|\n|-|-|\n|| x|\n",
        "|a|b|\n|-|-|\n| x||\n",
        "|a|b|\n|-|-|\n|||\n",
        "|a|b|\n|-|-|\n| _x_ | y |\n",
    ] {
        let mut doc = new_document(raw);
        let table = ryne_syntax::table::tables(&doc.parse).remove(0);
        let caret = table.rows[1].cells[1].content.end;
        doc.select(caret, false, false);
        doc.toggle_block_source();
        format(&mut doc).unwrap_or_else(|e| panic!("{raw:?}: {e}"));
        let table = ryne_syntax::table::tables(&doc.parse).remove(0);
        assert_eq!(doc.head, table.rows[1].cells[1].content.end, "{raw:?}");
        assert!(
            doc.block_source
                .as_ref()
                .unwrap()
                .visible(&doc.source)
                .is_ok()
        );
        let result = doc.source.text().to_owned();
        format(&mut doc).unwrap();
        assert_eq!(doc.source.text(), result);
        assert_eq!(doc.history_len(false), 1);
        doc.undo(false);
        assert_eq!(doc.source.text(), raw);
        assert_eq!(doc.head, caret);
    }
}

#[test]
fn formatter_guards_nodes_lines_and_pathological_table_padding() {
    let raw = format!("{}\n", "\n".repeat(20_000));
    let mut doc = new_document(&raw);
    assert!(format(&mut doc).is_err());
    assert!(!doc.dirty());
    let raw = format!(
        "| a |\n|---|\n| {} |\n{}",
        "x".repeat(100_000),
        "| y |\n".repeat(100)
    );
    let mut doc = new_document(&raw);
    format(&mut doc).unwrap();
    assert_eq!(doc.source.text(), raw);
    assert!(!doc.dirty());
}

#[test]
fn formatted_output_cannot_exceed_the_repeatable_input_limit() {
    let raw = format!("| a |\n|---|\n| {} |\n| y |\n", "x".repeat(600_000));
    let mut doc = new_document(&raw);
    assert!(format(&mut doc).is_err());
    assert_eq!(doc.source.text(), raw);
    assert!(!doc.dirty());
    assert_eq!(doc.history_len(false), 0);
}
