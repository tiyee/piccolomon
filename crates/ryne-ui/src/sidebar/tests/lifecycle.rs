use super::*;

#[gpui::test]
fn cancelled_or_failed_save_before_open_preserves_the_draft_and_allows_retry(
    cx: &mut TestAppContext,
) {
    let dir = tempdir().unwrap();
    let next = dir.path().join("open.md");
    let unwritable = dir.path().join("absent/draft.md");
    fs::write(&next, "SECOND FILE").unwrap();
    let (editor, cx) = cx.add_window_view(|_, cx| test_editor(cx));
    editor.update(cx, |e, _| {
        e.document.replace(0..0, "UNSAVED ").unwrap();
    });
    let saved = dir.path().join("draft.md");
    for destination in [None, Some(unwritable.clone()), Some(saved.clone())] {
        cx.update(|w, cx| editor.update(cx, |e, cx| e.open_external_path(next.clone(), w, cx)));
        cx.run_until_parked();
        cx.simulate_prompt_answer("保存并打开");
        cx.run_until_parked();
        cx.simulate_new_path_selection(|_| destination.clone());
        cx.run_until_parked();
        editor.read_with(cx, |e, _| {
            assert!(!e.operations.file_operation_pending.is_pending());
            if destination.as_ref() == Some(&saved) {
                assert_eq!(fs::read(&saved).unwrap(), b"UNSAVED plain text\r\n");
                assert_eq!(e.document.source.text(), "SECOND FILE");
                assert_eq!(e.document.files.path.as_ref(), Some(&next));
            } else {
                assert_eq!(e.document.source.text(), "UNSAVED plain text\r\n");
                assert!(e.document.dirty() && e.document.files.path.is_none());
                assert!(e.sidebar.browser.root.is_none());
                assert_eq!(e.document.history_len(false), 1);
                assert!(!unwritable.exists());
            }
        });
    }
}

fn note_entry() -> DirectoryEntry {
    DirectoryEntry {
        name: "note.md".into(),
        kind: EntryKind::Document,
        modified: None,
        bytes: Some(3),
    }
}

#[gpui::test]
fn editing_during_save_before_open_keeps_the_newer_revision(cx: &mut TestAppContext) {
    let dir = tempdir().unwrap();
    let current = dir.path().join("current.md");
    let next = dir.path().join("next.md");
    fs::write(&current, "original\r\n").unwrap();
    fs::write(&next, "next").unwrap();
    let (editor, cx) = cx.add_window_view(|w, cx| {
        let mut e = test_editor(cx);
        e.install_document(
            Document::open(&current, e.document.services()).unwrap(),
            Text::Opened,
            w,
            cx,
        );
        e
    });
    cx.update(|w, cx| {
        editor.update(cx, |e, cx| {
            e.document.replace(0..0, "first ").unwrap();
            e.open_external_path(next, w, cx);
        })
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("保存并打开");
    // Stop as soon as the snapshot is queued, before its receipt is delivered.
    while !editor.read_with(cx, |e, _| e.document.files.saving) {
        assert!(cx.background_executor.tick(), "save was never queued");
    }
    editor.update(cx, |e, _| {
        e.document.replace(0..0, "latest ").unwrap();
    });
    cx.run_until_parked();
    assert_eq!(fs::read(&current).unwrap(), b"first original\r\n");
    editor.read_with(cx, |e, _| {
        assert_eq!(e.document.files.path.as_ref(), Some(&current));
        assert_eq!(e.document.source.text(), "latest first original\r\n");
        assert!(e.document.dirty());
        assert_eq!(e.document.history_len(false), 2);
        assert!(!e.document.files.saving);
        assert!(!e.operations.file_operation_pending.is_pending());
    });
}

#[gpui::test]
fn late_summary_for_the_same_name_in_an_old_root_is_discarded(cx: &mut TestAppContext) {
    let dir = tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    for (root, text) in [(&a, "OLD"), (&b, "NEW")] {
        fs::create_dir(root).unwrap();
        fs::write(root.join("note.md"), text).unwrap();
    }
    let note = Path::new("note.md");
    let (editor, cx) = cx.add_window_view(|_, cx| test_editor(cx));
    let (release, held) = async_channel::bounded::<String>(1);
    let old_text = fs::read_to_string(a.join(note)).unwrap();
    editor.update(cx, |e, cx| {
        e.sidebar.browser.root(Some(a.clone()), true, false);
        e.sidebar.browser.insert(Path::new(""), note_entry());
        // Hold the completed I/O result before delivery to the window.
        let key = crate::sidebar::SummaryKey {
            generation: e.sidebar.browser.generation,
            path: note.to_owned(),
            bytes: Some(3),
            modified: None,
            revision: 0,
        };
        e.track_summary(key, async move { held.recv().await.unwrap() }, cx);
        e.sidebar.browser.root(Some(b), true, false);
        e.sidebar.reset_root(false, cx);
        e.sidebar.browser.insert(Path::new(""), note_entry());
        assert!(e.summary_for(note, cx).is_empty());
    });
    cx.run_until_parked();
    editor.update(cx, |e, cx| {
        assert_eq!(e.summary_for(note, cx), "NEW");
        assert_eq!(e.sidebar.summaries.len(), 1);
    });
    release.try_send(old_text).unwrap();
    cx.run_until_parked();
    editor.update(cx, |e, cx| {
        assert!(e.sidebar.summaries.is_empty());
        assert_eq!(e.sidebar.summary_cache.len(), 1);
        assert_eq!(e.summary_for(note, cx), "NEW");
    });
}

#[gpui::test]
fn closing_a_window_with_a_full_directory_queue_releases_its_worker(cx: &mut TestAppContext) {
    let dir = tempdir().unwrap();
    // The local scanner emits four full batches, then awaits space for End.
    for i in 0..4 * BATCH {
        fs::write(dir.path().join(format!("{i}.md")), []).unwrap();
    }
    let (editor, cx) = cx.add_window_view(|_, cx| test_editor(cx));
    let (queue, cancel) = editor.update(cx, |e, cx| {
        e.sidebar
            .browser
            .root(Some(dir.path().to_owned()), true, false);
        e.sidebar.prefs.visible = true;
        e.pump_directory(cx);
        let request = e.sidebar.request.as_ref().unwrap();
        (request.channel.clone(), request.cancel.clone())
    });
    assert_eq!(queue.capacity(), Some(4));
    // Do not run the foreground consumer while filling the bounded channel.
    while cx.dispatcher.tick(true) {}
    assert!(queue.is_full());
    assert_eq!(queue.sender_count(), 1);
    drop(editor);
    cx.update(|w, _| w.remove_window());
    // Keep our receiver alive and do not drain it. Window teardown itself must
    // unblock the sender, even though the receiver task has not run yet.
    assert!(cancel.load(Ordering::Relaxed));
    assert!(queue.is_closed());
    while cx.dispatcher.tick(true) {}
    assert_eq!(queue.sender_count(), 0);
    cx.run_until_parked();
}

#[gpui::test]
fn filter_owns_candidate_enter_arrows_paste_and_escape(cx: &mut TestAppContext) {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("候选.md"), "a different document").unwrap();
    cx.update(crate::bind_window_keys);
    let (editor, cx) = cx.add_window_view(|_, cx| {
        let mut e = test_editor(cx);
        e.sidebar
            .browser
            .root(Some(dir.path().to_owned()), true, false);
        e.sidebar.prefs.visible = true;
        e.sidebar.prefs.list = true;
        e
    });
    cx.simulate_resize(size(px(920.), px(600.)));
    click(cx, "sidebar-filter");
    let (filter, original, selection) = editor.read_with(cx, |e, _| {
        (
            e.sidebar.filter.clone(),
            e.document.source.id(),
            e.document.selection(),
        )
    });
    // GPUI has no native candidate picker in its test platform. Set the marked
    // range, then deliver Enter + its committed text through the window so
    // key bindings, focus and the registered platform input handler all run.
    cx.update(|w, cx| {
        filter.update(cx, |f, cx| {
            assert!(f.focus.is_focused(w));
            f.replace_and_mark_text_in_range(None, "候", Some(1..1), w, cx);
        })
    });
    cx.simulate_keystrokes("down escape");
    cx.update(|w, cx| {
        assert!(filter.read(cx).focus.is_focused(w));
        assert!(filter.read(cx).marked.is_some());
        assert_eq!(
            editor.read(cx).sidebar.browser.list,
            [PathBuf::from("候选.md")]
        );
        let mut confirm = gpui::Keystroke::parse("enter").unwrap();
        confirm.key_char = Some("候选".into());
        assert!(w.dispatch_keystroke(confirm, cx));
    });
    cx.run_until_parked();
    filter.read_with(cx, |f, _| {
        assert_eq!(f.value, "候选");
        assert!(f.marked.is_none());
    });
    cx.simulate_keystrokes("left");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("😀".into()));
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-v"
    } else {
        "ctrl-v"
    });
    assert_eq!(filter.read_with(cx, |f, _| f.value.clone()), "候😀选");
    cx.simulate_keystrokes("escape");
    cx.update(|w, cx| assert!(editor.read(cx).sidebar.focus.is_focused(w)));
    cx.simulate_keystrokes("escape");
    cx.update(|w, cx| {
        let e = editor.read(cx);
        assert!(e.focus.is_focused(w));
        assert_eq!(e.document.source.id(), original);
        assert_eq!(e.document.selection(), selection);
        assert_eq!(e.document.history_len(false), 0);
        assert!(!e.document.dirty());
        assert!(!e.operations.file_operation_pending.is_pending());
    });
    cx.simulate_input("Z");
    editor.read_with(cx, |e, _| {
        assert_eq!(
            e.document.source.text(),
            format!(
                "{}Z{}",
                &"plain text\r\n"[..selection.start],
                &"plain text\r\n"[selection.end..]
            )
        );
        assert_eq!(e.document.history_len(false), 1);
        assert_eq!(e.sidebar.browser.filter, "候😀选");
    });
}
