use super::*;
fn edit(range: Range<usize>, text: &str) -> Edit {
    Edit {
        range,
        replacement: text.into(),
    }
}
#[test]
fn invalid_and_stale_candidates_never_publish_or_change_history() {
    let mut core = DocumentCore::new("中😀\r\nend");
    let old = core.source().clone();
    for edits in [
        vec![edit(1..2, "x")],
        vec![edit(8..8, "x")],
        vec![edit(0..3, "x"), edit(2..7, "y")],
    ] {
        assert!(PreparedEdit::prepare(core.source(), edits).is_err());
        assert_eq!(core.source().id(), old.id());
        assert_eq!(core.history_len(false), 0)
    }
    let stale = PreparedEdit::prepare(core.source(), vec![edit(0..3, "文")]).unwrap();
    let live = PreparedEdit::prepare(core.source(), vec![edit(0..3, "字")]).unwrap();
    core.publish(live, 1).unwrap();
    assert_eq!(core.publish(stale, 2), Err(Error::StaleSnapshot));
    assert_eq!(core.history_len(false), 1);
    assert_eq!(core.source().text(), "字😀\r\nend");
    let other = Source::new(core.source().text());
    assert_eq!(core.mark_saved(&other), Err(Error::StaleSnapshot));
}

#[test]
fn history_publication_requires_the_prepared_entry_and_direction() {
    let mut core = DocumentCore::new("中\r\n");
    for text in ["a", "b"] {
        let prepared = PreparedEdit::prepare(core.source(), vec![edit(0..0, text)]).unwrap();
        core.publish(prepared, ()).unwrap();
    }
    let (undo, ()) = core.prepare_history(false).unwrap().unwrap();
    core.publish_history(undo, false, ()).unwrap();
    let source = core.source().clone();
    let lengths = (core.history_len(false), core.history_len(true));

    let arbitrary = PreparedEdit::prepare(core.source(), vec![edit(0..1, "x")]).unwrap();
    assert_eq!(
        core.publish_history(arbitrary, false, ()),
        Err(Error::StaleSnapshot)
    );
    let (undo, ()) = core.prepare_history(false).unwrap().unwrap();
    assert_eq!(
        core.publish_history(undo, true, ()),
        Err(Error::StaleSnapshot)
    );
    let (redo, ()) = core.prepare_history(true).unwrap().unwrap();
    assert_eq!(core.publish(redo, ()), Err(Error::StaleSnapshot));
    assert_eq!(core.source().id(), source.id());
    assert_eq!((core.history_len(false), core.history_len(true)), lengths);
    assert!(core.dirty());

    let (redo, ()) = core.prepare_history(true).unwrap().unwrap();
    core.publish_history(redo, true, ()).unwrap();
    assert_eq!(core.source().text(), "ba中\r\n");
}
#[test]
fn multi_edit_history_and_written_snapshot_baseline_are_atomic() {
    let mut core = DocumentCore::new("a\r\nb\r\nc");
    let plan =
        PreparedEdit::prepare(core.source(), vec![edit(0..1, "中"), edit(3..4, "😀")]).unwrap();
    core.publish(plan, 42).unwrap();
    let written = core.source().clone();
    core.mark_saved(&written).unwrap();
    assert!(!core.dirty());
    let plan = PreparedEdit::prepare(core.source(), vec![edit(0..3, "新")]).unwrap();
    core.publish(plan, 7).unwrap();
    core.mark_saved(&written).unwrap();
    assert!(core.dirty());
    let (undo, checkpoint) = core.prepare_history(false).unwrap().unwrap();
    assert_eq!(checkpoint, 7);
    core.publish_history(undo, false, 8).unwrap();
    assert!(!core.dirty());
    let (undo, checkpoint) = core.prepare_history(false).unwrap().unwrap();
    assert_eq!(checkpoint, 42);
    core.publish_history(undo, false, 9).unwrap();
    assert_eq!(core.source().text(), "a\r\nb\r\nc");
    assert!(core.dirty());
    let (redo, checkpoint) = core.prepare_history(true).unwrap().unwrap();
    assert_eq!(checkpoint, 9);
    core.publish_history(redo, true, 10).unwrap();
    assert_eq!(core.source().text(), written.text());
    assert!(!core.dirty());
}

#[test]
fn dirty_preserves_exact_bytes_through_retyping_old_receipts_and_history() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw = format!("中😀{newline}tail");
        let mut core = DocumentCore::new(&raw);
        let saved = core.source().clone();
        for text in ["TAIL", "tail"] {
            let start = core.source().text().len() - 4;
            let edit =
                PreparedEdit::prepare(core.source(), vec![edit(start..start + 4, text)]).unwrap();
            core.publish(edit, ()).unwrap();
            assert_eq!(core.dirty(), text != "tail");
        }
        let newer = core.source().clone();
        assert_ne!(newer.id(), saved.id());
        core.mark_saved(&newer).unwrap();
        let edit = PreparedEdit::prepare(core.source(), vec![edit(0..3, "文")]).unwrap();
        core.publish(edit, ()).unwrap();
        core.mark_saved(&saved).unwrap();
        assert!(core.dirty());
        let (edit, ()) = core.prepare_history(false).unwrap().unwrap();
        core.publish_history(edit, false, ()).unwrap();
        assert!(!core.dirty());
        let (edit, ()) = core.prepare_history(true).unwrap().unwrap();
        core.publish_history(edit, true, ()).unwrap();
        assert!(core.dirty());
        assert!(core.mark_saved(&Source::new(&raw)).is_err());
        assert!(core.dirty());
    }
}

#[test]
fn history_accounting_observes_large_inverse_edits_without_changing_history() {
    let raw = "x".repeat(1024 * 1024);
    let mut core = DocumentCore::new(&raw);
    assert_eq!(core.history_memory(|_: &usize| 0), HistoryMemory::default());
    let prepared = PreparedEdit::prepare(core.source(), vec![edit(0..raw.len(), "short")]).unwrap();
    core.publish(prepared, 7).unwrap();
    let source = core.source().id();
    let usage = core.history_memory(|_| 0);
    assert_eq!((usage.undo_entries, usage.redo_entries), (1, 0));
    assert!(usage.reserved_bytes >= raw.len());
    assert_eq!(core.source().id(), source);
    assert!(core.dirty());
    let (prepared, checkpoint) = core.prepare_history(false).unwrap().unwrap();
    assert_eq!(checkpoint, 7);
    core.publish_history(prepared, false, 8).unwrap();
    let usage = core.history_memory(|_| 0);
    assert_eq!((usage.undo_entries, usage.redo_entries), (0, 1));
    assert_eq!(core.source().text(), raw);
    assert!(!core.dirty());
}

#[test]
fn recovered_source_has_no_saved_baseline_until_first_save() {
    for raw in ["", "中😀\r\n> row\r"] {
        let mut core = DocumentCore::recovered(raw);
        assert!(core.dirty());
        assert_eq!(core.history_len(false), 0);
        let edit = PreparedEdit::prepare(core.source(), vec![edit(0..0, "x")]).unwrap();
        core.publish(edit, ()).unwrap();
        let (undo, ()) = core.prepare_history(false).unwrap().unwrap();
        core.publish_history(undo, false, ()).unwrap();
        assert_eq!(core.source().text(), raw);
        assert!(core.dirty());
        core.mark_saved(&core.source().clone()).unwrap();
        assert!(!core.dirty());
    }
}
