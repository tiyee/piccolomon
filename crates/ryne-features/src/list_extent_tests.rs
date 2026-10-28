use super::*;
use editable_markdown_parser::Source;

// Previous implementation remains an independent oracle for blank-line ownership.
fn scan(state: &EditingState, item: &list::Item) -> Range<usize> {
    let lines = code_block::lines(state.source.text());
    let code = code_block::blocks(&state.parse);
    let text = state.source.text();
    let boundary = item.range.end.max(item.body);
    let index = lines.partition_point(|line| line.end < boundary);
    let mut end = lines
        .get(index)
        .filter(|line| line.start < boundary && boundary <= line.end)
        .map_or(boundary, |line| line.end);
    let first = lines.partition_point(|line| line.start <= item.range.start);
    let last = lines.partition_point(|line| line.start < end);
    for line in lines[first.min(last)..last].iter().rev() {
        if code.iter().any(|block| block.range.contains(&line.start)) {
            break;
        }
        let start = list::quote_end(&state.parse, line);
        if !text[start..line.end].trim().is_empty() {
            break;
        }
        end = line.start;
    }
    item.range.start..end
}

#[test]
fn indexed_extents_match_scan_and_borrow_snapshot_directories() {
    for newline in ["\n", "\r\n", "\r"] {
        for raw in [
            "- one\n  continuation\n\n- two\n\n\nparagraph\n",
            "9. parent\n   - [ ] nested\n\n     ```rust\n     x\n\n     ```\n\n10. next\n",
            "> - quote\n>   continuation\n>\n>   ~~~text\n>   > literal\n>\n>   ~~~\n>\n> - next\n",
            "- parent\n\n      indented\n\n      code\n\n- next\n",
            "- ```\n  empty fence below\n  ```\n\n- \n",
            "- item\n  ```\n  unclosed\n\n",
        ] {
            let raw = raw.replace('\n', newline);
            let state = EditingState::new(Source::new(&raw));
            let context = Extents::new(&state);
            assert!(std::ptr::eq(
                context.lines,
                state.parse.semantic().index().lines.as_slice()
            ));
            assert!(std::ptr::eq(context.code, code_block::cached(&state.parse)));
            for item in list::cached_items(&state.parse) {
                assert_eq!(
                    context.get(item),
                    scan(&state, item),
                    "{raw:?} item {}",
                    item.id
                );
            }
        }
    }
}

#[test]
fn mixed_lists_and_fences_have_logarithmic_code_queries() {
    for count in [100usize, 200, 400, 800] {
        let raw = "- item\n  continuation\n\n".repeat(count)
            + &"```rust\nlet x = 1;\n```\n\n".repeat(count);
        let state = EditingState::new(Source::new(&raw));
        let context = Extents::new(&state);
        let items = list::cached_items(&state.parse);
        assert_eq!(items.len(), count);
        assert_eq!(context.code.len(), count);
        for item in items {
            assert_eq!(context.get(item), scan(&state, item));
        }
        let comparisons = context.code_comparisons.get();
        assert!(comparisons <= 2 * count * (count.ilog2() as usize + 2));
        println!("list_extent items={count} code_blocks={count} comparisons={comparisons}");
    }
}
