use super::{browser, view, Browser};
use crate::window_controls_tests::test_editor;
use crate::{Document, Language, Text, ToggleSidebar};
use gpui::{px, size, EntityInputHandler, Modifiers, TestAppContext, VisualTestContext};
use ryne_extensions::directory::*;
use std::{fs, io, time::Duration};
use std::{
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc},
};
use tempfile::tempdir;

fn click(cx: &mut VisualTestContext, id: &'static str) {
    let bounds = cx.debug_bounds(id).expect(id);
    cx.simulate_click(bounds.center(), Modifiers::default());
}

#[::core::prelude::v1::test]
fn display_roots_are_installed_not_requested_and_siblings_reuse_state() {
    let dir = tempdir().unwrap();
    let a = dir.path().join("a");
    let b = a.join("b");
    let mut state = Browser::default();
    assert!(state.installed(Some(&a.join("d.md"))));
    let first = state.generation;
    state.expanded.insert("b".into());
    state.filter = "manual".into();
    assert!(!state.installed(Some(&a.join("e.md"))));
    assert!(state.expanded.contains(Path::new("b")));
    assert_eq!(state.filter, "manual");
    assert!(state.installed(Some(&b.join("f.md"))));
    assert!(state.expanded.is_empty());
    assert!(state.filter.is_empty());
    assert_eq!(state.root.as_ref(), Some(&b));
    assert!(state.generation > first);
    state.root(Some(a.clone()), true, false);
    assert_eq!(state.relative_current(), Some(Path::new("b/f.md")));
    assert!(!state.installed(None));
    assert_eq!(state.root, Some(a));
    state.installed(Some(Path::new("single.md")));
    assert_eq!(state.root, Some(std::env::current_dir().unwrap()));
    state.installed(None);
    assert!(state.root.is_none());
}

#[::core::prelude::v1::test]
fn empty_folders_require_a_complete_scan_and_no_later_child() {
    let mut b = Browser::default();
    let dir = Path::new("sub");
    assert!(!b.known_empty(dir));
    for end in [
        Err("denied".into()),
        Ok(ScanEnd {
            examined: 0,
            status: ScanStatus::Truncated,
        }),
        Ok(ScanEnd {
            examined: 0,
            status: ScanStatus::Cancelled,
        }),
        Ok(ScanEnd {
            examined: 1,
            status: ScanStatus::Complete,
        }), // filtered child
    ] {
        b.finish(dir, end);
        assert!(!b.known_empty(dir));
    }
    b.finish(dir, Ok(ScanEnd::default()));
    assert!(b.known_empty(dir));
    b.insert(
        dir,
        DirectoryEntry {
            name: "new.md".into(),
            kind: EntryKind::Document,
            modified: None,
            bytes: None,
        },
    );
    assert!(!b.known_empty(dir));
}

#[::core::prelude::v1::test]
fn list_order_prefix_decoding_and_bounded_discovery() {
    let mut b = Browser::default();
    b.root(Some(std::env::temp_dir()), false, false);
    for (dir, name, modified) in [
        ("b", "same.md", Some(std::time::UNIX_EPOCH)),
        ("a", "same.md", Some(std::time::UNIX_EPOCH)),
        ("", "missing.txt", None),
    ] {
        b.insert(
            Path::new(dir),
            DirectoryEntry {
                name: name.into(),
                kind: EntryKind::Document,
                modified,
                bytes: None,
            },
        );
    }
    b.rows();
    assert_eq!(
        b.list,
        ["a/same.md", "b/same.md", "missing.txt"].map(PathBuf::from)
    );
    b.filter = "B/SAME".replace('/', std::path::MAIN_SEPARATOR_STR);
    b.dirty_rows = true;
    b.rows();
    assert_eq!(b.list, [PathBuf::from("b/same.md")]);
    assert_eq!(
        browser::summary(b"\r first\n\r second\nthird"),
        "first second"
    );
    assert!(browser::summary(b"ok\xff").is_empty());
    assert!(browser::summary(b"ok\0").is_empty());
    let mut bytes = vec![b'a'; PREFIX_LIMIT - 1];
    bytes.extend_from_slice("😀".as_bytes());
    assert_eq!(browser::summary(&bytes).len(), PREFIX_LIMIT - 1);
    let mut p = PathBuf::new();
    for _ in 0..MAX_DEPTH {
        p.push("sub")
    }
    b.insert(
        &p,
        DirectoryEntry {
            name: "deep".into(),
            kind: EntryKind::Directory,
            modified: None,
            bytes: None,
        },
    );
    assert!(b.limited());
}
#[gpui::test]
fn outline_jump_dock_and_overlay_preserve_editor_state(cx: &mut TestAppContext) {
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut e = test_editor(cx);
        let raw = format!(
            "# first\n\n{}\n## middle\n\n{}",
            "body\n\n".repeat(60),
            "tail\n\n".repeat(60)
        );
        e.document = e.document.fork(&raw);
        e.document.select(1, false, true);
        e.document.select(15, true, true);
        e.refresh(cx);
        window.focus(&e.focus);
        e
    });
    cx.simulate_resize(size(px(920.), px(680.)));
    cx.update(|w, cx| w.draw(cx).clear());
    let before = editor.read_with(cx, |e, _| {
        (
            e.document.source.id(),
            e.document.selection(),
            e.document.dirty(),
            e.document.history_len(false),
            e.viewport.geometry.clone().unwrap(),
        )
    });
    let toggle = cx.debug_bounds("toggle-sidebar").unwrap();
    let format = cx.debug_bounds("format-source").unwrap();
    let mode = cx.debug_bounds("toggle-source").unwrap();
    assert!(
        toggle.left() > px(460.)
            && toggle.right() <= format.left()
            && format.right() <= mode.left()
    );
    assert_eq!(toggle.center().y, mode.center().y);
    assert_eq!(cx.debug_bounds("document-titlebar").unwrap().left(), px(0.));
    cx.simulate_click(toggle.center(), Modifiers::default());
    cx.update(|w, cx| w.draw(cx).clear());
    let cap = cx.debug_bounds("sidebar-titlebar").unwrap();
    let panel = cx.debug_bounds("sidebar").unwrap();
    assert_eq!(cap.top(), px(0.));
    let menu_bar = cx.debug_bounds("window-menu-bar").unwrap();
    assert_eq!(cap.bottom(), menu_bar.top());
    assert_eq!(menu_bar.bottom(), panel.top());
    assert_eq!(cap.right(), panel.right());
    assert_eq!(
        cx.debug_bounds("document-titlebar").unwrap().left(),
        panel.right()
    );
    editor.read_with(cx, |e, _| {
        assert!(!before.4.is_current(e));
        assert_eq!(e.sidebar.dock, 260.);
        assert!(e.viewport.geometry.as_ref().unwrap().bounds.left() >= px(260.));
    });
    let heading = cx.debug_bounds("sidebar-row-1").unwrap();
    cx.simulate_click(heading.center(), Modifiers::default());
    cx.update(|w, cx| w.draw(cx).clear());
    editor.read_with(cx, |e, _| {
        assert!(e.viewport.scroll > px(500.));
        assert_eq!(e.sidebar.active, Some(1));
        assert_eq!(
            (
                e.document.source.id(),
                e.document.selection(),
                e.document.dirty(),
                e.document.history_len(false)
            ),
            (before.0, before.1.clone(), before.2, before.3)
        );
    });
    cx.simulate_resize(size(px(520.), px(560.)));
    cx.update(|w, cx| w.draw(cx).clear());
    let panel = cx.debug_bounds("sidebar").unwrap();
    assert_eq!(
        cx.debug_bounds("sidebar-titlebar").unwrap().right(),
        panel.right()
    );
    editor.read_with(cx, |e, _| {
        assert_eq!(e.viewport.geometry.as_ref().unwrap().bounds.left(), px(0.))
    });
    click(cx, "sidebar-row-0");
    cx.update(|w, cx| w.draw(cx).clear());
    assert_eq!(cx.debug_bounds("document-titlebar").unwrap().left(), px(0.));
    editor.read_with(cx, |e, _| {
        assert!(!e.sidebar.prefs.visible);
        assert_eq!(e.document.selection(), before.1);
        assert!(e.viewport.scroll < px(100.));
    });
}

#[gpui::test]
fn tree_and_list_open_filter_and_save_in_the_same_window(cx: &mut TestAppContext) {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::create_dir(root.join("sub")).unwrap();
    fs::create_dir(root.join("vacant")).unwrap();
    let first = root.join("first.md");
    let second = root.join("sub/second.md");
    fs::write(&first, "# First").unwrap();
    fs::write(&second, "# Second\r\n").unwrap();
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut e = test_editor(cx);
        let doc = Document::open(&first, e.document.services()).unwrap();
        e.install_document(doc, Text::Opened, window, cx);
        e
    });
    cx.simulate_resize(size(px(920.), px(600.)));
    editor.read_with(cx, |e, _| {
        assert_eq!(e.sidebar.browser.root.as_deref(), Some(root.as_path()));
        assert!(e.sidebar.request.is_none());
    });
    cx.update(|w, cx| {
        editor.update(cx, |e, cx| {
            e.sidebar_toggle(&ToggleSidebar, w, cx);
            e.pump_directory(cx);
            assert_eq!(
                e.sidebar.folder_icon(Path::new(""), true),
                "icon:folder_sync"
            );
        })
    });
    cx.run_until_parked();
    editor.read_with(cx, |e, _| {
        assert!(e.sidebar.browser.entries.contains_key(Path::new("sub")));
        assert_eq!(
            e.sidebar.folder_icon(Path::new(""), true),
            "icon:folder_open"
        );
        assert_eq!(
            e.sidebar.folder_icon(Path::new("vacant"), false),
            "icon:folder"
        );
        assert!(!e
            .sidebar
            .browser
            .entries
            .contains_key(Path::new("sub/second.md")));
    });
    cx.update(|_, cx| {
        editor.update(cx, |e, cx| {
            let first = Path::new("first.md");
            assert_eq!(
                e.sidebar.document_icon(first, None),
                "icon:text_bullet_list_square"
            );
            e.summary_for(first, cx);
            assert_eq!(e.sidebar.document_icon(first, None), "icon:folder_sync");
        })
    });
    click(cx, "list-view");
    cx.run_until_parked();
    editor.read_with(cx, |e, _| {
        assert_eq!(
            e.sidebar.folder_icon(Path::new("vacant"), false),
            "icon:folder_prohibited"
        );
        assert_eq!(
            e.sidebar.document_icon(Path::new("first.md"), None),
            "icon:text_bullet_list_square"
        );
    });
    let filter = editor.read_with(cx, |e, _| e.sidebar.filter.clone());
    click(cx, "sidebar-filter");
    cx.update(|w, cx| {
        filter.update(cx, |f, cx| {
            f.replace_and_mark_text_in_range(None, "😀待", Some(2..3), w, cx);
            assert_eq!(f.selected_text_range(false, w, cx).unwrap().range, 2..3);
            f.replace_text_in_range(None, "SECOND", w, cx);
        })
    });
    cx.run_until_parked();
    editor.read_with(cx, |e, _| {
        assert_eq!(e.sidebar.browser.list, [PathBuf::from("sub/second.md")]);
        assert_eq!(e.document.source.text(), "# First");
    });
    cx.update(|_, cx| {
        editor.update(cx, |e, cx| {
            e.document.replace(7..7, " edited").unwrap();
            e.refresh(cx);
        })
    });
    click(cx, "sidebar-row-0");
    cx.run_until_parked();
    cx.simulate_prompt_answer("保存并打开");
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&first).unwrap(), "# First edited");
    editor.read_with(cx, |e, _| {
        assert_eq!(e.document.files.path.as_ref(), Some(&second));
        assert_eq!(e.sidebar.browser.root, Some(root.join("sub")));
        assert!(e.sidebar.browser.filter.is_empty());
        assert_eq!(e.document.history_len(false), 0);
        assert!(!e.document.dirty());
    });
}

struct BrokenDirectory;
impl DirectoryBrowser for BrokenDirectory {
    fn enumerate(
        &self,
        _: ScanRequest<'_>,
        _: &dyn Fn() -> bool,
        _: &mut dyn FnMut(Vec<DirectoryEntry>) -> bool,
    ) -> io::Result<ScanEnd> {
        panic!("injected")
    }
    fn prefix(&self, _: &Path) -> io::Result<Vec<u8>> {
        Err(io::Error::other("unreadable"))
    }
    fn metadata(&self, _: &Path) -> io::Result<DirectoryEntry> {
        Err(io::Error::other("missing"))
    }
}
#[gpui::test]
fn stale_requests_and_failed_manual_roots_leave_the_installed_document(cx: &mut TestAppContext) {
    let (editor, cx) = cx.add_window_view(|_, cx| test_editor(cx));
    cx.update(|_, cx| {
        editor.update(cx, |e, cx| {
            e.desktop.directories = Arc::new(BrokenDirectory);
            let root = std::env::temp_dir().join("a");
            e.sidebar.browser.root(Some(root.clone()), false, false);
            e.sidebar.prefs.visible = true;
            e.pump_directory(cx);
            let request = e.sidebar.request.as_ref().unwrap().cancel.clone();
            e.sidebar
                .browser
                .root(Some(root.with_file_name("b")), false, false);
            e.sidebar.reset_root(false, cx);
            e.sidebar.browser.root(Some(root), false, false);
            e.sidebar.reset_root(false, cx);
            assert!(request.load(Ordering::Relaxed));
        })
    });
    cx.run_until_parked();
    let root = editor.read_with(cx, |e, _| {
        assert!(e.sidebar.request.is_none());
        assert_eq!(e.sidebar.browser.errors.len(), 1);
        e.sidebar.browser.root.clone()
    });
    cx.update(|_, cx| {
        editor.update(cx, |e, cx| {
            e.sidebar.choice = Some(PathBuf::from("unreadable"));
            e.pump_directory(cx);
        })
    });
    cx.run_until_parked();
    editor.read_with(cx, |e, _| {
        assert_eq!(e.sidebar.browser.root, root);
        assert!(e.sidebar.choice.is_none());
        assert!(e.sidebar.request.is_none());
        assert_eq!(e.document.source.text(), "plain text\r\n");
    });
}

#[gpui::test]
fn saving_untitled_before_unreadable_open_keeps_successful_path(cx: &mut TestAppContext) {
    let dir = tempdir().unwrap();
    let saved = dir.path().join("saved.md");
    let (editor, cx) = cx.add_window_view(|_, cx| test_editor(cx));
    cx.update(|w, cx| {
        editor.update(cx, |e, cx| {
            e.document.replace(0..0, "改").unwrap();
            e.open_external_path(dir.path().join("gone.md"), w, cx);
        })
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("保存并打开");
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(saved.clone()));
    cx.run_until_parked();
    editor.read_with(cx, |e, _| {
        assert_eq!(e.document.files.path.as_ref(), Some(&saved));
        assert_eq!(e.sidebar.browser.root.as_deref(), Some(dir.path()));
        assert!(!e.document.dirty());
        assert_eq!(
            e.document.source.text(),
            fs::read_to_string(&saved).unwrap()
        );
        assert!(!e.operations.file_operation_pending.is_pending());
    });
}

#[::core::prelude::v1::test]
fn modified_dates_cover_epoch_and_leap_days() {
    use view::modified_label;
    assert_eq!(
        modified_label(
            std::time::UNIX_EPOCH - Duration::from_secs(60),
            Language::En
        ),
        "1969-12-31 23:59 UTC"
    );
    assert_eq!(
        modified_label(
            std::time::UNIX_EPOCH + Duration::from_secs(951782400),
            Language::Fr
        ),
        "29/02/2000 00:00 UTC"
    );
}

#[derive(Default)]
struct MeasuredDirectory {
    enumerations: std::sync::atomic::AtomicUsize,
    summaries: std::sync::atomic::AtomicUsize,
}
impl DirectoryBrowser for MeasuredDirectory {
    fn enumerate(
        &self,
        r: ScanRequest<'_>,
        cancelled: &dyn Fn() -> bool,
        emit: &mut dyn FnMut(Vec<DirectoryEntry>) -> bool,
    ) -> io::Result<ScanEnd> {
        self.enumerations.fetch_add(1, Ordering::Relaxed);
        ryne_services::directory::LocalDirectory.enumerate(r, cancelled, emit)
    }
    fn prefix(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.summaries.fetch_add(1, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(2));
        ryne_services::directory::LocalDirectory.prefix(path)
    }
    fn metadata(&self, path: &Path) -> io::Result<DirectoryEntry> {
        ryne_services::directory::LocalDirectory.metadata(path)
    }
}

#[gpui::test]
#[ignore = "creates a 10,000-document fixture and measures GPUI test-window frames"]
fn large_directory_and_outline_sample(cx: &mut TestAppContext) {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let entry = root.join("index.md");
    let source = (0..1000)
        .map(|i| format!("## Section {i}\n\ntext\n\n"))
        .collect::<String>();
    fs::write(&entry, source).unwrap();
    for i in 0..100 {
        let folder = root.join(format!("folder-{i:03}"));
        fs::create_dir(&folder).unwrap();
        for j in 0..100 {
            fs::write(folder.join(format!("sample-{j:03}.md")), b"preview\n").unwrap();
        }
    }
    fs::create_dir(root.join("target")).unwrap();
    fs::write(root.join("target/ignored.md"), "ignored").unwrap();
    let service = Arc::new(MeasuredDirectory::default());
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut e = test_editor(cx);
        e.desktop.directories = service.clone();
        e.install_document(
            Document::open(&entry, e.document.services()).unwrap(),
            Text::Opened,
            window,
            cx,
        );
        e
    });
    cx.simulate_resize(size(px(920.), px(680.)));
    let start = std::time::Instant::now();
    cx.update(|w, cx| editor.update(cx, |e, cx| e.sidebar_toggle(&ToggleSidebar, w, cx)));
    cx.run_until_parked();
    let tree_time = start.elapsed();
    assert_eq!(service.enumerations.load(Ordering::Relaxed), 1);
    let list_start = std::time::Instant::now();
    click(cx, "list-view");
    cx.run_until_parked();
    let scan_time = list_start.elapsed();
    let read = service.enumerations.load(Ordering::Relaxed);
    let (entries, documents, peak) = editor.read_with(cx, |e, _| {
        assert!(e.sidebar.browser.limited());
        assert!(e.sidebar.browser.entries.len() <= ENTRY_LIMIT);
        (
            e.sidebar.browser.entries.len(),
            e.sidebar.browser.list.len(),
            e.sidebar.queue_peak.load(Ordering::Relaxed),
        )
    });
    assert!(documents >= 9800);
    let filter = editor.read_with(cx, |e, _| e.sidebar.filter.clone());
    cx.update(|w, cx| {
        filter.update(cx, |f, cx| {
            f.replace_text_in_range(None, "sample-045", w, cx)
        })
    });
    cx.run_until_parked();
    assert_eq!(service.enumerations.load(Ordering::Relaxed), read);
    click(cx, "outline-tab");
    cx.run_until_parked();
    let outline_start = std::time::Instant::now();
    let ptr = editor.read_with(cx, |e, _| {
        let outline = ryne_syntax::SemanticQuery::new(&e.document.parse).outline();
        assert_eq!(outline.len(), 1000);
        outline.as_ptr()
    });
    let mut frames = Vec::new();
    for i in 1..=30 {
        let t = std::time::Instant::now();
        cx.update(|w, cx| {
            editor.update(cx, |e, _| e.viewport.scroll = px(i as f32 * 1500.));
            w.draw(cx).clear();
        });
        frames.push(t.elapsed());
    }
    frames.sort_unstable();
    assert_eq!(
        ptr,
        editor.read_with(cx, |e, _| ryne_syntax::SemanticQuery::new(
            &e.document.parse
        )
        .outline()
        .as_ptr())
    );
    println!("tree first drawable {tree_time:?}, list scan {scan_time:?}, {entries} entries, {documents} documents, {read} enumerations, {} prefixes, channel peak {peak}, 1000 headings / 30 scroll frames {:?}, frame p50 {:?} p95 {:?} max {:?}",service.summaries.load(Ordering::Relaxed),outline_start.elapsed(),frames[15],frames[28],frames[29]);
}

#[gpui::test]
fn queued_save_intent_and_older_revision_bind_only_after_accepted_write(cx: &mut TestAppContext) {
    use ryne_session::SaveIntent;
    let dir = tempdir().unwrap();
    let file = dir.path().join("note.md");
    let manual = dir.path().join("manual");
    fs::create_dir(&manual).unwrap();
    let (editor, cx) = cx.add_window_view(|_, cx| test_editor(cx));
    cx.update(|w, cx| {
        editor.update(cx, |e, cx| {
            e.document.replace(0..0, "first").unwrap();
            let first = e.document.queue_save(file.clone()).unwrap();
            e.document.replace(0..0, "second").unwrap();
            assert!(!e.finish_save_receipt(first.run(), w, cx));
            assert!(e.document.dirty());
            assert_eq!(e.sidebar.browser.root.as_deref(), Some(dir.path()));
            e.sidebar.browser.root(Some(manual.clone()), true, false);
            let ordinary = e.document.queue_save(file.clone()).unwrap();
            let _ = e
                .document
                .queue_save_with_intent(file.clone(), SaveIntent::SaveAs);
            let _ = e.document.queue_save(file.clone());
            assert!(!e.finish_save_receipt(ordinary.run(), w, cx));
        })
    });
    cx.run_until_parked();
    editor.read_with(cx, |e, _| {
        assert!(!e.document.dirty());
        assert_eq!(e.sidebar.browser.root.as_ref(), Some(&manual));
    });
    cx.update(|w, cx| {
        editor.update(cx, |e, cx| {
            let explicit = e
                .document
                .queue_save_with_intent(file.clone(), SaveIntent::SaveAs)
                .unwrap();
            assert!(e.finish_save_receipt(explicit.run(), w, cx));
            assert_eq!(e.sidebar.browser.root.as_deref(), Some(dir.path()));
        })
    });
}

#[gpui::test]
fn resizing_rejects_stale_geometry_and_ime_blocks_panel_navigation(cx: &mut TestAppContext) {
    let (editor, cx) = cx.add_window_view(|w, cx| {
        let e = test_editor(cx);
        w.focus(&e.focus);
        e
    });
    cx.simulate_resize(size(px(920.), px(680.)));
    cx.update(|w, cx| {
        editor.update(cx, |e, cx| {
            e.replace_and_mark_text_in_range(None, "预😀", Some(3..3), w, cx);
            e.sidebar_toggle(&ToggleSidebar, w, cx);
            assert!(!e.sidebar.prefs.visible);
            e.unmark_text(w, cx);
            e.sidebar_toggle(&ToggleSidebar, w, cx);
        })
    });
    cx.update(|w, cx| w.draw(cx).clear());
    let geometry = editor.read_with(cx, |e, _| e.viewport.geometry.clone().unwrap());
    let divider = cx.debug_bounds("sidebar-divider").unwrap().center();
    cx.simulate_mouse_down(divider, gpui::MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        gpui::point(px(310.), divider.y),
        Some(gpui::MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        gpui::point(px(310.), divider.y),
        gpui::MouseButton::Left,
        Modifiers::default(),
    );
    cx.update(|w, cx| w.draw(cx).clear());
    assert_eq!(
        cx.debug_bounds("sidebar-titlebar").unwrap().right(),
        px(310.)
    );
    editor.read_with(cx, |e, _| {
        assert_eq!(e.sidebar.dock, 310.);
        assert!(!geometry.is_current(e));
        assert!(!e.sidebar.resizing);
    });
}

#[::core::prelude::v1::test]
fn preferences_round_trip_only_presentation_state() {
    use super::Preferences;
    let dir = tempdir().unwrap();
    let path = dir.path().join("sidebar.json");
    let mut p = Preferences::load(Some(path.clone()));
    assert!(!p.visible);
    assert_eq!(p.directory, None);
    p.width = 324.;
    p.visible = true;
    p.directory = Some(true);
    p.list = true;
    p.save().unwrap();
    let p = Preferences::load(Some(path.clone()));
    assert_eq!(p.width, 324.);
    assert!(p.visible && p.list);
    assert_eq!(p.directory, Some(true));
    fs::write(
        &path,
        br#"{"width":9000,"directory":"invalid","visible":123}"#,
    )
    .unwrap();
    let p = Preferences::load(Some(path));
    assert_eq!(p.width, 260.);
    assert_eq!(p.directory, None);
    assert!(!p.visible);
}

#[cfg(unix)]
#[test]
fn lossy_labels_do_not_merge_different_native_file_names() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let mut browser = Browser::default();
    let root = std::env::temp_dir();
    let current = OsStr::from_bytes(b"note\xff.md");
    browser.installed(Some(&root.join(current)));
    for name in [current, OsStr::from_bytes(b"note\xfe.md")] {
        browser.insert(
            Path::new(""),
            DirectoryEntry {
                name: name.to_owned(),
                kind: EntryKind::Document,
                bytes: None,
                modified: None,
            },
        );
    }
    browser.rows();
    assert_eq!(browser.list.len(), 2);
    assert_eq!(
        browser.list[0].to_string_lossy(),
        browser.list[1].to_string_lossy()
    );
    assert_eq!(browser.relative_current(), Some(Path::new(current)));
}

#[gpui::test]
fn header_search_keeps_the_filtered_heading_source_target(cx: &mut TestAppContext) {
    let (editor, cx) = cx.add_window_view(|w, cx| {
        let mut e = test_editor(cx);
        let raw = format!(
            "# A\n\n{}\n## Find me\n\n{}",
            "paragraph\n\n".repeat(50),
            "tail\n\n".repeat(50)
        );
        e.document = e.document.fork(&raw);
        e.refresh(cx);
        w.focus(&e.focus);
        e
    });
    let original_selection = editor.read_with(cx, |e, _| e.document.selection());
    cx.simulate_resize(size(px(920.), px(680.)));
    cx.update(|w, cx| editor.update(cx, |e, cx| e.sidebar_toggle(&ToggleSidebar, w, cx)));
    cx.run_until_parked();
    click(cx, "sidebar-search");
    cx.run_until_parked();
    cx.simulate_input("FIND");
    cx.run_until_parked();
    editor.read_with(cx, |e, _| {
        assert_eq!(e.sidebar.heading_matches, [1]);
        assert_eq!(e.document.selection(), original_selection);
    });
    click(cx, "sidebar-row-0");
    cx.run_until_parked();
    editor.read_with(cx, |e, _| {
        assert_eq!(e.sidebar.active, Some(1));
        assert!(e.viewport.scroll > px(500.));
        assert_eq!(e.document.history_len(false), 0);
    });
}

mod lifecycle;
