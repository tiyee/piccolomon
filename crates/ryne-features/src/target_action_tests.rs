use super::*;
use editable_markdown_parser::Source;

fn compare(provider: &dyn RenderActionProvider, raw: &str, max: usize) {
    let state = EditingState::new(Source::new(raw));
    let full = provider.controls(&state).unwrap();
    assert!(!full.is_empty());
    for control in &full {
        let target = &control.target;
        let expected = full
            .iter()
            .filter(|c| c.target == *target)
            .collect::<Vec<_>>();
        let actual = provider
            .controls_for_target(&state, target)
            .unwrap()
            .unwrap();
        assert!(actual.len() <= max);
        assert_eq!(
            format!("{expected:?}"),
            format!("{:?}", actual.iter().collect::<Vec<_>>())
        );
        let mut wrong = target.clone();
        wrong.part = vec![usize::MAX];
        assert!(provider
            .controls_for_target(&state, &wrong)
            .unwrap()
            .unwrap()
            .is_empty());
    }
}
#[test]
fn target_controls_match_full_actions_for_containers_and_newlines() {
    let formulas = Arc::new(ryne_services::math::FormularyRenderer::default());
    for newline in ["\n", "\r\n", "\r"] {
        compare(
            &table_actions::TableActions {
                formulas: formulas.clone(),
            },
            &"> | a | b |\n> | :--- | ---: |\n> | 中 | |\n> | x | y |\n".replace('\n', newline),
            1,
        );
        compare(&list_actions::ListActions { formulas: formulas.clone() },
            &"- [ ] first\n  - nested\n\n    ```rust\n    let x = 1;\n    ```\n- [x] second\n- last\n".replace('\n', newline), 2);
    }
}
#[test]
fn large_table_target_builds_one_control() {
    let raw = "| a | b | c |\n| --- | --- | --- |\n".to_owned() + &"| x | y | z |\n".repeat(800);
    let state = EditingState::new(Source::new(&raw));
    let provider = table_actions::TableActions {
        formulas: Arc::new(ryne_services::math::FormularyRenderer::default()),
    };
    let table = &ryne_syntax::table::cached(&state.parse)[0];
    let target = RenderTarget {
        document: state.source.id(),
        role: "table.cell",
        owner: table.range.clone(),
        part: vec![800, 2],
    };
    let controls = provider
        .controls_for_target(&state, &target)
        .unwrap()
        .unwrap();
    assert_eq!(controls.len(), 1);
    assert_eq!(controls[0].actions.len(), 13);
    assert_eq!(controls[0].target, target);
}
