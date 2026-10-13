use super::regression_tests::{new_document, services};
use super::*;

fn request(
    doc: &EditorSession,
    row: usize,
    column: usize,
    command: &'static str,
) -> RenderActionRequest {
    let control = doc
        .render_controls()
        .unwrap()
        .into_iter()
        .find(|c| c.target.part == [row, column])
        .unwrap();
    RenderActionRequest {
        provider: control.provider,
        target: control.target,
        command,
    }
}
const RAW: &str = "before\n\n| A | B | C |\n| :--- | :---: | ---: |\n| 中😀 | a\\|b | `code` |\n| next | x | y |\n\nafter\n";
#[test]
fn all_render_commands_are_atomic_preserve_context_and_round_trip() {
    for newline in ["\n", "\r\n", "\r"] {
        for prefix in ["", "> ", ">   "] {
            let raw = format!(
                "{prefix}| A | B | C |{newline}{prefix}| :--- | --- | ---: |{newline}{prefix}| 中😀 | a\\|b | `code` |{newline}{prefix}| next | x | y |"
            );
            for command in [
                "table.align-left",
                "table.align-center",
                "table.align-right",
                "table.insert-below",
                "table.insert-above",
                "table.insert-left",
                "table.insert-right",
                "table.move-row-down",
                "table.move-column-left",
                "table.move-column-right",
                "table.delete-row",
                "table.delete-column",
            ] {
                let mut doc = new_document(&raw);
                let request = request(&doc, 1, 1, command);
                doc.execute_render_action(&request)
                    .unwrap_or_else(|e| panic!("{command} {raw:?}: {e}"));
                assert_eq!(
                    doc.display().tables.len(),
                    1,
                    "{command}: {:?}",
                    doc.source.text()
                );
                assert_eq!(doc.history_len(false), 1, "{command}");
                assert!(doc.dirty());
                let changed = doc.source.text().to_owned();
                for line in ryne_syntax::code_block::lines(&changed) {
                    assert!(changed[line].starts_with(prefix));
                }
                doc.undo(false);
                assert_eq!(doc.source.text(), raw);
                assert!(!doc.dirty());
                doc.undo(true);
                assert_eq!(doc.source.text(), changed);
            }
        }
    }
}
#[test]
fn column_operations_keep_escaped_pipes_alignment_and_neighbor_bytes() {
    let mut doc = new_document(RAW);
    doc.execute_render_action(&request(&doc, 1, 1, "table.move-column-left"))
        .unwrap();
    assert_eq!(
        doc.source.text(),
        "before\n\n| B | A | C |\n| :---: | :--- | ---: |\n| a\\|b | 中😀 | `code` |\n| x | next | y |\n\nafter\n"
    );
    doc.undo(false);
    doc.execute_render_action(&request(&doc, 1, 1, "table.delete-column"))
        .unwrap();
    assert_eq!(
        doc.source.text(),
        "before\n\n| A | C |\n| :--- | ---: |\n| 中😀 | `code` |\n| next | y |\n\nafter\n"
    );
}
#[test]
fn optional_outer_pipes_and_empty_fields_remain_structural_after_column_operations() {
    for raw in [
        "A | B | C\n--- | --- | ---\na |  | c\n",
        "| A | B\n| --- | ---\n| a | b\n",
        "A | B |\n--- | --- |\na | b |\n",
    ] {
        for command in [
            "table.delete-column",
            "table.insert-left",
            "table.insert-right",
            "table.move-column-right",
        ] {
            let mut doc = new_document(raw);
            let display = doc.display();
            let old = display
                .tables
                .first()
                .unwrap_or_else(|| panic!("source fallback: {raw:?}"))
                .source
                .align
                .len();
            doc.execute_render_action(&request(&doc, 1, 0, command))
                .unwrap();
            let display = doc.display();
            assert_eq!(
                display.tables.len(),
                1,
                "{raw:?} {command}: {:?}",
                doc.source.text()
            );
            let columns = display.tables[0].source.align.len();
            assert_eq!(
                columns,
                match command {
                    "table.delete-column" => old - 1,
                    "table.insert-left" | "table.insert-right" => old + 1,
                    _ => old,
                }
            );
            doc.undo(false);
            assert_eq!(doc.source.text(), raw);
        }
    }
}
#[test]
fn row_reordering_keeps_header_and_last_newline_convention() {
    let raw = "| A |\r\n| --- |\r\n| first |\r\n| last |";
    let mut doc = new_document(raw);
    doc.execute_render_action(&request(&doc, 2, 0, "table.move-row-up"))
        .unwrap();
    assert_eq!(
        doc.source.text(),
        "| A |\r\n| --- |\r\n| last |\r\n| first |"
    );
    doc.undo(false);
    assert_eq!(doc.source.text(), raw);
}
#[test]
fn alignment_only_edits_selected_delimiter_and_active_choice_does_not_add_history() {
    let mut doc = new_document(RAW);
    doc.execute_render_action(&request(&doc, 1, 1, "table.align-right"))
        .unwrap();
    assert_eq!(doc.source.text(), RAW.replace(":---:", "---:"));
    let key = doc.source.id();
    let history = doc.history_len(false);
    doc.execute_render_action(&request(&doc, 1, 1, "table.align-right"))
        .unwrap();
    assert_eq!(doc.source.id(), key);
    assert_eq!(doc.history_len(false), history);
}
#[test]
fn operations_use_clicked_cell_and_preserve_outside_selection_and_completion_checkpoints() {
    let mut doc = new_document(&format!("{RAW}\n**"));
    let at = doc.source.text().len();
    doc.select(at, false, false);
    doc.insert_text(at..at, "x").unwrap();
    let original = doc.source.text().to_owned();
    let selection = doc.selection();
    let closers = doc.closers.clone();
    doc.execute_render_action(&request(&doc, 1, 0, "table.delete-row"))
        .unwrap();
    assert!(doc.source.text().ends_with("**x**"));
    assert!(!doc.source.text().contains("中😀"));
    for range in &doc.closers {
        assert_eq!(&doc.source.text()[range.clone()], "**");
    }
    doc.undo(false);
    assert_eq!(doc.source.text(), original);
    assert_eq!(doc.selection(), selection);
    assert_eq!(doc.closers, closers);
}
#[test]
fn disabled_boundaries_and_source_ime_or_stale_targets_cannot_execute() {
    let raw = "| A |\n| --- |\n| x |\n";
    for (r, cmd) in [
        (0, "table.delete-row"),
        (0, "table.insert-above"),
        (1, "table.delete-column"),
        (1, "table.move-row-up"),
        (1, "table.move-row-down"),
        (1, "table.move-column-left"),
        (1, "table.move-column-right"),
    ] {
        let mut doc = new_document(raw);
        let req = request(&doc, r, 0, cmd);
        assert!(doc.execute_render_action(&req).is_err());
        assert_eq!(doc.source.text(), raw);
        assert!(!doc.dirty());
    }
    for mode in 0..5 {
        let mut doc = new_document(RAW);
        let req = request(&doc, 1, 0, "table.delete-row");
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
                doc.insert_text(0..0, "new\n").unwrap();
            }
            _ => doc = new_document(RAW),
        }
        let before = doc.source.text().to_owned();
        let n = doc.history_len(false);
        assert!(doc.execute_render_action(&req).is_err());
        assert_eq!(doc.source.text(), before);
        assert_eq!(doc.history_len(false), n);
    }
}
#[test]
fn controls_and_malformed_table_fallback_are_read_only() {
    let doc = new_document(RAW);
    let key = doc.source.id();
    let selection = doc.selection();
    let controls = doc.render_controls().unwrap();
    assert_eq!(controls.len(), 9);
    assert!(controls.iter().all(|c| c.actions.len() == 13));
    assert_eq!(doc.source.id(), key);
    assert_eq!(doc.selection(), selection);
    assert!(!doc.dirty());
    assert_eq!(doc.history_len(false), 0);
    assert!(
        new_document("|a|b|\n|---|---|\n|short|\n")
            .render_controls()
            .unwrap()
            .is_empty()
    );
    assert!(
        new_document("A | B\n--- | ---\na | \n")
            .render_controls()
            .unwrap()
            .is_empty()
    );
}
struct Custom;
#[test]
fn focused_cell_tracks_row_and_column_moves_at_grapheme_boundaries() {
    for command in ["table.move-row-down", "table.move-column-right"] {
        let mut doc = new_document(RAW);
        let at = doc.source.text().find("中😀").unwrap() + "中".len();
        doc.select(at, false, false);
        doc.execute_render_action(&request(&doc, 1, 0, command))
            .unwrap();
        assert_eq!(
            doc.head,
            doc.source.text().find("中😀").unwrap() + "中".len()
        );
        assert!(doc.selection().is_empty());
        doc.undo(false);
        assert_eq!(doc.head, at);
    }
}
#[test]
fn nested_table_operations_preserve_complete_list_quote_context() {
    let raw = "> - item\r\n>\r\n>   | A | B |\r\n>   | --- | --- |\r\n>   | 中😀 | x |\r\n>   | c | d |\r\n\r\nafter\r\n";
    for command in [
        "table.insert-right",
        "table.insert-below",
        "table.align-center",
        "table.move-column-right",
        "table.delete-column",
    ] {
        let mut doc = new_document(raw);
        doc.execute_render_action(&request(&doc, 1, 0, command))
            .unwrap();
        assert!(doc.source.text().starts_with("> - item\r\n>\r\n"));
        assert!(doc.source.text().ends_with("\r\n\r\nafter\r\n"));
        assert_eq!(doc.display().tables.len(), 1);
        doc.undo(false);
        assert_eq!(doc.source.text(), raw);
    }
}
struct Reject;
impl Validator for Reject {
    fn validate(&self, _: &EditingState, _: &CommandPlan) -> Result<(), String> {
        Err("rejected".into())
    }
}
#[test]
fn render_action_validation_failure_does_not_publish_any_state() {
    let mut doc = new_document(RAW);
    doc.add_validator(Arc::new(Reject));
    let key = doc.source.id();
    let selection = doc.selection();
    assert!(
        doc.execute_render_action(&request(&doc, 1, 1, "table.delete-column"))
            .is_err()
    );
    assert_eq!(doc.source.id(), key);
    assert_eq!(doc.source.text(), RAW);
    assert_eq!(doc.selection(), selection);
    assert_eq!(doc.history_len(false), 0);
}
impl RenderActionProvider for Custom {
    fn id(&self) -> &'static str {
        "custom"
    }
    fn commands(&self) -> &'static [&'static str] {
        &["custom.select"]
    }
    fn controls(&self, s: &EditingState) -> Result<Vec<RenderControl>, String> {
        Ok(vec![RenderControl {
            provider: "custom",
            id: "custom".into(),
            target: RenderTarget {
                document: s.source.id(),
                role: "custom.body",
                owner: 0..s.source.text().len(),
                part: vec![],
            },
            focus: 0..s.source.text().len(),
            placement: RenderControlPlacement::Trailing,
            trailing_space: 24.,
            actions: vec![RenderAction {
                command: "custom.select",
                label: "custom.select",
                fallback: "Select",
                icon: None,
                enabled: true,
                checked: false,
                group: 0,
                submenu: None,
                shortcut: None,
            }],
        }])
    }
    fn plan(&self, s: &EditingState, _: &RenderActionRequest) -> Result<CommandPlan, String> {
        let mut state = s.clone();
        state.anchor = 0;
        state.head = s.source.text().len();
        Ok(CommandPlan::Select {
            base: s.source.id(),
            state,
            handled: true,
        })
    }
}
#[test]
fn extra_provider_registers_an_executable_custom_role_without_session_branches() {
    let features = ryne_features::MarkdownFeatures::new(
        Arc::new(ryne_services::math::FormularyRenderer),
        vec![],
    )
    .unwrap()
    .with_render_action_provider(Arc::new(Custom))
    .unwrap();
    let mut ports = services();
    ports.features = Arc::new(features);
    let mut doc = EditorSession::new("custom😀", ports);
    let c = doc.render_controls().unwrap().remove(0);
    doc.execute_render_action(&RenderActionRequest {
        provider: c.provider,
        target: c.target,
        command: "custom.select",
    })
    .unwrap();
    assert_eq!(doc.selection(), 0..doc.source.text().len());
    assert!(!doc.dirty());
    assert_eq!(doc.history_len(false), 0);
}
