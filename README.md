# Ryne (如言) — Markdown that takes shape as you write

[简体中文](README.zh-CN.md)

Ryne is a native Markdown editor for Windows and macOS, built for everyday notes, technical documentation and long-form writing. Text wraps in a centered reading column so you can focus on the page.

Write directly in the document as you read it. Move the cursor into bold, italic or struck-through text, a link or a span of inline code to reveal its Markdown marks. Move away and the formatting returns. Click a heading to edit its marks. When you need to change the structure, you can open the source for a single block or the whole file.

- **Headings, lists and quotes.** Organize chapters with six heading levels. Write directly in numbered lists, bullet lists, task lists and nested quotes. Continue a list with Enter, adjust its indentation or click a checkbox to tick off a task.
- **Alerts.** Top-level `> [!NOTE]`, `TIP`, `IMPORTANT`, `WARNING` and `CAUTION` blocks show a status icon, title, colored left border and tinted background. Edit the body directly or click the title to edit block source. Unknown and nested alert types retain their literal markers.
- **Browse files and headings.** The first button in the top-right shortcut group opens the sidebar. This group is in the title bar on macOS and in the menu bar below it on Windows. Under **Files**, expand folders or switch to the list to filter by name or relative path. Opening a document sets its parent folder as the new root. Use the icon at the top left of the sidebar to switch to **Outline**, where H1–H6 icons identify heading levels and each nested level indents by one icon’s width. Choose a heading to scroll there and keep your text selection.
- **Editable tables.** Click a cell to type and use the keyboard to move between cells. The cell menu lets you insert, move or delete rows and columns and set column alignment.
- **Readable code.** Use inline code for a command or a fenced block for a longer example. Press Enter after the opening fence to add its closing fence. Choose a language for syntax highlighting in Rust, Python, JavaScript, TypeScript, JSON or Bash.
- **Diagrams and formulas.** Preview Mermaid diagrams and inline or block LaTeX formulas alongside your text. Click either to edit its source. To change the formula typeface, choose **Ryne → Math font**.
- **Formatting and PDF export.** Tidy the Markdown in one undoable step. When you’re ready to share, export a paginated A4 PDF with your tables, formulas and Mermaid diagrams.
- **Local Markdown files.** Open and save ordinary `.md` files with the original indentation and line endings of untouched text. Select, copy or paste across lines, and undo or redo edits.

Ryne’s menus and editable welcome guide are available in Simplified Chinese, Traditional Chinese, English, Japanese, Korean, French and Spanish.

Editing reuses cached text layout when its content and formatting are unchanged. The first layout and parsing still process the whole document; viewport layout is a later stage of the [block-tree implementation plan](docs/reviews/block_tree/EXECUTION.md).

## Get Ryne

- **Windows 10 or 11 (64-bit x64).** Download the ZIP ending in `win-x86_64.zip`, extract it, and open `Ryne.exe` inside the `Ryne` folder. [Ryne downloads](https://github.com/tiyee/ryne/releases)
- **macOS.** Choose the ZIP ending in `mac-arm_64.zip` for Apple Silicon or `mac-x86_64.zip` for Intel. Extract it, drag `Ryne.app` to Applications, and open it. You can also open a Markdown file from Finder with **Open With → Ryne**. [Ryne downloads](https://github.com/tiyee/ryne/releases)

On Windows, double-click `Register-context-menu.cmd` in the extracted `Ryne` folder once to add **Open with Ryne** to the right-click menu for `.md`, `.markdown`, `.mdown` and `.mkd` files, and register Ryne under **Open with**. No administrator rights are needed, and your default editor stays unchanged. On Windows 11, the direct command may be under **Show more options**. Before moving or deleting the folder, run `Unregister-context-menu.cmd`; after moving it, run the registration script again. Microsoft Store packages register their file associations during installation; use **Open with → Ryne** without running these scripts.

## Write your first note

1. Open Ryne and try editing the welcome guide. Choose **File → New** for a blank note, or **File → Open** to work on an existing one.
2. Type `# Meeting notes` and press **Enter**. Under the heading, type `- [ ] Send minutes`. Click the checkbox to mark the task done.
3. Choose **File → Save**, give the note a name such as `meeting.md`, and pick a folder. Next time, use **File → Open Recent** to find it.

To keep the original and save a second copy, choose **File → Save As**. You can also reload the file from disk in the File menu. Ryne asks before discarding unsaved edits.

To change the language of the menus and the untouched welcome guide, choose **Ryne → Language**. Ryne remembers your choice and keeps any edits you’ve made to the guide.

Choose **Ryne → Theme** for **Follow system, Dark or Light**. Ryne follows the system appearance by default and remembers your choice for the next launch.

## Work with Markdown

| What you’re editing | What to do |
| --- | --- |
| Lists and quotes | Press **Enter** to continue a list or quote. Press **Enter** on an empty item or quote line to leave one level. In a list, **Tab** indents the current item and **Shift+Tab** moves it back. |
| Tables | Click a cell to edit it. **Tab** moves to the next cell, **Enter** moves down the column, and **Shift+Enter** adds a line within the cell. Open the cell’s **⋮** menu to add a row or change column alignment. |
| Code | Start a line with three backticks and a language such as `python`, then press **Enter**. Ryne adds the closing fence. Type your code between the fences, or choose a language from the menu above the code. |
| Diagrams and math | Use `mermaid` as the code block language for a diagram, type a formula such as `$E=mc^2$` in a sentence, or put `$$…$$` in its own paragraph for a centered block equation. Move the cursor away to see the result. Click the diagram or formula to edit it again. |

To insert a table or another block without typing the syntax, hover just below the last line and click **+**. To insert one between two existing blocks, hover over the first block’s upper-left icon and choose **Add below**.

To edit the Markdown marks for a list, table or quote, choose **Edit block source** from that same menu. To see the whole file’s Markdown, click the **eye** in the shortcut group. Click the **code** icon to return to the formatted view. Standalone images from HTTP(S), FTP, `file:` and `data:` URLs (PNG, JPEG, GIF first frame, WebP and SVG) render proportionally within the available text width, including in lists and quotes; click an image to edit its Markdown. Failed or oversized images show a broken-image icon followed by a red, struck-through URL; addresses longer than 256 characters end with an ellipsis. Click the icon or URL to edit the complete Markdown source. Data URLs accept Base64 and percent encoding; local files use `file:///...` URLs. Relative paths, images mixed with text, and raw HTML remain editable source.

Formulas with missing glyphs, unresolved references, custom macro definitions or unsupported styling remain editable LaTeX source. This includes colored or boxed formulas, negative spacing and matrix rules that the current renderer cannot reproduce.

## Save and share

- **Keep the original formatting.** Saving preserves the file’s line endings. To tidy the Markdown itself, click **Format** in the shortcut group. You can undo the formatting in one step.
- **Export a PDF.** Choose **File → Export PDF** to save a paginated A4 copy with selectable body text and vector formulas. You can continue editing the original Markdown file afterward.
- **Recover an untitled draft.** In versions with draft recovery, reopen Ryne without opening a file after an interruption. If a draft is available, choose **Recover** to review it, then **File → Save** to keep it. **Later** leaves the draft for another time.
