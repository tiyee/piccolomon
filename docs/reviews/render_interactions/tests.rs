use super::{features::*, protocol::*};
use ryne_extensions::{CommandPlan, EditingState, Validator};
use ryne_session::{Composition, EditorSession};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

fn host() -> Host {
    Host::new(vec![Box::new(TableActions), Box::new(ListActions)]).unwrap()
}
fn frame(host: &Host, doc: &EditorSession) -> Frame {
    let specs = host.contribute(doc).unwrap();
    let surfaces = fixture_surfaces(&specs);
    host.layout(doc, Context::rendered(doc.source.id()), &surfaces, specs)
        .unwrap()
}
fn invocation(
    host: &Host,
    doc: &EditorSession,
    frame: &Frame,
    role: &str,
    part: &[usize],
    index: usize,
) -> Invocation {
    let control = frame
        .controls
        .iter()
        .find(|c| c.spec.target.role == role && c.spec.target.part == part)
        .unwrap();
    match host
        .activate(doc, frame.context, frame, control.bounds.center())
        .unwrap()
    {
        Activation::Menu(mut m) => m.take(index).unwrap(),
        Activation::Command(c) => {
            assert_eq!(index, 0);
            c
        }
    }
}
const TABLE: &str = "before\n\n| A | B |\n| :--- | ---: |\n| 中😀 | x\\|y |\n| c | d |\n\nafter\n";

#[test]
fn cell_operation_uses_clicked_target_instead_of_caret_and_is_one_undo() {
    let host = host();
    let mut doc = session(TABLE).unwrap();
    doc.select(TABLE.len() - 1, false, false);
    let selection = doc.selection();
    let frame = frame(&host, &doc);
    let command = invocation(&host, &doc, &frame, "table.cell", &[1, 1], 0);
    host.execute(&mut doc, frame.context, command).unwrap();
    assert_eq!(doc.source.text(), TABLE.replace("| 中😀 | x\\|y |\n", ""));
    assert_eq!(doc.history_len(false), 1);
    assert!(doc.dirty());
    assert_eq!(doc.display().tables.len(), 1);
    doc.undo(false);
    assert_eq!(doc.source.text(), TABLE);
    assert_eq!(doc.selection(), selection);
    assert!(!doc.dirty());
    doc.undo(true);
    assert!(!doc.source.text().contains("中😀"));
}

#[test]
fn table_insert_preserves_prefix_and_all_three_newline_styles() {
    for newline in ["\n", "\r\n", "\r"] {
        for prefix in ["", "> "] {
            for suffix in [true, false] {
                let raw = format!("{prefix}| A | B |{newline}{prefix}| --- | --- |{newline}{prefix}| 中😀 | x |{}", if suffix {newline} else {""});
                let mut doc = session(&raw).unwrap();
                let host = host();
                let frame = frame(&host, &doc);
                let command = invocation(&host, &doc, &frame, "table.cell", &[1, 0], 1);
                host.execute(&mut doc, frame.context, command).unwrap();
                let addition = format!("{prefix}|  |  |");
                let expected = if suffix {
                    format!("{raw}{addition}{newline}")
                } else {
                    format!("{raw}{newline}{addition}")
                };
                assert_eq!(doc.source.text(), expected);
                assert_eq!(doc.display().tables[0].source.rows.len(), 3);
                doc.undo(false);
                assert_eq!(doc.source.text(), raw);
            }
        }
    }
}

#[test]
fn header_insert_follows_delimiter_and_header_delete_is_disabled() {
    let mut doc = session(TABLE).unwrap();
    let host = host();
    let frame = frame(&host, &doc);
    let control = frame
        .controls
        .iter()
        .find(|c| c.spec.target.part == [0, 0])
        .unwrap();
    let Activation::Menu(mut menu) = host
        .activate(&doc, frame.context, &frame, control.bounds.center())
        .unwrap()
    else {
        panic!("menu")
    };
    assert!(menu.take(0).is_err());
    let command = menu.take(1).unwrap();
    assert!(menu.take(1).is_err());
    host.execute(&mut doc, frame.context, command).unwrap();
    assert!(doc
        .source
        .text()
        .contains("| :--- | ---: |\n|  |  |\n| 中😀"));
}

#[test]
fn list_direct_button_inserts_after_subtree_with_container_prefix() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw = format!("> 1. parent{newline}>    - child{newline}> 2. next{newline}");
        let mut doc = session(&raw).unwrap();
        let host = host();
        let frame = frame(&host, &doc);
        let control = frame
            .controls
            .iter()
            .find(|c| c.spec.target.role == "list.item" && c.spec.target.owner.start == 0)
            .unwrap();
        let Activation::Command(command) = host
            .activate(&doc, frame.context, &frame, control.bounds.center())
            .unwrap()
        else {
            panic!("direct button")
        };
        host.execute(&mut doc, frame.context, command).unwrap();
        assert_eq!(
            doc.source.text(),
            format!("> 1. parent{newline}>    - child{newline}> 2. new{newline}> 2. next{newline}")
        );
        doc.undo(false);
        assert_eq!(doc.source.text(), raw);
    }
}

#[test]
fn menu_and_layout_are_read_only() {
    let doc = session(TABLE).unwrap();
    let host = host();
    let key = doc.source.id();
    let before = doc.selection();
    let frame = frame(&host, &doc);
    let _ = invocation(&host, &doc, &frame, "table.cell", &[1, 0], 0);
    assert_eq!(doc.source.id(), key);
    assert_eq!(doc.source.text(), TABLE);
    assert_eq!(doc.selection(), before);
    assert!(!doc.dirty());
    assert_eq!(doc.history_len(false), 0);
}

#[test]
fn stale_revision_and_cross_document_commands_are_rejected() {
    let host = host();
    for foreign in [false, true] {
        let mut doc = session(TABLE).unwrap();
        let frame = frame(&host, &doc);
        let command = invocation(&host, &doc, &frame, "table.cell", &[1, 0], 0);
        if foreign {
            doc = session(TABLE).unwrap();
        } else {
            doc.insert_text(0..0, "changed\n").unwrap();
        }
        let before = doc.source.text().to_owned();
        let history = doc.history_len(false);
        let context = Context::rendered(doc.source.id());
        assert!(host.execute(&mut doc, context, command).is_err());
        assert_eq!(doc.source.text(), before);
        assert_eq!(doc.history_len(false), history);
    }
}

#[test]
fn unchanged_source_with_new_layout_epoch_rejects_old_frame_and_menu() {
    let host = host();
    let mut doc = session(TABLE).unwrap();
    let frame = frame(&host, &doc);
    let command = invocation(&host, &doc, &frame, "table.cell", &[1, 0], 0);
    let context = Context {
        epoch: frame.context.epoch + 1,
        ..frame.context
    };
    assert!(host
        .activate(&doc, context, &frame, frame.controls[0].bounds.center())
        .is_err());
    assert!(host.execute(&mut doc, context, command).is_err());
    assert!(!doc.dirty());
}

#[test]
fn source_modes_dragging_and_real_ime_block_execution() {
    for mode in 0..5 {
        let host = host();
        let mut doc = session(TABLE).unwrap();
        let frame = frame(&host, &doc);
        let command = invocation(&host, &doc, &frame, "table.cell", &[1, 0], 0);
        let mut context = frame.context;
        match mode {
            0 => doc.set_source_mode(true),
            1 => doc.set_block_source(Some(command_target_scope(&frame))),
            2 => context.dragging = true,
            3 => context.composing = true,
            _ => {
                doc.composition = Some(Composition {
                    range: 0..0,
                    text: "候选".into(),
                    selection: 0..0,
                })
            }
        }
        assert!(host.execute(&mut doc, context, command).is_err());
        assert_eq!(doc.source.text(), TABLE);
        assert_eq!(doc.history_len(false), 0);
    }
}
fn command_target_scope(frame: &Frame) -> std::ops::Range<usize> {
    frame.controls[0].spec.target.owner.clone()
}

#[test]
fn local_source_hides_its_controls() {
    let host = host();
    let mut doc = session(TABLE).unwrap();
    let initial = frame(&host, &doc);
    doc.set_block_source(Some(command_target_scope(&initial)));
    assert!(frame(&host, &doc).controls.is_empty());
    assert!(!doc.dirty());
}

struct Reject;
impl Validator for Reject {
    fn validate(&self, _: &EditingState, _: &CommandPlan) -> Result<(), String> {
        Err("rejected".into())
    }
}
#[test]
fn real_session_validator_rejection_has_no_partial_effects() {
    let host = host();
    let mut doc = session(TABLE).unwrap();
    doc.add_validator(Arc::new(Reject));
    let frame = frame(&host, &doc);
    let command = invocation(&host, &doc, &frame, "table.cell", &[1, 0], 0);
    let key = doc.source.id();
    let selection = doc.selection();
    assert!(host.execute(&mut doc, frame.context, command).is_err());
    assert_eq!(doc.source.id(), key);
    assert_eq!(doc.selection(), selection);
    assert!(!doc.dirty());
    assert_eq!(doc.history_len(false), 0);
}

// This third extension adds a new target role and a command without changing Host.
struct Badge {
    enabled: Arc<AtomicBool>,
}
impl RenderExtension for Badge {
    fn id(&self) -> &'static str {
        "badge"
    }
    fn commands(&self) -> &'static [&'static str] {
        &["badge.select"]
    }
    fn contribute(&self, state: &EditingState) -> Result<Vec<Contribution>, String> {
        Ok(vec![Contribution {
            provider: self.id(),
            id: "badge".into(),
            target: Target {
                document: state.source.id(),
                role: "custom.badge",
                owner: 0..state.source.text().len(),
                part: vec![],
            },
            placement: Placement::After,
            priority: 20,
            actions: vec![Action {
                command: "badge.select",
                label_key: "badge.select",
                enabled: true,
                disabled_reason: None,
            }],
        }])
    }
    fn available(&self, _: &EditingState, target: &Target, command: &str) -> bool {
        self.enabled.load(Ordering::SeqCst)
            && target.role == "custom.badge"
            && command == "badge.select"
    }
    fn plan(&self, state: &EditingState, target: &Target, _: &str) -> Result<CommandPlan, String> {
        let mut next = state.clone();
        next.anchor = target.owner.start;
        next.head = target.owner.end;
        Ok(CommandPlan::Select {
            base: state.source.id(),
            state: next,
            handled: true,
        })
    }
}
#[test]
fn third_extension_registers_new_role_and_non_edit_command_without_host_changes() {
    let host = Host::new(vec![Box::new(Badge {
        enabled: Arc::new(AtomicBool::new(true)),
    })])
    .unwrap();
    let mut doc = session("custom 中😀").unwrap();
    let frame = frame(&host, &doc);
    let command = invocation(&host, &doc, &frame, "custom.badge", &[], 0);
    host.execute(&mut doc, frame.context, command).unwrap();
    assert_eq!(doc.selection(), 0..doc.source.text().len());
    assert!(!doc.dirty());
    assert_eq!(doc.history_len(false), 0);
}
#[test]
fn eligibility_is_rechecked_after_menu_creation() {
    let enabled = Arc::new(AtomicBool::new(true));
    let host = Host::new(vec![Box::new(Badge {
        enabled: enabled.clone(),
    })])
    .unwrap();
    let mut doc = session("custom").unwrap();
    let frame = frame(&host, &doc);
    let command = invocation(&host, &doc, &frame, "custom.badge", &[], 0);
    enabled.store(false, Ordering::SeqCst);
    assert!(host.execute(&mut doc, frame.context, command).is_err());
    assert_eq!(doc.selection(), 0..0);
}

#[test]
fn duplicate_registration_is_rejected() {
    assert!(Host::new(vec![Box::new(TableActions), Box::new(TableActions)]).is_err());
}
#[test]
fn invalid_or_missing_geometry_is_rejected() {
    let doc = session(TABLE).unwrap();
    let host = host();
    let context = Context::rendered(doc.source.id());
    let specs = host.contribute(&doc).unwrap();
    assert!(host.layout(&doc, context, &[], specs.clone()).is_err());
    let mut surfaces = fixture_surfaces(&specs);
    surfaces[0].bounds.x = f32::NAN;
    assert!(host.layout(&doc, context, &surfaces, specs).is_err());
}
#[test]
fn overlap_is_diagnosed_instead_of_losing_an_operation() {
    let doc = session(TABLE).unwrap();
    let host = host();
    let specs = host.contribute(&doc).unwrap();
    let mut surfaces = fixture_surfaces(&specs);
    surfaces[1].bounds = surfaces[0].bounds;
    assert!(host
        .layout(&doc, Context::rendered(doc.source.id()), &surfaces, specs)
        .is_err());
}
#[test]
fn malformed_markdown_stays_editable_without_interaction_contributions() {
    let doc = session("|a|b|\n|---|---|\n|ragged|\n").unwrap();
    assert!(host().contribute(&doc).unwrap().is_empty());
    assert!(!doc.dirty());
}

#[test]
fn table_delete_preserves_nested_container_and_adjacent_source() {
    let raw = "> - item\r\n>\r\n>   | A | B |\r\n>   | --- | --- |\r\n>   | 中😀 | x |\r\n>   | c | d |\r\n\r\nafter\r\n";
    let mut doc = session(raw).unwrap();
    let host = host();
    let frame = frame(&host, &doc);
    let command = invocation(&host, &doc, &frame, "table.cell", &[1, 0], 0);
    host.execute(&mut doc, frame.context, command).unwrap();
    assert_eq!(doc.source.text(), raw.replace(">   | 中😀 | x |\r\n", ""));
    assert_eq!(doc.display().tables.len(), 1);
    doc.undo(false);
    assert_eq!(doc.source.text(), raw);
}

#[test]
fn list_button_preserves_missing_final_newline() {
    let raw = "- 中😀";
    let mut doc = session(raw).unwrap();
    let host = host();
    let frame = frame(&host, &doc);
    let command = invocation(&host, &doc, &frame, "list.item", &[], 0);
    host.execute(&mut doc, frame.context, command).unwrap();
    assert_eq!(doc.source.text(), "- 中😀\n- new");
    doc.undo(false);
    assert_eq!(doc.source.text(), raw);
}

#[test]
fn operation_transforms_existing_completion_checkpoint_and_undo_restores_it() {
    let mut doc = session(&format!("{TABLE}\n**")).unwrap();
    let end = doc.source.text().len();
    doc.select(end, false, false);
    doc.insert_text(end..end, "x").unwrap();
    assert!(!doc.closers.is_empty());
    let closers = doc.closers.clone();
    let raw = doc.source.text().to_owned();
    let host = host();
    let frame = frame(&host, &doc);
    let command = invocation(&host, &doc, &frame, "table.cell", &[1, 0], 0);
    host.execute(&mut doc, frame.context, command).unwrap();
    for closer in &doc.closers {
        assert_eq!(&doc.source.text()[closer.clone()], "**");
    }
    assert!(!doc.closers.is_empty());
    doc.undo(false);
    assert_eq!(doc.source.text(), raw);
    assert_eq!(doc.closers, closers);
}

struct IncompleteRegistration;
impl RenderExtension for IncompleteRegistration {
    fn id(&self) -> &'static str {
        "table"
    }
    fn commands(&self) -> &'static [&'static str] {
        &["table.delete-row"]
    }
    fn contribute(&self, state: &EditingState) -> Result<Vec<Contribution>, String> {
        TableActions.contribute(state)
    }
    fn available(&self, _: &EditingState, _: &Target, _: &str) -> bool {
        false
    }
    fn plan(&self, _: &EditingState, _: &Target, _: &str) -> Result<CommandPlan, String> {
        Err("not callable".into())
    }
}
#[test]
fn contribution_referencing_an_unregistered_handler_is_rejected() {
    let host = Host::new(vec![Box::new(IncompleteRegistration)]).unwrap();
    let doc = session(TABLE).unwrap();
    assert!(host.contribute(&doc).is_err());
    assert!(!doc.dirty());
}
