# Lessons from writ and adoption plan

[简体中文](Writ_Comparison_and_Adoption.zh-CN.md)

This dated comparison records the pre-change baseline. For current implementation and remaining limits, see [Editing core contracts](Editing_Core.md).

Review date: 2026-10-11. Status: proposed implementation tasks; this document does not add runtime capabilities or dependencies.

This is an implementation addendum to the [v4 technical plan](https://github.com/iDigimon/markdown-engine/blob/main/docs/Editable_GFM_Engine_v4_Complete_Technical_Plan.md), particularly sections 5, 8, 10, 11, and 16–18. The [v4 behavior specification](https://github.com/iDigimon/markdown-engine/blob/main/docs/Editable_GFM_Engine_v4_Typora_Hybrid_Specification.md) remains authoritative. Adopt writ's explicit display mappings and practical interaction mechanisms while preserving this project's transaction, source-fidelity, and dialect contracts.

## 1. Review baseline and current implementation

The external reference is writ 0.19.3 at commit [`0f32b7f2a99ffe11065a8834b64c8057061dbb60`](https://github.com/wilfreddenton/writ/tree/0f32b7f2a99ffe11065a8834b64c8057061dbb60). All external source links below identify that commit.

The local comparison uses [the library implementation](../src/lib.rs) and [the pinned GFM integration tests](../tests/gfm_fixtures.rs).

| Area | writ implementation | Current local implementation |
| --- | --- | --- |
| Storage | Rope buffer and chunk-based parser input | Immutable `Arc<str>` snapshots; edits copy the complete text |
| Parsing | tree-sitter block tree and separate inline trees, with old trees passed into reparsing | Full-document pulldown-cmark event parsing |
| Markers | Typed block markers and source ranges used for display and continuation | `partition()` divides raw source at event boundaries; all atoms still have root ownership |
| Display mapping | Bidirectional per-line `SegmentMap`, with hidden and collapsed regions | `Direct` / `Transform` runs and projection-to-source `resolve()` |
| Reveal interaction | Rendering uses cursor location to decide whether to show source | `Session` transforms its source range but is not integrated into `preview_inline()` |
| Layout and input | Render/height caches, viewport layout, scroll anchoring, and platform preedit display | No layout engine or platform IME integration; composition data isolation is implemented |

The existing primitives do not constitute a full semantic tree, syntax ownership model, general edit planner, or complete block projection. Native backend HTML matches 629/677 pinned examples; that is a diagnostic baseline, not full GFM compatibility.

## 2. W01 — Explicit projection and hidden-source mapping

Reference: writ's [`segment_map.rs`](https://github.com/wilfreddenton/writ/blob/0f32b7f2a99ffe11065a8834b64c8057061dbb60/src/segment_map.rs) explicitly separates verbatim display segments, hidden source ranges, and ranges replaced by display tokens.

Extend the projection model with styles, hidden-source records, generated anchors, and object identity. Keep hidden records in a separate collapsed-source index so a source position inside a hidden delimiter can locate both a display boundary and a reveal target. Add source-to-projection lookup alongside projection-to-source resolution.

Preserve these distinctions:

- `PositionMap` maps old source coordinates to new source coordinates after a transaction. It is separate from source/display mapping within one snapshot.
- Direct runs permit exact byte-offset translation. Transform runs resolve through their complete provenance and `Upstream` / `Downstream` affinity; do not interpolate positions by length.
- Hidden markers, generated separators, and objects require explicit results or anchors rather than invented direct source offsets.
- Projection and layout hits must carry their source/projection/layout version context. Reject stale hits before planning an edit.

Acceptance: test both lookup directions, both affinities, empty/all-hidden content, document endpoints, nested delimiters, and transformed values. For `&fjlig; → fj`, projection byte 1 resolves to source 0 upstream and 7 downstream under the current endpoint policy. Direct-run round trips must preserve valid UTF-8 boundaries; hidden and transformed ranges need explicit policies rather than a global bijection.

## 3. W02 — Typed markers and container provenance

Reference: writ's [`marker.rs`](https://github.com/wilfreddenton/writ/blob/0f32b7f2a99ffe11065a8834b64c8057061dbb60/src/marker.rs) represents marker kinds and byte ranges, including list, quote, heading, checkbox, and fence markers.

Introduce typed syntax records with source ranges, ownership, and roles. Derive display decorations and continuation requests from those records through the projection and planner layers. Keep syntax ownership independent of rendering policy; a display marker list alone is insufficient to account for every source byte.

Acceptance: use nested quote/list/task cases and non-contiguous inline input. In `> **a\n> b**` (where `\n` denotes a newline), the second quote prefix belongs to the blockquote, not the strong span. Preserve tabs, spaces, delimiter spelling, and mixed line endings. Validate complete source coverage and ownership relationships in addition to byte-for-byte serialization.

## 4. W03 — Connect reveal sessions to projection

Reference: writ's [`render.rs`](https://github.com/wilfreddenton/writ/blob/0f32b7f2a99ffe11065a8834b64c8057061dbb60/src/render.rs) builds display text and styles according to whether the cursor is inside a line or inline region.

Build projection from a parse snapshot plus explicit interaction state. Active source windows emit original source pieces; preview regions emit semantic text, styles, or objects. A session remains anchored in source coordinates when reparsing changes the semantic node kind. Cursor movement can change projection without reparsing unchanged source.

Acceptance: demonstrate click → reveal → source edit → reparse → leave → preview. Replay `**b** → **** → **x**`: after deleting `b`, keep the four-star source session editable even though the semantic result is a thematic break. Preserve selection and the visible source anchor across both transitions. Freeze the reveal set during drag selection and keep composition sessions active until the platform protocol completes.

This work needs W01 and sufficient W02 provenance for the supported syntax subset. If provenance is insufficient, use the v4 SourceBlock fallback rather than guessing a hidden range.

## 5. W04 — Projection caches, viewport layout, and scroll anchoring

Reference: writ's [`doc_layout.rs`](https://github.com/wilfreddenton/writ/blob/0f32b7f2a99ffe11065a8834b64c8057061dbb60/src/doc_layout.rs) caches line rendering and measured heights, materializes viewport content, and restores a scroll anchor after rebuilding.

Use Markdown blocks and logical ranges as this project's cache units, with finer units only where provenance and layout permit. Account for source/parse identity, reveal state, composition, and decoration changes in projection invalidation; layout additionally depends on width, font, scale, and resolved object sizes. Reuse only entries whose inputs remain valid.

Before rebuilding, capture a source anchor and its relative viewport position. Transform the anchor through edits, resolve it in the new projection, and compensate for height changes. A measured-height index can later locate visible blocks plus overscan; keep the active cursor and composition context materialized.

Acceptance: compare cached output with an uncached build; exercise cursor-only updates, width/font changes, table reveal, asynchronous image sizing, and edits above the viewport. Measure projection, shaping/layout, and rendering separately. This is a proposed adaptation; writ's line-oriented model does not establish correctness for this project's cross-line block semantics.

## 6. W05 — Platform composition overlay

Reference: writ's [`shell.rs`](https://github.com/wilfreddenton/writ/blob/0f32b7f2a99ffe11065a8834b64c8057061dbb60/src/shell.rs) handles preedit as display-only state and updates the OS candidate-window location from the composition caret.

Connect the existing `Composition` isolation to the v4 input-context snapshot and host adapter. Keep preedit text, marked selection, and overlay-to-source mapping in one versioned input context. Commit through one revision-checked source transaction; update/cancel must leave source bytes, source revision, and history unchanged. Define commit/cancel completion before processing queued undo, mode switches, or external edits.

Acceptance: replay Chinese input, emoji, selection replacement, cancel, stale commit, undo after commit, and candidate-window positioning. Test UTF-16 surrogate boundaries at the platform bridge and grapheme boundaries in the final composed projection, including `*e*` followed by U+0301. Validate real OS callback sequences separately from headless isolation tests.

## 7. W06 — Storage and incremental parsing evaluation

References: writ's [`buffer.rs`](https://github.com/wilfreddenton/writ/blob/0f32b7f2a99ffe11065a8834b64c8057061dbb60/src/buffer.rs) uses Rope storage; [`parser.rs`](https://github.com/wilfreddenton/writ/blob/0f32b7f2a99ffe11065a8834b64c8057061dbb60/src/parser.rs) supplies rope chunks and edited old trees to tree-sitter.

Retain the planned storage abstraction (`len_bytes`, fragment access, cursor, and atomic patch application). Evaluate a persistent Rope or piece tree after measuring source-copy cost. Preserve immutable snapshots, UTF-8 byte ranges, CRLF boundaries, all-or-nothing validation, inverse transactions, and affinity semantics.

Evaluate incremental syntax, projection reuse, and layout virtualization independently. In the reviewed writ code, inline scopes are still traversed and node/style caches can be rebuilt across the document; old-tree reuse alone does not prove an entirely local edit pipeline. Do not adopt an unconditional `O(log n + k)` latency claim.

A tree-sitter backend experiment must remain isolated until it passes the fixed dialect and provenance checks. Keep the current pulldown-cmark backend and options as the baseline. Test incremental results against full parsing, including changed reference definitions and unresolved users, fences, tables, and list boundaries. Expand invalidation or fall back to full parsing when reuse cannot be justified; worst-case propagation may reach EOF.

Acceptance: compare semantics, source ranges, ownership, and maps with full parsing; verify storage substitution against the same edit/inverse tests. Benchmark 10/100 KiB and 1 MiB documents, long paragraphs, nested containers, and long delimiters, reporting p95 and memory alongside the measured scope.

## 8. Constraints on adoption

- Preserve source spelling during ordinary edits. The reviewed writ buffer can renumber ordered lists during editing. Here, display numbering belongs in decorations; source renumbering requires an explicit planner command and reversible transaction.
- Preserve affinity-aware transform handling. writ's collapsed-token lookup chooses the source start and has no affinity argument; that policy cannot replace this project's transform contract.
- Keep the headless core independent of GPUI, Vello, winit, file watching, GitHub, and network I/O. Borrow mechanisms without adding writ as a runtime dependency.
- Keep GFM 0.29 and the explicit tables/strikethrough/tasklists options. Do not import additional dialect features or adjust the 629/677 diagnostic count just to accommodate a different backend.

## 9. Implementation order and stage gates

All tasks below are pending. They refine the existing M0–M4 plan without changing its release gates.

| Order / stage | Deliverable | Completion evidence |
| --- | --- | --- |
| 1 / M0–M1 | W02 provenance experiments and W01 projection mappings for the supported subset | Ownership, hidden-range, transform, affinity, and stale-hit regressions |
| 2 / M1 | W03 minimal interactive view and W05 platform composition | Reveal/edit/preview replay, semantic-kind transition, and real IME results |
| 3 / M2 | Extend W01–W03 to structured blocks and commands | Nested containers, tables, tasks, code, SourceBlock, and inverse-edit regressions |
| 4 / M1 measurements, M4 optimization | W04 cache and layout work; W06 storage and parsing evaluation | Uncached/full-parse equivalence, scroll stability, measured latency and memory |

Complete the supported projection/interaction loop before changing storage or the parsing backend. Full GFM, complete ownership, general planning, GUI/IME integration, and the incremental engine remain subject to their original acceptance requirements.

## 10. Evidence and limits

During the comparison, `cargo test --workspace --locked` passed 21 unit tests, 2 integration tests, and 1 doctest. Those tests cover existing primitives, not the pending tasks above.

The reviewed writ `segment_map.rs` was compiled independently in Rust 2021 mode and its 5 unit tests passed. A separate synthetic probe supplied `&fjlig; → fj` as a collapsed token and observed display byte 1 mapping to source 0. This establishes that mapping helper's policy; it does not establish how writ itself renders entities.

The complete writ application was not built or exercised. Real GUI behavior, OS IME integration, and end-to-end performance were not verified in that comparison. No backend, dependency, fixture, or recorded feasibility result is changed by this document.
