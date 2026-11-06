//! Exercise the actual editor div tree with GPUI's platform event dispatcher.
//! No native AppKit/Win32 loop is involved, so failed checks stay in the test runner.
use super::{
    math_fonts, DesktopServices, Document, Editor, ExportRegistry, Language, Localization,
    SelectAll, SessionServices, Text,
};
use gpui::{
    point, px, Context, KeyBinding, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent,
    TestAppContext,
};
use std::sync::Arc;

fn test_editor(cx: &mut Context<Editor>) -> Editor {
    let formulas = Arc::new(ryne_services::math::FormularyRenderer::default());
    let document = Document::new(
        "plain text\r\n",
        SessionServices {
            features: Arc::new(
                ryne_features::MarkdownFeatures::new(formulas.clone(), vec![])
                    .expect("test features"),
            ),
            files: Arc::new(ryne_services::files::AtomicFileStore),
        },
    );
    let mut editor = Editor::new(
        document,
        DesktopServices {
            exports: ExportRegistry::new(vec![]).expect("test exports"),
            artifacts: Arc::new(ryne_services::files::AtomicArtifactStore),
            updates: Arc::new(ryne_services::updates::GithubUpdates),
        },
        formulas,
        math_fonts::MathFontPreference::load(None),
        Box::new(ryne_services::highlight::CodeHighlighter::default()),
        Localization::for_smoke(),
        cx,
    );
    editor.smoke_mode = true;
    editor
}

#[gpui::test]
fn titlebar_preserves_native_mouse_handling_and_document_focus(cx: &mut TestAppContext) {
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = test_editor(cx);
        window.focus(&editor.focus);
        editor
    });
    cx.update(|window, cx| {
        window.draw(cx).clear();
        cx.bind_keys([KeyBinding::new("cmd-a", SelectAll, Some("MarkdownEditor"))]);
    });
    let before = editor.read_with(cx, |editor, _| {
        (
            editor.document.source.id(),
            editor.document.selection(),
            editor.document.history_len(false),
            editor.document.dirty(),
        )
    });
    // Test builds render the Windows header on every host. Exercise the same
    // row and hit targets without invoking native Win32 operations.
    let drag = cx.debug_bounds("window-drag").expect("caption drag area");
    assert!(drag.size.width > px(0.));
    for (index, selector) in ["app-menu-0", "app-menu-1"].into_iter().enumerate() {
        let menu = cx.debug_bounds(selector).expect("menu button");
        assert_eq!(menu.center().y, drag.center().y);
        assert!(menu.right() <= drag.left());
        cx.simulate_click(menu.center(), Modifiers::default());
        assert_eq!(editor.read_with(cx, |e, _| e.menus.top_menu), Some(index));
        cx.update(|window, cx| window.draw(cx).clear());
        let popup = cx.debug_bounds("app-menu-popup").expect("open menu");
        assert!(popup.top() >= menu.bottom());
    }
    for area in [
        "window-drag",
        "window-minimize",
        "window-maximize",
        "window-close",
    ] {
        let bounds = cx.debug_bounds(area).expect("caption hit target");
        let position = bounds.center();
        assert_eq!(position.y, drag.center().y);
        if area != "window-drag" {
            assert!(bounds.left() >= drag.right());
        }
        cx.simulate_mouse_move(position, None, Modifiers::default());
        for click_count in [1, 2] {
            cx.simulate_event(MouseDownEvent {
                button: MouseButton::Left,
                position,
                click_count,
                ..Default::default()
            });
            // Windows treats either prevent_default or stopped propagation as
            // handled, skipping native dragging / recording the pressed button.
            assert!(
                !cx.update(|window, _| window.default_prevented()),
                "{area:?} press suppressed native handling"
            );
            cx.simulate_event(MouseUpEvent {
                button: MouseButton::Left,
                position,
                click_count,
                ..Default::default()
            });
        }
    }
    editor.read_with(cx, |editor, _| {
        assert_eq!(
            before,
            (
                editor.document.source.id(),
                editor.document.selection(),
                editor.document.history_len(false),
                editor.document.dirty(),
            )
        );
    });
    assert!(cx.update(|window, cx| editor.read(cx).focus.is_focused(window)));
    // The document must still acquire focus on click, and its ancestor action
    // handlers / key context must remain on the focused node's dispatch path.
    let position = editor.read_with(cx, |editor, _| {
        editor
            .viewport
            .geometry
            .as_ref()
            .expect("test geometry")
            .position(2)
            + point(px(1.), px(8.))
    });
    cx.update(|window, _| window.blur());
    cx.simulate_mouse_move(position, None, Modifiers::default());
    cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::default());
    assert!(cx.update(|window, _| window.default_prevented()));
    cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::default());
    cx.simulate_keystrokes("cmd-a");
    assert!(cx.update(|window, cx| editor.read(cx).focus.is_focused(window)));
    editor.read_with(cx, |editor, _| {
        assert_eq!(
            editor.document.selection(),
            0..editor.document.source.text().len()
        );
        assert_eq!(editor.document.source.text(), "plain text\r\n");
        assert_eq!(editor.document.history_len(false), 0);
        assert!(!editor.document.dirty());
    });

    // Use the same should-close callback as the application. Reaching the
    // native close path must still wait for saves and confirm unsaved edits.
    let weak = editor.downgrade();
    cx.update(|window, cx| {
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |editor, cx| editor.request_close(window, cx))
                .unwrap_or(true)
        });
    });
    assert!(cx.simulate_close());
    editor.update(cx, |editor, _| editor.document.files.saving = true);
    assert!(!cx.simulate_close());
    editor.update(cx, |editor, _| {
        editor.document.files.saving = false;
        editor
            .document
            .replace(0..0, "unsaved ")
            .expect("test edit");
    });
    assert!(!cx.simulate_close());
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer(Language::ZhHans.text(Text::Cancel));
    cx.run_until_parked();
    editor.read_with(cx, |editor, _| {
        assert!(!editor.operations.close_prompt);
        assert!(!editor.operations.allow_close);
        assert_eq!(editor.document.source.text(), "unsaved plain text\r\n");
        assert!(editor.document.dirty());
        assert_eq!(editor.document.history_len(false), 1);
    });
}
