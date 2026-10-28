// The runner invokes the current helper with opt-in semantic query metrics.
pub fn audit() {
    for count in [100usize, 200, 400, 800] {
        let raw = "```rust\nlet value = 42;\n```\n\n".repeat(count);
        let state = EditingState::new(editable_markdown_parser::Source::new(&raw));
        let blocks = code_block::blocks(&state.parse);
        let before = ryne_syntax::query_metrics::snapshot();
        for block in &blocks {
            assert!(language_range(&state, block).is_some());
        }
        let visits = ryne_syntax::query_metrics::snapshot().candidates - before.candidates;
        assert_eq!(visits, count);
        println!(
            "code_language_controls blocks={count} ast_nodes={} code_candidates={visits}",
            state.parse.nodes.len()
        );
    }
}
