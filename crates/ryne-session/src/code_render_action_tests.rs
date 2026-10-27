use super::regression_tests::new_document;
use super::*;
use ryne_services::highlight::{Class, CodeHighlighter};

fn request(doc: &EditorSession, command: &'static str) -> RenderActionRequest {
    let control = doc
        .render_controls()
        .unwrap()
        .into_iter()
        .find(|c| c.provider == "code")
        .unwrap();
    RenderActionRequest {
        provider: control.provider,
        target: control.target,
        command,
    }
}
#[test]
fn fence_language_preserves_body_metadata_selection_containers_and_history() {
    for newline in ["\n", "\r\n", "\r"] {
        for (opening, prefix) in [("```", ""), ("~~~~", ""), ("> - ````", ">   ")] {
            let closing = if opening.ends_with("````") {
                "````"
            } else {
                opening
            };
            let raw = format!(
                "before{newline}{newline}{opening}  python extra  {newline}{prefix}**中😀**{newline}{prefix}{closing}{newline}{newline}after"
            );
            let mut doc = new_document(&raw);
            let start = raw.find("中😀").unwrap();
            doc.select(start, false, false);
            doc.select(start + "中😀".len(), true, false);
            let selection = doc.selection();
            let stale = request(&doc, "code.language-json");
            doc.execute_render_action(&request(&doc, "code.language-rust"))
                .unwrap();
            assert_eq!(doc.source.text(), raw.replacen("python", "rust", 1));
            assert_eq!(doc.selection(), selection.start - 2..selection.end - 2);
            assert_eq!(doc.history_len(false), 1);
            assert!(doc.execute_render_action(&stale).is_err());
            doc.undo(false);
            assert_eq!(doc.source.text(), raw);
            assert_eq!(doc.selection(), selection);
            assert!(!doc.dirty());
            doc.undo(true);
            assert_eq!(doc.source.text(), raw.replacen("python", "rust", 1));
        }
    }
}
#[test]
fn auto_explicit_modes_round_trip_without_display_edits() {
    let mut doc = new_document("```python\nfn main() { let s = \"中😀\"; }\n```\n");
    doc.execute_render_action(&request(&doc, "code.language-auto"))
        .unwrap();
    let auto = doc.source.text().to_owned();
    assert!(auto.starts_with("```\n"));
    let selection = doc.selection();
    let mut highlighter = CodeHighlighter::default();
    // Auto detection may exhaust its 50 ms budget under parallel test load.
    // Its literal fallback must preserve the same editable body and auto mode.
    let display = doc.highlighted_display(&mut highlighter);
    assert_eq!(display.code_blocks.len(), 1);
    assert!(display.code_blocks[0].source.language.is_empty());
    assert!(display.text.contains("fn main() { let s = \"中😀\"; }"));
    assert_eq!(doc.source.text(), auto);
    assert_eq!(doc.selection(), selection);
    assert_eq!(doc.history_len(false), 1);
    doc.execute_render_action(&request(&doc, "code.language-auto"))
        .unwrap();
    assert_eq!(doc.history_len(false), 1);
    doc.execute_render_action(&request(&doc, "code.language-rust"))
        .unwrap();
    assert!(doc.source.text().starts_with("```rust\n"));
    assert!(doc
        .highlighted_display(&mut highlighter)
        .spans
        .iter()
        .any(|s| s.style.syntax == Some(Class::Keyword)));
    let at = doc.source.text().find("fn main").unwrap();
    doc.select(at, false, false);
    doc.line_break(false).unwrap();
    assert!(doc.source.text().starts_with("```rust\n"));
    doc.undo(false);
    doc.undo(false);
    assert_eq!(doc.source.text(), auto);
    let path = std::env::temp_dir().join(format!("ryne-code-language-{}.md", std::process::id()));
    doc.save(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), auto.as_bytes());
    let reopened = doc.open_with(&path).unwrap();
    assert!(reopened
        .render_controls()
        .unwrap()
        .iter()
        .find(|c| c.provider == "code")
        .unwrap()
        .actions
        .iter()
        .any(|a| a.checked && a.command == "code.language-auto"));
    std::fs::remove_file(path).unwrap();
}
#[test]
fn menu_defaults_aliases_and_source_ime_protection() {
    for (info, selected) in [("", "auto"), ("RS", "rust"), ("jsx", "javascript")] {
        let mut doc = new_document(&format!("```{info}\nbody\n```"));
        let control = doc
            .render_controls()
            .unwrap()
            .into_iter()
            .find(|c| c.provider == "code")
            .unwrap();
        assert!(control
            .actions
            .iter()
            .any(|a| a.checked && a.command.ends_with(selected)));
        let req = request(&doc, "code.language-json");
        doc.select(doc.source.text().find("body").unwrap(), false, false);
        doc.update_composition(None, "预编辑", None).unwrap();
        assert!(doc.execute_render_action(&req).is_err());
        doc.cancel_composition();
        assert!(doc.toggle_block_source());
        assert!(doc
            .execute_render_action(&request_from(control, "code.language-json"))
            .is_err());
        assert!(doc
            .render_controls()
            .unwrap()
            .iter()
            .all(|c| c.provider != "code"));
        doc.set_source_mode(true);
        assert!(doc.render_controls().unwrap().is_empty());
    }
}
fn request_from(
    control: ryne_extensions::RenderControl,
    command: &'static str,
) -> RenderActionRequest {
    RenderActionRequest {
        provider: control.provider,
        target: control.target,
        command,
    }
}
#[test]
fn all_choices_keep_code_literal_and_new_templates_default_to_auto() {
    let raw = "```\n**literal** | `code`\n```";
    let doc = new_document(raw);
    let actions = doc
        .render_controls()
        .unwrap()
        .into_iter()
        .find(|c| c.provider == "code")
        .unwrap()
        .actions;
    for action in actions {
        let mut doc = new_document(raw);
        doc.execute_render_action(&request(&doc, action.command))
            .unwrap();
        assert_eq!(ryne_syntax::code_block::blocks(&doc.parse)[0].rows.len(), 1);
        assert!(doc.source.text().contains("\n**literal** | `code`\n"));
        doc.undo(false);
        assert_eq!(doc.source.text(), raw);
    }
    let mut doc = new_document("");
    doc.append_block(InsertBlock::Code).unwrap();
    assert_eq!(doc.source.text(), "```\n\n```");
    assert_eq!(doc.head, 4);
}

#[test]
fn current_default_is_clean_and_indented_literal_fences_are_not_language_fields() {
    let mut doc = new_document("```\nfn main() {}\n```\n");
    doc.execute_render_action(&request(&doc, "code.language-auto"))
        .unwrap();
    assert_eq!(doc.source.text(), "```\nfn main() {}\n```\n");
    assert!(!doc.dirty());
    assert_eq!(doc.history_len(false), 0);
    for raw in [
        "    ```rust\n    literal\n",
        ">     ~~~python\n>     literal\n",
    ] {
        assert!(new_document(raw)
            .render_controls()
            .unwrap()
            .iter()
            .all(|c| c.provider != "code"));
    }
    let mut doc = new_document("~~~python\n**literal**\n");
    doc.execute_render_action(&request(&doc, "code.language-rust"))
        .unwrap();
    assert_eq!(doc.source.text(), "~~~rust\n**literal**\n");
    doc.undo(false);
    assert_eq!(doc.source.text(), "~~~python\n**literal**\n");
}

#[test]
fn auto_clears_info_and_removed_empty_command_cannot_edit() {
    for newline in ["\n", "\r\n", "\r"] {
        for info in ["python", "auto", "text", "rust extra"] {
            let raw = format!("> ~~~~  {info}  {newline}> 中😀{newline}> ~~~~{newline}");
            let mut doc = new_document(&raw);
            let at = raw.find("中😀").unwrap();
            doc.select(at, false, false);
            let removed = request(&doc, "code.language-empty");
            assert!(doc.execute_render_action(&removed).is_err());
            assert!(!doc.dirty());
            assert!(doc
                .render_controls()
                .unwrap()
                .iter()
                .filter(|c| c.provider == "code")
                .flat_map(|c| &c.actions)
                .all(|a| a.fallback != "<empty>"));
            doc.execute_render_action(&request(&doc, "code.language-auto"))
                .unwrap();
            assert_eq!(doc.source.text(), raw.replacen(info, "", 1));
            assert!(ryne_syntax::code_block::blocks(&doc.parse)[0]
                .language
                .trim()
                .is_empty());
            assert_eq!(
                &doc.source.text()[doc.head..doc.head + "中😀".len()],
                "中😀"
            );
            assert_eq!(doc.history_len(false), 1);
            doc.undo(false);
            assert_eq!(doc.source.text(), raw);
            assert_eq!(doc.head, at);
            doc.undo(true);
            assert_eq!(doc.source.text(), raw.replacen(info, "", 1));
        }
    }
}
