use super::{store, Draft, Slot};
use crate::window_controls_tests::test_editor;
use crate::{welcome, Save};
use gpui::{TestAppContext, VisualTestContext};
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};
use std::{io, path::PathBuf, time::Duration};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "ryne-draft-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
                Err(e) => panic!("create fixture: {e}"),
            }
        }
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn live_slots_are_isolated_and_retired_writers_cannot_recreate_a_copy() {
    let dir = Directory::new();
    let first = Draft::new(None);
    let second = Draft::new(None);
    first.write(&dir.0, "first").unwrap();
    second.write(&dir.0, "中\r\n😀\r").unwrap();
    assert_eq!(store::candidates(&dir.0).unwrap(), vec![0, 1]);
    assert!(Slot::read(&dir.0, 0).unwrap().is_none());
    assert!(Slot::read(&dir.0, 1).unwrap().is_none());
    drop(second); // Process exit releases the lease and keeps the copy.
    let (slot, text) = Slot::read(&dir.0, 1).unwrap().unwrap();
    assert_eq!(text, "中\r\n😀\r");
    assert!(Slot::read(&dir.0, 1).unwrap().is_none());
    drop(slot); // Later.
    first.retire().unwrap();
    let newer = Draft::new(None);
    newer.write(&dir.0, "new occupant").unwrap();
    first.write(&dir.0, "delayed stale work").unwrap();
    assert_eq!(
        fs::read_to_string(dir.0.join("0.md")).unwrap(),
        "new occupant"
    );
    assert_eq!(Slot::read(&dir.0, 1).unwrap().unwrap().1, text);
}

#[test]
fn write_failure_keeps_previous_complete_copy_and_retries() {
    let dir = Directory::new();
    let draft = Draft::new(None);
    draft.write(&dir.0, "previous").unwrap();
    fs::create_dir(dir.0.join("0.tmp")).unwrap();
    assert!(draft.write(&dir.0, "next").is_err());
    assert_eq!(fs::read_to_string(dir.0.join("0.md")).unwrap(), "previous");
    fs::remove_dir(dir.0.join("0.tmp")).unwrap();
    draft.write(&dir.0, "").unwrap();
    drop(draft);
    assert_eq!(Slot::read(&dir.0, 0).unwrap().unwrap().1, "");
}

fn tick(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
}

#[gpui::test]
fn recover_later_or_discard_and_save_the_latest_committed_source(cx: &mut TestAppContext) {
    let dir = Directory::new();
    let raw = "中\r\n> 😀\r";
    Draft::new(None).write(&dir.0, raw).unwrap();
    let (editor, cx) = cx.add_window_view(|_, cx| {
        let mut editor = test_editor(cx);
        editor.recovery.directory = Some(dir.0.clone());
        editor.default_document = Some(welcome::DefaultDocument::new(
            &editor.document,
            editor.locale.language,
        ));
        editor
    });
    let offer = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| editor.update(cx, |e, cx| e.offer_drafts(window, cx)));
        cx.run_until_parked();
        assert!(cx.has_pending_prompt());
    };
    offer(cx);
    cx.simulate_prompt_answer("稍后处理");
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(dir.0.join("0.md")).unwrap(), raw);
    offer(cx);
    cx.simulate_prompt_answer("丢弃草稿");
    cx.run_until_parked();
    assert!(!dir.0.join("0.md").exists());
    Draft::new(None).write(&dir.0, raw).unwrap();
    offer(cx);
    cx.simulate_prompt_answer("恢复");
    cx.run_until_parked();
    editor.read_with(cx, |e, _| {
        assert_eq!(e.document.source.text(), raw);
        assert!(e.document.dirty());
        assert!(e.document.files.path.is_none());
        assert!(e.default_document.is_none());
        assert_eq!(e.document.history_len(false), 0);
    });
    let path = dir.0.join("saved.md");
    cx.update(|window, cx| {
        editor.update(cx, |e, cx| {
            e.save_to(dir.0.join("0.md"), false, window, cx);
            assert!(e.document.files.path.is_none());
            assert!(!e.document.files.saving);
            assert!(e.document.dirty());
        })
    });
    cx.update(|window, cx| {
        editor.update(cx, |e, cx| {
            // Save failure must preserve the only copy.
            e.save_to(dir.0.join("missing/file.md"), false, window, cx);
        })
    });
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(dir.0.join("0.md")).unwrap(), raw);
    cx.update(|window, cx| {
        editor.update(cx, |e, cx| {
            e.save_to(path.clone(), false, window, cx);
            let end = e.document.source.text().len();
            e.document.insert_text(end..end, "more").unwrap();
            e.document.update_composition(None, "临时", None).unwrap();
            e.refresh(cx);
        })
    });
    tick(cx);
    assert_eq!(fs::read_to_string(&path).unwrap(), raw);
    assert_eq!(
        fs::read_to_string(dir.0.join("0.md")).unwrap(),
        format!("{raw}more")
    );
    cx.update(|window, cx| {
        editor.update(cx, |e, cx| {
            e.document.cancel_composition();
            e.save(&Save, window, cx);
        })
    });
    cx.run_until_parked();
    assert!(!dir.0.join("0.md").exists());
    assert_eq!(fs::read_to_string(&path).unwrap(), format!("{raw}more"));
}

#[gpui::test]
fn ordinary_edits_create_a_draft_only_after_committed_changes(cx: &mut TestAppContext) {
    let dir = Directory::new();
    let (editor, cx) = cx.add_window_view(|_, cx| {
        let mut editor = test_editor(cx);
        editor.recovery.directory = Some(dir.0.clone());
        editor.document = editor.document.fork("");
        editor
    });
    cx.update(|_, cx| {
        editor.update(cx, |e, cx| {
            e.document.update_composition(None, "候选", None).unwrap();
            e.refresh(cx);
        })
    });
    tick(cx);
    assert!(store::candidates(&dir.0).unwrap().is_empty());
    cx.update(|_, cx| {
        editor.update(cx, |e, cx| {
            e.commit_composition(cx);
        })
    });
    tick(cx);
    assert_eq!(fs::read_to_string(dir.0.join("0.md")).unwrap(), "候选");
    cx.update(|_, cx| {
        editor.update(cx, |e, cx| {
            e.document.try_undo(false).unwrap();
            e.refresh(cx);
        })
    });
    tick(cx);
    assert!(store::candidates(&dir.0).unwrap().is_empty());
}

#[gpui::test]
fn failed_replacement_and_cancel_keep_copy_until_new_document_is_installed(
    cx: &mut TestAppContext,
) {
    let dir = Directory::new();
    let (editor, cx) = cx.add_window_view(|_, cx| {
        let mut e = test_editor(cx);
        e.recovery.directory = Some(dir.0.clone());
        e.document = e.document.fork("");
        e.document.replace(0..0, "my note").unwrap();
        e.refresh(cx);
        e
    });
    tick(cx);
    let path = dir.0.join("0.md");
    cx.update(|window, cx| {
        editor.update(cx, |e, cx| {
            e.open_external_path(dir.0.join("missing/note.md"), window, cx)
        })
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("放弃修改并打开");
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&path).unwrap(), "my note");
    for answer in ["取消", "放弃修改"] {
        cx.update(|window, cx| editor.update(cx, |e, cx| e.new_document(&crate::New, window, cx)));
        cx.run_until_parked();
        cx.simulate_prompt_answer(answer);
        cx.run_until_parked();
        if answer == "取消" {
            assert_eq!(fs::read_to_string(&path).unwrap(), "my note");
        }
    }
    tick(cx);
    assert!(!path.exists());
    editor.read_with(cx, |e, _| assert_eq!(e.document.source.text(), ""));
}

#[gpui::test]
fn a_stale_startup_answer_keeps_both_the_draft_and_the_users_edit(cx: &mut TestAppContext) {
    let dir = Directory::new();
    Draft::new(None).write(&dir.0, "older draft").unwrap();
    let (editor, cx) = cx.add_window_view(|_, cx| {
        let mut e = test_editor(cx);
        e.recovery.directory = Some(dir.0.clone());
        e.default_document = Some(welcome::DefaultDocument::new(
            &e.document,
            e.locale.language,
        ));
        e
    });
    cx.update(|window, cx| editor.update(cx, |e, cx| e.offer_drafts(window, cx)));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.update(|_, cx| {
        editor.update(cx, |e, cx| {
            e.document.replace(0..0, "new text ").unwrap();
            e.refresh(cx);
        })
    });
    cx.simulate_prompt_answer("恢复");
    tick(cx);
    assert_eq!(
        fs::read_to_string(dir.0.join("0.md")).unwrap(),
        "older draft"
    );
    editor.read_with(cx, |e, _| {
        assert!(e.document.source.text().starts_with("new text "));
        assert_eq!(
            fs::read_to_string(dir.0.join("1.md")).unwrap(),
            e.document.source.text()
        );
    });
}
