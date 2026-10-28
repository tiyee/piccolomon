use super::*;
use editable_markdown_parser::Source;

// Previous full scan kept as an independent behavioral oracle.
fn scan(parse: &ParseSnapshot, block: &CodeBlock) -> Option<Range<usize>> {
    if block
        .rows
        .first()
        .is_some_and(|r| r.line.start == block.range.start)
    {
        return None;
    }
    let semantic = parse.semantic();
    let node = semantic.nodes.iter().find(|node| {
        matches!(node.kind, NodeKind::CodeBlock(_)) && block.range.contains(&node.envelope.start)
    })?;
    let text = parse.source().text();
    let start = node.envelope.start;
    let line = &text[start..];
    let marker = *line.as_bytes().first()?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }
    let width = line.bytes().take_while(|b| *b == marker).count();
    if width < 3 {
        return None;
    }
    let end = start + line.find(['\r', '\n']).unwrap_or(line.len());
    let rest = &text[start + width..end];
    let token = start + width + rest.len() - rest.trim_start_matches([' ', '\t']).len();
    let len = text[token..end]
        .find(char::is_whitespace)
        .unwrap_or(end - token);
    Some(token..token + len)
}

#[test]
fn language_tokens_match_scan_with_containers_newlines_and_drafts() {
    let raw = concat!(
        "```rust extra metadata\n中😀\n```\n\n",
        "> - ~~~~\tpython extra\n>   body\n>   ~~~~\n\n",
        "  ```\n```\n\n",
        "    ```literal\n    body\n\n",
        "~~~typescript\nunfinished\n",
    );
    for newline in ["\n", "\r\n", "\r"] {
        for raw in [
            raw.to_owned(),
            format!("prefix😀\n\n{raw}"),
            "```rust".into(),
        ] {
            let raw = raw.replace('\n', newline);
            let parse = ParseSnapshot::with_math(Source::new(&raw));
            for block in cached(&parse) {
                let found = language_range(&parse, block);
                assert_eq!(found, scan(&parse, block), "{newline:?} {block:?}");
                if let Some(range) = found {
                    assert!(raw.get(range).is_some());
                }
            }
            assert_eq!(parse.source().text(), raw);
        }
    }
}

#[test]
fn many_fences_visit_one_code_candidate_each() {
    for count in [100, 200, 400, 800] {
        let raw = "```rust\nfn main() {}\n```\n\n".repeat(count);
        let parse = ParseSnapshot::with_math(Source::new(&raw));
        let blocks = cached(&parse);
        assert_eq!(blocks.len(), count);
        let before = crate::query_metrics::snapshot();
        for block in blocks {
            assert_eq!(language_range(&parse, block), scan(&parse, block));
        }
        assert_eq!(
            crate::query_metrics::snapshot().candidates - before.candidates,
            count
        );
    }
}
