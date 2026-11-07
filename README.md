# Ryne (如言) — Markdown that takes shape as you write

[简体中文](README.zh-CN.md)

Ryne is a native Markdown editor for Windows and macOS, built for everyday notes, technical documentation and long-form writing. Text wraps in a centered reading column so you can focus on the page.

Write directly in the document as you read it. Move the cursor into bold, italic or struck-through text, a link or a span of inline code to reveal its Markdown marks. Move away and the formatting returns. Click a heading to edit its marks. When you need to change the structure, you can open the source for a single block or the whole file.

- **Headings, lists and quotes.** Organize chapters with six heading levels. Write directly in numbered lists, bullet lists, task lists and nested quotes. Continue a list with Enter, adjust its indentation or click a checkbox to tick off a task.
- **Browse files and headings.** The first button in the toolbar’s right-hand group opens the sidebar. Under **Files**, expand folders or switch to the list to filter by name or relative path. Opening a document sets its parent folder as the new root. Use the icon at the top left of the sidebar to switch to **Outline**, then choose a heading to scroll there and keep your text selection.
- **Editable tables.** Click a cell to type and use the keyboard to move between cells. The cell menu lets you insert, move or delete rows and columns and set column alignment.
- **Readable code.** Use inline code for a command or a fenced block for a longer example. Press Enter after the opening fence to add its closing fence. Choose a language for syntax highlighting in Rust, Python, JavaScript, TypeScript, JSON or Bash.
- **Diagrams and formulas.** Preview Mermaid diagrams and inline LaTeX formulas alongside your text. Click either to edit its source. To change the formula typeface, choose **Ryne → Math font**.
- **Formatting and PDF export.** Tidy the Markdown in one undoable step. When you’re ready to share, export a paginated A4 PDF with your tables, formulas and Mermaid diagrams.
- **Local Markdown files.** Open and save ordinary `.md` files with the original indentation and line endings of untouched text. Select, copy or paste across lines, and undo or redo edits.

Ryne’s menus and editable welcome guide are available in Simplified Chinese, Traditional Chinese, English, Japanese, Korean, French and Spanish.

## Get Ryne

- **Windows 10 or 11 (64-bit x64).** Download the ZIP ending in `win-x86_64.zip`, extract it, and open `Ryne.exe` inside the `Ryne` folder. [Ryne downloads](https://github.com/tiyee/ryne/releases)
- **macOS.** Choose the ZIP ending in `mac-arm_64.zip` for Apple Silicon or `mac-x86_64.zip` for Intel. Extract it, drag `Ryne.app` to Applications, and open it. You can also open a Markdown file from Finder with **Open With → Ryne**. [Ryne downloads](https://github.com/tiyee/ryne/releases)

## Write your first note

1. Open Ryne and try editing the welcome guide. Choose **File → New** for a blank note, or **File → Open** to work on an existing one.
2. Type `# Meeting notes` and press **Enter**. Under the heading, type `- [ ] Send minutes`. Click the checkbox to mark the task done.
3. Choose **File → Save**, give the note a name such as `meeting.md`, and pick a folder. Next time, use **File → Open Recent** to find it.

To keep the original and save a second copy, choose **File → Save As**. You can also reload the file from disk in the File menu. Ryne asks before discarding unsaved edits.

To change the language of the menus and the untouched welcome guide, choose **Ryne → Language**. Ryne remembers your choice and keeps any edits you’ve made to the guide.

## Work with Markdown

| What you’re editing | What to do |
| --- | --- |
| Lists and quotes | Press **Enter** to continue a list or quote. Press **Enter** on an empty item or quote line to leave one level. In a list, **Tab** indents the current item and **Shift+Tab** moves it back. |
| Tables | Click a cell to edit it. **Tab** moves to the next cell, **Enter** moves down the column, and **Shift+Enter** adds a line within the cell. Open the cell’s **⋮** menu to add a row or change column alignment. |
| Code | Start a line with three backticks and a language such as `python`, then press **Enter**. Ryne adds the closing fence. Type your code between the fences, or choose a language from the menu above the code. |
| Diagrams and math | Use `mermaid` as the code block language for a diagram, or type a formula such as `$E=mc^2$` in a sentence. Move the cursor away to see the result. Click the diagram or formula to edit it again. |

To insert a table or another block without typing the syntax, hover just below the last line and click **+**. To insert one between two existing blocks, hover over the first block’s upper-left icon and choose **Add below**.

To edit the Markdown marks for a list, table or quote, choose **Edit block source** from that same menu. To see the whole file’s Markdown, click the **eye** in the toolbar. Click the **code** icon to return to the formatted view. Standalone images from HTTP(S), FTP, `file:` and `data:` URLs (PNG, JPEG, GIF first frame, WebP and SVG) render proportionally within the available text width, including in lists and quotes; click an image to edit its Markdown. Failed or oversized images show a broken-image icon followed by a red, struck-through URL; addresses longer than 256 characters end with an ellipsis. Click the icon or URL to edit the complete Markdown source. Data URLs accept Base64 and percent encoding; local files use `file:///...` URLs. Relative paths, images mixed with text, and raw HTML remain editable source.

## Save and share

- **Keep the original formatting.** Saving preserves the file’s line endings. To tidy the Markdown itself, click **Format** in the toolbar. You can undo the formatting in one step.
- **Export a PDF.** Choose **File → Export PDF** to save an A4 copy. Choose **Export PDF v2** if you want selectable body text and vector formulas. Both leave your Markdown file available for further editing.
- **Recover an untitled draft.** In versions with draft recovery, reopen Ryne without opening a file after an interruption. If a draft is available, choose **Recover** to review it, then **File → Save** to keep it. **Later** leaves the draft for another time.
