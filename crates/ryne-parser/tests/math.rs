use ryne_parser::{
    project, Affinity, Edit, HtmlPolicy, Interaction, NodeKind, Origin, ParseSnapshot, Source,
    TokenKind,
};

#[test]
fn opt_in_math_has_literal_content_and_exact_marker_ownership() {
    let text = "中文 **$a_1 + \\frac{b}{c} + `x` + [y](z)$**\r\n\r\n$$x^2$$\r\n";
    let plain = ParseSnapshot::new(Source::new(text));
    assert!(!plain.math_enabled);
    assert!(!plain
        .nodes
        .iter()
        .any(|n| matches!(n.kind, NodeKind::InlineMath(_) | NodeKind::DisplayMath(_))));
    let parse = ParseSnapshot::with_math(Source::new(text));
    assert_eq!(parse.syntax.serialize(parse.source()).unwrap(), text);
    let (id, node) = parse
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| matches!(n.kind, NodeKind::InlineMath(_)))
        .unwrap();
    assert_eq!(
        node.kind,
        NodeKind::InlineMath("a_1 + \\frac{b}{c} + `x` + [y](z)".into())
    );
    assert!(node.children.is_empty());
    assert_eq!(
        &text[node.envelope.clone()],
        "$a_1 + \\frac{b}{c} + `x` + [y](z)$"
    );
    let tokens = parse.syntax.nodes[id]
        .owned_tokens
        .iter()
        .map(|&id| &parse.syntax.tokens[id])
        .collect::<Vec<_>>();
    assert_eq!(tokens.first().unwrap().kind, TokenKind::OpenMarker);
    assert_eq!(tokens.last().unwrap().kind, TokenKind::CloseMarker);
    assert_eq!(&text[tokens[0].range.clone()], "$");
    assert!(parse
        .nodes
        .iter()
        .any(|n| n.kind == NodeKind::DisplayMath("x^2".into())));
    let html = parse.html(HtmlPolicy::Export);
    assert!(html.contains("$a_1 + \\frac{b}{c} + `x` + [y](z)$"));
}

#[test]
fn math_does_not_parse_in_code_html_destinations_or_escaped_dollars() {
    let text = "`$inline$` \\$escaped\\$ [link](https://a/$url$)\n\n```tex\n$fenced$\n```\n\n    $indented$\n\n<div>\n$html$\n</div>\n\n$unfinished\n";
    let parse = ParseSnapshot::with_math(Source::new(text));
    assert!(!parse
        .nodes
        .iter()
        .any(|n| matches!(n.kind, NodeKind::InlineMath(_) | NodeKind::DisplayMath(_))));
    assert_eq!(parse.syntax.serialize(parse.source()).unwrap(), text);
}

#[test]
fn nested_math_object_reveals_source_and_reversible_edit_preserves_crlf() {
    let text = "> - 中文 $x_1$ 😀\r\n\r\n| A | B |\r\n| --- | --- |\r\n| $y$ | z |\r\n";
    let parse = ParseSnapshot::with_math(Source::new(text));
    let mut interaction = Interaction::new(parse.source());
    let folded = project(&parse, &interaction).unwrap();
    let run = folded.runs.iter().find(|r| matches!(&r.origin, Origin::Object { node, .. } if matches!(parse.nodes[*node].kind, NodeKind::InlineMath(_)))).unwrap();
    let hit = folded
        .hit(run.projected.start, Affinity::Downstream)
        .unwrap();
    interaction.enter(&folded, &hit).unwrap();
    let open = project(&parse, &interaction).unwrap();
    assert!(open.text.contains("$x_1$"));
    assert_eq!(parse.source().text(), text);
    let start = text.find("x_1").unwrap();
    let edited = parse
        .source()
        .apply(
            parse.source().id(),
            vec![Edit {
                range: start..start + 3,
                replacement: "\\sqrt{中文}".into(),
            }],
        )
        .unwrap();
    let updated = ParseSnapshot::with_dialect_and_math(
        edited.source.clone(),
        parse.dialect,
        parse.math_enabled,
    );
    assert!(updated
        .nodes
        .iter()
        .any(|n| n.kind == NodeKind::InlineMath("\\sqrt{中文}".into())));
    assert_eq!(
        updated.syntax.serialize(updated.source()).unwrap(),
        edited.source.text()
    );
    assert_eq!(
        edited.source.commit(edited.inverse).unwrap().source.text(),
        text
    );
    assert_eq!(
        parse
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::InlineMath(_)))
            .count(),
        2
    );
}
