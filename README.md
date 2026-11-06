# Ryne (如言)

[简体中文](README.zh-CN.md)

Ryne is a Markdown editor for Windows and macOS. You write directly in the formatted document. Click a **bold phrase** to edit its `**` marks, or click a heading to change its level. When you move on, the marks fold away. Your notes are saved as ordinary `.md` files you can open in another editor.

## Get Ryne

- **Windows 10 or 11 (64-bit x64).** Download the ZIP ending in `win-x86_64.zip`, extract it, and open `Ryne.exe` inside the `Ryne` folder. The release page requires a GitHub account with access to the repository. [Ryne downloads](https://github.com/tiyee/ryne/releases)
- **macOS.** If you have a copy of `Ryne.app`, drag it to Applications and open it. You can also open a Markdown file from Finder with **Open With → Ryne**. The v0.2.6 download is for Windows only.

## Write your first note

1. Open Ryne and try editing the welcome guide. Choose **File → New** for a blank note, or **File → Open** to work on an existing one.
2. Type `# Meeting notes` and press **Enter**. Under the heading, type `- [ ] Send minutes`. Click the checkbox to mark the task done.
3. Choose **File → Save**, give the note a name such as `meeting.md`, and pick a folder. Next time, use **File → Open Recent** to find it.

To change the language of the menus and the untouched welcome guide, choose **Ryne → Language**.

## Work with Markdown

| What you’re editing | What to do |
| --- | --- |
| Lists and quotes | Press **Enter** to continue a list or quote. Press **Enter** on an empty item or quote line to leave one level. In a list, **Tab** indents the current item and **Shift+Tab** moves it back. |
| Tables | Click a cell to edit it. **Tab** moves to the next cell, **Enter** moves down the column, and **Shift+Enter** adds a line within the cell. Open the cell’s **⋮** menu to add a row or change column alignment. |
| Code | Start a line with three backticks and a language such as `python`, then press **Enter**. Ryne adds the closing fence. Type your code between the fences, or choose a language from the menu above the code. |
| Diagrams and math | Use `mermaid` as the code block language for a diagram, or type a formula such as `$E=mc^2$` in a sentence. Move the cursor away to see the result. Click the diagram or formula to edit it again. |

To insert a table or another block without typing the syntax, hover just below the last line and click **+**. To insert one between two existing blocks, hover over the first block’s upper-left icon and choose **Add below**.

To edit the Markdown marks for a list, table or quote, choose **Edit block source** from that same menu. To see the whole file’s Markdown, click the **eye** in the toolbar. Click the **code** icon to return to the formatted view. Images and raw HTML appear as editable source.

## Save and share

- **Keep the original formatting.** Saving preserves the file’s line endings. To tidy the Markdown itself, click **Format** in the toolbar. You can undo the formatting in one step.
- **Export a PDF.** Choose **File → Export PDF** to save an A4 copy. Choose **Export PDF v2** if you want selectable body text and vector formulas. Both leave your Markdown file available for further editing.
- **Recover an untitled draft.** In versions with draft recovery, reopen Ryne without opening a file after an interruption. If a draft is available, choose **Recover** to review it, then **File → Save** to keep it. **Later** leaves the draft for another time.
