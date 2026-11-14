# Editing core contracts and validation

[简体中文](Editing_Core.zh-CN.md)

Status: current checkout, 2026-10-12; API changes are not published. This document records the implemented minimum editing loop. The v4 specification and earlier feasibility/comparison reports describe targets or dated experiments. This contract supersedes their earlier Plain Text default: **Hybrid copy defaults to Markdown**, with Plain Text as an explicit command.

## Identity and migration

Replace numeric `source.apply(revision, edits)` with `source.apply(source.id(), edits)`. `Source` fields are read-only through `text()`, `revision()` and `id()`. Apply the returned inverse with `applied.source.commit(applied.inverse)`. `Transaction`, `Composition`, `Session`, `Interaction`, parse snapshots and projection hits retain source identity. `PositionMap` binds both endpoints; an old map cannot transform a session twice or target another branch/document.

A cloned Source shares document identity and a revision allocator. Each successful change gets a unique revision, even when edits branch from the same old snapshot. NoChange and validation failures allocate no source revision. IDs are process-local and do not establish persistent file identity or enforce a host's single-writer policy. A host must compare incoming operations with its current source, not an archived snapshot.

All ranges use half-open UTF-8 bytes. Atomic transaction validation rejects stale/foreign identities, reversed/out-of-bounds ranges, split Unicode scalars, split CRLF and overlapping/ambiguous patches. Adjacent nonempty replacements remain valid. Inverse patches pair new ranges with old ranges, merge adjacent patches, absorb unchanged CR/LF bytes at invalid new endpoints, and merge overlaps before producing the inverse transaction.

`map_bytes(base, offset, affinity)` checks identity, bounds and UTF-8, but intentionally permits CRLF interiors. `map_cursor(old, new, offset, affinity)` additionally validates editable old endpoints and snaps new CRLF interiors upstream/downstream. `Session::transform()` follows this cursor policy. Projection cursor validation rejects UTF-8, CRLF and final grapheme interiors, including graphemes crossing semantic nodes. Layout geometry and bidi affinity are host concerns.

The current checkout pins `unicode-segmentation` to 1.13.3 (Unicode 17.0.0 data; dependency minimum Rust version 1.85). Projection cursor validation, source-to-projection affinity and directional deletion use these grapheme boundaries. Regressions cover a Unicode 17 combining mark across semantic runs, combining accents, emoji ZWJ sequences, flags, Chinese text and exact undo. Earlier feasibility results retain their original 1.12.0 / Unicode 16.0.0 baseline and were not regenerated for this upgrade.

## Parse and provenance model

`ParseSnapshot` owns one immutable source and `Dialect`, semantic nodes with parent/child indices, `LosslessSyntax`, `FieldOrigin` records and a normalized reference index. Backend event types do not appear in this interface. Node indices are local to this snapshot; consumers must carry its identity. Both resolved and unresolved reference users are indexed; rebuilding after a definition edit updates resolution. Incremental invalidation is not implemented.

Syntax tokens cover original bytes exactly once, with token kind, owner and reverse ownership indices. Inline markers, code fences, headings, task markers and container prefixes have explicit roles. Multiline inline content is a list of source fragments excluding container prefixes. In `> **a\r\n> b**`, both quote prefixes belong to BlockQuote; Strong owns its delimiter pair. Syntax envelopes include owned indentation independently of semantic content envelopes. Inline reveal windows exclude container prefixes; edits crossing them require revealing the container. Unknown or unclassified syntax remains `Source`/`Trivia`, retaining every byte.

Provenance is conservative: transformed field positions are not interpolated. Destination/title sources can cover an entire link or reference definition; complex table separators, container indentation and uncommon marker forms are not a complete syntax grammar. Such fields are not eligible for unrestricted direct semantic editing. `LosslessSyntax::serialize(source)` rejects another snapshot; legacy `serialize(source_text, partition(...))` remains an unchecked contiguous-range utility whose invalid inputs may panic.

## Projection and planning

`project(parse, interaction)` combines semantic leaves with active raw-source windows. Direct runs map exact bytes. Transform and MultiSource runs retain atomic provenance. Hidden syntax has a display anchor and explicit reveal window; Generated content has an anchor; Object content has a semantic node and source spans. Display objects currently use U+FFFC. Block separators are generated display content, not source bytes.

`Projection::hit()` returns source identity, projection revision, source fragments, resolved source endpoint, display position, hit kind and reveal target. `source_to_projection()` handles hidden markers and final grapheme boundaries. Rebuilding projection invalidates old hits even if source is unchanged. `resolve()` is a convenience endpoint query; edit requests should retain the entire hit.

Interaction keeps reveal windows and selection anchored in source. Enter a hit, project raw source, commit an edit, transform interaction, rebuild parse, then project again. `**b** → **** → **x**` remains editable despite Strong becoming Rule. Leaving folds the windows; dragging freezes the reveal set; a composition window stays active. Preedit is isolated data; host protocol integration and display overlay remain host work.

The planner accepts versioned intents and returns these outcomes:

| Outcome | Caller action |
| --- | --- |
| `Commit` | Commit the bound transaction against the current Source |
| `RevealThenRetry` | Reveal the returned source window, rebuild projection and create fresh hits |
| `PendingConfirmation` | Present the concrete transaction and source selection for explicit user approval |
| `Unsupported` | Retain input and offer the returned source window |
| `NoChange` | Retain source and history |

Implemented commands: source-window replacement, preview text replacement, forward/backward grapheme deletion, partial entity/escape edits, explicit format removal, task toggle and table pipe escaping. Preview deletion of all formatted content removes its delimiters. Literal punctuation uses numeric entities, or escaped table pipes. Before committing preview replacement, reparse candidate bytes and compare the resulting display with the requested display replacement. A mismatch requires reveal. Source-window edits preserve exactly what the user entered.

Preview editing across multiple atomic transformations, code encoders, objects, generated separators and complex blocks uses reveal/unsupported fallbacks. A selection crossing hidden interior syntax may produce a pending transaction. The full v4 command set (split/join/indent/outdent, structural table commands, general formatting and arbitrary cross-block edits) remains pending.

Clipboard helpers provide explicit Markdown or PlainText data for a source selection. The default is Markdown. PlainText currently slices display data; object alt text and arbitrary semantic Markdown export require further work. GPUI/layout, real OS IME callbacks and candidate windows, clipboard I/O, file I/O and undo history installation belong to host adapters.

## HTML validation

The current checkout pins `pulldown-cmark` to 0.13.6, which [fixes quadratic parsing of nested image wikilinks](https://github.com/pulldown-cmark/pulldown-cmark/releases/tag/v0.13.6). The default options still explicitly enable only tables, strikethrough and task lists; math remains opt-in and wikilinks remain disabled. The regression gates cover all 677 source round trips, the 629/677 native HTML baseline with the same 48 exact differences, and the owned renderer's 677 examples. The reference-definition blank-line regression from the 0.13.5 upgrade remains in place.

`tests/backend_upgrade.rs` locks the extension options and verifies that nested wikilink-like input stays literal through HTML, projection and reversible Unicode edits with LF/CRLF/CR, both with and without math. A backend-only test explicitly enables wikilinks and checks the innermost-image behavior at depths 2 and 4096; it reproduces the old behavior on 0.13.5 without relying on a timing threshold. This does not enable wikilinks in the editor or establish an end-to-end performance bound. Historical feasibility measurements remain on 0.13.4.

The owned renderer addresses the original 48 native differences by category: quote escaping, table layout/alignment attributes, task HTML, block newlines, extended autolinks, old inline-comment syntax, and tagfilter. `backend_html()` remains the unchanged diagnostic backend, whose exact output for each known difference is stored in `tests/fixtures/backend-differences.json` alongside its example and section. Every other example must still match; equal totals cannot hide swapped regressions.

The owned renderer compares all 677 pinned examples byte-for-byte, without normalization. Specification sections have explicit profiles: base autolinks (`6.8`) disable extensions through CommonMark029, GFM extensions use Gfm029, raw HTML is preserved except in the disallowed-HTML section. Production `html()` uses Gfm029 and global tagfilter. This separation follows the [fixed GFM specification](https://github.github.com/gfm/). Tagfilter preserves source bytes and does not replace HTML sanitization. Passing this corpus is not proof of compatibility on every possible input.

Run the standard checks in [the development guide](README.md). Regressions include new-CRLF inverses, 4,000 deterministic multi-patch cases, branch/document rejection, stale projection hits, final graphemes, bidirectional mapping, ownership, reference dependencies, session transitions and planner commands. Historical benchmark results were not regenerated and do not describe this implementation.

Upgrade validation on 2026-11-14 (macOS, Rust 1.99.1): `cargo test --locked` passed 447 tests with 2 existing ignored tests (external image download and large sidebar measurement). All three GFM fixture gates passed unchanged. Architecture checks, `cargo fmt --check`, and Clippy with `--locked --all-targets -- -D warnings` passed. The application's full 396-stage `--smoke-test` passed using real window geometry and editing callbacks, including nested blocks, source modes, undo/redo, formulas, Mermaid and IME protection. This does not cover manual OS input-method candidate windows or Windows hardware. The independent block-tree prototype also pins 0.13.6 and passed its five tests.
