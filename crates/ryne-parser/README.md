# ryne-parser

[简体中文](README.zh-CN.md)

Source-preserving Markdown parsing and editing primitives for hybrid editors. Source and projection coordinates are UTF-8 byte offsets.

## Workspace dependency

Add the parser to a Ryne workspace member’s manifest:

```toml
[dependencies]
ryne-parser.workspace = true
```

## Edit and restore source

```rust
use ryne_parser::{Affinity, Edit, Source};

let source = Source::new("**world**");
let applied = source.apply(source.id(), vec![Edit {
    range: 2..7,
    replacement: "Rust".into(),
}]).unwrap();
assert_eq!(applied.source.text(), "**Rust**");
assert_eq!(applied.positions.map_bytes(source.id(), 7, Affinity::Downstream), Ok(6));
let restored = applied.source.commit(applied.inverse).unwrap();
assert_eq!(restored.source.text(), source.text());
```

`SnapshotId` combines document identity and revision. `Source::apply()` validates all ranges against one immutable snapshot before changing anything. It rejects foreign/stale snapshots, invalid UTF-8 or CRLF endpoints, overlaps, and ambiguous insertions. Adjacent nonempty replacements are allowed; merge same-position and replacement-boundary insertions first. Byte-identical edits return `Error::NoChange`.

Inverse transactions bind to the resulting snapshot. Their ranges absorb newly formed CRLF pairs and merge when necessary, restoring the exact original bytes. Revisions are unique within a document, including branches from older snapshots. Identities are process-local, not persistent file IDs.

`PositionMap::map_bytes()` preserves affinity and exact byte mapping, including positions inside newly formed CRLF. Use `map_cursor()` for editable source positions: it validates both snapshots and repairs CRLF positions according to affinity. Final display graphemes are validated by `Projection::validate_cursor()` across all runs.

## Parse, reveal and plan edits

```rust
use ryne_parser::{project, Affinity, EditIntent, EditPlan, EditPlanner,
    Interaction, ParseSnapshot, Source};

let parse = ParseSnapshot::new(Source::new("&fjlig;"));
let interaction = Interaction::new(parse.source());
let projection = project(&parse, &interaction).unwrap();
assert_eq!(projection.text, "fj");
let intent = EditIntent::Replace {
    start: projection.hit(0, Affinity::Downstream).unwrap(),
    end: projection.hit(1, Affinity::Upstream).unwrap(),
    text: String::new(),
};
let EditPlan::Commit(transaction) = EditPlanner::plan(
    &parse, &projection, &interaction, intent,
).unwrap() else { panic!("expected an entity edit") };
let applied = parse.source().commit(transaction).unwrap();
assert_eq!(applied.source.text(), "j");
```

`ParseSnapshot` owns its source, dialect, backend-independent semantic tree, typed lossless syntax, field provenance, and resolved/unresolved reference index. Syntax tokens cover every source byte once and have one owner. Marker roles and container prefixes are explicit; multiline Strong content excludes quote prefixes. `LosslessSyntax::serialize()` checks snapshot identity. `partition()` remains a raw range utility, not a typed syntax tree.

`project(parse, interaction)` emits raw source for active windows and semantic preview elsewhere. Mapping origins distinguish direct text, atomic transforms, multiple source fragments, generated separators, hidden syntax, and objects. Versioned hits retain provenance and reveal targets; `source_to_projection()` provides the other lookup direction. `Interaction::enter()` reveals a hit; `leave()` folds it. Transform interaction through an applied position map before reparsing. Dragging can freeze reveal windows; a composition window stays revealed.

The planner handles source-window input, preview text replacement and grapheme deletion, partial entity edits, format removal, task toggles, and table pipe escaping. It returns `Commit`, `RevealThenRetry`, `PendingConfirmation`, `Unsupported`, or `NoChange`. Complex blocks, objects, code-span encoders, and edits across unsupported provenance use explicit fallbacks. This is a minimal editing loop, not the complete v4 command set.

## HTML and capability limits

`html()` renders the owned GFM semantic model with tagfilter. `backend_html()` retains raw pulldown-cmark output for diagnostics. The pinned backend still matches 629/677 examples; each of its 48 differences is locked individually. The owned renderer matches all 677 fixed examples using their documented section profiles: base autolinks use `Dialect::CommonMark029`, extensions use `Gfm029`, and disallowed HTML uses `HtmlPolicy::Export`. `HtmlPolicy::Specification` retains raw HTML. Tagfilter is not a general HTML sanitizer.

Passing this corpus does not establish complete GFM compatibility for arbitrary input. Marker extraction and field provenance are conservative in complex blocks; destination/title origins can cover an entire definition or link. There is no styled block layout, bidi engine, incremental parser, storage optimization, or real GUI integration.

`Composition` isolates preedit from source; update/cancel do not change bytes or revisions, and commit checks identity. `utf16_to_utf8()` rejects surrogate interiors. GPUI, OS IME callback handling, candidate-window placement, clipboard I/O and layout hit validation belong to host adapters. Core clipboard data defaults to Markdown (`CopyFormat::default()`); Plain Text is explicit. Object alt-text export and arbitrary cross-block Markdown selection remain limited.

Generate the local API reference with `cargo doc --locked -p ryne-parser --no-deps`.

## Opt-in math extension

`ParseSnapshot::with_math(source)` enables the pinned backend’s dollar-math extension. `with_dialect_and_math(source, dialect, enabled)` selects it explicitly; `new()` and `with_dialect()` keep it disabled. Preserve `math_enabled` when reparsing.

`NodeKind::InlineMath(String)` and `DisplayMath(String)` contain literal LaTeX, without Markdown children. Envelopes include `$` / `$$`; owned opening/closing marker tokens and content ranges retain half-open UTF-8 source coordinates, including container context. Code, escaped dollars, link destinations and HTML follow the backend’s context rules. Unclosed delimiters remain ordinary source.

Preview emits formula objects with source provenance; reveal restores editable raw source. HTML export emits escaped original math source including delimiters. Layout, colors and LaTeX validity belong to the host; this library has no typesetting or GUI dependency.

Both math forms follow the backend's inline-element rules. They can span lines within one block, but blank lines and Markdown block boundaries can interrupt them. Complete `$$` blocks with internal blank lines, `math` fences and Mermaid diagrams are not implemented.

## License

Library code is MIT licensed. Bundled specification fixtures and related excerpts have a separate [CC-BY-SA-4.0 notice](tests/fixtures/NOTICE.md).
