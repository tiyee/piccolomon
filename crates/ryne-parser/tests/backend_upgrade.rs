use pulldown_cmark::{html::push_html, Options, Parser};
use ryne_parser::{
    backend_html, options, project, Edit, HtmlPolicy, Interaction, NodeKind, ParseSnapshot, Source,
};

#[test]
fn backend_upgrade_keeps_optional_extensions_explicit() {
    assert_eq!(
        options(),
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
    );
}

#[test]
fn nested_wikilink_like_text_stays_literal_and_reversibly_editable() {
    for newline in ["\n", "\r\n", "\r"] {
        for body in ["![[![[中😀.png]]]]", "![[外层|![[中😀.png|说明]]]]"] {
            let text = format!("{body}{newline}");
            for math_enabled in [false, true] {
                let parse = ParseSnapshot::with_dialect_and_math(
                    Source::new(&text),
                    ryne_parser::Dialect::Gfm029,
                    math_enabled,
                );
                assert_eq!(parse.syntax.serialize(parse.source()).unwrap(), text);
                assert!(!parse.nodes.iter().any(|node| matches!(
                    node.kind,
                    NodeKind::Link { .. } | NodeKind::Image { .. }
                )));
                assert_eq!(backend_html(&text), format!("<p>{body}</p>\n"));
                assert_eq!(parse.html(HtmlPolicy::Export), format!("<p>{body}</p>\n"));
                let projected = project(&parse, &Interaction::new(parse.source())).unwrap();
                assert!(projected.text.contains(body));
                for token in &parse.syntax.tokens {
                    assert!(text.get(token.range.clone()).is_some());
                }

                let start = text.find("中😀").unwrap();
                let applied = parse
                    .source()
                    .apply(
                        parse.source().id(),
                        vec![Edit {
                            range: start..start + "中😀".len(),
                            replacement: "新👩‍💻".into(),
                        }],
                    )
                    .unwrap();
                assert_eq!(applied.source.text(), text.replace("中😀", "新👩‍💻"));
                assert_eq!(
                    applied
                        .source
                        .commit(applied.inverse)
                        .unwrap()
                        .source
                        .text(),
                    text
                );
            }
        }
    }
}

#[test]
fn upstream_nested_image_wikilink_fix_prefers_the_innermost_image() {
    // Exercise the 0.13.6 fix directly without enabling wikilinks in Ryne.
    // A semantic assertion avoids a machine-dependent timing threshold.
    for depth in [2, 4096] {
        let source = format!("{}中.png{}", "![[".repeat(depth), "]]".repeat(depth));
        let mut actual = String::new();
        push_html(
            &mut actual,
            Parser::new_ext(&source, Options::ENABLE_WIKILINKS),
        );
        assert_eq!(
            actual,
            format!(
                "<p>{}<img src=\"%E4%B8%AD.png\" alt=\"中.png\" />{}</p>\n",
                "![[".repeat(depth - 1),
                "]]".repeat(depth - 1),
            )
        );
    }
}
