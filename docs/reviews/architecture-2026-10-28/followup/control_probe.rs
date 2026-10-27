// The runner extracts the current language_range helper and counts predicate visits.
pub fn audit() {
    use std::sync::atomic::Ordering;
    for count in [100usize, 200, 400, 800] {
        let raw = "```rust\nlet value = 42;\n```\n\n".repeat(count);
        let state = EditingState::new(editable_markdown_parser::Source::new(&raw));
        let blocks = code_block::blocks(&state.parse);
        AUDIT_VISITS.store(0, Ordering::Relaxed);
        for block in &blocks {
            assert!(language_range(&state, block).is_some());
        }
        let visits = AUDIT_VISITS.load(Ordering::Relaxed);
        assert!(visits >= count * (count + 1) / 2);
        println!(
            "code_language_controls blocks={count} ast_nodes={} predicate_visits={visits}",
            state.parse.nodes.len()
        );
    }
}
