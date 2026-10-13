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
            let name = target.icon.expect("block should have an asset");
            assert_icon_renders(name);
        }
    }
    #[test]
    fn render_menu_svg_assets_are_embedded_and_nonempty() {
        for name in ["more_vertical", "table_add", "delete", "arrow_move", "checkmark_square", "add_square", "arrow_turn_right"] {
            assert_icon_renders(name);
        }
    }
    fn assert_icon_renders(name: &str) {
            use gpui::AssetSource;
            let data = (crate::assets::Assets {formulas:std::sync::Arc::new(NoFormulas)})
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

