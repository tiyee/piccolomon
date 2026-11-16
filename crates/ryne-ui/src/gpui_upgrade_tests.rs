//! GPUI upgrade contracts, exercised through both platform and action entry points.
use super::{Document, Paste};
use crate::window_controls_tests::test_editor;
use gpui::{ClipboardItem, EntityInputHandler, KeyDownEvent, Keystroke, TestAppContext};

#[gpui::test]
fn platform_paste_preserves_action_transactions_and_ime(cx: &mut TestAppContext) {
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = test_editor(cx);
        window.focus(&editor.focus, cx);
        editor
    });
    for (source, text, preedit) in [
        ("- item\r\n", "first\r\nsecond😀", None),
        ("> quote\r", "first\rsecond", None),
        ("**bold**\n", "*literal*", None),
        ("prefix ", "pasted😀", Some("中文")),
    ] {
        let mut outcomes = Vec::new();
        for native in [false, true] {
            cx.update(|window, cx| {
                editor.update(cx, |e, cx| {
                    e.document = Document::new(source, e.document.services());
                    e.document
                        .select(source.trim_end_matches(['\r', '\n']).len(), false, false);
                    e.refresh(cx);
                    if let Some(preedit) = preedit {
                        e.replace_and_mark_text_in_range(None, preedit, None, window, cx);
                        assert_eq!(e.document.source.text(), source);
                    }
                    let item = ClipboardItem::new_string(text.into());
                    if native {
                        EntityInputHandler::paste(e, item, window, cx);
                    } else {
                        cx.write_to_clipboard(item);
                        e.paste(&Paste, window, cx);
                    }
                    assert!(e.document.composition().is_none());
                    let after = e.document.source.text().to_owned();
                    assert_ne!(after, source);
                    let selection = e.document.selection();
                    let history = e.document.history_len(false);
                    assert_eq!(history, if preedit.is_some() { 2 } else { 1 });
                    e.document.try_undo(false).unwrap();
                    let undone = e.document.source.text().to_owned();
                    if preedit.is_none() {
                        assert_eq!(undone, source);
                    } else {
                        assert_eq!(undone, "prefix 中文");
                    }
                    e.document.try_undo(true).unwrap();
                    assert_eq!(e.document.source.text(), after);
                    outcomes.push((after, selection, history, undone));
                });
            });
        }
        assert_eq!(
            outcomes[0], outcomes[1],
            "paste routes diverged for {source:?}"
        );
    }
}

#[gpui::test]
fn character_preference_does_not_open_block_action_menu(cx: &mut TestAppContext) {
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut e = test_editor(cx);
        let source = "| A | B |\n| - | - |\n| value | second |\n";
        e.document = Document::new(source, e.document.services());
        e.document
            .select(source.find("value").unwrap(), false, false);
        e.refresh(cx);
        window.focus(&e.focus, cx);
        e
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        editor.update(cx, |e, cx| {
            let mut event = KeyDownEvent {
                keystroke: Keystroke::parse("alt-enter").unwrap(),
                is_held: false,
                prefer_character_input: true,
            };
            let source = e.document.source.id();
            e.render_shortcut(&event, window, cx);
            assert!(e.menus.render_menu.is_none());
            assert_eq!(e.document.source.id(), source);
            event.prefer_character_input = false;
            e.render_shortcut(&event, window, cx);
            assert!(e.menus.render_menu.is_some());
            assert_eq!(e.document.source.id(), source);
        });
    });
}
