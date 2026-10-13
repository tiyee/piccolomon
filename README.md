# Ryne (如言)

[简体中文](README.zh-CN.md)

A native GPUI Markdown editor named Ryne (如言). One window, one document, with inline Markdown
markers revealed around the caret and folded back into styled text when it leaves.
This repository builds a desktop application and is not published as a crate.

Mermaid fences support native color previews and source editing with Merman 0.7.0.
See the [renderer evaluation](docs/13_Mermaid_Renderer_Evaluation.md) for dependency
compatibility, rendering details and the original comparison.

## Run

Requires a current stable Rust toolchain (development uses Rust 1.99), plus
Xcode command-line tools on macOS, or Visual Studio 2022/2026 Build Tools with
the C++ desktop workload and Windows 10/11 SDK on Windows x64.
GPUI remains pinned to 0.2.2. The parser uses the
published `editable_markdown_parser` 0.0.4 crate with its opt-in math API.
GPUI's `runtime_shaders` feature avoids requiring the separately downloaded Metal
compiler component.

```sh
cargo run --locked
cargo run --locked -- /absolute/path/to/note.md
```

Build a locally launchable app with `sh scripts/bundle-macos.sh` (append `debug`
for a debug build). The output is `target/Ryne.app`, ad-hoc signed for local
use, without notarization or file associations.
The bundle's app icon is generated from `assets/logo.svg` at standard and Retina sizes.
Bundling refreshes this app's Launch Services registration after replacing its resources.

On Windows, run `pwsh -File scripts/bundle-windows.ps1` (add `-Profile debug`
for a debug build). The output is `dist/Ryne-windows-x64.zip`; extract it and
launch `Ryne/Ryne.exe`. The portable package includes the MSVC runtime statically
and ships the embedded STIX font's license notices. Release builds discover
`fxc.exe` from the installed Windows SDK; `GPUI_FXC_PATH` can override its location.
Windows packages are unsigned.

If a registry mirror has not synchronized `editable_markdown_parser` 0.0.4,
use the official crates.io registry or wait for the mirror to synchronize.

## GitHub Actions packages

The **Package desktop apps** workflow runs on pushes, pull requests and manual
**Run workflow** requests. It builds release packages with the locked dependencies,
checks architecture boundaries and Rust formatting, and uploads ZIP files plus
SHA-256 checksums to the run's **Artifacts** section (retained for 14 days).

| Build environment | Architecture | Package |
| --- | --- | --- |
| macOS 15 | Apple Silicon / Intel | `Ryne-macos-15-{arm64,x86_64}.zip` |
| macOS 26 | Apple Silicon / Intel | `Ryne-macos-26-{arm64,x86_64}.zip` |
| Windows Server 2022 | x64 | `Ryne-windows-2022-x64.zip` |
| Windows Server 2025 | x64 | `Ryne-windows-2025-x64.zip` |

Runner labels follow GitHub's [available images](https://github.com/actions/runner-images#available-images).
Windows Server labels identify build environments; both packages target Windows
10/11 x64, rather than separate desktop OS editions. CI packaging does not validate
GUI, GPU or IME behavior on Windows 10/11 or older macOS releases. The macOS binary
deployment target and bundle metadata are both set to 12.0; packages are ad-hoc
signed and are not notarized. There are no Linux jobs or automatic GitHub Releases.

The separate **Publish to Microsoft Store** workflow builds a Windows x64 MSIX
and optionally submits an update through Microsoft's Store CLI. Run it manually
with a four-part version and leave `submit` unchecked to inspect the package first.
Configure the Partner Center identity Variables and authentication Secrets before
submitting; the first Store publication is completed in Partner Center.
See [Store publishing setup](docs/19_Windows_Store_Publishing.md) for the exact
settings, commands and signing behavior.

## Editing

- Interface languages: Simplified Chinese, Traditional Chinese, English, Japanese,
  Korean, French and Spanish. Use **Language** in the toolbar to switch immediately
  or follow the system language. Explicit choices survive restarts (macOS:
  `~/Library/Application Support/Ryne/language`); unsupported system languages fall
  back to English. Toolbar, block/insert menus, file prompts, status messages,
  welcome examples and newly inserted placeholders are localized. Switching keeps
  the current document, selection, undo/redo and IME composition intact. Technical
  error details from the OS and renderers retain their original text. Native file
  dialog controls follow macOS language settings.
- The document uses Helvetica Neue with PingFang SC fallback, 18 px body text
  and a centered reading column up to 760 px wide. Headings use a lighter
  semibold hierarchy; paragraph and block spacing follows semantic Markdown
  boundaries while retaining editable source blank lines. Code and local/full
  source use Menlo at 15 px. Unavailable fonts fall back to system fonts.
  Layout changes preserve source bytes and do not mark the file as modified.
- The caret follows the text size, with matching IME geometry. It blinks every
  500 ms while idle, becomes visible immediately after input or navigation, and
  stays steady during IME composition and dragging. Unfocused windows hide it.
  The 2 px caret uses the natural text insertion point without extra spacing or
  splitting shaped text, strikethroughs, or inline code backgrounds. Blinking,
  focus and selection leave wrapping unchanged; mouse and IME share the same layout.
- An invisible append block follows the document content. Hover over that area to
  reveal a `+`, then hover over or click it to open the insert menu. Add paragraphs,
  H1–H6 headings, ordered/unordered/task lists, quotes, code, Mermaid, tables,
  images, inline formulas or separators. Insertion selects the editable placeholder
  or places the caret in the new body, preserves existing bytes and local line
  endings, and undoes in one step. Hovering never dirties the document. The entry
  hides during drag selection, IME composition and whole-document source mode.
  Unclosed structures that would swallow the new block must be completed first.
- The hovered block, or the block containing the caret while the editor has focus, shows a subtle type
  control just outside its upper-left corner: H1–H6, paragraph, list, quote,
  image, code, Mermaid or table. Controls use embedded SVGs from `assets/icons` for
  H1–H6, text, ordered/unordered/task lists, code/raw blocks, images and tables.
  Matching outline icons identify quotes (indented text with a vertical bar),
  Mermaid (connected flowchart nodes) and separators (a horizontal rule).
  Lists show one type control at the upper-right corner of the entire outer list;
  individual items, including nested items, show only their own three-dot button.
  The list type menu selects or copies the complete list.
  The menu header uses the same SVG type icon as the block control.
  The control is vertically centered on the block's first text line, including code and table bodies.
  Hover over the control for local source, select-block and
  copy-source actions; click a menu item to execute it. Moving away, Escape or clicking outside dismisses the menu. The control
  follows nesting, wrapping and scrolling, and hides during drag selection, IME
  composition and whole-document source mode. Showing controls never changes source
  or undo history. Block dragging and type conversion are not implemented.
- Bold, emphasis, strikethrough, inline code and links have inline previews.
  After typing an opening inline delimiter, the first body character completes
  the closing delimiter after the caret and keeps source editing active. This
  covers `*` / `**` / `***`, `_` / `__` / `___`, `~~`, backtick runs, `[` (adding
  `]()`), and `$`. Markdown delimiter rules still apply; escaped or invalid input
  stays literal. Typing an automatically inserted closer skips over it without
  changing source. Completion and the body character undo together; IME completion
  happens only after confirmation. Code and local/whole-document source modes stay literal.
- Single-line `$LaTeX$` formulas render inline with Formulary 0.3.0 and math-core 0.5.5,
  using embedded STIX Two Math
  glyph outlines, baseline alignment, wrapping and variable row height. Clicking
  a formula or entering it with an arrow reveals its literal source; leaving folds
  it again. Lists and table cells support the same interaction. Code stays literal.
  Failed, oversized or multiline formulas and `$$...$$` display math stay editable
  as source. Source remains authoritative; reveal and rendering do not dirty files.
  Missing glyphs (including Chinese inside formulas), unsupported font variants
  and recovered MathML structures also keep the complete source visible.
  Formula images are monochrome; two-dimensional formula editing is not included.
  The historical [RaTeX / Formulary comparison](docs/reviews/formulary_renderer/README.md)
  remains available with baseline guides and 1×/2× images. RaTeX is no longer
  a dependency of the editor. The bundle includes the STIX font license notices.
- H1–H6 and Setext headings have level-specific typography and line heights.
  At a block start, type one to six `#` characters; they remain visible until the
  first non-`#` character, then fold into a heading. A missing separator space is
  inserted with that character in one undoable edit (for example, `##Title` becomes
  `## Title`). Code and block/whole-document source modes keep literal input.
  Clicking a heading reveals its structural markers; moving the caret outside folds them.
  Hovering alone keeps the markers hidden.
  Revealed markers can be edited directly or through source mode.
- Clicking or navigating into an inline element reveals its source, including
  enclosing inline formats. Moving outside folds the markers again.
- Ordinary paragraphs support multiline editing and visual wrapping. Enter splits
  a paragraph with a blank line; Shift+Enter inserts a Markdown hard break (two
  spaces and a newline). Both preserve the local LF / CRLF / CR convention.
- Backspace at a paragraph start or Delete at its end joins adjacent ordinary
  paragraphs. Splits, joins and multiline replacements each undo as one operation.
  Selection and paste span lines; vertical navigation retains the preferred column.
- Ordered and unordered lists render numbers, bullets and nested indentation.
  Body text stays editable while list markers stay folded; wrapped and continuation
  rows align with the body. Enter continues an item, and an empty item exits one
  level. Shift+Enter adds a hard break inside the item. Tab / Shift+Tab indent or
  outdent the current item and its children; a first sibling cannot indent.
  Backspace at the body start outdents or removes the marker; Delete joins adjacent
  sibling items without deleting neighboring blocks. Structural edits undo atomically.
  Hover or focus an item to reveal a three-dot SVG button before its number/bullet.
  Its menu inserts an empty sibling before/after, moves siblings, indents/outdents,
  or deletes an item and its children. Moves preserve subtrees and ordered marker
  positions; boundary operations are disabled. Buttons do not change wrapping.
  Alt+Enter opens the focused item menu; arrow keys, Enter and Escape navigate it.
- Home / End target the current visual row. Visual wrapping never changes source.
- Backtick-fenced, tilde-fenced and indented code blocks hide structural syntax and
  display a language label, monospaced content and a code background. Markdown in
  code remains literal. Empty blocks and code inside containers are editable.
  Enter / Shift+Enter insert code newlines with indentation; boundary deletion protects fences.
  An opening backtick or tilde fence stays visible while its language is being typed.
  Return completes a matching closing fence and places the caret on an empty code
  line. This is one undoable edit and preserves container indentation and LF / CRLF / CR.
- Closed `mermaid` fences show color diagrams when the caret leaves. Click a diagram
  or enter with arrow keys to edit its literal code; Escape folds it again. Block
  and whole-document source modes expose fences and language fields. Nested lists
  and quotes preserve their container prefixes; selection, undo and save use the
  original Markdown. Rendering runs in the background without dirtying the file.
  Invalid, incomplete or oversized diagrams stay editable as code; rendering errors
  appear beside the language label. Previews have bounded input, image and cache sizes.
- Tree-sitter highlights fenced Rust, Python, JavaScript/JSX, TypeScript/TSX, JSON
  and Bash code, including common aliases (`rs`, `py`, `js`, `ts`, `sh`). Highlights
  update while editing and remain visible in block and whole-document source modes.
  Pasting code or inserting a code newline automatically fills an empty fence language
  when the bundled grammars identify distinctive, valid syntax. The language and input
  share one undo step; existing language fields and source modes remain literal.
  Detection skips ambiguous, incomplete and over-32-KiB bodies, with at most eight
  attempts per input and a 50 ms deadline per attempt. JSX uses `javascript`.
  Unlabelled, unknown-language and indented code keeps plain monospaced styling.
  Highlighting caches unchanged code independently of source positions and never
  changes source or history. Blocks above 256 KiB use plain styling; language injections
  and incremental parsing of an edited block are not yet implemented.
- Pipe tables render a grid, styled header, column alignment and wrapped cells.
  Edit cell content directly, including inline formatting and empty cells. Tab / Shift+Tab
  move between cells; Enter moves down in the same column. Advancing past the last row
  appends a row. Shift+Enter inserts a cell line break, saved as `<br>`. Pipes typed or
  pasted into a cell are escaped. Boundary deletion protects column separators.
  Ragged tables remain editable source so no cell content is hidden.
  Hover a cell or edit/select its text to show a vertical-ellipsis SVG button on
  its right edge. Click for column alignment, row/column insertion, movement and
  deletion; unavailable boundary operations are disabled. Reserved button space
  keeps wrapping stable. Opening the menu preserves the source selection; each
  edit is one undo step. Alt+Enter opens the focused cell menu; arrows, Enter and
  Escape navigate it. Operation shortcuts are shown in the menu.
- In any content block, hover over its type control and choose local source, click the block-source button or press Cmd+Shift+B to edit
  columns and alignment, list markers, fences and language information. Toggle again or press Escape to return to preview.
  Nested block source includes its outer container context.
- Quotes, images and unsupported syntax remain editable source.
  Enter / Shift+Enter insert a literal newline in these blocks and in local or whole-document source mode.
- One document owns the text, selection and undo history. Viewing source does
  not modify the file or add an undo entry.
- Opening and saving use native file dialogs. Saving writes original source
  bytes through a temporary file and rename, preserving existing line endings.
  Writes run in a serial background queue. Edits made during a save remain unsaved;
  an earlier save receipt cannot close a window with newer changes.
- IME preedit is temporary; committing produces one source transaction.

| Shortcut | Action |
| --- | --- |
| Cmd+O | Open a file |
| Cmd+S / Cmd+Shift+S | Save / Save As |
| Cmd+Z / Cmd+Shift+Z | Undo / Redo |
| Cmd+Shift+M | Toggle whole-document source |
| Cmd+Shift+B | Toggle current block source |
| Escape | Dismiss block menu, cancel preedit or fold source |
| Cmd+A/C/X/V | Select all / copy source / cut / paste |
| Enter / Shift+Enter | Split paragraph, continue list or move down in table / hard break |
| Tab / Shift+Tab | Move between table cells or indent / outdent list item |
| Up / Down, Shift+Up / Down | Move by visual row / extend selection |
| Home / End or Cmd+Left / Right | Visual row start / end (add Shift to select) |

The demo uses full parsing and layout, with no large-document performance
guarantee. Copy currently copies the exact source selection; Markdown fragment
serialization is a later feature. It does not implement quote
continuation,
table column resizing, multi-cell TSV paste, full bidi caret behavior, accessibility text editing, or file watching.

## Internal architecture

The application uses nine private Rust workspace packages, all with the `ryne-`
prefix: `ryne-app`, `ryne-core`, `ryne-syntax`, `ryne-projection`, `ryne-extensions`,
`ryne-features`, `ryne-services`, `ryne-session` and `ryne-ui`. None are published;
the executable remains `ryne`. The root assembles static features and typed service
adapters. Only the UI package depends on GPUI; source transactions, semantic rules,
projection and background jobs can be tested without a window.

The session validates read-only command plans and publishes source, matching syntax,
selection and history together. Input normalization and completion share one draft
and one undo step. Formula, diagram, highlighting and file implementations are
injected through ports. The diagram host handles service generations, cancellation,
resource budgets and rebinding cached content to current source ranges.

See [implementation and validation](docs/18_Ryne_Architecture_Implementation.md)
for the dependency graph, regression coverage and internal extension limits. This
remains a desktop application with static built-in features; the internal API is
not a published plugin SDK.

[Render-mode interaction design](docs/20_Render_Mode_Interaction_Extensibility.md)
describes custom controls and commands contributed by rendering extensions. Its
[headless prototype](docs/reviews/render_interactions/README.md) uses the real
session and undo system. Table cell controls are now integrated into the GUI; see
the [implementation record](docs/21_Render_Mode_Table_Interactions.md).

## Checks

```sh
python3 scripts/check-architecture.py
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo run --locked -- --smoke-test
```

The smoke check opens a real window and checks layout hit testing, reveal/fold,
Unicode input, IME callbacks, undo, source mode, paragraph split/join, hard breaks,
wrap boundaries, vertical navigation, multiline selection/replacement, code layout and
hit testing, fence protection, empty/nested code, block-source/language edits and saving/reopening,
Tree-sitter highlighting, Unicode token hit testing, highlight refresh/history and unknown-language fallback,
ordered/bullet/nested list layout, hit testing, hanging wraps, continuation, exit,
indent/outdent, list source edits and selection, table alignment, wrapped/empty cell hit testing,
cell navigation, row insertion, cell line breaks, nested tables and source edits,
focused/hovered block-control geometry, hover menu callbacks and gap crossing without caret movement, copy/select/source actions,
composition/source-mode hiding and stale menu target rejection, then exits.
IME callback checks do not replace manual candidate-window checks with each IME.
The smoke runner ignores external pointer movement and invokes editing callbacks
against real window geometry. Failures print an error and exit without unwinding
through the native event loop. `--smoke-test-failure` checks this path (expected exit 1).

The original architecture documents and parser review remain in `docs/`. Their
full product scope extends beyond this small inline editing application.

MIT licensed.

![logo](./ryne.jpg)
