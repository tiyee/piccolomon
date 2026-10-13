use editable_markdown_parser::*;

#[test]
fn markdown_copy_preserves_partial_strong_format() {
    let parse = ParseSnapshot::new(Source::new("**world**"));
    let projection = project(&parse, &Interaction::new(parse.source())).unwrap();
    let selection = Session::new(parse.source(), 2..7).unwrap();
    assert_eq!(copy_selection(&parse, &projection, &selection, CopyFormat::Markdown).unwrap(), "**world**");
}

#[test]
fn markdown_copy_preserves_link_destination() {
    let parse = ParseSnapshot::new(Source::new("[label](https://example.org)"));
    let projection = project(&parse, &Interaction::new(parse.source())).unwrap();
    let selection = Session::new(parse.source(), 1..6).unwrap();
    assert_eq!(copy_selection(&parse, &projection, &selection, CopyFormat::Markdown).unwrap(), "[label](https://example.org)");
}

#[test]
fn plain_copy_is_independent_of_reveal() {
    let parse = ParseSnapshot::new(Source::new("**world**"));
    let mut interaction = Interaction::new(parse.source());
    let compact = project(&parse, &interaction).unwrap();
    let selection = Session::new(parse.source(), 0..9).unwrap();
    let before = copy_selection(&parse, &compact, &selection, CopyFormat::PlainText).unwrap();
    interaction.enter(&compact, &compact.hit(2, Affinity::Downstream).unwrap()).unwrap();
    let expanded = project(&parse, &interaction).unwrap();
    let after = copy_selection(&parse, &expanded, &selection, CopyFormat::PlainText).unwrap();
    assert_eq!(after, before);
}

#[test]
fn nested_emphasis_reveals_inline_ancestors() {
    let parse = ParseSnapshot::new(Source::new("**a *b* c**"));
    let mut interaction = Interaction::new(parse.source());
    let compact = project(&parse, &interaction).unwrap();
    interaction.enter(&compact, &compact.hit(2, Affinity::Downstream).unwrap()).unwrap();
    assert_eq!(project(&parse, &interaction).unwrap().text, "**a *b* c**");
}

#[test]
fn strong_inside_link_reveals_entire_link() {
    let parse = ParseSnapshot::new(Source::new("[**x**](https://example.org)"));
    let mut interaction = Interaction::new(parse.source());
    let compact = project(&parse, &interaction).unwrap();
    interaction.enter(&compact, &compact.hit(0, Affinity::Downstream).unwrap()).unwrap();
    assert_eq!(project(&parse, &interaction).unwrap().text, "[**x**](https://example.org)");
}

#[test]
fn heading_content_focus_keeps_block_marker_hidden() {
    let parse = ParseSnapshot::new(Source::new("# title\n"));
    let mut interaction = Interaction::new(parse.source());
    let compact = project(&parse, &interaction).unwrap();
    interaction.enter(&compact, &compact.hit(1, Affinity::Downstream).unwrap()).unwrap();
    assert_eq!(project(&parse, &interaction).unwrap().text, "title");
}

#[test]
fn reference_index_preserves_nbsp_distinction() {
    let parse = ParseSnapshot::new(Source::new("[a b]: /one\n[a\u{a0}b]: /two\n\n[a b] [a\u{a0}b]\n"));
    assert_eq!(parse.references.len(), 2, "NBSP is not GFM 0.29 label whitespace: {:?}", parse.references);
}

#[test]
fn table_preview_preserves_cell_boundaries() {
    let parse = ParseSnapshot::new(Source::new("| a | b |\n| - | - |\n| c | d |\n"));
    let projection = project(&parse, &Interaction::new(parse.source())).unwrap();
    assert_ne!(projection.text, "ab\ncd", "adjacent cells have no display separator or block layout descriptor");
}

#[test]
fn extended_autolink_with_entity_keeps_complete_target() {
    assert_eq!(html("https://example.org/?a=1&amp;b=2"), "<p><a href=\"https://example.org/?a=1&amp;amp;b=2\">https://example.org/?a=1&amp;amp;b=2</a></p>\n");
}

#[test]
fn extended_autolink_precedes_emphasis_inside_url() {
    assert_eq!(html("https://example.org/a*b*c"), "<p><a href=\"https://example.org/a*b*c\">https://example.org/a*b*c</a></p>\n");
}

#[test]
fn extended_autolink_short_url_does_not_panic() {
    assert_eq!(html("www."), "<p>www.</p>\n");
    assert_eq!(html("https://"), "<p>https://</p>\n");
}
