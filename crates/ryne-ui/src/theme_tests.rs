//! Real GPUI action dispatch and layout-cache coverage without native event-loop assertions.
use crate::theme::{self, Mode, Palette, Settings};
use crate::window_controls_tests::test_editor;
use crate::{menus, Document};
use gpui::{BorrowAppContext, EntityInputHandler, TestAppContext, WindowAppearance};

#[gpui::test]
fn theme_action_repaints_without_changing_document_geometry_or_ime(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("theme");
    let (editor, cx) = cx.add_window_view(|window, cx| {
        cx.set_global(Settings::load(Some(path.clone()), WindowAppearance::Light));
        let mut editor = test_editor(cx);
        editor.document = Document::new(
            "# Heading\r\n\r\n> Quote\r\n\r\n- **list**\r\n\r\n```rust\r\nlet value = 42;\r\n```\r\n\r\n| A | B |\r\n| - | - |\r\n| a | b |\r\n",
            editor.document.services(),
        );
        editor.refresh(cx);
        window.focus(&editor.focus);
        editor
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let original = editor.read_with(cx, |e, _| e.viewport.geometry.clone().unwrap());
    let source = editor.read_with(cx, |e, _| e.document.source.id());
    let display = editor.read_with(cx, |e, _| e.viewport.display.clone());
    for mode in [Mode::Dark, Mode::Light, Mode::System] {
        cx.update(|window, cx| {
            window.dispatch_action(Box::new(menus::SelectTheme { mode }), cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        editor.read_with(cx, |e, cx| {
            assert_eq!(theme::selected(cx), mode);
            assert_eq!(
                Settings::load(Some(path.clone()), WindowAppearance::Dark).mode,
                mode
            );
            assert_eq!(e.document.source.id(), source);
            assert!(!e.document.dirty());
            assert_eq!(e.document.history_len(false), 0);
            assert!(std::sync::Arc::ptr_eq(&display, &e.viewport.display));
            let geometry = e.viewport.geometry.as_ref().unwrap();
            assert_eq!(
                geometry.text_layout.key.colors,
                mode.palette(WindowAppearance::Light)
            );
            assert_eq!(geometry.height, original.height);
            assert_eq!(geometry.lines.len(), original.lines.len());
            for (line, before) in geometry.lines.iter().zip(original.lines.iter()) {
                assert_eq!(
                    (line.start, line.top, line.line_height),
                    (before.start, before.top, before.line_height)
                );
            }
            if mode == Mode::Dark {
                assert!(!std::sync::Arc::ptr_eq(
                    &geometry.text_layout,
                    &original.text_layout
                ));
            }
        });
    }
    // An OS event changes System immediately, but cannot override an explicit choice.
    cx.update(|_, cx| {
        cx.update_global::<Settings, _>(|s, _| s.system = WindowAppearance::Dark);
        cx.refresh_windows();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    editor.read_with(cx, |e, _| {
        assert_eq!(
            e.viewport.geometry.as_ref().unwrap().text_layout.key.colors,
            Palette::DARK
        );
    });
    cx.update(|window, cx| {
        editor.update(cx, |e, cx| {
            e.replace_and_mark_text_in_range(None, "预😀", Some(3..3), window, cx);
            let composition = e.document.composition().cloned();
            let selection = e.document.selection();
            let before = e.document.source.id();
            let history = e.document.history_len(false);
            e.viewport.dragging = true;
            e.change_theme(&menus::SelectTheme { mode: Mode::Light }, window, cx);
            assert_eq!(e.document.composition().cloned(), composition);
            assert_eq!(e.document.selection(), selection);
            assert_eq!(e.document.source.id(), before);
            assert_eq!(e.document.history_len(false), history);
            assert!(!e.document.dirty());
            assert!(e.viewport.dragging);
        });
        cx.update_global::<Settings, _>(|s, _| s.system = WindowAppearance::Dark);
        assert_eq!(theme::palette(cx), Palette::LIGHT);
        window.draw(cx).clear();
    });
}
