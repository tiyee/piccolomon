use ryne_parser::*;
use std::ops::Range;
fn edit(range: Range<usize>, replacement: &str) -> Edit {
    Edit {
        range,
        replacement: replacement.into(),
    }
}
fn roundtrip(source: &Source, edits: Vec<Edit>) {
    let applied = source.apply(source.id(), edits).unwrap();
    for edit in &applied.inverse.edits {
        let s = applied.source.text();
        assert!(s.is_char_boundary(edit.range.start) && s.is_char_boundary(edit.range.end));
        for p in [edit.range.start, edit.range.end] {
            assert!(!(p > 0 && p < s.len() && &s.as_bytes()[p - 1..=p] == b"\r\n"));
        }
    }
    assert_eq!(
        applied
            .source
            .commit(applied.inverse)
            .unwrap()
            .source
            .text(),
        source.text()
    );
}
#[test]
fn inverse_absorbs_new_crlf_and_merges_neighboring_patches() {
    for (text, edits) in [
        ("\rX\n", vec![edit(1..2, "")]),
        ("\n", vec![edit(0..0, "\r")]),
        ("\r", vec![edit(1..1, "\n")]),
        ("aXb", vec![edit(0..1, "\r"), edit(2..3, "\n")]),
        (
            "\rX\nY\rZ\n",
            vec![edit(1..2, ""), edit(3..4, ""), edit(5..6, "")],
        ),
        ("ab", vec![edit(0..1, "\r"), edit(1..2, "\n")]),
    ] {
        roundtrip(&Source::new(text), edits);
    }
}
#[test]
fn randomized_multiple_patches_restore_exact_bytes() {
    let mut seed = 123u64;
    let values = ["", "\r", "\n", "\r\n", "中", "😀", "e\u{301}", "**", "\t"];
    for _ in 0..4000 {
        let mut text = String::new();
        for _ in 0..12 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            text.push_str(values[(seed >> 32) as usize % values.len()]);
        }
        let source = Source::new(&text);
        let positions = text
            .char_indices()
            .map(|(p, _)| p)
            .chain(std::iter::once(text.len()))
            .filter(|&p| !(p > 0 && p < text.len() && &text.as_bytes()[p - 1..=p] == b"\r\n"))
            .collect::<Vec<_>>();
        let mut edits = Vec::new();
        let mut i = 0;
        while i + 1 < positions.len() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            if seed.is_multiple_of(3) {
                edits.push(edit(
                    positions[i]..positions[i + usize::from(seed & 8 == 0)],
                    values[(seed >> 32) as usize % values.len()],
                ));
            }
            i += 2;
        }
        match source.apply(source.id(), edits.clone()) {
            Ok(_) => roundtrip(&source, edits),
            Err(Error::NoChange) => (),
            Err(e) => panic!("{text:?}: {e:?}"),
        }
    }
}
#[test]
fn snapshot_bound_objects_reject_other_documents_and_branches() {
    let a = Source::new("abc");
    let b = Source::new("abc");
    assert!(matches!(
        b.apply(a.id(), vec![edit(0..1, "x")]),
        Err(Error::StaleSnapshot)
    ));
    let composition = Composition::new(&a, 0..1).unwrap();
    assert!(matches!(
        composition.commit(&b, "x"),
        Err(Error::StaleSnapshot)
    ));
    let mut session = Session::new(&a, 0..1).unwrap();
    assert_eq!(session.visible(&b), Err(Error::StaleSnapshot));
    let first = a.apply(a.id(), vec![edit(0..1, "x")]).unwrap();
    let second = a.apply(a.id(), vec![edit(0..1, "y")]).unwrap();
    assert_ne!(first.source.id(), second.source.id());
    assert!(matches!(
        second.source.commit(first.inverse),
        Err(Error::StaleSnapshot)
    ));
    assert!(matches!(
        first
            .positions
            .map_cursor(&b, &first.source, 0, Affinity::Upstream),
        Err(Error::StaleSnapshot)
    ));
    session.transform(&first.positions).unwrap();
    assert_eq!(
        session.transform(&first.positions),
        Err(Error::StaleSnapshot)
    );
}
#[test]
fn mapped_cursor_repairs_crlf_while_byte_map_remains_exact() {
    let source = Source::new("\rX\n");
    let applied = source.apply(source.id(), vec![edit(1..2, "")]).unwrap();
    assert_eq!(
        applied
            .positions
            .map_bytes(source.id(), 1, Affinity::Downstream),
        Ok(1)
    );
    assert_eq!(
        applied
            .positions
            .map_cursor(&source, &applied.source, 1, Affinity::Upstream),
        Ok(0)
    );
    assert_eq!(
        applied
            .positions
            .map_cursor(&source, &applied.source, 1, Affinity::Downstream),
        Ok(2)
    );
    let mut session = Session::new(&source, 1..2).unwrap();
    session.transform(&applied.positions).unwrap();
    assert_eq!(session.visible(&applied.source), Ok("\r\n"));
}
#[test]
fn final_projection_validates_cross_node_graphemes() {
    let projection = preview_inline("*e*\u{301}😀\r\n中");
    assert_eq!(projection.validate_cursor(1), Err(Error::SplitGrapheme));
    assert_eq!(projection.validate_cursor(3), Ok(()));
    assert_eq!(projection.validate_cursor(4), Err(Error::InvalidRange));
}
#[test]
fn unicode_17_combining_mark_spans_projection_runs() {
    // U+1ACF is a combining mark introduced in Unicode 17.0.
    let parse = ParseSnapshot::new(Source::new("*e*\u{1acf}"));
    let projection = project(&parse, &Interaction::new(parse.source())).unwrap();
    assert_eq!(projection.text, "e\u{1acf}");
    assert_eq!(projection.validate_cursor(0), Ok(()));
    assert_eq!(projection.validate_cursor(1), Err(Error::SplitGrapheme));
    assert_eq!(projection.validate_cursor(4), Ok(()));
    for (affinity, expected) in [(Affinity::Upstream, 0), (Affinity::Downstream, 4)] {
        let hit = projection
            .source_to_projection(parse.source(), 2, affinity)
            .unwrap();
        assert_eq!(hit.projected, expected);
    }
    assert_eq!(
        parse.syntax.serialize(parse.source()).unwrap(),
        "*e*\u{1acf}"
    );
}
#[test]
fn quote_prefixes_belong_to_containers_not_strong() {
    let parse = ParseSnapshot::new(Source::new("> **a\r\n> b**\n"));
    let quote = parse
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::BlockQuote)
        .unwrap();
    let strong = parse
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Strong)
        .unwrap();
    for position in [0, 1, 7, 8] {
        let token = parse
            .syntax
            .tokens
            .iter()
            .find(|t| t.range.contains(&position))
            .unwrap();
        assert_eq!(token.owner, quote);
        assert_eq!(token.kind, TokenKind::ContainerPrefix);
    }
    assert!(parse.syntax.nodes[strong]
        .owned_tokens
        .iter()
        .all(|&t| parse.syntax.tokens[t].kind != TokenKind::ContainerPrefix));
    assert_eq!(
        parse.syntax.serialize(parse.source()).unwrap(),
        parse.source().text()
    );
}
#[test]
fn reference_definition_blank_lines_preserve_html_and_source_ranges() {
    for newline in ["\n", "\r\n"] {
        for blank in ["    ", "\t"] {
            let text = format!("[label]: /target{newline}{blank}{newline}中😀 [label]{newline}");
            let parse = ParseSnapshot::new(Source::new(&text));
            let expected = "<p>中😀 <a href=\"/target\">label</a></p>\n";
            assert_eq!(backend_html(&text), expected);
            assert_eq!(parse.html(HtmlPolicy::Export), expected);
            assert_eq!(parse.syntax.serialize(parse.source()).unwrap(), text);
            for node in &parse.nodes {
                assert!(node.envelope.start <= node.envelope.end);
                assert!(node.envelope.end <= text.len());
                assert!(text.is_char_boundary(node.envelope.start));
                assert!(text.is_char_boundary(node.envelope.end));
            }
            let mut end = 0;
            for token in &parse.syntax.tokens {
                assert_eq!(token.range.start, end);
                assert!(text.is_char_boundary(token.range.start));
                assert!(text.is_char_boundary(token.range.end));
                end = token.range.end;
            }
            assert_eq!(end, text.len());
        }
    }
}
#[test]
fn syntax_owner_indices_cover_unicode_and_nested_containers() {
    for source in [
        "> - **中\n>   😀**\r\n",
        "- [ ] a\n  - > b\n",
        "`a\n b`",
        "[a](url \"t\")",
        "\t x\n",
    ] {
        let parse = ParseSnapshot::new(Source::new(source));
        let mut end = 0;
        for (index, token) in parse.syntax.tokens.iter().enumerate() {
            assert_eq!(token.range.start, end);
            assert!(
                source.is_char_boundary(token.range.start)
                    && source.is_char_boundary(token.range.end)
            );
            assert!(parse.syntax.nodes[token.owner]
                .owned_tokens
                .contains(&index));
            end = token.range.end;
        }
        assert_eq!(end, source.len());
    }
}
#[test]
fn bidirectional_mapping_returns_hidden_reveal_and_rejects_stale_hits() {
    let parse = ParseSnapshot::new(Source::new("a **中** &fjlig;"));
    let interaction = Interaction::new(parse.source());
    let projection = project(&parse, &interaction).unwrap();
    let hidden = projection
        .source_to_projection(parse.source(), 2, Affinity::Downstream)
        .unwrap();
    assert_eq!(hidden.kind, HitKind::Hidden);
    assert_eq!(hidden.reveal, 2..9);
    for source in [0, 1, 4, 7, 9] {
        let hit = projection
            .source_to_projection(parse.source(), source, Affinity::Downstream)
            .unwrap();
        if hit.kind == HitKind::Direct {
            assert_eq!(hit.source, source);
        }
    }
    let rebuilt = project(&parse, &interaction).unwrap();
    assert_eq!(rebuilt.check_hit(&hidden), Err(Error::StaleProjection));
    let other = Source::new(parse.source().text());
    assert!(matches!(
        projection.source_to_projection(&other, 0, Affinity::Downstream),
        Err(Error::StaleSnapshot)
    ));
}
#[test]
fn source_session_and_selection_survive_strong_rule_strong() {
    let mut parse = ParseSnapshot::new(Source::new("**b**"));
    let mut interaction = Interaction::new(parse.source());
    let preview = project(&parse, &interaction).unwrap();
    let hit = preview.hit(0, Affinity::Downstream).unwrap();
    interaction.enter(&preview, &hit).unwrap();
    interaction.selection = Session::new(parse.source(), 2..3).unwrap();
    assert_eq!(project(&parse, &interaction).unwrap().text, "**b**");
    for (range, text, expected) in [(2..3, "", "****"), (2..2, "x", "**x**")] {
        let applied = parse
            .source()
            .apply(parse.source().id(), vec![edit(range, text)])
            .unwrap();
        interaction.transform(&applied.positions).unwrap();
        parse = ParseSnapshot::new(applied.source);
        assert_eq!(project(&parse, &interaction).unwrap().text, expected);
        assert!(interaction.selection.visible(parse.source()).is_ok());
    }
    interaction.leave();
    assert_eq!(project(&parse, &interaction).unwrap().text, "x");
}
#[test]
fn drag_freezes_reveal_and_composition_keeps_source_visible() {
    let parse = ParseSnapshot::new(Source::new("**b**"));
    let mut interaction = Interaction::new(parse.source());
    interaction
        .sessions
        .push(Session::new(parse.source(), 0..5).unwrap());
    interaction.begin_drag();
    interaction.leave();
    assert_eq!(project(&parse, &interaction).unwrap().text, "**b**");
    interaction.end_drag();
    assert_eq!(project(&parse, &interaction).unwrap().text, "b");
    interaction.composition = Some(Session::new(parse.source(), 0..5).unwrap());
    assert_eq!(project(&parse, &interaction).unwrap().text, "**b**");
}
fn execute_plan(parse: &ParseSnapshot, projection: &Projection, intent: EditIntent) -> Applied {
    match EditPlanner::plan(parse, projection, &Interaction::new(parse.source()), intent).unwrap() {
        EditPlan::Commit(tx) => parse.source().commit(tx).unwrap(),
        other => panic!("unexpected plan {other:?}"),
    }
}
#[test]
fn planner_handles_entity_partial_delete_without_manual_source_ranges() {
    let parse = ParseSnapshot::new(Source::new("&fjlig;"));
    let p = project(&parse, &Interaction::new(parse.source())).unwrap();
    let a = execute_plan(
        &parse,
        &p,
        EditIntent::Replace {
            start: p.hit(0, Affinity::Downstream).unwrap(),
            end: p.hit(1, Affinity::Upstream).unwrap(),
            text: String::new(),
        },
    );
    assert_eq!(preview_inline(a.source.text()).text, "j");
    assert_eq!(a.source.commit(a.inverse).unwrap().source.text(), "&fjlig;");
}
#[test]
fn planner_toggles_tasks_removes_format_and_escapes_tables() {
    for (source, expected, command) in [("- [ ] a", "- [x] a", 0), ("**b**", "b", 1)] {
        let parse = ParseSnapshot::new(Source::new(source));
        let p = project(&parse, &Interaction::new(parse.source())).unwrap();
        let at = p.hit(0, Affinity::Downstream).unwrap();
        let a = execute_plan(
            &parse,
            &p,
            if command == 0 {
                EditIntent::ToggleTask { at }
            } else {
                EditIntent::RemoveFormat { at }
            },
        );
        assert_eq!(a.source.text(), expected);
        assert_eq!(a.source.commit(a.inverse).unwrap().source.text(), source);
    }
    let parse = ParseSnapshot::new(Source::new("| a |\n| - |\n"));
    let p = project(&parse, &Interaction::new(parse.source())).unwrap();
    let a = execute_plan(
        &parse,
        &p,
        EditIntent::Replace {
            start: p.hit(0, Affinity::Downstream).unwrap(),
            end: p.hit(1, Affinity::Upstream).unwrap(),
            text: "x|y".into(),
        },
    );
    assert!(a.source.text().contains("x\\|y"));
    assert!(html(a.source.text()).contains("<th>x|y</th>"));
}
#[test]
fn planner_uses_final_graphemes_for_directional_delete() {
    for grapheme in ["e\u{301}", "e\u{1acf}", "👩‍💻", "🇨🇳", "中"] {
        for (intent_forward, caret) in [(true, 0), (false, grapheme.len())] {
            let parse = ParseSnapshot::new(Source::new(&format!("{grapheme}b")));
            let p = project(&parse, &Interaction::new(parse.source())).unwrap();
            let at = p.hit(caret, Affinity::Downstream).unwrap();
            let a = execute_plan(
                &parse,
                &p,
                if intent_forward {
                    EditIntent::DeleteForward { at }
                } else {
                    EditIntent::DeleteBackward { at }
                },
            );
            assert_eq!(a.source.text(), "b");
            assert_eq!(
                a.source.commit(a.inverse).unwrap().source.text(),
                parse.source().text()
            );
        }
    }
}
#[test]
fn production_html_filters_tags_without_modifying_source() {
    let source = "<SCRIPT>x</SCRIPT>\n\n<scripture>ok</scripture>\n";
    let parse = ParseSnapshot::new(Source::new(source));
    assert!(parse.html(HtmlPolicy::Export).contains("&lt;SCRIPT>"));
    assert!(parse.html(HtmlPolicy::Export).contains("<scripture>"));
    assert!(parse.html(HtmlPolicy::Specification).contains("<SCRIPT>"));
    assert_eq!(parse.source().text(), source);
}

#[test]
fn hidden_markers_map_to_final_grapheme_edges_and_source_eof() {
    let parse = ParseSnapshot::new(Source::new("*e*\u{301}"));
    let p = project(&parse, &Interaction::new(parse.source())).unwrap();
    for (affinity, expected) in [(Affinity::Upstream, 0), (Affinity::Downstream, 3)] {
        let hit = p.source_to_projection(parse.source(), 2, affinity).unwrap();
        assert_eq!(hit.projected, expected);
        assert_eq!(p.check_hit(&hit), Ok(()));
    }
    let parse = ParseSnapshot::new(Source::new("**x**"));
    let p = project(&parse, &Interaction::new(parse.source())).unwrap();
    let hit = p
        .source_to_projection(parse.source(), 5, Affinity::Downstream)
        .unwrap();
    assert_eq!(hit.source, 5);
    assert_eq!(hit.projected, 1);
    assert_eq!(hit.kind, HitKind::Hidden);
}
#[test]
fn parse_reference_dependencies_include_unresolved_users_and_definition_origins() {
    let parse = ParseSnapshot::new(Source::new("[entry][key]\n"));
    assert!(parse.references["KEY"].definition.is_none());
    assert!(!parse.references["KEY"].users.is_empty());
    let next = parse
        .source()
        .apply(parse.source().id(), vec![edit(13..13, "\n[key]: /url\n")])
        .unwrap();
    let resolved = ParseSnapshot::new(next.source);
    assert!(!resolved.references["KEY"].users.is_empty());
    let definition = resolved.references["KEY"].definition.clone().unwrap();
    assert!(resolved
        .fields
        .iter()
        .any(|f| f.field == FieldKind::Destination && f.sources.contains(&definition)));
    assert_eq!(
        parse.syntax.serialize(resolved.source()),
        Err(Error::StaleSnapshot)
    );
}
#[test]
fn generated_objects_and_multiple_sources_retain_explicit_origins() {
    let parse = ParseSnapshot::new(Source::new("a\n\nb\n\n![alt](/image)\n\n> `a\n> b`\n"));
    let p = project(&parse, &Interaction::new(parse.source())).unwrap();
    assert!(p
        .runs
        .iter()
        .any(|r| matches!(r.origin, Origin::Generated { .. })));
    assert!(p
        .runs
        .iter()
        .any(|r| matches!(r.origin, Origin::Object { .. })));
    assert!(p
        .runs
        .iter()
        .any(|r| matches!(r.origin, Origin::MultiSource(_))));
    let code = parse
        .nodes
        .iter()
        .position(|n| matches!(n.kind, NodeKind::Code(_)))
        .unwrap();
    for range in &parse.syntax.nodes[code].content {
        assert!(!parse.source().text()[range.clone()].contains('>'));
    }
}
#[test]
fn deleting_preview_format_content_does_not_create_a_rule() {
    let parse = ParseSnapshot::new(Source::new("**b**"));
    let p = project(&parse, &Interaction::new(parse.source())).unwrap();
    let a = execute_plan(
        &parse,
        &p,
        EditIntent::DeleteForward {
            at: p.hit(0, Affinity::Downstream).unwrap(),
        },
    );
    assert_eq!(a.source.text(), "");
    assert_eq!(a.source.commit(a.inverse).unwrap().source.text(), "**b**");
}
#[test]
fn planner_reports_no_change_reveal_confirmation_and_unsupported() {
    let parse = ParseSnapshot::new(Source::new("ab"));
    let p = project(&parse, &Interaction::new(parse.source())).unwrap();
    let at = p.hit(0, Affinity::Downstream).unwrap();
    assert!(matches!(
        EditPlanner::plan(
            &parse,
            &p,
            &Interaction::new(parse.source()),
            EditIntent::DeleteBackward { at: at.clone() }
        )
        .unwrap(),
        EditPlan::NoChange
    ));
    assert!(matches!(
        EditPlanner::plan(
            &parse,
            &p,
            &Interaction::new(parse.source()),
            EditIntent::ToggleTask { at }
        )
        .unwrap(),
        EditPlan::Unsupported { .. }
    ));
    let parse = ParseSnapshot::new(Source::new("`x`"));
    let p = project(&parse, &Interaction::new(parse.source())).unwrap();
    assert!(matches!(
        EditPlanner::plan(
            &parse,
            &p,
            &Interaction::new(parse.source()),
            EditIntent::DeleteForward {
                at: p.hit(0, Affinity::Downstream).unwrap()
            }
        )
        .unwrap(),
        EditPlan::RevealThenRetry { .. }
    ));
    let parse = ParseSnapshot::new(Source::new("a **b** c"));
    let p = project(&parse, &Interaction::new(parse.source())).unwrap();
    assert!(matches!(
        EditPlanner::plan(
            &parse,
            &p,
            &Interaction::new(parse.source()),
            EditIntent::Replace {
                start: p.hit(0, Affinity::Downstream).unwrap(),
                end: p.hit(p.text.len(), Affinity::Upstream).unwrap(),
                text: "x".into()
            }
        )
        .unwrap(),
        EditPlan::PendingConfirmation { .. }
    ));
}

#[test]
fn planner_accepts_partial_entity_edits_between_direct_runs_and_rejects_old_hits() {
    let parse = ParseSnapshot::new(Source::new("a &fjlig; b"));
    let p = project(&parse, &Interaction::new(parse.source())).unwrap();
    let intent = EditIntent::Replace {
        start: p.hit(2, Affinity::Downstream).unwrap(),
        end: p.hit(3, Affinity::Upstream).unwrap(),
        text: String::new(),
    };
    let a = execute_plan(&parse, &p, intent.clone());
    assert_eq!(a.source.text(), "a j b");
    let newer = project(&parse, &Interaction::new(parse.source())).unwrap();
    assert!(matches!(
        EditPlanner::plan(&parse, &newer, &Interaction::new(parse.source()), intent),
        Err(Error::StaleProjection)
    ));
}
#[test]
fn invalid_session_transform_is_atomic_and_copy_defaults_to_markdown() {
    let source = Source::new("**中**");
    let parse = ParseSnapshot::new(source.clone());
    let projection = project(&parse, &Interaction::new(&source)).unwrap();
    let selection = Session::new(&source, 0..source.text().len()).unwrap();
    assert_eq!(
        copy_selection(&parse, &projection, &selection, CopyFormat::default()).unwrap(),
        "**中**"
    );
    assert_eq!(
        copy_selection(&parse, &projection, &selection, CopyFormat::PlainText).unwrap(),
        "中"
    );
    let applied = source.apply(source.id(), vec![edit(2..5, "x")]).unwrap();
    let mut invalid = Session {
        base: source.id(),
        range: 3..4,
    };
    assert_eq!(
        invalid.transform(&applied.positions),
        Err(Error::InvalidRange)
    );
    assert_eq!(invalid.base, source.id());
    assert_eq!(invalid.range, 3..4);
}

#[test]
fn multiline_inline_reveal_excludes_container_prefix_and_offers_block_fallback() {
    let parse = ParseSnapshot::new(Source::new("> **a\n> b**\n"));
    let mut interaction = Interaction::new(parse.source());
    let preview = project(&parse, &interaction).unwrap();
    interaction
        .enter(&preview, &preview.hit(0, Affinity::Downstream).unwrap())
        .unwrap();
    let expanded = project(&parse, &interaction).unwrap();
    assert_eq!(expanded.text, "**a\nb**");
    let plan = EditPlanner::plan(
        &parse,
        &expanded,
        &interaction,
        EditIntent::Replace {
            start: expanded.hit(0, Affinity::Downstream).unwrap(),
            end: expanded
                .hit(expanded.text.len(), Affinity::Upstream)
                .unwrap(),
            text: "x".into(),
        },
    )
    .unwrap();
    let EditPlan::RevealThenRetry { window } = plan else {
        panic!("expected container fallback")
    };
    interaction
        .sessions
        .push(Session::new(parse.source(), window).unwrap());
    assert_eq!(
        project(&parse, &interaction).unwrap().text,
        "> **a\n> b**\n"
    );
}
#[test]
fn html_block_comment_rules_differ_from_inline_comments() {
    assert_eq!(html("<!-- foo--bar -->\n"), "<!-- foo--bar -->\n");
    assert_eq!(
        html("x <!-- foo--bar -->\n"),
        "<p>x &lt;!-- foo--bar --&gt;</p>\n"
    );
}

#[test]
fn deletion_at_display_endpoints_does_not_rewrite_entities() {
    let parse = ParseSnapshot::new(Source::new("&fjlig;"));
    let interaction = Interaction::new(parse.source());
    let p = project(&parse, &interaction).unwrap();
    for intent in [
        EditIntent::DeleteBackward {
            at: p.hit(0, Affinity::Downstream).unwrap(),
        },
        EditIntent::DeleteForward {
            at: p.hit(2, Affinity::Downstream).unwrap(),
        },
    ] {
        assert!(matches!(
            EditPlanner::plan(&parse, &p, &interaction, intent).unwrap(),
            EditPlan::NoChange
        ));
    }
}
