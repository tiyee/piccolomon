use super::*;
use ryne_parser::Edit;

fn parse(text: &str) -> ParseSnapshot {
    ParseSnapshot::with_math(Source::new(text))
}
fn update(old: &BlockDirectory, range: Range<usize>, text: &str) -> BlockDirectory {
    let applied = old
        .source
        .apply(
            old.snapshot(),
            vec![Edit {
                range,
                replacement: text.into(),
            }],
        )
        .unwrap();
    old.updated(
        &ParseSnapshot::with_math(applied.source),
        &applied.positions,
    )
    .unwrap()
}
fn paragraphs(dir: &BlockDirectory) -> Vec<BlockId> {
    dir.entries
        .iter()
        .filter(|e| e.role == BlockRole::Paragraph)
        .map(|e| e.id)
        .collect()
}
#[test]
fn duplicate_insertion_does_not_steal_identity_and_internal_split_merge_keeps_first() {
    let old = BlockDirectory::new(&parse("same\n\nsame\n\nlast"));
    let ids = paragraphs(&old);
    let inserted = update(&old, 0..0, "same\n\n");
    assert_ne!(paragraphs(&inserted)[0], ids[0]);
    assert_eq!(paragraphs(&inserted)[1..], ids);
    let split = update(&old, 2..2, "\n\n");
    assert_eq!(paragraphs(&split)[0], ids[0]);
    assert_ne!(paragraphs(&split)[1], ids[0]);
    assert_eq!(paragraphs(&split)[2..], ids[1..]);
    let merged = update(&split, 2..4, "");
    assert_eq!(paragraphs(&merged), ids);
    let prefix = update(&old, 0..0, "prefix ");
    assert_eq!(paragraphs(&prefix), ids);
    assert_eq!(prefix.entries[0].content_revision, 1);
    assert_eq!(prefix.entries[1].content_revision, 0);
    let leading = update(&old, 0..1, "S");
    assert_eq!(paragraphs(&leading), ids);
    let replaced = update(&old, 0..4, "same again");
    assert_ne!(paragraphs(&replaced)[0], ids[0]);
}
#[test]
fn deleted_and_pasted_blocks_get_fresh_ids_and_reopening_resets_identity() {
    let old = BlockDirectory::new(&parse("one\n\ntwo\n\nthree"));
    let removed = update(&old, 0..5, "");
    let pasted = update(
        &removed,
        removed.source.text().len()..removed.source.text().len(),
        "\n\none",
    );
    assert_eq!(paragraphs(&pasted)[..2], paragraphs(&old)[1..]);
    assert!(!paragraphs(&old).contains(paragraphs(&pasted).last().unwrap()));
    let reopened = BlockDirectory::new(&parse(old.source.text()));
    assert!(reopened
        .entries
        .iter()
        .all(|e| !old.ids.contains_key(&e.id)));
    assert_eq!(
        old.entries(reopened.snapshot()).err(),
        Some(Error::StaleSnapshot)
    );
}
#[test]
fn raw_partition_nested_ranges_empty_cells_and_affinities_match_scan() {
    for newline in ["\n", "\r\n", "\r"] {
        let text = format!("{}\n\nunsupported", "> - 中😀\n>   continuation\n>\n>   ```rust\n>   **literal**\n>   ```\n\n| A | B |\n| --- | --- |\n| | é |\n").replace('\n', newline);
        let parse = parse(&text);
        let dir = BlockDirectory::new(&parse);
        assert_eq!(dir.snapshot(), parse.source().id());
        let joined: String = dir.pieces.iter().map(|p| &text[p.range.clone()]).collect();
        assert_eq!(joined, text);
        for piece in &dir.pieces {
            for p in [piece.range.start, piece.range.end] {
                assert!(text.is_char_boundary(p));
                assert!(
                    !(p > 0
                        && text.as_bytes().get(p - 1) == Some(&b'\r')
                        && text.as_bytes().get(p) == Some(&b'\n'))
                );
            }
        }
        for entry in &dir.entries {
            assert_eq!(
                dir.get(dir.snapshot(), entry.id).unwrap().unwrap().range,
                entry.range
            );
            if let Some(parent) = entry.parent {
                let parent = &dir.entries[dir.ids[&parent]];
                assert!(
                    parent.range.start <= entry.range.start && entry.range.end <= parent.range.end
                );
                assert_eq!(parent.depth + 1, entry.depth);
            }
        }
        for p in (0..=text.len()).filter(|&p| text.is_char_boundary(p)) {
            for affinity in [Affinity::Upstream, Affinity::Downstream] {
                let expected: Vec<_> = dir
                    .entries
                    .iter()
                    .filter(|e| {
                        (e.range.is_empty() && e.range.start == p)
                            || match affinity {
                                Affinity::Upstream => e.range.start < p && p <= e.range.end,
                                Affinity::Downstream => e.range.start <= p && p < e.range.end,
                            }
                    })
                    .map(|e| e.id)
                    .collect();
                assert_eq!(
                    dir.at(dir.snapshot(), p, affinity)
                        .unwrap()
                        .iter()
                        .map(|e| e.id)
                        .collect::<Vec<_>>(),
                    expected
                );
            }
        }
    }
    let empty = BlockDirectory::new(&parse(""));
    assert!(empty.entries.is_empty() && empty.pieces.is_empty());
}
#[test]
fn random_transactions_match_fresh_semantics_and_preserve_raw_partition() {
    const SEED: u64 = 0x14b10c7;
    let mut seed = SEED;
    let mut dir = BlockDirectory::new(&parse(
        "> - body\r\n>   **bold**\r\n\r\n```\r\ntext\r\n```\r\n\r\nlast😀",
    ));
    let mut trace = Vec::new();
    for step in 0..160 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let text = dir.source.text();
        let boundaries: Vec<_> = (0..=text.len())
            .filter(|&p| {
                text.is_char_boundary(p)
                    && !(p > 0
                        && text.as_bytes().get(p - 1) == Some(&b'\r')
                        && text.as_bytes().get(p) == Some(&b'\n'))
            })
            .collect();
        let start = boundaries[seed as usize % boundaries.len()];
        let replacement = ["x", "\r\n", "中", "**", "\n\n", "- "][step % 6];
        trace.push((start, replacement));
        dir = update(&dir, start..start, replacement);
        let fresh = BlockDirectory::new(&ParseSnapshot::with_math(dir.source.clone()));
        assert_eq!(
            dir.entries
                .iter()
                .map(|e| (e.role, &e.range, e.depth))
                .collect::<Vec<_>>(),
            fresh
                .entries
                .iter()
                .map(|e| (e.role, &e.range, e.depth))
                .collect::<Vec<_>>(),
            "seed={SEED:x} trace={trace:?}"
        );
        assert_eq!(dir.pieces, fresh.pieces);
        assert_eq!(dir.ids.len(), dir.entries.len());
    }
}
