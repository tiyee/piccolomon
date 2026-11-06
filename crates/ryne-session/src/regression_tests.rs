use super::*;
use ryne_features::templates::{MarkdownTemplate, ITEMS};
use ryne_parser::{Affinity, NodeKind};
use ryne_services::highlight::{Class, CodeHighlighter};
use ryne_syntax::{code_block, list, table};
pub(super) fn services() -> SessionServices {
    SessionServices {
        features: Arc::new(
            ryne_features::MarkdownFeatures::new(
                Arc::new(ryne_services::math::FormularyRenderer::default()),
                vec![ryne_features::diagrams::FencedPreview {
                    id: "mermaid",
                    language: "mermaid",
                    renderer: Arc::new(ryne_services::mermaid::MermanRenderer),
                }],
            )
            .expect("test registry"),
        ),
        files: Arc::new(ryne_services::files::AtomicFileStore),
    }
}
pub(super) fn new_document(text: &str) -> EditorSession {
    EditorSession::new(text, services())
}
fn open_document(path: &Path) -> std::io::Result<EditorSession> {
    EditorSession::open(path, services())
}
#[test]
fn code_row_index_matches_projection_across_containers_and_source_modes() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw = concat!(
            "```rust\n中😀\n\nnext\n```\n\n",
            "> ~~~python\n> value = 1\n> ~~~\n\n",
            "- item\n\n    ```\n    nested\n    ```\n\n",
            "    indented\n\n```\n```\n\ntail"
        )
        .replace('\n', newline);
        let mut doc = new_document(&raw);
        let snapshot = doc.source.id();
        let check = |display: Display| {
            let index = display.code_row_index();
            for position in 0..=display.text.len() {
                assert_eq!(index.code_row(position), display.code_row(position));
            }
        };
        check(doc.display());
        doc.select(raw.find("value").unwrap(), false, false);
        assert!(doc.toggle_block_source());
        check(doc.display());
        assert!(doc.toggle_block_source());
        doc.set_source_mode(true);
        check(doc.display());
        doc.set_source_mode(false);
        assert_eq!(doc.source.id(), snapshot);
        assert_eq!(doc.history_len(false), 0);
        assert!(!doc.dirty());
    }
}
#[test]
fn quote_presentation_body_editing_and_modes_preserve_bytes_and_history() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw = "> 中😀\n> second\n> > nested\n\noutside".replace('\n', newline);
        let mut doc = new_document(&raw);
        doc.normalize_heading_caret();
        assert_eq!(doc.head, 2);
        let source = doc.source.id();
        assert_eq!(doc.display().quotes.len(), 2);
        doc.select(raw.find("😀").unwrap(), false, false);
        let selection = doc.selection();
        assert!(doc.toggle_block_source());
        assert!(doc.display().quotes.is_empty());
        assert!(doc.display().text.starts_with("> 中😀\n> second"));
        assert!(doc.toggle_block_source());
        doc.set_source_mode(true);
        assert!(doc.display().quotes.is_empty());
        doc.set_source_mode(false);
        assert_eq!(doc.source.id(), source);
        assert_eq!(doc.selection(), selection);
        assert_eq!(doc.history_len(false), 0);
        assert!(!doc.dirty());
        doc.insert_text(doc.selection(), "新增").unwrap();
        assert_eq!(doc.source.text(), raw.replacen("😀", "新增😀", 1));
        assert_eq!(doc.history_len(false), 1);
        doc.undo(false);
        assert_eq!(doc.source.text(), raw);
        assert_eq!(doc.selection(), selection);
        assert!(!doc.dirty());
        doc.undo(true);
        assert_eq!(doc.source.text(), raw.replacen("😀", "新增😀", 1));
    }
}
mod automatic_code_language {
    use super::*;
    fn document(raw: &str) -> EditorSession {
        let mut ports = services();
        ports.features = Arc::new(
            ryne_features::MarkdownFeatures::new(
                Arc::new(ryne_services::math::FormularyRenderer::default()),
                vec![],
            )
            .unwrap(),
        );
        EditorSession::new(raw, ports)
    }
    #[test]
    fn pasted_code_keeps_auto_with_exact_history_and_mapping() {
        for newline in ["\n", "\r\n", "\r"] {
            for (opening, prefix, closing) in [
                ("```", "", "```"),
                ("~~~~", "", "~~~~"),
                ("> - ```", ">   ", ">   ```"),
            ] {
                let raw = format!(
                    "前😀{newline}{newline}{opening}  {newline}{prefix}{newline}{closing}{newline}{newline}尾"
                );
                let mut doc = document(&raw);
                let block = code_block::blocks(&doc.parse).remove(0);
                let caret = block.rows[0].content.start;
                doc.select(caret, false, false);
                doc.insert_text_from(
                    caret..caret,
                    "fn main() { let s = \"中😀\"; }",
                    InputOrigin::Paste,
                )
                .unwrap();
                let mut expected = raw.clone();
                expected.insert_str(caret, "fn main() { let s = \"中😀\"; }");
                assert_eq!(doc.source.text(), expected);
                assert_eq!(doc.head, caret + "fn main() { let s = \"中😀\"; }".len());
                assert_eq!(doc.history_len(false), 1);
                assert!(code_block::blocks(&doc.parse)[0].language.is_empty());
                let mut highlights = CodeHighlighter::default();
                assert!(doc
                    .highlighted_display(&mut highlights)
                    .spans
                    .iter()
                    .any(|s| s.style.syntax == Some(Class::Keyword)));
                doc.undo(false);
                assert_eq!(doc.source.text(), raw);
                assert_eq!(doc.selection(), caret..caret);
                assert!(!doc.dirty());
                doc.undo(true);
                assert_eq!(doc.source.text(), expected);
                assert_eq!(doc.history_len(false), 1);
            }
        }
    }
    #[test]
    fn return_detects_completed_body_without_touching_other_blocks_or_navigation() {
        let raw = "```\nfn main() {}\n```\n\n```\nconst n = 1;\n```\n";
        let mut doc = document(raw);
        let at = raw.find("{}\n").unwrap() + 2;
        doc.select(at, false, false);
        doc.select(0, false, false);
        assert_eq!(doc.source.text(), raw);
        assert_eq!(doc.history_len(false), 0);
        doc.select(at, false, false);
        doc.line_break(false).unwrap();
        assert_eq!(doc.source.text(), raw.replacen("{}\n", "{}\n\n", 1));
        assert_eq!(doc.head, at + 1);
        assert_eq!(doc.history_len(false), 1);
        doc.undo(false);
        assert_eq!(doc.source.text(), raw);
    }
    #[test]
    fn explicit_languages_source_modes_and_ambiguous_code_remain_literal() {
        for raw in [
            "```text\n\n```\n",
            "```unknown\n\n```\n",
            "    x = 1\n",
            "```\n\n```\n",
        ] {
            let mut doc = document(raw);
            let caret = code_block::blocks(&doc.parse)[0].anchor;
            doc.select(caret, false, false);
            let input = if raw == "```\n\n```\n" {
                "print(42)"
            } else {
                "fn main() {}"
            };
            doc.insert_text_from(caret..caret, input, InputOrigin::Paste)
                .unwrap();
            assert!(!doc.source.text().contains("```rust"));
        }
        for block_mode in [false, true] {
            let mut doc = document("```\n\n```\n");
            doc.select(4, false, false);
            if block_mode {
                assert!(doc.toggle_block_source());
            } else {
                doc.set_source_mode(true);
            }
            doc.insert_text_from(4..4, "fn main() {}\n", InputOrigin::Paste)
                .unwrap();
            assert_eq!(doc.source.text(), "```\nfn main() {}\n\n```\n");
        }
    }
    #[test]
    fn whole_blocks_pasted_together_preserve_unicode_selection_and_save_bytes() {
        let raw = "前😀\r\n尾";
        let mut doc = document(raw);
        doc.select(0, false, false);
        doc.select("前😀".len(), true, false);
        let selection = doc.selection();
        let pasted = "```\r\nconst n = 42;\r\n```\r\n\r\n~~~\r\n{\"中😀\": 1}\r\n~~~\r\n";
        doc.insert_text_from(selection.clone(), pasted, InputOrigin::Paste)
            .unwrap();
        let expected = format!("{pasted}\r\n尾");
        assert_eq!(doc.source.text(), expected);
        assert_eq!(doc.head, pasted.len());
        let path =
            std::env::temp_dir().join(format!("ryne-language-test-{}.md", std::process::id()));
        doc.save(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), expected.as_bytes());
        std::fs::remove_file(path).unwrap();
        doc.undo(false);
        assert_eq!(doc.source.text(), raw);
        assert_eq!(doc.selection(), selection);
    }
}
mod original {
    use super::*;
    #[test]
    fn append_templates_preserve_source_newlines_selection_and_atomic_history() {
        for newline in ["\n", "\r\n", "\r"] {
            for raw in [
                "",
                "中😀",
                "- parent\n  - child",
                "> quote",
                "```rust\n**literal**\n```",
                "| A | B |\n| --- | --- |\n| x | y |",
                "tail\n\n  ",
            ] {
                let raw = raw.replace('\n', newline);
                for &(label, kind) in ITEMS {
                    let mut document = new_document(&raw);
                    document.select(0, false, false);
                    document.select(raw.len(), true, false);
                    let original_selection = document.selection();
                    document
                        .append_block(kind)
                        .unwrap_or_else(|error| panic!("{label} after {raw:?}: {error}"));
                    let inserted = document.source.text().to_owned();
                    assert!(inserted.starts_with(&raw));
                    assert!(document.dirty());
                    assert!(document.selection().start >= raw.len());
                    assert!(inserted.is_char_boundary(document.head));
                    // An empty document has no newline convention to preserve.
                    let expected_newline = if raw.is_empty() || !raw.contains(['\r', '\n']) {
                        "\n"
                    } else {
                        newline
                    };
                    let (template, selection) = kind.template(expected_newline);
                    let start = raw.len()
                        + if raw.is_empty() {
                            0
                        } else {
                            2 * expected_newline.len()
                        };
                    assert_eq!(
                        inserted,
                        format!(
                            "{}{}{}",
                            raw,
                            if raw.is_empty() {
                                String::new()
                            } else {
                                expected_newline.repeat(2)
                            },
                            template
                        )
                    );
                    assert_eq!(
                        document.selection(),
                        start + selection.start..start + selection.end
                    );
                    let target = document.selection();
                    document.undo(false);
                    assert_eq!(document.source.text(), raw);
                    assert_eq!(document.selection(), original_selection);
                    assert!(!document.dirty());
                    document.undo(true);
                    assert_eq!(document.source.text(), inserted);
                    assert_eq!(document.selection(), target);
                }
            }
        }
    }

    #[test]
    fn append_rejects_unclosed_structures_without_source_or_history_changes() {
        for raw in ["```rust\nlet a = 1;", "~~~\ncode", "<!-- unfinished"] {
            let mut document = new_document(raw);
            let snapshot = document.source.id();
            assert!(document
                .append_block(ryne_extensions::InsertBlock::Heading(2))
                .is_err());
            assert_eq!(document.source.id(), snapshot);
            assert_eq!(document.source.text(), raw);
            assert!(!document.dirty());
            assert!(document.history_len(false) == 0);
        }
    }

    #[test]
    fn appended_code_is_literal_and_templates_edit_in_their_own_context() {
        let mut document = new_document("- existing");
        document
            .append_block(ryne_extensions::InsertBlock::Code)
            .unwrap();
        document
            .insert_text(document.selection(), "**literal**")
            .unwrap();
        assert_eq!(
            document.source.text(),
            "- existing\n\n```\n**literal**\n```"
        );
        assert!(!document.display().spans.iter().any(|span| span.style.bold));
        document
            .append_block(ryne_extensions::InsertBlock::Table)
            .unwrap();
        document.insert_text(document.selection(), "新列").unwrap();
        assert_eq!(table::tables(&document.parse).len(), 1);
        assert!(document
            .source
            .text()
            .ends_with("| 新列 | 列二 |\n| --- | --- |\n|  |  |"));
    }
    #[test]
    fn inline_input_completes_after_body_and_keeps_caret_inside() {
        for (marker, close) in [
            ("*", "*"),
            ("**", "**"),
            ("***", "***"),
            ("_", "_"),
            ("__", "__"),
            ("___", "___"),
            ("~~", "~~"),
            ("`", "`"),
            ("``", "``"),
            ("[", "]()"),
            ("$", "$"),
        ] {
            for body in ["文字", "👩‍💻"] {
                let mut document = new_document("");
                for ch in marker.chars() {
                    document
                        .insert_text(document.selection(), &ch.to_string())
                        .unwrap();
                    document.normalize_heading_caret();
                }
                assert_eq!(document.source.text(), marker);
                document.insert_text(document.selection(), body).unwrap();
                document.normalize_heading_caret();
                let paired = format!("{marker}{body}{close}");
                assert_eq!(document.source.text(), paired, "{marker}");
                assert_eq!(document.head, marker.len() + body.len());
                assert_eq!(document.display().text, paired);
                assert!(document.reveal.is_some());
                document.insert_text(document.selection(), "续").unwrap();
                assert_eq!(document.source.text(), format!("{marker}{body}续{close}"));
                assert_eq!(document.head, marker.len() + body.len() + "续".len());
                document.undo(false);
                document.undo(false);
                assert_eq!(document.source.text(), marker);
                assert_eq!(document.head, marker.len());
                document.undo(true);
                assert_eq!(document.source.text(), paired);
                assert_eq!(document.head, marker.len() + body.len());
                let snapshot = document.source.id();
                let history = document.history_len(false);
                for ch in close.chars() {
                    document
                        .insert_text(document.selection(), &ch.to_string())
                        .unwrap();
                }
                assert_eq!(document.source.text(), paired);
                assert_eq!(document.head, paired.len());
                assert_eq!(document.source.id(), snapshot);
                assert_eq!(document.history_len(false), history);
                if marker != "$" {
                    assert_eq!(document.display().text, body);
                }
                document.undo(false);
                for _ in marker.chars() {
                    document.undo(false);
                }
                assert!(!document.dirty());
            }
        }
    }
    #[test]
    fn inline_completion_nests_remaps_and_reveals_ancestors() {
        let mut document = new_document("");
        for input in ["*", "*", "粗", " ", "_", "斜"] {
            document.insert_text(document.selection(), input).unwrap();
        }
        assert_eq!(document.source.text(), "**粗 _斜_**");
        assert_eq!(document.display().text, "**粗 _斜_**");
        assert_eq!(document.head, "**粗 _斜".len());
        document.insert_text(document.selection(), "_").unwrap();
        document.insert_text(document.selection(), "**").unwrap();
        assert_eq!(document.source.text(), "**粗 _斜_**");
        assert_eq!(document.display().text, "粗 斜");
        assert!(document
            .display()
            .spans
            .iter()
            .any(|span| span.style.bold && span.style.italic));

        let mut document = new_document("before ");
        document.select(7, false, false);
        for input in ["`", "a", "**literal**"] {
            document.insert_text(document.selection(), input).unwrap();
        }
        assert_eq!(document.source.text(), "before `a**literal**`");
        assert_eq!(document.closers.len(), 1);
        document.replace(0..0, "中").unwrap();
        document.select(document.source.text().len() - 1, false, false);
        document.insert_text(document.selection(), "`").unwrap();
        assert_eq!(document.source.text(), "中before `a**literal**`");
        assert_eq!(document.display().text, "中before a**literal**");
    }
    #[test]
    fn inline_completion_preserves_containers_cells_and_newlines() {
        for newline in ["\n", "\r\n", "\r"] {
            for raw in [
                "# \n\nend",
                "- \n\nend",
                "> \n\nend",
                "| a | b |\n|---|---|\n|  | c |\n",
            ] {
                let raw = raw.replace('\n', newline);
                let start = if raw.starts_with('|') {
                    raw.find("|  |").unwrap() + 2
                } else {
                    2
                };
                for marker in ["**", "_", "~~", "`", "``"] {
                    let mut document = new_document(&raw);
                    document.select(start, false, false);
                    for input in [marker, "中😀"] {
                        document.insert_text(document.selection(), input).unwrap();
                        document.normalize_heading_caret();
                    }
                    assert_eq!(
                        document.source.text(),
                        format!("{}{marker}中😀{marker}{}", &raw[..start], &raw[start..]),
                        "{marker} {raw:?}"
                    );
                    assert_eq!(document.head, start + marker.len() + "中😀".len());
                    document.select(document.source.text().len(), false, false);
                    assert!(document.display().text.contains("中😀"));
                    assert!(!document.display().text.contains(marker));
                    for _ in 0..2 {
                        document.undo(false);
                    }
                    assert_eq!(document.source.text(), raw);
                    assert!(!document.dirty());
                }
            }
        }
    }
    #[test]
    fn inline_completion_keeps_literals_escapes_selections_and_existing_markers() {
        for (raw, caret, input) in [
            ("\\**", 3, "a"),
            ("word_", 5, "a"),
            ("**", 2, " a"),
            ("**", 2, "a "),
            ("**", 2, "a\nb"),
            ("~", 1, "a"),
            ("`**`", 3, "a"),
            ("$**$", 3, "a"),
            ("```md\n**\n```", 8, "a"),
            ("    **", 6, "a"),
            ("<script>\n**\n</script>", 11, "a"),
            ("**bold**", 2, "a"),
            ("**bold**", 8, "a"),
        ] {
            let mut document = new_document(raw);
            document.select(caret, false, false);
            document.insert_text(document.selection(), input).unwrap();
            assert_eq!(
                document.source.text(),
                format!("{}{input}{}", &raw[..caret], &raw[caret..]),
                "{raw:?}"
            );
        }
        for local in [false, true] {
            let mut document = new_document("**");
            document.select(2, false, false);
            if local {
                assert!(document.toggle_block_source());
            } else {
                document.set_source_mode(true);
            }
            document.insert_text(document.selection(), "a").unwrap();
            assert_eq!(document.source.text(), "**a");
        }
        let mut document = new_document("**replace");
        document.select(2, false, false);
        document.select(9, true, false);
        document.insert_text(document.selection(), "body").unwrap();
        assert_eq!(document.source.text(), "**body");

        let mut document = new_document("**");
        document.select(2, false, false);
        document.insert_text(document.selection(), "a").unwrap();
        document.replace(0..2, "").unwrap();
        document.select(1, false, false);
        document.insert_text(document.selection(), "*").unwrap();
        assert_eq!(
            document.source.text(),
            "a***",
            "removing the opener makes its former closer literal"
        );
    }
    #[test]
    fn fence_enter_completes_once_and_preserves_container_context_and_history() {
        for newline in ["\n", "\r\n", "\r"] {
            for (prefix, continuation) in [
                ("", ""),
                ("   ", "   "),
                ("- ", "  "),
                ("> ", "> "),
                ("> - ", ">   "),
                ("- outer\n  - ", "    "),
            ] {
                let prefix = prefix.replace('\n', newline);
                for (marker, language) in [("```", "rust"), ("~~~~", ""), ("````", "mermaid")] {
                    let raw = format!("before{newline}{newline}{prefix}{marker}{language}");
                    let mut document = new_document(&raw);
                    document.select(raw.len(), false, false);
                    document.normalize_heading_caret();
                    assert_eq!(document.head, raw.len());
                    assert!(document.display().text.contains(marker));
                    document.line_break(false).unwrap();
                    let expected =
                        format!("{raw}{newline}{continuation}{newline}{continuation}{marker}");
                    assert_eq!(document.source.text(), expected, "{prefix:?}");
                    assert_eq!(
                        document.head,
                        raw.len() + newline.len() + continuation.len()
                    );
                    assert_eq!(document.display().code_blocks.len(), 1);
                    assert!(!document.display().text.contains(marker));
                    document
                        .insert_text(document.selection(), "**literal**")
                        .unwrap();
                    document.line_break(false).unwrap();
                    document
                        .insert_text(document.selection(), "_literal_")
                        .unwrap();
                    assert!(document
                        .source
                        .text()
                        .contains(&format!("**literal**{newline}{continuation}_literal_")));
                    assert_eq!(code_block::blocks(&document.parse).len(), 1);
                    for _ in 0..4 {
                        document.undo(false);
                    }
                    assert_eq!(document.source.text(), raw);
                    assert_eq!(document.head, raw.len());
                    assert!(!document.dirty());
                    document.undo(true);
                    assert_eq!(document.source.text(), expected);
                }
            }
        }
    }
    #[test]
    fn fence_typing_before_existing_newline_stays_source_until_enter() {
        for newline in ["\n", "\r\n", "\r"] {
            let raw = format!("{newline}{newline}tail");
            let mut document = new_document(&raw);
            for input in ["`", "`", "`", "r", "s"] {
                document.insert_text(document.selection(), input).unwrap();
                document.normalize_heading_caret();
            }
            assert_eq!(document.source.text(), format!("```rs{raw}"));
            assert!(document.display().text.starts_with("```rs\n"));
            assert_eq!(document.head, 5);
            document.line_break(false).unwrap();
            assert_eq!(
                document.source.text(),
                format!("```rs{newline}{newline}```{raw}")
            );
            assert_eq!(document.head, 5 + newline.len());
            assert!(document.display().text.ends_with("\n\ntail"));
            document.undo(false);
            assert_eq!(document.source.text(), format!("```rs{raw}"));
        }
        for source_mode in [false, true] {
            let mut document = new_document("```rust");
            document.select(7, false, false);
            if source_mode {
                document.set_source_mode(true);
            } else {
                assert!(document.toggle_block_source());
            }
            document.line_break(false).unwrap();
            assert_eq!(document.source.text(), "```rust\n");
        }
    }
    #[test]
    fn every_block_source_entry_is_presentation_only_and_preserves_history() {
        for newline in ["\n", "\r\n", "\r"] {
            for body in [
                "### title **bold**",
                "plain **bold**",
                "> quoted **bold**",
                "![alt](img.png)",
            ] {
                let raw = format!("{body}{newline}");
                let mut document = new_document(&raw);
                document.select(raw.find("bold").unwrap_or(4), false, false);
                let selection = document.selection();
                assert!(document.toggle_block_source());
                assert_eq!(
                    document.display().text,
                    raw.replace("\r\n", "\n").replace('\r', "\n")
                );
                assert_eq!(document.selection(), selection);
                assert!(document.history_len(false) == 0);
                assert!(!document.dirty());
                assert!(document.toggle_block_source());
                assert_eq!(document.source.text(), raw);
                document.replace(selection.clone(), "X").unwrap();
                assert!(document.toggle_block_source());
                assert!(document.toggle_block_source());
                document.undo(false);
                assert_eq!(document.source.text(), raw);
                assert!(!document.dirty());
                document.undo(true);
                assert!(document.dirty());
            }
        }
    }
    #[test]
    fn code_body_editing_keeps_fences_and_local_source_mode_is_reversible() {
        for newline in ["\n", "\r\n", "\r"] {
            let raw = format!("```rust{newline}  a{newline}  b{newline}```{newline}");
            let mut document = new_document(&raw);
            document.normalize_heading_caret();
            assert_eq!(document.head, raw.find("  a").unwrap());
            document.select(raw.find('a').unwrap() + 1, false, false);
            document.line_break(false).unwrap();
            assert_eq!(
                document.source.text(),
                raw.replacen(&format!("a{newline}"), &format!("a{newline}  {newline}"), 1)
            );
            document.undo(false);
            assert_eq!(document.source.text(), raw);
            document.line_break(true).unwrap();
            assert_eq!(
                document.source.text(),
                raw.replacen(&format!("a{newline}"), &format!("a{newline}  {newline}"), 1)
            );
            document.undo(false);
            let revision = document.source.revision();
            let selection = document.selection();
            assert!(document.toggle_block_source());
            assert_eq!(
                document.display().text,
                raw.replace("\r\n", "\n").replace('\r', "\n")
            );
            assert_eq!(document.selection(), selection);
            assert_eq!(document.source.revision(), revision);
            assert!(!document.dirty());
            document.select(3, false, false);
            document.replace(3..7, "python").unwrap();
            document.undo(false);
            assert!(document.block_source.is_some());
            assert_eq!(document.source.text(), raw);
            document.undo(true);
            assert!(document.block_source.is_some());
            assert!(document.toggle_block_source());
            assert_eq!(document.display().code_blocks[0].source.language, "python");
            document.undo(false);
            assert_eq!(document.source.text(), raw);
        }
    }
    #[test]
    fn empty_and_nested_code_can_be_edited_without_breaking_closing_fences() {
        for (raw, expected) in [
            ("```\n```", "```\n中😀\n```"),
            ("> ```\n> ```\n", "> ```\n> 中😀\n> ```\n"),
            ("- ```\n  ```\n", "- ```\n  中😀\n  ```\n"),
        ] {
            let mut document = new_document(raw);
            document.normalize_heading_caret();
            let anchor = document.head;
            document.insert_text(anchor..anchor, "中😀").unwrap();
            assert_eq!(document.source.text(), expected);
            assert_eq!(document.head, anchor + "中😀".len());
            assert!(document.display().text.contains("中😀"));
            assert_eq!(document.display().code_blocks.len(), 1);
            document.undo(false);
            assert_eq!(document.source.text(), raw);
            document.undo(true);
            assert_eq!(document.source.text(), expected);
        }
    }
    #[test]
    fn code_line_join_preserves_outer_boundaries_and_removes_only_structural_indent() {
        for raw in ["    a\n    b\n", "> ```\n> a\n> b\n> ```\n"] {
            for forward in [false, true] {
                let mut document = new_document(raw);
                let first = code_block::blocks(&document.parse)[0].rows[0].content.start;
                let last = code_block::blocks(&document.parse)[0].rows[1].content.end;
                document.select(first, false, false);
                assert!(document.remove_code_boundary(false).unwrap());
                assert_eq!(document.source.text(), raw);
                document.select(last, false, false);
                assert!(document.remove_code_boundary(true).unwrap());
                assert_eq!(document.source.text(), raw);
                document.select(
                    if forward {
                        raw.find('a').unwrap() + 1
                    } else {
                        raw.find('b').unwrap()
                    },
                    false,
                    false,
                );
                assert!(document.remove_code_boundary(forward).unwrap());
                assert!(document.source.text().contains("ab"));
                assert_eq!(document.display().text, "ab\n");
                document.undo(false);
                assert_eq!(document.source.text(), raw);
            }
        }
    }
    #[test]
    fn nested_code_newline_and_multiline_paste_keep_container_context() {
        for (raw, expected) in [
            ("> ```\n> a\n> ```\n", "> ```\n> a\n> b\n> c\n> ```\n"),
            ("- ```\n  a\n  ```\n", "- ```\n  a\n  b\n  c\n  ```\n"),
            ("    a\n", "    a\n    b\n    c\n"),
        ] {
            let mut document = new_document(raw);
            document.select(raw.find('a').unwrap() + 1, false, false);
            document
                .insert_text(document.selection(), "\nb\nc")
                .unwrap();
            assert_eq!(document.source.text(), expected);
            document.undo(false);
            document.line_break(false).unwrap();
            assert_eq!(code_block::blocks(&document.parse).len(), 1);
            assert_eq!(document.display().text, "a\n\n");
        }
        for raw in ["> ```\n", "> ```\n> a\n"] {
            let mut document = new_document(raw);
            document.select(raw.len(), false, false);
            document.insert_text(document.selection(), "b").unwrap();
            assert_eq!(document.source.text(), format!("{raw}> b"));
            assert_eq!(document.display().code_blocks.len(), 1);
        }
        for raw in ["```\n```", "> ```\n> ```\n"] {
            let mut document = new_document(raw);
            document.normalize_heading_caret();
            document.line_break(false).unwrap();
            let blocks = code_block::blocks(&document.parse);
            assert_eq!(blocks[0].rows.len(), 2);
            assert!(blocks[0].closing.is_some());
            document.normalize_heading_caret();
            assert_eq!(document.head, blocks[0].rows[1].content.start);
            document.undo(false);
            assert_eq!(document.source.text(), raw);
        }
    }
    #[test]
    fn paragraph_breaks_and_cross_line_selection_are_atomic_and_preserve_line_endings() {
        for newline in ["\n", "\r\n", "\r"] {
            let raw = format!("abcd{newline}{newline}tail");
            let mut document = new_document(&raw);
            document.select(2, false, false);
            document.line_break(false).unwrap();
            assert_eq!(
                document.source.text(),
                format!("ab{newline}{newline}cd{newline}{newline}tail")
            );
            assert_eq!(document.head, 2 + newline.len() * 2);
            document.undo(false);
            assert_eq!(document.source.text(), raw);
            assert_eq!(document.head, 2);
            document.line_break(true).unwrap();
            assert_eq!(
                document.source.text(),
                format!("ab  {newline}cd{newline}{newline}tail")
            );
            document.undo(false);
            document.select(1, false, false);
            document.select(raw.find("tail").unwrap() + 2, true, true);
            let selection = document.selection();
            document.line_break(false).unwrap();
            assert_eq!(document.source.text(), format!("a{newline}{newline}il"));
            document.undo(false);
            assert_eq!(document.source.text(), raw);
            assert_eq!(document.selection(), selection);
        }
    }
    #[test]
    fn source_and_unsupported_blocks_keep_literal_newline_behavior() {
        for raw in ["abcd", "<div>abcd</div>", "```\nabcd\n```"] {
            for hard in [false, true] {
                let mut document = new_document(raw);
                let caret = raw.find("abcd").map_or(3, |start| start + 2);
                document.select(caret, false, false);
                document.set_source_mode(raw == "abcd");
                document.line_break(hard).unwrap();
                let mut expected = raw.to_owned();
                expected.insert(caret, '\n');
                assert_eq!(document.source.text(), expected);
            }
        }
        let mut document = new_document("");
        document.line_break(false).unwrap();
        assert_eq!(document.source.text(), "\n");
    }
    #[test]
    fn paragraph_join_restores_enter_and_undo_restores_selection() {
        for forward in [false, true] {
            let mut document = new_document("ab\r\n\r\ncd");
            document.select(if forward { 2 } else { 6 }, false, false);
            let caret = document.head;
            assert!(document.join_paragraphs(forward).unwrap());
            assert_eq!(document.source.text(), "abcd");
            assert_eq!(document.head, 2);
            document.undo(false);
            assert_eq!(document.source.text(), "ab\r\n\r\ncd");
            assert_eq!(document.head, caret);
        }
        let mut document = new_document("ab");
        document.select(2, false, false);
        document.line_break(false).unwrap();
        assert!(document.join_paragraphs(false).unwrap());
        assert_eq!(document.source.text(), "ab");
        document.set_source_mode(true);
        document.line_break(false).unwrap();
        assert!(!document.join_paragraphs(false).unwrap());
    }
    #[test]
    fn paragraph_join_does_not_remove_intervening_blocks() {
        for block in ["# Heading", "- item", "> quote", "```\ncode\n```", "---"] {
            let raw = format!("before\n\n{block}\n\nafter");
            for forward in [false, true] {
                let mut document = new_document(&raw);
                document.select(
                    if forward {
                        "before".len()
                    } else {
                        raw.find("after").unwrap()
                    },
                    false,
                    false,
                );
                assert!(!document.join_paragraphs(forward).unwrap());
                assert_eq!(document.source.text(), raw);
                assert!(!document.dirty());
            }
        }
    }
    #[test]
    fn typing_heading_hashes_waits_for_body_and_folds_atomically() {
        for level in 1..=6 {
            for first in ["标题", " Title", "\tTitle", "👩‍💻"] {
                let mut document = new_document("");
                for count in 1..=level {
                    document.insert_text(document.selection(), "#").unwrap();
                    document.normalize_heading_caret();
                    assert_eq!(document.source.text(), "#".repeat(count));
                    assert_eq!(document.display().text, "#".repeat(count));
                    assert_eq!(document.head, count);
                }
                let hashes = "#".repeat(level);
                let snapshot = document.source.id();
                document.insert_text(document.selection(), first).unwrap();
                document.normalize_heading_caret();
                let separator = if first.starts_with([' ', '\t']) {
                    ""
                } else {
                    " "
                };
                let raw = format!("{hashes}{separator}{first}");
                assert_eq!(document.source.text(), raw);
                assert_eq!(document.head, raw.len());
                assert_eq!(document.display().text, first.trim_start());
                assert!(document.heading_reveal.is_none());
                assert!(document
                    .display()
                    .spans
                    .iter()
                    .any(|span| span.style.heading == level as u8));
                assert_eq!(document.history_len(false), level + 1);
                assert_ne!(document.source.id(), snapshot);
                let display = document.display();
                assert_eq!(display.position_at(document.head), display.text.len());
                document.insert_text(document.selection(), "续").unwrap();
                assert_eq!(document.display().text, format!("{}续", first.trim_start()));
                document.undo(false);
                document.undo(false);
                assert_eq!(document.source.text(), hashes);
                assert_eq!(document.display().text, hashes);
                assert_eq!(document.head, level);
                document.undo(true);
                assert_eq!(document.source.text(), raw);
                assert_eq!(document.display().text, first.trim_start());
                for _ in 0..=level {
                    document.undo(false);
                }
                assert_eq!(document.source.text(), "");
                assert!(!document.dirty());
            }
        }
    }
    #[test]
    fn heading_input_preserves_containers_newlines_and_click_reveal() {
        for newline in ["\n", "\r\n", "\r"] {
            for prefix in ["", "   ", "- ", "> ", "> - ", "- outer\n  - "] {
                let prefix = prefix.replace('\n', newline);
                let raw = format!("before{newline}{newline}{prefix}{newline}{newline}after");
                let mut document = new_document(&raw);
                let start = "before".len() + 2 * newline.len() + prefix.len();
                document.select(start, false, false);
                for _ in 0..3 {
                    document.insert_text(document.selection(), "#").unwrap();
                    document.normalize_heading_caret();
                    assert!(
                        document.display().text.contains('#'),
                        "{prefix:?} {newline:?}"
                    );
                }
                document.insert_text(document.selection(), "标题").unwrap();
                assert_eq!(
                    document.source.text(),
                    format!("{}### 标题{}", &raw[..start], &raw[start..])
                );
                assert!(
                    !document.display().text.contains('#'),
                    "{prefix:?} {newline:?}"
                );
                assert_eq!(document.head, start + "### 标题".len());
                let snapshot = document.source.id();
                document.select(start + 4, false, false);
                document.reveal_clicked_heading();
                assert!(document.display().text.contains("### 标题"));
                assert_eq!(document.source.id(), snapshot);
                document.select(document.source.text().len(), false, false);
                assert!(!document.display().text.contains('#'));
                document.undo(false);
                assert_eq!(
                    document.source.text(),
                    format!("{}###{}", &raw[..start], &raw[start..])
                );
            }
        }
    }
    #[test]
    fn heading_input_keeps_literal_contexts_and_invalid_prefixes() {
        for (raw, caret) in [
            ("text #", 6),
            ("#######", 7),
            ("\\#", 2),
            ("# #", 3),
            ("## ##", 5),
            ("    #", 5),
            ("`#`", 2),
            ("```md\n#\n```\n", 7),
            ("~~~\n#\n~~~\n", 5),
            ("<script>\n#\n</script>", 10),
            ("| a | b |\n|---|---|\n| # | c |\n", 22),
        ] {
            let mut document = new_document(raw);
            document.select(caret, false, false);
            document.insert_text(document.selection(), "中").unwrap();
            assert_eq!(
                document.source.text(),
                format!("{}中{}", &raw[..caret], &raw[caret..]),
                "{raw:?}"
            );
        }
        for local in [false, true] {
            let mut document = new_document("##");
            document.select(2, false, false);
            if local {
                assert!(document.toggle_block_source());
            } else {
                document.set_source_mode(true);
            }
            document.insert_text(document.selection(), "中").unwrap();
            assert_eq!(document.source.text(), "##中");
            assert_eq!(document.display().text, "##中");
        }
        for input in ["#######a", "##pasted", "##\nbody"] {
            let mut document = new_document("");
            document.insert_text(0..0, input).unwrap();
            assert_eq!(document.source.text(), input);
        }
        let mut document = new_document("#");
        document.select(1, false, false);
        document.insert_text(1..1, "##中").unwrap();
        assert_eq!(document.source.text(), "### 中");
        assert_eq!(document.display().text, "中");
    }
    #[test]
    fn heading_input_space_finishes_prefix_and_seventh_hash_stays_literal() {
        let mut document = new_document("");
        for input in ["#", "#", " "] {
            document.insert_text(document.selection(), input).unwrap();
            document.normalize_heading_caret();
        }
        assert_eq!(document.source.text(), "## ");
        assert_eq!(document.display().text, "");
        assert!(document.heading_reveal.is_none());
        document.insert_text(document.selection(), "正文").unwrap();
        assert_eq!(document.source.text(), "## 正文");
        assert_eq!(document.display().text, "正文");
        document.undo(false);
        document.undo(false);
        assert_eq!(document.source.text(), "##");
        assert_eq!(document.display().text, "##");

        let mut document = new_document("");
        for _ in 0..7 {
            document.insert_text(document.selection(), "#").unwrap();
            document.normalize_heading_caret();
        }
        document.insert_text(document.selection(), "正文").unwrap();
        assert_eq!(document.source.text(), "#######正文");
        assert_eq!(document.display().text, "#######正文");
        assert!(document
            .parse
            .nodes
            .iter()
            .all(|node| !matches!(node.kind, NodeKind::Heading(_))));

        let mut document = new_document("## existing");
        document.select(2, false, false);
        document.reveal_clicked_heading();
        document.insert_text(2..2, "#").unwrap();
        assert_eq!(document.display().text, "### existing");
        document.select(0, false, false);
        document.select(3, true, false);
        document.insert_text(document.selection(), "plain").unwrap();
        assert_eq!(document.source.text(), "plain existing");
        assert_eq!(document.display().text, "plain existing");
    }
    #[test]
    fn heading_click_changes_only_presentation_and_tracks_edits() {
        let raw = "# **标题** #\r\n\r\n## Other\n";
        let mut document = new_document(raw);
        document.normalize_heading_caret();
        let snapshot = document.source.id();
        let selection = document.selection();
        document.reveal_clicked_heading();
        assert_eq!(document.display().text, "# 标题 #\n\nOther\n");
        assert_eq!(document.source.id(), snapshot);
        assert_eq!(document.selection(), selection);
        assert!(!document.dirty());
        document.select(0, false, false);
        document.normalize_heading_caret();
        assert_eq!(document.head, 0);
        document.replace(0..0, "#").unwrap();
        assert_eq!(document.display().text, "## 标题 #\n\nOther\n");
        document.undo(false);
        assert_eq!(document.source.text(), raw);
        document.clear_heading_reveal();
        assert_eq!(document.display().text, "标题\n\nOther\n");
        assert!(!document.dirty());
    }
    #[test]
    fn heading_click_session_folds_on_caret_exit_and_preserves_history() {
        for heading in [
            "## 标题 ##\r\n",
            "标题\r\n====\r\n",
            "标题\r\n----\r\n",
            "- ### 标题\r\n",
        ] {
            let raw = format!("{heading}\r\n正文");
            let mut document = new_document(&raw);
            let title = raw.find("标题").unwrap();
            document.select(title, false, false);
            document.normalize_heading_caret();
            assert!(!document.display().text.contains('#'));
            assert!(!document.display().text.contains("===="));
            assert!(!document.display().text.contains("----"));
            let snapshot = document.source.id();
            document.reveal_clicked_heading();
            assert!(document.heading_reveal.is_some());
            assert!(document.display().text.contains(
                &heading
                    .trim_start_matches("- ")
                    .trim_end()
                    .replace("\r\n", "\n")
            ));
            assert_eq!(document.source.id(), snapshot);
            assert!(!document.dirty());
            document.select(title + "标".len(), false, false);
            assert!(document.heading_reveal.is_some());
            document.select(raw.find("正文").unwrap(), false, false);
            assert!(document.heading_reveal.is_none());
            document.select(title, false, false);
            assert!(
                document.heading_reveal.is_none(),
                "keyboard entry does not reveal markers"
            );
            assert!(document.history_len(false) == 0);
            assert!(document.history_len(true) == 0);
            assert_eq!(document.source.text(), raw);
        }
    }
    #[test]
    fn heading_body_edit_keeps_structure_and_source_mode_exposes_markers() {
        let mut document = new_document("## 标题 ##\r\n\r\n正文");
        let original = document.source.text().to_owned();
        document.normalize_heading_caret();
        assert_eq!(document.head, 3);
        assert_eq!(document.display().text, "标题\n\n正文");
        document.replace(3..3, "新").unwrap();
        assert_eq!(document.source.text(), "## 新标题 ##\r\n\r\n正文");
        document.undo(false);
        assert_eq!(document.source.text(), original);
        assert_eq!(document.head, 3);
        document.select(original.find("\r\n").unwrap(), false, false);
        document.normalize_heading_caret();
        assert_eq!(document.head, original.find(" ##").unwrap());
        document.set_source_mode(true);
        document.select(0, false, false);
        document.normalize_heading_caret();
        assert_eq!(document.head, 0);
        assert_eq!(document.display().text, original.replace("\r\n", "\n"));
        assert!(!document.dirty());
    }
    #[test]
    fn source_session_survives_syntax_changes_and_history_restores_bytes() {
        let mut document = new_document("**b**\r\n");
        document.select(2, false, false);
        document.replace(2..3, "").unwrap();
        assert_eq!(document.display().text, "****\n");
        document.replace(2..2, "中").unwrap();
        assert_eq!(document.source.text(), "**中**\r\n");
        document.undo(false);
        document.undo(false);
        assert_eq!(document.source.text(), "**b**\r\n");
        assert!(!document.dirty());
        document.undo(true);
        assert_eq!(document.source.text(), "****\r\n");
    }
    #[test]
    fn save_and_open_preserve_source_and_failed_save_keeps_dirty_state() {
        let path = std::env::temp_dir().join(format!("ryne-test-{}.md", std::process::id()));
        let mut document = new_document("\u{feff}# **中**\r\n\r\ntrailing  \n");
        document
            .replace(
                document.source.text().len()..document.source.text().len(),
                "😀",
            )
            .unwrap();
        assert!(document.save(&path.join("missing/file.md")).is_err());
        assert!(document.dirty());
        document.save(&path).unwrap();
        let reopened = open_document(&path).unwrap();
        assert_eq!(reopened.source.text(), document.source.text());
        assert!(!document.dirty());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn list_enter_continues_numbers_bullets_and_retains_line_endings_and_undo() {
        for newline in ["\n", "\r\n", "\r"] {
            for (raw, expected) in [
                (
                    format!("- 中😀{newline}- end"),
                    format!("- 中{newline}- 😀{newline}- end"),
                ),
                (
                    format!("3) 中😀{newline}1) end"),
                    format!("3) 中{newline}4) 😀{newline}1) end"),
                ),
                (
                    format!("> - 中😀{newline}> - end"),
                    format!("> - 中{newline}> - 😀{newline}> - end"),
                ),
                (
                    format!("- outer{newline}  + 中😀"),
                    format!("- outer{newline}  + 中{newline}  + 😀"),
                ),
            ] {
                let mut document = new_document(&raw);
                let caret = raw.find('😀').unwrap();
                document.select(caret, false, false);
                document.line_break(false).unwrap();
                assert_eq!(document.source.text(), expected);
                document.undo(false);
                assert_eq!(document.source.text(), raw);
                assert_eq!(document.head, caret);
                document.undo(true);
                assert_eq!(document.source.text(), expected);
            }
        }
    }
    #[test]
    fn empty_items_exit_one_level_and_source_mode_is_literal() {
        for (raw, expected) in [
            ("- ", ""),
            ("1. ", ""),
            ("- a\n  - b\n  - ", "- a\n  - b\n- "),
            ("> - ", "> "),
        ] {
            let mut document = new_document(raw);
            document.select(raw.len(), false, false);
            document.line_break(false).unwrap();
            assert_eq!(document.source.text(), expected);
            document.undo(false);
            assert_eq!(document.source.text(), raw);
        }
        let mut document = new_document("- a");
        document.select(3, false, false);
        let revision = document.source.revision();
        assert!(document.toggle_block_source());
        assert_eq!(document.source.revision(), revision);
        assert!(!document.dirty());
        assert!(document.display().lists.is_empty());
        document.line_break(false).unwrap();
        assert_eq!(document.source.text(), "- a\n");
        document.undo(false);
        assert!(document.toggle_block_source());
        assert_eq!(document.display().text, "a");
    }
    #[test]
    fn list_indent_moves_subtree_in_one_transaction_and_restores_selection() {
        for newline in ["\n", "\r\n", "\r"] {
            let raw = format!("- a{newline}- b{newline}  - 中😀{newline}- end{newline}");
            let expected = format!("- a{newline}  - b{newline}    - 中😀{newline}- end{newline}");
            let mut document = new_document(&raw);
            let caret = raw.find('b').unwrap();
            document.select(caret, false, false);
            document.select(caret + 1, true, false);
            assert!(document.indent_list(false).unwrap());
            assert_eq!(document.source.text(), expected);
            assert_eq!(&document.source.text()[document.selection()], "b");
            document.undo(false);
            assert_eq!(document.source.text(), raw);
            assert_eq!(document.selection(), caret..caret + 1);
            document.undo(true);
            assert!(document.indent_list(true).unwrap());
            assert_eq!(document.source.text(), raw);
        }
        let mut document = new_document("- first");
        document.select(3, false, false);
        assert!(document.indent_list(false).unwrap());
        assert_eq!(document.source.text(), "- first");
        assert!(!document.dirty());
    }
    #[test]
    fn list_continuations_paste_and_boundary_commands_protect_neighbors() {
        let raw = "- a\r\n- 中😀\r\n\r\nparagraph";
        let mut document = new_document(raw);
        let caret = raw.find('😀').unwrap();
        document.select(caret, false, false);
        document.line_break(true).unwrap();
        assert_eq!(
            document.source.text(),
            "- a\r\n- 中  \r\n  😀\r\n\r\nparagraph"
        );
        document.undo(false);
        document.insert_text(caret..caret, "x\ny").unwrap();
        assert_eq!(
            document.source.text(),
            "- a\r\n- 中x\n  y😀\r\n\r\nparagraph"
        );
        document.undo(false);
        document.select(3, false, false);
        assert!(document.remove_list_boundary(true).unwrap());
        assert_eq!(document.source.text(), "- a中😀\r\n\r\nparagraph");
        document.undo(false);
        let end = raw.find("\r\n\r\n").unwrap();
        document.select(end, false, false);
        assert!(document.remove_list_boundary(true).unwrap());
        assert_eq!(document.source.text(), raw);
        document.select(raw.find('中').unwrap(), false, false);
        assert!(document.remove_list_boundary(false).unwrap());
        assert_eq!(document.source.text(), "- a\r\n中😀\r\n\r\nparagraph");
        document.undo(false);
        assert_eq!(document.source.text(), raw);
    }
    #[test]
    fn list_indentation_retains_quote_and_literal_code_context() {
        let raw = "> - a\n> - b\n>   ```rust\n>   **code**\n>   ```\n";
        let mut document = new_document(raw);
        document.select(raw.find('b').unwrap(), false, false);
        assert!(document.indent_list(false).unwrap());
        assert_eq!(
            document.source.text(),
            "> - a\n>   - b\n>     ```rust\n>     **code**\n>     ```\n"
        );
        assert_eq!(document.display().code_blocks.len(), 1);
        assert!(document.display().text.contains("**code**"));
        assert!(document.indent_list(true).unwrap());
        assert_eq!(document.source.text(), raw);
        document.undo(false);
        document.undo(false);
        assert_eq!(document.source.text(), raw);
    }
    #[test]
    fn indenting_empty_item_keeps_it_a_list_and_does_not_create_a_heading() {
        for (raw, expected) in [
            ("- a\n- ", "- a\n\n  - "),
            ("> - a\n> - ", "> - a\n> \n>   - "),
        ] {
            let mut document = new_document(raw);
            document.select(raw.len(), false, false);
            assert!(document.indent_list(false).unwrap());
            assert_eq!(document.source.text(), expected);
            assert_eq!(list::items(&document.parse).len(), 2);
            assert!(!document
                .parse
                .nodes
                .iter()
                .any(|n| matches!(n.kind, NodeKind::Heading(_))));
            document.line_break(false).unwrap();
            assert!(document.source.text().ends_with("- "));
            assert_eq!(list::items(&document.parse).last().unwrap().parent, None);
            document.undo(false);
            document.undo(false);
            assert_eq!(document.source.text(), raw);
        }
    }
    #[test]
    fn list_boundary_commands_keep_hidden_inline_and_heading_markers_intact() {
        for raw in ["- # title\n- next", "- **title**\n- next"] {
            let mut document = new_document(raw);
            document.select(2, false, false);
            document.normalize_heading_caret();
            assert!(document.remove_list_boundary(false).unwrap());
            assert_eq!(document.source.text(), &raw[2..]);
            document.undo(false);
            let caret = raw.find("\n").unwrap();
            document.select(caret, false, false);
            assert!(document.remove_list_boundary(true).unwrap());
            assert!(
                document.source.text().ends_with("titlenext")
                    || document.source.text().ends_with("**next")
            );
            document.undo(false);
            assert_eq!(document.source.text(), raw);
        }
    }
    #[test]
    fn tab_prefixes_continue_at_tab_stops_and_outdent_without_losing_content() {
        let raw = "-\ta\n\t- 中😀\n\t  continuation";
        let mut document = new_document(raw);
        document.select(raw.find("中😀").unwrap(), false, false);
        assert!(document.indent_list(true).unwrap());
        assert_eq!(document.source.text(), "-\ta\n- 中😀\n  continuation");
        document.undo(false);
        assert_eq!(document.source.text(), raw);
        document.select(3, false, false);
        document.line_break(true).unwrap();
        assert_eq!(
            document.source.text(),
            "-\ta  \n    \n\t- 中😀\n\t  continuation"
        );
        document.undo(false);
        assert_eq!(document.source.text(), raw);
    }
    #[test]
    fn hidden_quote_and_list_prefixes_are_skipped_and_local_source_remains_editable() {
        let mut document = new_document("> - body\n>   continuation");
        document.normalize_heading_caret();
        assert_eq!(document.head, 4);
        document.select(2, false, false);
        document.normalize_heading_caret();
        assert_eq!(document.head, 4);
        let revision = document.source.revision();
        document.select(0, false, false);
        assert!(document.toggle_block_source());
        assert_eq!(document.display().text, document.source.text());
        assert_eq!(document.source.revision(), revision);
        assert!(!document.dirty());
    }
    #[test]
    fn ordered_continuation_preserves_children_when_marker_width_changes() {
        for (raw, expected) in [
            ("9. ab\n   - child", "9. a\n10. b\n    - child"),
            (
                "3. first\n1. ab\n   - child",
                "3. first\n1. a\n5. b\n   - child",
            ),
            (
                "999999999. ab\n           - child",
                "999999999. a\n1. b\n   - child",
            ),
        ] {
            let mut document = new_document(raw);
            document.select(raw.find("ab").unwrap() + 1, false, false);
            document.line_break(false).unwrap();
            assert_eq!(document.source.text(), expected);
            assert!(list::items(&document.parse)
                .last()
                .unwrap()
                .parent
                .is_some());
            document.undo(false);
            assert_eq!(document.source.text(), raw);
        }
    }
    #[test]
    fn table_cell_editing_preserves_structure_unicode_and_atomic_undo() {
        let raw = "| A | B |\r\n| --- | ---: |\r\n| 中😀 | |\r\n";
        let mut document = new_document(raw);
        let caret = raw.find('😀').unwrap();
        document.select(caret, false, false);
        document.insert_text(caret..caret, "x|y\nz").unwrap();
        assert!(document.source.text().contains("中x\\|y<br>z😀"));
        assert_eq!(document.display().tables.len(), 1);
        assert!(document.display().text.contains("中x|y\nz😀"));
        document.undo(false);
        assert_eq!(document.source.text(), raw);
        assert_eq!(document.head, caret);
        document.undo(true);
        assert_eq!(document.display().tables.len(), 1);
        document.undo(false);
        let cell = table::tables(&document.parse)[0].rows[1].cells[1]
            .content
            .start;
        document.select(cell, false, false);
        document.insert_text(cell..cell, "文").unwrap();
        assert_eq!(document.display().tables.len(), 1);
        assert!(document.display().text.ends_with("中😀\t文\n"));
        document.undo(false);
        assert_eq!(document.source.text(), raw);
    }
    #[test]
    fn table_navigation_is_clean_and_appending_rows_preserves_newlines_and_containers() {
        for newline in ["\n", "\r\n", "\r"] {
            for (first_prefix, prefix) in [("", ""), ("> ", "> "), ("- ", "  ")] {
                let raw = format!(
                    "{first_prefix}| A | B |{newline}{prefix}| --- | --- |{newline}{prefix}| x | y |{newline}"
                );
                let mut document = new_document(&raw);
                let table = table::tables(&document.parse).remove(0);
                document.select(table.rows[0].cells[0].content.start, false, false);
                let revision = document.source.revision();
                assert!(document.navigate_table(false, false).unwrap());
                assert_eq!(document.head, table.rows[0].cells[1].content.start);
                assert_eq!(document.source.revision(), revision);
                assert!(!document.dirty());
                assert!(document.navigate_table(true, false).unwrap());
                assert_eq!(document.head, table.rows[0].cells[0].content.start);
                document.line_break(false).unwrap();
                assert_eq!(document.head, table.rows[1].cells[0].content.start);
                document.line_break(false).unwrap();
                assert_eq!(
                    document.source.text(),
                    format!("{raw}{prefix}|  |  |{newline}")
                );
                assert_eq!(document.display().tables[0].rows.len(), 3);
                document.undo(false);
                assert_eq!(document.source.text(), raw);
                document.undo(true);
                assert_eq!(document.display().tables[0].rows.len(), 3);
            }
        }
    }
    #[test]
    fn table_boundary_and_source_entry_keep_markers_and_container_context() {
        let raw = "> | A | B |\n> | --- | --- |\n> | x | y |\n\nparagraph";
        let mut document = new_document(raw);
        let table = table::tables(&document.parse).remove(0);
        document.select(table.rows[1].cells[0].content.start, false, false);
        assert!(document.remove_table_boundary(false));
        document.select(table.rows[1].cells[0].content.end, false, false);
        assert!(document.remove_table_boundary(true));
        let revision = document.source.revision();
        assert!(document.toggle_block_source());
        assert!(document.display().text.contains("> | --- | --- |"));
        assert_eq!(document.source.revision(), revision);
        assert!(!document.dirty());
        let caret = document.head;
        document.line_break(false).unwrap();
        assert_eq!(
            document.source.text(),
            format!("{}\n{}", &raw[..caret], &raw[caret..])
        );
        document.undo(false);
        assert!(document.block_source.is_some());
        assert!(document.toggle_block_source());
        assert_eq!(document.display().tables.len(), 1);
        document.select(raw.len(), false, false);
        assert!(!document.navigate_table(false, false).unwrap());
    }
    #[test]
    fn table_cell_breaks_backslashes_and_compact_separators_are_atomic() {
        let raw = "|A|B|\n|---|---|\n|x\\|y|";
        let mut document = new_document(raw);
        // The backslash initially escapes a pipe, leaving too few cells: source fallback.
        assert!(document.display().tables.is_empty());
        let raw = "|A|B|\n|---|---|\n|x|y|";
        document = new_document(raw);
        let at = raw.find('x').unwrap() + 1;
        document.select(at, false, false);
        document.insert_text(at..at, "\\").unwrap();
        assert!(document.source.text().contains("|x\\ |y|"));
        assert_eq!(document.display().tables.len(), 1);
        let at = document.head;
        document.insert_text(at..at, "|").unwrap();
        assert_eq!(document.display().tables.len(), 1);
        assert!(document.display().text.contains("x|"));
        document.undo(false);
        document.undo(false);
        assert_eq!(document.source.text(), raw);
        document.select(raw.find('x').unwrap(), false, false);
        document.line_break(true).unwrap();
        assert!(document.display().text.contains("\nx"));
        assert!(document.source.text().contains("<br>x"));
        document.undo(false);
        assert_eq!(document.source.text(), raw);
        let last = table::tables(&document.parse)[0].rows[1].cells[1]
            .content
            .start;
        document.select(last, false, false);
        document.navigate_table(false, false).unwrap();
        assert_eq!(document.source.text(), format!("{raw}\n|  |  |"));
        document.undo(false);
        assert_eq!(document.source.text(), raw);
    }
    #[test]
    fn table_appending_rows_preserves_tabbed_list_content_column() {
        let raw = "-\t| A | B |\n    | --- | --- |\n    | x | y |\n";
        let mut document = new_document(raw);
        let tables = table::tables(&document.parse);
        assert_eq!(tables.len(), 1);
        let table = &tables[0];
        assert_eq!(table.continuation, "    ");
        document.select(table.rows[1].cells[1].content.start, false, false);
        document.navigate_table(false, false).unwrap();
        assert_eq!(document.source.text(), format!("{raw}    |  |  |\n"));
        assert_eq!(document.display().tables[0].rows.len(), 3);
        document.undo(false);
        assert_eq!(document.source.text(), raw);
    }
}

#[cfg(test)]
mod math_tests {
    use super::*;

    #[test]
    fn reveal_edit_undo_redo_and_save_preserve_original_math_bytes() {
        let raw = "> - 中文 $\\frac{x_1}{y}$ 😀\r\n\r\n`$literal$`\r\n";
        let mut document = new_document(raw);
        let position = raw.find("x_1").unwrap();
        document.select(position, false, false);
        assert!(!document.dirty());
        assert!(document.display().text.contains("$\\frac{x_1}{y}$"));
        document.select(0, false, false);
        assert_eq!(document.display().formulas.len(), 1);
        assert!(!document.dirty());
        document.select(position, false, false);
        document
            .insert_text(position..position + 3, "中文^2")
            .unwrap();
        assert!(document.source.text().contains("$\\frac{中文^2}{y}$"));
        document.undo(false);
        assert_eq!(document.source.text(), raw);
        assert!(!document.dirty());
        document.undo(true);
        assert!(document.source.text().contains("$\\frac{中文^2}{y}$"));
        let path = std::env::temp_dir().join(format!("ryne-math-test-{}.md", std::process::id()));
        document.save(&path).unwrap();
        assert_eq!(
            open_document(&path).unwrap().source.text(),
            document.source.text()
        );
        std::fs::remove_file(path).unwrap();
    }
}

mod highlighting {
    use super::*;
    #[test]
    fn view_changes_preserve_source_history_selection_and_maps() {
        let raw = "> ```rust\r\n> let s = \"中😀\";\r\n> ```\r\n\r\nafter\r\n";
        let mut document = new_document(raw);
        let mut highlighter = CodeHighlighter::default();
        let revision = document.source.revision();
        for source_mode in [false, true, false] {
            document.set_source_mode(source_mode);
            document.select(raw.find("中😀").unwrap(), false, false);
            let selection = document.selection();
            let highlighted = document.highlighted_display(&mut highlighter);
            let plain = document.display();
            assert_eq!(highlighted.text, plain.text);
            for (position, _) in highlighted.text.char_indices() {
                for affinity in [
                    ryne_parser::Affinity::Upstream,
                    ryne_parser::Affinity::Downstream,
                ] {
                    assert_eq!(
                        highlighted.source_at(position, affinity),
                        plain.source_at(position, affinity)
                    );
                }
            }
            for (position, _) in raw.char_indices() {
                assert_eq!(
                    highlighted.position_at(position),
                    plain.position_at(position)
                );
            }
            assert!(highlighted
                .spans
                .iter()
                .any(|span| span.style.syntax == Some(Class::String)));
            assert_eq!(document.selection(), selection);
            assert_eq!(document.source.revision(), revision);
            assert!(!document.dirty());
        }
        let at = raw.find("\"中😀\"").unwrap();
        assert!(document.toggle_block_source());
        let local = document.highlighted_display(&mut highlighter);
        assert_eq!(local.text, document.display().text);
        assert!(local.text.contains("> ```rust"));
        assert!(local
            .spans
            .iter()
            .any(|span| span.style.syntax == Some(Class::String)));
        assert!(document.toggle_block_source());
        assert_eq!(document.source.revision(), revision);
        assert!(!document.dirty());
        document.replace(at..at + "\"中😀\"".len(), "42").unwrap();
        let changed = document.highlighted_display(&mut highlighter);
        assert!(changed
            .spans
            .iter()
            .any(|span| &changed.text[span.display.clone()] == "42"
                && span.style.syntax == Some(Class::Constant)));
        document.undo(false);
        assert_eq!(document.source.text(), raw);
        assert!(!document.dirty());
        assert!(document
            .highlighted_display(&mut highlighter)
            .spans
            .iter()
            .any(|span| span.style.syntax == Some(Class::String)));
        document.undo(true);
        assert!(document
            .highlighted_display(&mut highlighter)
            .spans
            .iter()
            .any(|span| span.style.syntax == Some(Class::Constant)));
    }
}

mod formula_editing {
    use super::*;
    #[test]
    fn formulary_source_fallback_preserves_unicode_mapping_and_undo() {
        for newline in ["\n", "\r\n", "\r"] {
            let raw = format!(
                "前😀 $\\frac{{a}}{{b}}$ 后{newline}{newline}- $x_{{\\text{{中文}}}}$ 尾{newline}"
            );
            let mut document = new_document(&raw);
            let display = document.display();
            assert_eq!(display.formulas.len(), 1);
            assert!(display.text.contains("$x_{\\text{中文}}$"));
            let source = raw.find("中文").expect("fallback text");
            let shown = display.text.find("中文").expect("visible fallback text");
            assert_eq!(display.position_at(source), shown);
            assert_eq!(display.source_at(shown, Affinity::Downstream), source);
            document.select(source, false, false);
            assert_eq!(document.display().formulas.len(), 1);
            assert_eq!(document.source.text(), raw);
            assert!(!document.dirty());
            document
                .replace(source..source + "中文".len(), "y")
                .expect("edit");
            document.select(0, false, false);
            assert_eq!(document.display().formulas.len(), 2);
            document.undo(false);
            assert_eq!(document.source.text(), raw);
            assert!(!document.dirty());
            document.undo(true);
            document.select(0, false, false);
            assert_eq!(document.display().formulas.len(), 2);
            assert!(document.dirty());
        }
    }
}

mod mixed_highlighting {
    use super::*;
    #[test]
    fn incomplete_code_and_multiple_languages_keep_highlights_local() {
        let mut highlighter = CodeHighlighter::default();
        let raw = "```rust\nfn main() {\n    let s = \"未完成😀\n";
        let document = new_document(raw);
        let display = document.highlighted_display(&mut highlighter);
        assert!(display
            .spans
            .iter()
            .any(|span| span.style.syntax == Some(Class::Keyword)
                && display.text[span.display.clone()].contains("fn")));
        assert!(display.text.contains("未完成😀"));
        assert_eq!(document.source.text(), raw);

        let raw = "```rust\nlet n = 42;\n```\n\n```python\n# 注释😀\ns = 'hello'\n```\n\n```unknown\n**plain**\n```\n\n`let n = 42`\n";
        let document = new_document(raw);
        let display = document.highlighted_display(&mut highlighter);
        assert_eq!(display.code_blocks.len(), 3);
        assert!(display
            .spans
            .iter()
            .any(|span| span.style.syntax == Some(Class::Comment)
                && display.text[span.display.clone()].contains("注释😀")));
        let unknown = &display.code_blocks[2].source.range;
        assert!(display
            .spans
            .iter()
            .filter(|span| unknown.contains(&span.source.start))
            .all(|span| span.style.syntax.is_none() && !span.style.bold));
        assert!(display
            .spans
            .iter()
            .filter(|span| span.style.code)
            .all(|span| span.style.syntax.is_none()));
        assert!(display.text.contains("**plain**"));
        assert_eq!(highlighter.cache_entries(), 2);
        assert!(!document.dirty());
    }
}

#[test]
fn localized_append_preserves_unicode_selection_newlines_and_single_undo() {
    use ryne_extensions::{EditIntent, TemplateLabels};
    for labels in [
        TemplateLabels {
            text: "Texte",
            heading: "Titre",
            list_item: "Élément",
            todo: "Tâche",
            quote: "Citation",
            start: "Début",
            end: "Fin",
            column_one: "Colonne 1",
            column_two: "Colonne 2",
            image_description: "Description",
            image_url: "url-image",
        },
        TemplateLabels {
            text: "テキスト",
            heading: "見出し",
            list_item: "項目😀",
            todo: "タスク",
            quote: "引用",
            start: "開始",
            end: "終了",
            column_one: "列 1",
            column_two: "列 2",
            image_description: "画像",
            image_url: "画像URL",
        },
    ] {
        for newline in ["\n", "\r\n", "\r"] {
            let raw = format!("> - 原文😀{newline}  continuation{newline}");
            for &(_, kind) in ITEMS {
                let mut document = new_document(&raw);
                document.select(2, false, false);
                let original = document.selection();
                document
                    .dispatch(EditIntent::AppendLocalized {
                        template: kind,
                        labels: labels.clone(),
                    })
                    .unwrap();
                let (template, selection) = kind.template_with_labels(newline, &labels);
                let start = raw.len() + 2 * newline.len();
                assert_eq!(
                    document.source.text(),
                    format!("{raw}{}{template}", newline.repeat(2))
                );
                assert_eq!(
                    document.selection(),
                    start + selection.start..start + selection.end
                );
                assert!(document.source.text().is_char_boundary(document.head));
                let inserted = document.source.text().to_owned();
                document.undo(false);
                assert_eq!(document.source.text(), raw);
                assert_eq!(document.selection(), original);
                assert!(!document.dirty());
                document.undo(true);
                assert_eq!(document.source.text(), inserted);
            }
        }
    }
}

#[test]
fn formula_projection_reuses_more_than_256_live_resources_without_document_edits() {
    let raw = (0..257)
        .map(|i| format!("$x_{{{i}}}$\r\n\r\n"))
        .collect::<String>();
    let doc = new_document(&raw);
    let first = doc.display();
    assert_eq!(first.formulas.len(), 257);
    let second = doc.display();
    assert_eq!(second.formulas.len(), 257);
    for (a, b) in first.formulas.iter().zip(&second.formulas) {
        assert!(std::sync::Arc::ptr_eq(&a.formula, &b.formula));
        assert_eq!(a.source, b.source);
        assert_eq!(a.display, b.display);
    }
    assert_eq!(doc.source.text(), raw);
    assert_eq!(doc.history_len(false), 0);
    assert!(!doc.dirty());
}

#[test]
fn large_table_event_projection_keeps_empty_unicode_and_substitution_boundaries() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw = format!(
            "| A | B | C |{newline}| --- | --- | --- |{newline}{}",
            format!("| 中😀 | | a\\|b<br>c |{newline}").repeat(400)
        );
        let doc = new_document(&raw);
        let view = doc.display();
        let table = &view.tables[0];
        assert_eq!(table.rows.len(), 401);
        for (row_index, row) in table.rows.iter().enumerate().skip(1) {
            assert_eq!(&view.text[row[0].clone()], "中😀");
            assert!(row[1].is_empty());
            assert_eq!(&view.text[row[2].clone()], "a|b\nc");
            for (c, cell) in row.iter().enumerate() {
                let source = &table.source.rows[row_index].cells[c].content;
                assert_eq!(view.position_at(source.start), cell.start);
                assert_eq!(view.position_at(source.end), cell.end);
            }
        }
        assert_eq!(doc.source.text(), raw);
        assert!(!doc.dirty());
    }
}
