#[test]
fn embedded_block_icons_render_at_control_size() {
    for text in [
        "# title",
        "## title",
        "### title",
        "#### title",
        "##### title",
        "###### title",
        "title\n===",
        "paragraph",
        "- bullet",
        "1. numbered",
        "- [ ] task",
        "![alt](img.png)",
        "```rust\nliteral\n```",
        "<div>raw</div>",
        "| a | b |\n|---|---|\n| c | d |",
        "> quote",
        "```mermaid\ngraph LR\n```",
        "---",
    ] {
        let parse = ParseSnapshot::new(Source::new(text));
        let target = crate::block::at(&parse, 0).unwrap();
        let name = ryne_extensions::BlockMetadata::from(target.kind).icon.expect("block should have an asset");
        assert_icon_renders(name);
    }
}
#[test]
fn render_menu_svg_assets_are_embedded_and_nonempty() {
    for name in [
        "format",
        "image_failed",
        "eye",
        "code",
        "checkbox_checked",
        "checkbox_unchecked",
        "more_vertical",
        "chevron_down",
        "table_add",
        "delete",
        "arrow_move",
        "checkmark_square",
        "add_square",
        "arrow_turn_right",
        "quote_bubble",
        "quoter",
        "window_minimize",
        "window_maximize",
        "window_restore",
        "window_close",
        "reading_list",
        "folder",
        "folder_open",
        "folder_prohibited",
        "folder_sync",
        "panel_right_expand",
        "panel_left_expand",
        "text_bullet_list_square",
    ] {
        assert_icon_renders(name);
    }
}
fn assert_icon_renders(name: &str) {
    use gpui::AssetSource;
    let data = (crate::assets::Assets {
        formulas: std::sync::Arc::new(NoFormulas),
    })
    .load(&format!("icon:{name}"))
    .unwrap()
    .unwrap();
    let tree = resvg::usvg::Tree::from_data(&data, &Default::default()).unwrap();
    let mut pixels = resvg::tiny_skia::Pixmap::new(16, 16).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(
            16. / tree.size().width(),
            16. / tree.size().height(),
        ),
        &mut pixels.as_mut(),
    );
    assert!(
        pixels
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] > 0),
        "{name}"
    );
}

#[test]
fn language_chevron_has_centered_visible_bounds() {
    let data = include_bytes!("../../../assets/icons/chevron_down.svg");
    let tree = resvg::usvg::Tree::from_data(data, &Default::default()).unwrap();
    let mut pixels = resvg::tiny_skia::Pixmap::new(48, 48).unwrap();
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(4., 4.), &mut pixels.as_mut());
    let mut left = 48;
    let mut right = 0;
    let mut top = 48;
    let mut bottom = 0;
    for (index, pixel) in pixels.data().as_chunks::<4>().0.iter().enumerate() {
        if pixel[3] > 0 {
            let x = index % 48;
            let y = index / 48;
            left = left.min(x);
            right = right.max(x);
            top = top.min(y);
            bottom = bottom.max(y);
        }
    }
    assert!(left < right && top < bottom);
    assert!((left + right).abs_diff(47) <= 1);
    assert!((top + bottom).abs_diff(47) <= 1);
}
