# Package development documentation

[简体中文](README.zh-CN.md)

This directory contains implementation notes and development guidance for `ryne-parser`. Consumer-facing installation and API examples belong in the package's [English README](../README.md) and [Chinese README](../README.zh-CN.md).

Current runtime contracts, API migration and validation scope: [Editing core contracts and validation](Editing_Core.md). Earlier dated plans and comparisons are historical baselines.

## Reading order

1. Read the package README for the current public behavior and limitations.
2. Read [`src/lib.rs`](../src/lib.rs) and [`tests/gfm_fixtures.rs`](../tests/gfm_fixtures.rs) for implemented behavior and regression coverage.
3. Read the upstream [v4 behavior specification](https://github.com/iDigimon/markdown-engine/blob/main/docs/Editable_GFM_Engine_v4_Typora_Hybrid_Specification.md), [technical plan](https://github.com/iDigimon/markdown-engine/blob/main/docs/Editable_GFM_Engine_v4_Complete_Technical_Plan.md), and [feasibility report](https://github.com/iDigimon/markdown-engine/blob/main/docs/Editable_GFM_Engine_v4_Feasibility_Report.md) when changing architecture or semantics.
4. Read [Lessons from writ and adoption plan](Writ_Comparison_and_Adoption.md) for the proposed projection, marker, interaction, input, storage, and incremental work.
5. Read the upstream [feasibility guide](https://github.com/iDigimon/markdown-engine/blob/main/feasibility/v4/README.md) before running evidence-generating scripts or the auxiliary cmark-gfm comparison.

The upstream v4 documents currently contain Chinese text. Designs describe target capabilities; source, tests, and recorded executions establish implementation status.

## Implementation map

| Location | Responsibility |
| --- | --- |
| [`src/lib.rs`](../src/lib.rs) | Source identity, atomic transactions, position maps, sessions, composition and unit tests |
| `src/syntax.rs` / `src/projection.rs` | Owned parse snapshot, lossless ownership, source/preview mappings and reveal interaction |
| `src/planner.rs` / `src/render.rs` | Versioned edit intents, clipboard data policy and owned HTML renderer |
| [`tests/editing.rs`](../tests/editing.rs) | CRLF inverses, random multi-patch round trips, identities, mappings, sessions and planner regressions |
| [`tests/gfm_fixtures.rs`](../tests/gfm_fixtures.rs) | Source round trips and native HTML diagnostic baseline for 677 fixed examples |
| `tests/fixtures/` | GFM inputs, hash lock, and independent specification license notice |
| [`examples/v4_probe.rs`](../examples/v4_probe.rs) | Feasibility probe and benchmarks that reuse the library |
| Workspace `Cargo.toml` / `Cargo.lock` | Shared metadata, dependency declarations, and locked versions |

## Development constraints

Preserve original source bytes and UTF-8 byte coordinates. Reject edit endpoints inside CRLF. Validate all edits against one revision before applying any of them, preserve inverse transactions and affinity, and isolate composition preedit from source. Do not reconstruct source from semantic text or HTML.

The backend deliberately enables tables, strikethrough, and task lists. Keep its locked version and explicit options; `Options::all()` and `ENABLE_GFM` would broaden the dialect. The current 629/677 native HTML match count is a diagnostic baseline, not full GFM compatibility. Each native difference is now locked by example and exact output; review every changed example. The owned renderer has a separate 677-example gate with explicit section profiles.

Keep runtime and validation-only dependencies separate. Declare shared dependencies in the workspace and inherit them in this crate. Code, code comments, diagnostics, and default documentation use English. Maintain matching Chinese documentation with `.zh-CN.md` names and reciprocal language links.

## Validation

Run these commands from the workspace root using stable Rust/Cargo:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --locked --no-deps
cargo build --workspace --locked --examples
```

For focused checks:

```sh
cargo test -p ryne-parser --locked
cargo test -p ryne-parser --locked --doc
```

For documentation-only changes, check examples, links, bilingual consistency, and claimed behavior. Compile and run changed README examples explicitly; Cargo does not automatically test README code blocks as doctests.

Only when the task requires updated performance or feasibility evidence:

```sh
cargo run -p ryne-parser --locked --release --example v4_probe -- bench
```

Library code is MIT licensed; preserve the fixtures' separate [CC-BY-SA-4.0 notice](../tests/fixtures/NOTICE.md).
