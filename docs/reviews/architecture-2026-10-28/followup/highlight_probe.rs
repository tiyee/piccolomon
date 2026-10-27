// Appended to an isolated copy of highlight.rs; private state is instrumented here.
pub fn audit() {
    use editable_markdown_parser::Source;
    use ryne_extensions::SyntaxHighlighter;
    use ryne_syntax::ParseSnapshot;

    let raw = "```\nfn main() { let value = 42; }\n```\n";
    let parse = ParseSnapshot::with_math(Source::new(raw));
    let blocks = ryne_syntax::code_block::blocks(&parse);
    let mut highlighter = CodeHighlighter::default();
    highlighter.begin_frame();
    // Simulate the last block receiving a positive but exhausted frame budget.
    highlighter.detection_remaining = Duration::from_nanos(1);
    let first = highlighter.spans(raw, &blocks[0]);
    highlighter.end_frame();
    assert!(first.is_empty());
    assert!(!highlighter.has_pending_work());
    let attempts = highlighter.detection_attempts;
    highlighter.begin_frame();
    let second = highlighter.spans(raw, &blocks[0]);
    highlighter.end_frame();
    assert!(second.is_empty());
    assert_eq!(highlighter.detection_attempts, 0);
    assert!(!highlighter.has_pending_work());
    let mut fresh = CodeHighlighter::default();
    fresh.begin_frame();
    let successful = fresh.spans(raw, &blocks[0]);
    fresh.end_frame();
    assert!(!successful.is_empty());
    println!(
        "detection_timeout first_attempts={attempts} pending=false next_frame_attempts=0 next_frame_spans={} fresh_spans={}",
        second.len(),
        successful.len()
    );

    for count in [64usize, 80, 128] {
        let raw = (0..count)
            .map(|i| format!("```rust\nfn item_{i}() {{ let value = {i}; }}\n```\n\n"))
            .collect::<String>();
        let parse = ParseSnapshot::with_math(Source::new(&raw));
        let blocks = ryne_syntax::code_block::blocks(&parse);
        let mut highlighter = CodeHighlighter::default();
        highlighter.begin_frame();
        for block in &blocks {
            assert!(!highlighter.spans(&raw, block).is_empty());
        }
        highlighter.end_frame();
        let cold = highlighter.parses;
        highlighter.begin_frame();
        for block in &blocks {
            assert!(!highlighter.spans(&raw, block).is_empty());
        }
        highlighter.end_frame();
        let repeated = highlighter.parses - cold;
        assert_eq!(cold, count);
        assert_eq!(repeated, count.saturating_sub(64));
        println!(
            "highlight_working_set blocks={count} cached={} cold_parses={cold} warm_parses={repeated} pending={}",
            highlighter.cache_entries(),
            highlighter.has_pending_work()
        );
    }
}
