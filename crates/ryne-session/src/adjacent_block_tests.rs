use super::*;
use crate::regression_tests::new_document;
use ryne_features::templates::{ITEMS, MarkdownTemplate};
use ryne_syntax::block;

fn insert(
    doc: &mut EditorSession,
    target: &block::Block,
    before: bool,
    kind: InsertBlock,
) -> Result<bool, String> {
    doc.dispatch(EditIntent::InsertAdjacent {
        block: ryne_core::SourceRange::new(doc.source.id(), target.range.clone()),
        before,
        template: kind,
        labels: TemplateLabels::default(),
    })
}

#[test]
fn adjacent_templates_keep_neighbor_bytes_newlines_selection_and_single_undo() {
    for newline in ["\n", "\r\n", "\r"] {
        for body in [
            "## target",
            "目标😀 **bold**",
            "- target\n  - nested\n- second",
            "> target",
            "```rs\n**target**\n```",
            "    target\n    literal",
            "```mermaid\nflowchart LR\n target-->B\n```",
            "| target | B |\n|---|---|\n| c | d |",
            "![target](img.png)",
            "---",
            "<div>target</div>",
        ] {
            let raw = format!("before😀\n\n{body}\n\nafter\n").replace('\n', newline);
            for before in [false, true] {
                for &(label, kind) in ITEMS {
                    let mut doc = new_document(&raw);
                    let caret = raw
                        .find("target")
                        .or_else(|| raw.find("目标"))
                        .unwrap_or_else(|| raw.find("---").unwrap());
                    let target = block::at(&doc.parse, caret).unwrap();
                    let (boundary, _) = block::insertion_context(&doc.parse, &target).unwrap();
                    let at = if before { boundary.start } else { boundary.end };
                    doc.select(0, false, false);
                    doc.select("before😀".len(), true, false);
                    let selection = doc.selection();
                    insert(&mut doc, &target, before, kind)
                        .unwrap_or_else(|e| panic!("{label}, {before}, {raw:?}: {e}"));
                    let result = doc.source.text().to_owned();
                    let added = result.len() - raw.len();
                    assert_eq!(&result[..at], &raw[..at]);
                    assert_eq!(&result[at + added..], &raw[at..]);
                    assert_eq!(doc.history_len(false), 1);
                    let (template, editable) = kind.template(newline);
                    assert_eq!(&result[doc.selection()], &template[editable]);
                    let selected = doc.selection();
                    doc.undo(false);
                    assert_eq!(doc.source.text(), raw);
                    assert_eq!(doc.selection(), selection);
                    assert!(!doc.dirty());
                    doc.undo(true);
                    assert_eq!(doc.source.text(), result);
                    assert_eq!(doc.selection(), selected);
                }
            }
        }
    }
}

#[test]
fn first_block_only_allows_below_even_after_leading_blank_lines() {
    for raw in [
        "first",
        "\n\n# first\n",
        "\r\n\r\n- first\r\n",
        "> first\n",
        "```rs\nfirst\n```\n",
    ] {
        let mut doc = new_document(raw);
        let target = block::at(&doc.parse, raw.find("first").unwrap()).unwrap();
        assert!(!block::can_insert_above(&doc.parse, &target));
        assert!(insert(&mut doc, &target, true, InsertBlock::Paragraph).is_err());
        assert_eq!(doc.source.text(), raw);
        assert_eq!(doc.history_len(false), 0);
        insert(&mut doc, &target, false, InsertBlock::Heading(2)).unwrap();
        assert!(doc.source.text().starts_with(raw));
    }
}

#[test]
fn nested_insertions_keep_container_prefix_and_code_literal() {
    for newline in ["\n", "\r\n", "\r"] {
        for raw in [
            "before\n\n> - outer\n>\n>   ```rs\n>   target **literal**\n>   ```\n>\n>   tail\n\nafter",
            "before\n\n- outer\n\n  ## target\n\n  tail\n\nafter",
            "before\n\n> intro\n>\n> | target | B |\n> |---|---|\n> | c | d |\n>\n> tail\n\nafter",
        ] {
            let raw = raw.replace('\n', newline);
            for before in [false, true] {
                for &(_, kind) in ITEMS {
                    let mut doc = new_document(&raw);
                    let target = block::at(&doc.parse, raw.find("target").unwrap()).unwrap();
                    let (_, prefix) = block::insertion_context(&doc.parse, &target).unwrap();
                    assert!(!prefix.is_empty(), "{raw:?}");
                    insert(&mut doc, &target, before, kind)
                        .unwrap_or_else(|e| panic!("{kind:?}, {before}, {raw:?}: {e}"));
                    let selected = doc.selection();
                    assert!(
                        selected.start < doc.source.text().len()
                            || matches!(kind, InsertBlock::Rule)
                    );
                    if raw.contains("**literal**") {
                        assert!(
                            doc.source.text().contains(&format!(
                                "{prefix}target **literal**{newline}{prefix}```"
                            ))
                        );
                    }
                    doc.undo(false);
                    assert_eq!(doc.source.text(), raw);
                }
            }
        }
    }
}

#[test]
fn unsafe_and_obsolete_targets_do_not_modify_source_or_history() {
    for raw in ["before\n\n```rs\nunfinished", "before\n\n<!-- unfinished"] {
        let mut doc = new_document(raw);
        let target = block::at(&doc.parse, raw.find("unfinished").unwrap()).unwrap();
        assert!(insert(&mut doc, &target, false, InsertBlock::Heading(2)).is_err());
        assert_eq!(doc.source.text(), raw);
        assert_eq!(doc.history_len(false), 0);
    }
    let mut doc = new_document("before\n\n# target\n");
    let target = block::at(&doc.parse, 10).unwrap();
    let stale = doc.source.id();
    doc.insert_text(0..0, "new ").unwrap();
    let source = doc.source.clone();
    assert!(
        doc.dispatch(EditIntent::InsertAdjacent {
            block: ryne_core::SourceRange::new(stale, target.range),
            before: false,
            template: InsertBlock::Paragraph,
            labels: TemplateLabels::default(),
        })
        .is_err()
    );
    assert_eq!(doc.source.id(), source.id());
    assert_eq!(doc.history_len(false), 1);
    let target = block::at(&doc.parse, doc.source.text().find("target").unwrap()).unwrap();
    doc.set_source_mode(true);
    assert!(insert(&mut doc, &target, false, InsertBlock::Paragraph).is_err());
    doc.set_source_mode(false);
    doc.composition = Some(Composition {
        range: 0..0,
        text: "临".into(),
        selection: 0..0,
    });
    assert!(insert(&mut doc, &target, false, InsertBlock::Paragraph).is_err());
    assert_eq!(doc.source.id(), source.id());
    assert_eq!(doc.history_len(false), 1);
}
