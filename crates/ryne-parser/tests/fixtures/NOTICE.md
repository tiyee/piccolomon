# GFM fixture attribution

`gfm-0.29.json` contains the 677 examples and expected HTML from the published
[GitHub Flavored Markdown specification, version 0.29-gfm](https://github.github.com/gfm/),
copyright the specification's authors and contributors, based on the CommonMark
specification by John MacFarlane and contributors.

The specification is licensed under
[Creative Commons Attribution-ShareAlike 4.0 International](https://creativecommons.org/licenses/by-sa/4.0/).
The derived fixture dataset is distributed under that same license. The code
license of this repository does not replace the fixture license.

Modification: examples were extracted from the published HTML into JSON on
2026-10-11. Presentation spans were removed, HTML entities decoded once, and the
specification's visible tab arrows converted to actual tabs. Example numbers,
source text and expected HTML were preserved. No failing examples were removed.
Chapter labels are retained for diagnostics. Exact acquisition and dataset
SHA-256 values are recorded in `lock.json`.

`backend-differences.json` records native backend output for the 48 known
differences, indexed by the original example numbers and chapter labels. It
contains specification excerpts and is distributed under the same CC-BY-SA-4.0
attribution and license. It does not modify the pinned input dataset.

`feasibility/v4/results/conformance.json` also includes fixture excerpts and expected HTML;
those portions retain the same attribution and license.

The auxiliary cmark-gfm implementation has its own licenses; its source is
downloaded only into the ignored `target/` directory by `build_cmark.py`. Consult
the pinned upstream source's `COPYING` and dependency notices when redistributing it.
