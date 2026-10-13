# RaTeX / Formulary comparison prototype

This standalone binary preserves the comparison made before the editor switched
to Formulary. Ryne now uses Formulary; this tool still includes RaTeX for reference.
It compares identical inline LaTeX at 18 and 36 CSS px using:

- RaTeX 0.1.14, embedded KaTeX outlines and the former editor parameters.
- math-core 0.5.5 → Formulary 0.3.0 → STIX Two Math 2.13 b171 outlines.
- resvg 0.45.1 for 1× / 2× PNG output.

From the repository root, with current stable Rust:

```sh
cargo run --locked --manifest-path docs/reviews/formulary_renderer/Cargo.toml
open docs/reviews/formulary_renderer/output/index.html
```

The HTML report shows side-by-side formulas, baseline guides, metrics, intermediate
MathML and expandable raster comparisons. `output/comparison.png` is a six-case
overview at 36 px / 2×. `output/results.tsv` records all 17 fixtures at both sizes
and both backends. Outputs and build files are ignored; the checked-in
`results.tsv` and `samples/comparison.png` preserve the evaluated run.

To compare a custom formula, use a separate output directory:

```sh
cargo run --locked --manifest-path docs/reviews/formulary_renderer/Cargo.toml -- \
  --output target/formulary-custom --latex '\frac{a_1}{\sqrt{b}}'
```

Invalid formulas are recorded as sample errors with their original source.
The process returns success when the report was produced; file/raster errors
return failure. Input is limited to one line, 4096 bytes and nesting depth 64;
layout and pixel dimensions are also bounded. This is an offline CLI evaluation,
not a hardened renderer for untrusted documents.

```sh
cargo fmt --manifest-path docs/reviews/formulary_renderer/Cargo.toml --check
cargo test --locked --manifest-path docs/reviews/formulary_renderer/Cargo.toml
cargo clippy --locked --manifest-path docs/reviews/formulary_renderer/Cargo.toml --all-targets -- -D warnings
```

The comparison binary does not exercise the editor, so its GUI smoke test is
outside this prototype's validation scope. Browser SVG and resvg PNG output do not establish identical
GPUI rasterization, IME placement, source mapping or editing behavior. Both
engines emit their SVG colors here; Ryne paints formula SVGs as a single
color. This historical prototype uses direct 18/36 px SVG emission and warns on
unsupported font variants. The editor instead enlarges SVG coordinates to retain
script precision, normalizes metrics to em, uses 0.06 em padding and keeps the
source for unsupported variants. Renderer padding is included in each reported ascent/descent. PNGs should
be inspected at their stated density, without enlarging the 1× image.

Formulary's missing-glyph detection runs on parsed token text (after mathvariant
mapping), since its display list omits unmapped glyphs. Missing Chinese glyphs
cause source fallback. Variation selectors use the font's variation cmap; an
unsupported mapping yields an explicit warning while retaining the default base
glyph. MathML parse recoveries are also reported as warnings, not full support.

See [the evaluation](../../15_Formulary_Renderer_Prototype.md) for results.

## Font provenance

The shared unmodified font, OFL license and FONTLOG in `assets/fonts/stix-two-math`
were downloaded from the upstream
[STIX fonts v2.13b171 tag](https://github.com/stipub/stixfonts/tree/v2.13b171).
No system font installation is needed for Formulary. Overview labels use system
fonts; RaTeX's CJK fallback depends on the host's available fonts.

- File: `assets/fonts/stix-two-math/STIXTwoMath-Regular.otf` (repository root)
- Upstream path: `fonts/static_otf/STIXTwoMath-Regular.otf`
- SHA-256: `3a5f3f26f40d5698b3c62dd085d48d6663696a3f80825aab8b553d5097518e8c`
- License: SIL Open Font License 1.1, included alongside the font as `OFL.txt`.
