use ryne_parser::{backend_html, partition, serialize, Dialect, HtmlPolicy, ParseSnapshot, Source};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Fixture {
    example: usize,
    markdown: String,
    html: String,
    section: String,
}

fn fixtures() -> Vec<Fixture> {
    serde_json::from_str(include_str!("fixtures/gfm-0.29.json")).unwrap()
}

#[test]
fn all_pinned_gfm_inputs_preserve_source_bytes() {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), 677);
    for (index, fixture) in fixtures.iter().enumerate() {
        assert_eq!(fixture.example, index + 1);
        let tokens = partition(&fixture.markdown);
        assert_eq!(serialize(&fixture.markdown, &tokens), fixture.markdown);
    }
}

#[test]
fn backend_output_matches_every_recorded_difference() {
    #[derive(Deserialize)]
    struct Difference {
        example: usize,
        section: String,
        actual: String,
    }
    let differences: Vec<Difference> =
        serde_json::from_str(include_str!("fixtures/backend-differences.json")).unwrap();
    let difference_count = differences.len();
    let known = differences
        .into_iter()
        .map(|d| (d.example, d))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(known.len(), difference_count, "duplicate difference IDs");
    assert!(known.keys().all(|&id| (1..=677).contains(&id)));
    for fixture in fixtures() {
        let actual = backend_html(&fixture.markdown);
        if let Some(difference) = known.get(&fixture.example) {
            assert_eq!(fixture.section, difference.section);
            assert_ne!(
                actual, fixture.html,
                "resolved difference {} needs review",
                fixture.example
            );
            assert_eq!(
                actual, difference.actual,
                "changed difference {}",
                fixture.example
            );
        } else {
            assert_eq!(actual, fixture.html, "new difference {}", fixture.example);
        }
    }
}

#[test]
fn owned_semantic_renderer_matches_fixed_gfm_examples() {
    let mut failures = Vec::new();
    for fixture in fixtures() {
        let dialect = if fixture.section == "6.8Autolinks" {
            Dialect::CommonMark029
        } else {
            Dialect::Gfm029
        };
        let parse = ParseSnapshot::with_dialect(Source::new(&fixture.markdown), dialect);
        assert_eq!(
            parse.syntax.serialize(parse.source()).unwrap(),
            fixture.markdown
        );
        for (index, token) in parse.syntax.tokens.iter().enumerate() {
            assert!(fixture.markdown.is_char_boundary(token.range.start));
            assert!(fixture.markdown.is_char_boundary(token.range.end));
            assert!(parse.syntax.nodes[token.owner]
                .owned_tokens
                .contains(&index));
            let owner = &parse.syntax.nodes[token.owner];
            assert!(
                owner.envelope.start <= token.range.start && token.range.end <= owner.envelope.end,
                "owner outside envelope in example {}: {token:?}",
                fixture.example
            );
        }
        let policy = if fixture.section.contains("Disallowed Raw HTML") {
            HtmlPolicy::Export
        } else {
            HtmlPolicy::Specification
        };
        let actual = parse.html(policy);
        if actual != fixture.html {
            failures.push(format!(
                "{}: {:?}\nexpected {:?}\nactual   {:?}",
                fixture.example, fixture.markdown, fixture.html, actual
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} differences:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
