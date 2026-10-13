use super::*;
pub(super) const SAMPLE: &str = "# Ryne（如言）\n\n点击 **粗体**、*斜体*、~~删除线~~、`inline code` 或 [链接](https://example.org)。\n\n光标进入时展开 Markdown 标记，离开后收起。\n试试 **粗体中的 *斜体***，也可以直接输入 Markdown。\n\n| 功能 | 状态 | 示例 |\n| :--- | :---: | ---: |\n| 表格编辑 | 可用 | `code` |\n| Tab 切换单元格 | Shift+Enter 换行 | 123 |\n\n- 无序列表：点击正文编辑\n  - 嵌套列表与 **粗体**\n- Enter 续项，空项 Enter 退出\n\n1. 有序列表\n2. Tab 缩进，Shift+Tab 反缩进\n\n```rust\nfn main() {\n    println!(\"Hello, Markdown\");\n}\n```\n\nEnter 分段 · Shift+Enter 硬换行\n支持跨行选择、粘贴和撤销，长段落随窗口宽度自动折行。\n\n```mermaid\nflowchart LR\n    A[打开文件] --> B[编辑 Markdown] --> C[保存]\n```\n\n行内公式：$E = mc^2$，以及 $\\frac{a_1}{\\sqrt{b}}$。\n\nCmd+O 打开 · Cmd+S 保存 · Cmd+Z 撤销\nCmd+Shift+M 切换全文源码\n\n";
impl Editor {
    pub(super) fn smoke_pointer(
        &mut self,
        target: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let geometry = self.geometry.as_ref().ok_or("missing pointer layout")?;
        let position = match target {
            0 => geometry.position(0) + point(px(5.), px(15.)),
            1 => {
                geometry.position(
                    self.display
                        .text
                        .find('\n')
                        .ok_or("missing paragraph boundary")?
                        + 1,
                ) + point(px(5.), px(8.))
            }
            _ => point(px(8.), px(8.)),
        };
        if target == 2 {
            self.editor_hover(&false, window, cx);
        } else {
            self.mouse_move(
                &MouseMoveEvent {
                    position,
                    ..Default::default()
                },
                window,
                cx,
            );
        }
        Ok(())
    }

    pub(super) fn start_smoke(
        &mut self,
        force_failure: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Never unwind a failed check through the native AppKit event loop.
        macro_rules! check {
            ($condition:expr $(, $message:literal)?) => {
                if !$condition {
                    return Err(format!("check failed: {}", stringify!($condition)));
                }
            };
        }
        macro_rules! check_eq {
            ($left:expr, $right:expr $(, $message:literal)?) => {
                match (&$left, &$right) {
                    (left, right) if *left != *right => {
                        return Err(format!(
                            "check failed: {} != {}",
                            stringify!($left),
                            stringify!($right)
                        ));
                    }
                    _ => {}
                }
            };
        }
        self.smoke_mode = true;
        let blink_seen = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = blink_seen.clone();
        let blink_observer = cx.observe_self(move |editor, _| {
            if !editor.caret_visible {
                observed.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let _blink_observer = blink_observer;
            let mut typography_anchor: Option<(usize, Pixels)> = None;
            for stage in 0..196 {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(500))
                    .await;
                if stage >= 44 {
                    for _ in 0..40 {
                        let pending = this.update_in(cx, |editor, _, _| editor.document.jobs.pending()).unwrap_or(false);
                        if !pending { break; }
                        cx.background_executor().timer(std::time::Duration::from_millis(100)).await;
                    }
                    cx.background_executor().timer(std::time::Duration::from_millis(100)).await;
                }
                // Hover callbacks correctly reject stale layout. A background
                // macOS window can defer native repaint; draw the actual window
                // outside the editor borrow before using its source geometry.
                for _ in 0..40 {
                    if let Err(error) = cx.update(|window, cx| {
                        window.refresh();
                        window.draw(cx).clear();
                    }) {
                        eprintln!("GUI smoke redraw failed at stage {stage}: {error}");
                        std::process::exit(1);
                    }
                    let ready = this.update_in(cx, |editor, window, cx| {
                        let ready = editor.geometry.as_ref().is_some_and(|geometry| {
                            geometry.snapshot == editor.document.source.id()
                                && geometry.display.text == editor.display.text
                                && geometry.display.spans == editor.display.spans
                                && geometry.source_mode == editor.document.source_mode
                                && geometry.bounds.size.width == window.viewport_size().width
                        });
                        if !ready { cx.notify(); }
                        ready
                    }).unwrap_or(false);
                    if ready { break; }
                    cx.background_executor().timer(std::time::Duration::from_millis(50)).await;
                }
                let result = this.update_in(cx, |editor, window, cx| -> Result<(), String> {
                    if stage % 10 == 0 || stage >= 80 {
                        eprintln!("GUI smoke stage {stage}");
                    }
                    let geometry = editor.geometry.as_ref().ok_or("native frame was not laid out")?.clone();
                    check_eq!(geometry.snapshot, editor.document.source.id());
                    check_eq!(geometry.display.text, editor.display.text);
                    check!(!geometry.lines.is_empty());
                    if stage == 0 {
                        check!(!force_failure, "injected smoke failure");
                        // Establish a folded starting frame before checking interaction.
                        editor.document.clear_heading_reveal();
                        editor.refresh(cx);
                        return Ok(());
                    }
                    match stage - 1 {
                        0 => {
                            check_eq!(geometry.lines[0].shaped.unwrapped_layout.font_size, px(32.));
                            check!(geometry.lines[0].line_height >= px(44.));
                            check!(editor.display.text.starts_with("Ryne（如言）\n"));
                            check_eq!(editor.document.head, 2);
                            check!(!editor.display.text.contains("**粗体**"));
                            let source = editor.document.source.text().find("粗体").ok_or("missing bold sample")?;
                            let position = geometry.position(editor.display.position_at(source));
                            editor.mouse_down(
                                &MouseDownEvent {
                                    button: MouseButton::Left,
                                    position: position + point(px(1.), px(8.)),
                                    click_count: 1,
                                    ..Default::default()
                                },
                                window,
                                cx,
                            );
                            editor.dragging = false;
                            check_eq!(editor.document.head, source);
                            check!(editor.display.text.contains("**粗体**"));
                            check_eq!(editor.document.source.text(), SAMPLE);
                        }
                        1 => {
                            let offset = editor.document.head;
                            let position = geometry.position(editor.display.position_at(offset));
                            check_eq!(geometry.hit_caret(position).0, editor.display.position_at(offset));
                            editor.move_to(2, false, cx);
                            editor.replace_text_in_range(None, "新", window, cx);
                            check!(editor.document.source.text().starts_with("# 新Ryne"));
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), SAMPLE);
                            editor.move_to(offset, false, cx);
                            editor.replace_text_in_range(None, "测试😀", window, cx);
                            check!(editor.document.source.text().contains("**测试😀粗体**"));
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), SAMPLE);
                            editor.replace_and_mark_text_in_range(None, "zhong", Some(5..5), window, cx);
                            editor.replace_and_mark_text_in_range(None, "中", Some(1..1), window, cx);
                            check_eq!(editor.document.source.text(), SAMPLE);
                            editor.escape(&Escape, window, cx);
                            check_eq!(editor.document.source.text(), SAMPLE);
                            editor.replace_and_mark_text_in_range(None, "zhong", Some(5..5), window, cx);
                            editor.replace_text_in_range(None, "中", window, cx);
                            check!(editor.document.source.text().contains("**中粗体**"));
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), SAMPLE);
                        }
                        2 => {
                            editor.move_to(0, false, cx);
                            check!(!editor.display.text.contains("**粗体**"));
                            let revision = editor.document.source.revision();
                            editor.toggle_source(&ToggleSource, window, cx);
                            check_eq!(editor.display.text, SAMPLE);
                            editor.toggle_source(&ToggleSource, window, cx);
                            check_eq!(editor.document.source.revision(), revision);
                            let path = std::env::temp_dir().join(format!("ryne-smoke-{}.md", std::process::id()));
                            editor.document.save(&path).map_err(|error| error.to_string())?;
                            let reopened = editor.document.open_with(&path).map_err(|error| error.to_string())?;
                            check_eq!(reopened.source.text(), SAMPLE);
                            std::fs::remove_file(path).map_err(|error| error.to_string())?;
                            editor.document = editor.document.fork("**ab**\r\n");
                            editor.move_to(6, false, cx);
                            editor.backspace(&Backspace, window, cx);
                            check_eq!(editor.document.source.text(), "**a**\r\n");
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), "**ab**\r\n");
                            check_eq!(editor.document.head, 6);
                            editor.move_to(0, false, cx);
                            editor.delete(&Delete, window, cx);
                            check_eq!(editor.document.source.text(), "**b**\r\n");
                            editor.document = editor.document.fork("");
                            editor.move_to(0, false, cx);
                        }
                        3 => {
                            editor.replace_text_in_range(None, "**新文档**", window, cx);
                            check_eq!(editor.document.source.text(), "**新文档**");
                            editor.document = editor.document.fork("# **标题** #\r\n\r\n## 二级\n\n### 三级\n\n#### 四级\n\n##### 五级\n\n###### 六级\n");
                            editor.move_to(0, false, cx);
                        }
                        4 => {
                            for (word, level) in [("标题", 1), ("二级", 2), ("三级", 3), ("四级", 4), ("五级", 5), ("六级", 6)] {
                                let line = geometry.lines.iter().find(|line| line.shaped.text.contains(word)).ok_or("heading was not laid out")?;
                                let (font_size, height) = heading_metrics(level);
                                check_eq!(line.shaped.unwrapped_layout.font_size, px(font_size));
                                check!(line.line_height >= px(height));
                            }
                            let position = geometry.position(0);
                            editor.mouse_down(&MouseDownEvent {
                                button: MouseButton::Left, position: position + point(px(1.), px(15.)),
                                click_count: 1, ..Default::default()
                            }, window, cx);
                            editor.dragging = false;
                            check_eq!(editor.document.head, editor.document.source.text().find("标题").ok_or("missing heading sample")?);
                            check!(editor.display.text.starts_with("# **标题** #\n"));
                            editor.toggle_source(&ToggleSource, window, cx);
                        }
                        5 => {
                            check_eq!(geometry.lines[0].shaped.unwrapped_layout.font_size, px(theme::CODE.0));
                            check!(geometry.lines[0].line_height >= px(theme::CODE.1));
                            check!(editor.display.text.starts_with("# **标题** #\n"));
                            editor.toggle_source(&ToggleSource, window, cx);
                            check!(editor.display.text.starts_with("**标题**\n"));
                            editor.document = editor.document.fork("# 标题 #\r\n\r\n正文");
                            editor.move_to(0, false, cx);
                        }
                        6 => {
                            editor.smoke_pointer(0, window, cx)?;
                            check!(editor.document.heading_reveal.is_none(), "hover must keep heading markers folded");
                            check!(editor.display.text.starts_with("标题\n"));
                            check_eq!(editor.document.head, 2);
                            check!(!editor.document.dirty());
                        }
                        7 => {
                            let position = geometry.position(0) + point(px(1.), px(15.));
                            editor.mouse_down(&MouseDownEvent {
                                button: MouseButton::Left, position, click_count: 1, ..Default::default()
                            }, window, cx);
                            editor.dragging = false;
                            check_eq!(editor.document.head, 2, "click resolves against the folded heading");
                            check!(editor.document.heading_reveal.is_some(), "click reveals heading markers");
                            check!(editor.display.text.starts_with("# 标题 #\n"));
                        }
                        8 => {
                            let position = geometry.position(0) + point(px(1.), px(15.));
                            editor.mouse_down(&MouseDownEvent {
                                button: MouseButton::Left, position, click_count: 1, ..Default::default()
                            }, window, cx);
                            editor.dragging = false;
                            check_eq!(editor.document.head, 0, "revealed heading marker can be selected");
                            editor.smoke_pointer(1, window, cx)?;
                            check!(editor.document.heading_reveal.is_some(), "moving the pointer away keeps clicked heading editable");
                            editor.smoke_pointer(2, window, cx)?;
                        }
                        9 => {
                            check!(editor.document.heading_reveal.is_some(), "leaving editor must not fold a clicked heading");
                            let position = geometry.position(editor.display.position_at(editor.document.source.text().find("正文").ok_or("missing body")?)) + point(px(1.), px(8.));
                            editor.mouse_down(&MouseDownEvent {
                                button: MouseButton::Left, position, click_count: 1, ..Default::default()
                            }, window, cx);
                            editor.dragging = false;
                            check!(editor.document.heading_reveal.is_none(), "clicking another block folds heading markers");
                            check!(editor.display.text.starts_with("标题\n"));
                            check_eq!(editor.document.source.text(), "# 标题 #\r\n\r\n正文");
                            check!(!editor.document.dirty());
                            editor.document = editor.document.fork("abcd\r\n\r\ntail");
                            editor.move_to(2, false, cx);
                        }
                        10 => {
                            editor.enter(&Enter, window, cx);
                            check_eq!(editor.document.source.text(), "ab\r\n\r\ncd\r\n\r\ntail");
                            editor.backspace(&Backspace, window, cx);
                            check_eq!(editor.document.source.text(), "abcd\r\n\r\ntail");
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), "ab\r\n\r\ncd\r\n\r\ntail");
                            editor.undo(&Undo, window, cx);
                            editor.hard_break(&HardBreak, window, cx);
                            check_eq!(editor.document.source.text(), "ab  \r\ncd\r\n\r\ntail");
                            editor.undo(&Undo, window, cx);
                            editor.toggle_source(&ToggleSource, window, cx);
                            editor.enter(&Enter, window, cx);
                            check_eq!(editor.document.source.text(), "ab\r\ncd\r\n\r\ntail");
                            editor.undo(&Undo, window, cx);
                            editor.replace_and_mark_text_in_range(None, "中", Some(1..1), window, cx);
                            editor.enter(&Enter, window, cx);
                            check_eq!(editor.document.source.text(), "ab中cd\r\n\r\ntail");
                            editor.undo(&Undo, window, cx);
                            editor.document = editor.document.fork("0123456789abcdef\nx\n0123456789abcdef\n");
                            editor.document.set_source_mode(true);
                            editor.move_to(8, false, cx);
                        }
                        11 => {
                            editor.down(&Down, window, cx);
                            check_eq!(editor.document.head, 18);
                            editor.down(&Down, window, cx);
                            check_eq!(editor.document.head, 27);
                            editor.up(&Up, window, cx);
                            editor.up(&Up, window, cx);
                            check_eq!(editor.document.head, 8);
                            editor.select_down(&SelectDown, window, cx);
                            editor.select_down(&SelectDown, window, cx);
                            check_eq!(editor.document.selection(), 8..27);
                            let quads = geometry.selection_quads(8..27, rgb(0xabcdef).into());
                            check_eq!(quads.len(), 3);
                            check!(quads.windows(2).all(|pair| pair[0].bounds.origin.y < pair[1].bounds.origin.y));
                            editor.backspace(&Backspace, window, cx);
                            check_eq!(editor.document.source.text(), "0123456789abcdef\n");
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.selection(), 8..27);
                            editor.document = editor.document.fork(&"自动折行 😀 e\u{301} Markdown text wraps across visual rows. ".repeat(12));
                            editor.document.set_source_mode(true);
                            editor.move_to(0, false, cx);
                        }
                        12 => {
                            check!(!geometry.display.text.contains('\n'));
                            check_eq!(geometry.rows.first().ok_or("missing wrapped start")?.range.start, 0);
                            check_eq!(geometry.rows.last().ok_or("missing wrapped end")?.range.end, geometry.display.text.len());
                            check!(geometry.rows.len() > 2);
                            check!(!editor.document.dirty(), "visual wrapping must not insert source newlines");
                            let first = geometry.rows.first().ok_or("missing first visual row")?;
                            let second = geometry.rows.get(1).ok_or("missing second visual row")?;
                            let boundary = first.range.end;
                            check_eq!(boundary, second.range.start);
                            let previous = geometry.position_with_affinity(boundary, Affinity::Upstream);
                            let next = geometry.position_with_affinity(boundary, Affinity::Downstream);
                            check!(previous.y < next.y);
                            check_eq!(geometry.hit_caret(previous + point(px(0.), px(8.))).0, boundary);
                            check_eq!(geometry.hit_caret(next + point(px(0.), px(8.))).0, boundary);
                            for row in &geometry.rows {
                                check_eq!(geometry.display.snap(row.range.start), row.range.start);
                                check_eq!(geometry.display.snap(row.range.end), row.range.end);
                            }
                            editor.move_to(boundary, false, cx);
                            editor.end(&End, window, cx);
                            check_eq!(editor.document.head, second.range.end);
                            check_eq!(editor.caret_affinity, Affinity::Upstream);
                            editor.home(&Home, window, cx);
                            check_eq!(editor.document.head, second.range.start);
                            editor.select_end(&SelectEnd, window, cx);
                            check_eq!(editor.document.selection(), second.range.clone());
                            let quads = geometry.selection_quads(first.range.start..second.range.end, rgb(0xabcdef).into());
                            check_eq!(quads.len(), 2);
                            check!(quads.iter().all(|quad| quad.bounds.size.width <= geometry.bounds.size.width - px(geometry.page.top)));
                            editor.document = editor.document.fork("前言 **第一行\n第二行** 后记\n\n结束");
                            editor.move_to(editor.document.source.text().find("第一行").ok_or("missing inline sample")?, false, cx);
                        }
                        13 => {
                            check!(editor.display.text.contains("**第一行\n第二行**"));
                            let start = editor.document.head;
                            let end = editor.document.source.text().find("** 后记").ok_or("missing inline end")?;
                            let point = geometry.position(geometry.display.position_at(end)) + point(px(0.), px(8.));
                            editor.dragging = true;
                            editor.mouse_move(&MouseMoveEvent { position: point, ..Default::default() }, window, cx);
                            editor.dragging = false;
                            check_eq!(editor.document.selection(), start..end);
                            check_eq!(geometry.selection_quads(geometry.display.position_at(start)..geometry.display.position_at(end), rgb(0xabcdef).into()).len(), 2);
                            editor.replace_text_in_range(None, "替换\r\n多行😀", window, cx);
                            check_eq!(editor.document.source.text(), "前言 **替换\r\n多行😀** 后记\n\n结束");
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.selection(), start..end);
                            editor.redo(&Redo, window, cx);
                            let path = std::env::temp_dir().join(format!("ryne-multiline-smoke-{}.md", std::process::id()));
                            editor.document.save(&path).map_err(|error| error.to_string())?;
                            let reopened = editor.document.open_with(&path).map_err(|error| error.to_string())?;
                            check_eq!(reopened.source.text(), editor.document.source.text());
                            std::fs::remove_file(path).map_err(|error| error.to_string())?;
                        }
                        14 => {
                            check_eq!(geometry.display.text, editor.display.text);
                            editor.document = editor.document.fork("before\n\n```rust\r\n  **中😀**\r\n  second\r\n```\r\n\r\nafter");
                            editor.move_to(0, false, cx);
                        }
                        15 => {
                            check_eq!(geometry.display.code_blocks.len(), 1);
                            check_eq!(geometry.display.code_blocks[0].source.language, "rust");
                            check!(geometry.display.text.contains("**中😀**"));
                            check!(!geometry.display.text.contains("```"));
                            let line = geometry.lines.iter().find(|line| line.shaped.text.contains("中😀")).ok_or("missing native code row")?;
                            check_eq!(line.code, Some(0));
                            check!(line.line_height >= px(theme::CODE.1));
                            check_eq!(line.inset, px(16.));
                            check_eq!(line.shaped.unwrapped_layout.font_size, px(theme::CODE.0));
                            let offset = editor.document.source.text().find("中😀").ok_or("missing code body")?;
                            let position = geometry.position(geometry.display.position_at(offset)) + point(px(0.), px(8.));
                            editor.mouse_down(&MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }, window, cx);
                            editor.dragging = false;
                            check_eq!(editor.document.head, offset);
                            let original = editor.document.source.text().to_owned();
                            editor.replace_and_mark_text_in_range(None, "文", Some(1..1), window, cx);
                            editor.enter(&Enter, window, cx);
                            check!(editor.document.source.text().contains("**文中😀**"));
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), original);
                            let anchor = editor.display.code_blocks[0].source.anchor;
                            editor.move_to(anchor, false, cx);
                            editor.backspace(&Backspace, window, cx);
                            check_eq!(editor.document.source.text(), original);
                            editor.move_to(offset, false, cx);
                            editor.enter(&Enter, window, cx);
                            check!(editor.document.source.text().contains("**\r\n  中😀**"));
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), original);
                            let revision = editor.document.source.revision();
                            editor.toggle_block_source(&ToggleBlockSource, window, cx);
                            check_eq!(editor.document.source.revision(), revision);
                            check!(editor.document.block_source.is_some());
                        }
                        16 => {
                            check!(geometry.display.code_blocks.is_empty());
                            check!(geometry.display.text.contains("```rust"));
                            let offset = editor.document.source.text().find("rust").ok_or("missing code language")?;
                            editor.move_to(offset, false, cx);
                            editor.move_to(offset + 4, true, cx);
                            editor.replace_text_in_range(None, "python", window, cx);
                            editor.toggle_block_source(&ToggleBlockSource, window, cx);
                            check!(editor.document.block_source.is_none());
                        }
                        17 => {
                            check_eq!(geometry.display.code_blocks[0].source.language, "python");
                            check!(!geometry.display.text.contains("```"));
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.display.code_blocks[0].source.language, "rust");
                            editor.document = editor.document.fork("```rust\n```");
                            editor.move_to(0, false, cx);
                        }
                        18 => {
                            check_eq!(geometry.display.text, "\n");
                            check_eq!(geometry.lines[0].code, Some(0));
                            let position = geometry.position(0) + point(px(0.), px(8.));
                            editor.mouse_down(&MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }, window, cx);
                            editor.dragging = false;
                            check_eq!(editor.document.head, "```rust\n".len());
                            editor.replace_text_in_range(None, "😀", window, cx);
                            check_eq!(editor.document.source.text(), "```rust\n😀\n```");
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), "```rust\n```");
                            editor.redo(&Redo, window, cx);
                            check_eq!(editor.document.source.text(), "```rust\n😀\n```");
                            editor.document = editor.document.fork("> ```\n> ```\n");
                            editor.move_to(0, false, cx);
                        }
                        19 => {
                            check_eq!(geometry.display.code_blocks.len(), 1);
                            check!(geometry.lines[0].inset > px(16.));
                            editor.replace_text_in_range(None, "中\n文😀", window, cx);
                            check_eq!(editor.document.source.text(), "> ```\n> 中\n> 文😀\n> ```\n");
                            let path = std::env::temp_dir().join(format!("ryne-code-smoke-{}.md", std::process::id()));
                            editor.document.save(&path).map_err(|error| error.to_string())?;
                            let reopened = editor.document.open_with(&path).map_err(|error| error.to_string())?;
                            check_eq!(reopened.source.text(), editor.document.source.text());
                            std::fs::remove_file(path).map_err(|error| error.to_string())?;
                            editor.toggle_source(&ToggleSource, window, cx);
                        }
                        20 => {
                            check!(geometry.display.text.contains("> ```"));
                            check_eq!(geometry.display.text, editor.document.source.text());
                            editor.document = editor.document.fork("3) 中😀\r\n1) second\r\n   - nested\r\n     continuation\r\n\r\n- bullet\r\n");
                            editor.move_to(0, false, cx);
                        }
                        21 => {
                            let labels = geometry.display.lists.iter().filter(|v| v.source.first).map(|v| v.source.item.label.as_str()).collect::<Vec<_>>();
                            check_eq!(labels, vec!["3.", "4.", "•", "•"]);
                            check!(!geometry.display.text.contains("3)"));
                            let first = geometry.lines.iter().find(|l| l.shaped.text.contains("中😀")).ok_or("missing list body")?;
                            let nested = geometry.lines.iter().find(|l| l.shaped.text.contains("nested")).ok_or("missing nested list")?;
                            let continuation = geometry.lines.iter().find(|l| l.shaped.text.contains("continuation")).ok_or("missing continuation")?;
                            check_eq!(first.inset, px(32.));
                            check!(nested.inset > first.inset);
                            check_eq!(nested.inset, continuation.inset);
                            // Click in the marker column: the caret still belongs to the editable body.
                            let position = geometry.position(first.start) + point(px(-12.), px(8.));
                            editor.mouse_down(&MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }, window, cx);
                            editor.dragging = false;
                            check_eq!(editor.document.head, 3);
                            let original = editor.document.source.text().to_owned();
                            editor.replace_and_mark_text_in_range(None, "文", Some(1..1), window, cx);
                            editor.enter(&Enter, window, cx);
                            check!(editor.document.source.text().starts_with("3) 文中😀"));
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), original);
                            let second = original.find("second").ok_or("missing ordered body")?;
                            editor.move_to(second, false, cx);
                            editor.tab(&Tab, window, cx);
                            check!(editor.document.source.text().contains("\r\n   1) second"));
                            editor.outdent(&Outdent, window, cx);
                            check_eq!(editor.document.source.text(), original);
                            editor.enter(&Enter, window, cx);
                            check!(editor.document.source.text().contains("5) second"));
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), original);
                            let revision = editor.document.source.revision();
                            editor.toggle_block_source(&ToggleBlockSource, window, cx);
                            check_eq!(editor.document.source.revision(), revision);
                        }
                        22 => {
                            check!(geometry.display.text.starts_with("3) 中😀"));
                            check_eq!(geometry.display.lists.len(), 1);
                            editor.move_to(0, false, cx);
                            editor.move_to(1, true, cx);
                            editor.replace_text_in_range(None, "7", window, cx);
                            editor.toggle_block_source(&ToggleBlockSource, window, cx);
                        }
                        23 => {
                            check_eq!(geometry.display.lists[0].source.item.label, "7.");
                            check_eq!(geometry.display.lists[1].source.item.label, "8.");
                            let raw = format!("- parent\n  - {}\n- end", "中😀 wrapped words ".repeat(24));
                            editor.document = editor.document.fork(&raw);
                            editor.move_to(raw.find("中😀").ok_or("missing wrapped body")?, false, cx);
                        }
                        24 => {
                            let line = geometry.lines.iter().find(|l| l.shaped.text.contains("wrapped")).ok_or("missing wrapped list")?;
                            let end = geometry.display.text[line.start..].find('\n').map_or(geometry.display.text.len(), |end| line.start + end);
                            let rows = geometry.rows.iter().filter(|row| line.start <= row.range.start && row.range.end <= end).collect::<Vec<_>>();
                            check!(rows.len() > 1);
                            check_eq!(line.inset, px(64.));
                            let position = geometry.position_with_affinity(rows[1].range.start, Affinity::Downstream);
                            check_eq!(position.x, geometry.bounds.left() + px(geometry.page.left) + line.inset);
                            let (hit, _) = geometry.hit_caret(position + point(px(0.), px(8.)));
                            check_eq!(hit, rows[1].range.start);
                            let original = editor.document.source.text().to_owned();
                            let start = original.find("中😀").ok_or("missing list selection")?;
                            editor.move_to(start, false, cx);
                            editor.move_to(original.len(), true, cx);
                            editor.replace_text_in_range(None, "replacement", window, cx);
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), original);
                            let path = std::env::temp_dir().join(format!("ryne-list-smoke-{}.md", std::process::id()));
                            editor.document.save(&path).map_err(|e| e.to_string())?;
                            check_eq!(editor.document.open_with(&path).map_err(|e| e.to_string())?.source.text(), original);
                            std::fs::remove_file(path).map_err(|e| e.to_string())?;
                            editor.document = editor.document.fork("- ");
                            editor.move_to(0, false, cx);
                        }
                        25 => {
                            check_eq!(geometry.display.lists.len(), 1);
                            check_eq!(geometry.lines[0].inset, px(32.));
                            let position = geometry.position(0) + point(px(0.), px(8.));
                            editor.mouse_down(&MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }, window, cx);
                            editor.dragging = false;
                            check_eq!(editor.document.head, 2);
                            editor.replace_text_in_range(None, "😀", window, cx);
                            editor.enter(&Enter, window, cx);
                            check_eq!(editor.document.source.text(), "- 😀\n- ");
                            editor.enter(&Enter, window, cx);
                            check_eq!(editor.document.source.text(), "- 😀\n");
                            editor.undo(&Undo, window, cx);
                            editor.undo(&Undo, window, cx);
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), "- ");
                        }
                        26 => {
                            check_eq!(geometry.display.lists.len(), 1);
                            let raw = format!("| A | B | C |\r\n| :--- | :---: | ---: |\r\n| 中😀 | center | 123 |\r\n| {}<br>second | short | |\r\n\r\nafter", "wrapped words ".repeat(18));
                            editor.document = editor.document.fork(&raw);
                            editor.move_to(0, false, cx);
                        }
                        27 => {
                            check_eq!(geometry.display.tables.len(), 1);
                            let table = &geometry.display.tables[0];
                            check_eq!(table.rows.len(), 3);
                            check!(!geometry.display.text.contains(":---"));
                            let right = geometry.row_for(table.rows[1][2].start, Affinity::Downstream).ok_or("missing right cell")?;
                            let line = &geometry.lines[right.line];
                            let cell = line.cell.ok_or("missing cell rectangle")?;
                            check_eq!(line.align, TextAlign::Right);
                            check!(right.align_offset > px(0.));
                            let position = geometry.position(table.rows[1][2].start) + point(px(0.), px(8.));
                            editor.mouse_down(&MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }, window, cx);
                            editor.dragging = false;
                            check_eq!(editor.document.head, table.source.rows[1].cells[2].content.start);
                            // Header padding must select its column even above the first text baseline.
                            let header = geometry.row_for(table.rows[0][2].start, Affinity::Downstream).ok_or("missing header cell")?;
                            let header_cell = geometry.lines[header.line].cell.ok_or("missing header rectangle")?;
                            let position = geometry.bounds.origin + point(px(geometry.page.left) + header_cell.left()+px(2.), px(geometry.page.top)+header_cell.top()+px(2.)-geometry.scroll);
                            let (hit, _) = geometry.hit_caret(position);
                            check!(table.rows[0][2].start <= hit && hit <= table.rows[0][2].end);
                            let long = geometry.row_for(table.rows[2][0].start, Affinity::Downstream).ok_or("missing wrapped cell")?;
                            let short = geometry.row_for(table.rows[2][1].start, Affinity::Downstream).ok_or("missing short cell")?;
                            let short_cell = geometry.lines[short.line].cell.ok_or("missing tall cell rectangle")?;
                            check!(short_cell.size.height > px(theme::TABLE.1 + 16.));
                            check_eq!(short_cell.size.height, geometry.lines[long.line].cell.ok_or("missing long cell rectangle")?.size.height);
                            let position = geometry.bounds.origin + point(px(geometry.page.left)+short_cell.right()-px(2.), px(geometry.page.top)+short_cell.bottom()-px(2.)-geometry.scroll);
                            let (hit, _) = geometry.hit_caret(position);
                            check_eq!(hit, table.rows[2][1].end);
                            check!(cell.right() > header_cell.left());
                            editor.move_to(table.source.rows[1].cells[1].content.start, false, cx);
                            editor.down(&Down, window, cx);
                            check!(table.source.rows[2].cells[1].content.start <= editor.document.head && editor.document.head <= table.source.rows[2].cells[1].content.end);
                        }
                        28 => {
                            let original = editor.document.source.text().to_owned();
                            let caret = original.find('😀').ok_or("missing table Unicode")?;
                            let position = geometry.position(geometry.display.position_at(caret)) + point(px(0.), px(8.));
                            editor.mouse_down(&MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }, window, cx);
                            editor.dragging = false;
                            if editor.document.head != caret {
                                return Err(format!("table click expected {caret}, got {}, point {position:?}, hit {:?}", editor.document.head, geometry.hit_caret(position)));
                            }
                            editor.replace_and_mark_text_in_range(None, "文", Some(1..1), window, cx);
                            editor.enter(&Enter, window, cx);
                            check!(editor.document.source.text().contains("中文😀"));
                            check_eq!(editor.display.tables[0].rows.len(), 3);
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), original);
                            editor.replace_text_in_range(None, "|", window, cx);
                            check!(editor.document.source.text().contains("中\\|😀"));
                            check_eq!(editor.display.tables.len(), 1);
                            editor.undo(&Undo, window, cx);
                            editor.hard_break(&HardBreak, window, cx);
                            check!(editor.document.source.text().contains("中<br>😀"));
                            check!(editor.display.text.contains("中\n😀"));
                            editor.undo(&Undo, window, cx);
                            let table = &editor.display.tables[0];
                            let anchor = table.source.rows[2].cells[1].content.start;
                            editor.move_to(anchor, false, cx);
                            editor.enter(&Enter, window, cx);
                            check_eq!(editor.display.tables[0].rows.len(), 4);
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), original);
                            editor.redo(&Redo, window, cx);
                            check_eq!(editor.display.tables[0].rows.len(), 4);
                            editor.undo(&Undo, window, cx);
                            let revision = editor.document.source.revision();
                            editor.toggle_block_source(&ToggleBlockSource, window, cx);
                            check_eq!(editor.document.source.revision(), revision);
                        }
                        29 => {
                            check!(geometry.display.tables.is_empty());
                            check!(geometry.display.text.contains(":---:"));
                            let at = editor.document.source.text().find(":---:").ok_or("missing alignment source")?;
                            editor.move_to(at, false, cx);
                            editor.move_to(at+5, true, cx);
                            editor.replace_text_in_range(None, "---:", window, cx);
                            editor.toggle_block_source(&ToggleBlockSource, window, cx);
                        }
                        30 => {
                            check_eq!(geometry.display.tables[0].source.align[1], editable_markdown_parser::Alignment::Right);
                            let offset = geometry.display.tables[0].rows[1][1].start;
                            let row = geometry.row_for(offset, Affinity::Downstream).ok_or("missing aligned table row")?;
                            check_eq!(geometry.lines[row.line].align, TextAlign::Right);
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.display.tables[0].source.align[1], editable_markdown_parser::Alignment::Center);
                            editor.document = editor.document.fork("| | |\n| --- | --- |\n| | |\n");
                            editor.move_to(0, false, cx);
                        }
                        31 => {
                            check_eq!(geometry.display.text, "\t\n\t\n");
                            let table = &geometry.display.tables[0];
                            let anchor = table.source.rows[0].cells[1].content.start;
                            let position = geometry.position(table.rows[0][1].start) + point(px(0.), px(8.));
                            editor.mouse_down(&MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }, window, cx);
                            editor.dragging = false;
                            check_eq!(editor.document.head, anchor);
                            let original = editor.document.source.text().to_owned();
                            editor.replace_text_in_range(None, "😀", window, cx);
                            check!(editor.display.text.starts_with("\t😀\n"));
                            check_eq!(editor.display.tables.len(), 1);
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), original);
                            let anchor = editor.display.tables[0].source.rows[1].cells[1].content.start;
                            editor.move_to(anchor, false, cx);
                            editor.tab(&Tab, window, cx);
                            check_eq!(editor.display.tables[0].rows.len(), 3);
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), original);
                            editor.document = editor.document.fork("> - | A | B |\n>   | --- | --- |\n>   | x | y |\n");
                            editor.move_to(0, false, cx);
                        }
                        32 => {
                            check_eq!(geometry.display.tables.len(), 1);
                            let table = &geometry.display.tables[0];
                            let line = geometry.row_for(table.rows[0][0].start, Affinity::Downstream).ok_or("missing nested table")?;
                            check!(geometry.lines[line.line].inset > px(32.));
                            let original = editor.document.source.text().to_owned();
                            let anchor = table.source.rows[1].cells[1].content.start;
                            editor.move_to(anchor, false, cx);
                            editor.tab(&Tab, window, cx);
                            check_eq!(editor.document.source.text(), format!("{original}>   |  |  |\n"));
                            editor.replace_text_in_range(None, "中\n文|😀", window, cx);
                            check!(editor.document.source.text().contains("中<br>文\\|😀"));
                            check_eq!(editor.display.tables.len(), 1);
                            let path = std::env::temp_dir().join(format!("ryne-table-smoke-{}.md", std::process::id()));
                            editor.document.save(&path).map_err(|e| e.to_string())?;
                            check_eq!(editor.document.open_with(&path).map_err(|e| e.to_string())?.source.text(), editor.document.source.text());
                            std::fs::remove_file(path).map_err(|e| e.to_string())?;
                            editor.undo(&Undo, window, cx);
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), original);
                            editor.toggle_block_source(&ToggleBlockSource, window, cx);
                            check_eq!(editor.document.block_source.as_ref().ok_or("missing table source scope")?.range.start, 0);
                            editor.toggle_source(&ToggleSource, window, cx);
                        }
                        33 => {
                            if !geometry.display.tables.is_empty() {
                                return Err(format!("table source frame stale: source_mode={}, document_tables={}, geometry_tables={}, source_equal={}", editor.document.source_mode, editor.display.tables.len(), geometry.display.tables.len(), geometry.display.text == editor.display.text));
                            }
                            check_eq!(geometry.display.text, editor.document.source.text());
                            editor.escape(&Escape, window, cx);
                            check_eq!(editor.display.tables.len(), 1);
                            editor.document = editor.document.fork("```rust\r\nfn main() {\r\n    let s = \"中😀\";\r\n}\r\n```\r\n");
                            editor.move_to(0, false, cx);
                        }
                        34 => {
                            use ryne_projection::Class;
                            check_eq!(geometry.display.code_blocks.len(), 1);
                            check!(geometry.display.spans.iter().any(|span| span.style.syntax == Some(Class::Keyword) && geometry.display.text[span.display.clone()].contains("fn")));
                            check!(geometry.display.spans.iter().any(|span| span.style.syntax == Some(Class::String) && geometry.display.text[span.display.clone()].contains("中😀")));
                            let offset = editor.document.source.text().find("中😀").ok_or("missing highlighted Unicode string")?;
                            let position = geometry.position(geometry.display.position_at(offset)) + point(px(0.), px(8.));
                            editor.mouse_down(&MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }, window, cx);
                            editor.dragging = false;
                            check_eq!(editor.document.head, offset);
                            let original = editor.document.source.text().to_owned();
                            editor.move_to(offset - 1, false, cx);
                            editor.move_to(offset + "中😀\"".len(), true, cx);
                            editor.replace_text_in_range(None, "42", window, cx);
                            check!(editor.display.spans.iter().any(|span| span.style.syntax == Some(Class::Constant)));
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), original);
                            check!(!editor.document.dirty());
                            let revision = editor.document.source.revision();
                            editor.toggle_source(&ToggleSource, window, cx);
                            check_eq!(editor.document.source.revision(), revision);
                        }
                        35 => {
                            check_eq!(geometry.display.text, editor.document.source.text().replace("\r\n", "\n"));
                            check!(geometry.display.spans.iter().any(|span| span.style.syntax == Some(ryne_projection::Class::String)));
                            editor.move_to(3, false, cx);
                            editor.move_to(7, true, cx);
                            editor.replace_text_in_range(None, "unknown", window, cx);
                            editor.toggle_source(&ToggleSource, window, cx);
                        }
                        36 => {
                            check_eq!(geometry.display.code_blocks[0].source.language, "unknown");
                            check!(geometry.display.spans.iter().all(|span| span.style.syntax.is_none()));
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.display.code_blocks[0].source.language, "rust");
                            check!(editor.display.spans.iter().any(|span| span.style.syntax == Some(ryne_projection::Class::String)));
                            let path = std::env::temp_dir().join(format!("ryne-highlight-smoke-{}.md", std::process::id()));
                            editor.document.save(&path).map_err(|error| error.to_string())?;
                            let reopened = editor.document.open_with(&path).map_err(|error| error.to_string())?;
                            check_eq!(reopened.source.text(), editor.document.source.text());
                            std::fs::remove_file(path).map_err(|error| error.to_string())?;
                            editor.document = editor.document.fork("");
                            editor.move_to(0, false, cx);
                            editor.replace_text_in_range(None, "```\r\nfn main() { let s = \"中😀\"; }\r\n```\r\n", window, cx);
                            check!(editor.document.source.text().starts_with("```rust\r\n"));
                            check_eq!(editor.display.code_blocks[0].source.language, "rust");
                            check!(editor.display.spans.iter().any(|span| span.style.syntax == Some(ryne_projection::Class::Keyword)));
                            check_eq!(editor.document.history_len(false), 1);
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), "");
                            check!(!editor.document.dirty());
                            editor.document = editor.document.fork("正文 $\\frac{x_1}{\\sqrt{y}}$ 😀 和 $a$。\r\n\r\n- 列表 $b^2$ 尾\r\n\r\n| $h$ | B |\r\n| --- | --- |\r\n| $c_1$ | 文 |\r\n");
                            editor.move_to(0, false, cx);
                        }
                        37 => {
                            check_eq!(geometry.display.formulas.len(), 5);
                            check!(!editor.document.dirty());
                            let formula = &geometry.display.formulas[0];
                            let row = geometry.row_for(formula.display.start, Affinity::Downstream).ok_or("missing formula row")?;
                            let mixed = geometry.lines[row.line].mixed.as_ref().ok_or("missing mixed layout")?;
                            check!(row.line_height > px(LINE_HEIGHT));
                            check!(mixed.baseline >= px(formula.formula.ascent * mixed.font_size));
                            check!(geometry.math_painted >= 5);
                            check!(geometry.math_error.is_none());
                            let start = geometry.position_with_affinity(formula.display.start, Affinity::Downstream);
                            let end = geometry.position_with_affinity(formula.display.end, Affinity::Upstream);
                            check!((end.x - start.x - px(formula.formula.width * mixed.font_size)).abs() < px(0.1));
                            editor.mouse_down(&MouseDownEvent { button: MouseButton::Left,
                                position: start + point((end.x-start.x)/2., row.line_height/2.),
                                ..Default::default() }, window, cx);
                            editor.dragging = false;
                            check_eq!(editor.document.head, formula.source.start+1);
                            check!(editor.display.text.contains("$\\frac{x_1}{\\sqrt{y}}$"));
                            check!(!editor.document.dirty());
                        }
                        38 => {
                            check!(geometry.display.text.contains("$\\frac{x_1}{\\sqrt{y}}$"));
                            let raw = editor.document.source.text().to_owned();
                            let caret = raw.find("x_1").ok_or("missing math body")?;
                            editor.move_to(caret, false, cx);
                            editor.replace_and_mark_text_in_range(None, "zhong", Some(5..5), window, cx);
                            editor.replace_and_mark_text_in_range(None, "中", Some(1..1), window, cx);
                            check_eq!(editor.document.source.text(), raw);
                            editor.replace_text_in_range(None, "中", window, cx);
                            check!(editor.document.source.text().contains("中x_1"));
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), raw);
                            editor.move_to(0, false, cx);
                        }
                        39 => {
                            check_eq!(geometry.display.formulas.len(), 5);
                            let source = geometry.display.formulas[0].source.clone();
                            editor.move_to(source.start, false, cx);
                            editor.right(&Right, window, cx);
                            check_eq!(editor.document.head, source.start+1);
                            editor.move_to(source.end, false, cx);
                            editor.left(&Left, window, cx);
                            check_eq!(editor.document.head, source.end-1);
                            editor.move_to(0, false, cx);
                        }
                        40 => {
                            let raw = editor.document.source.text().to_owned();
                            let formula = &geometry.display.formulas[0];
                            let start = geometry.position_with_affinity(formula.display.start, Affinity::Downstream);
                            let end = geometry.position_with_affinity(formula.display.end, Affinity::Upstream);
                            check_eq!(geometry.hit_caret(start).0, formula.display.start);
                            check_eq!(geometry.hit_caret(end).0, formula.display.end);
                            editor.move_to(0, false, cx);
                            editor.move_to(formula.source.end, true, cx);
                            editor.replace_text_in_range(None, "替换", window, cx);
                            check!(editor.document.source.text().starts_with("替换 😀"));
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), raw);
                            editor.move_to(0, false, cx);
                        }
                        41 => {
                            let table = &geometry.display.tables[0];
                            let cell = &table.rows[0][0];
                            let row = geometry.row_for(cell.start, Affinity::Downstream).ok_or("missing math cell")?;
                            check!(geometry.lines[row.line].mixed.is_some());
                            let caret = table.source.rows[0].cells[0].content.start + 1;
                            editor.move_to(caret, false, cx);
                            check_eq!(editor.display.tables.len(), 1);
                            editor.tab(&Tab, window, cx);
                            check_eq!(editor.document.head, table.source.rows[0].cells[1].content.start);
                            check!(!editor.document.dirty());
                            editor.move_to(0, false, cx);
                        }
                        42 => {
                            let runs = geometry.display.spans.iter().map(|span| TextRun {
                                len: span.display.len(), font: font(".SystemUIFont"),
                                color: rgb(0x282b30).into(), background_color: None, underline: None, strikethrough: None,
                            }).collect::<Vec<_>>();
                            let end = geometry.display.text.find('\n').ok_or("missing math paragraph")?;
                            let rows = shape_inline(&geometry.display, &runs, 0..end, (18., LINE_HEIGHT), px(70.), window);
                            check!(rows.len() >= 3);
                            for row in &rows {
                                check_eq!(geometry.display.snap(row.start), row.start);
                            }
                            check_eq!(rows.iter().filter_map(|r| r.mixed.as_ref()).flat_map(|m| &m.pieces)
                                .filter(|p| matches!(p.content, InlineContent::Math(_))).count(), 2);
                            editor.toggle_source(&ToggleSource, window, cx);
                            check!(editor.display.formulas.is_empty());
                            check!(editor.display.text.contains("$\\frac{x_1}{\\sqrt{y}}$"));
                            check!(!editor.document.dirty());
                            let raw = editor.document.source.text().to_owned();
                            let path = std::env::temp_dir().join(format!("ryne-math-smoke-{}.md", std::process::id()));
                            editor.document.save(&path).map_err(|e| e.to_string())?;
                            check_eq!(editor.document.open_with(&path).map_err(|e| e.to_string())?.source.text(), raw);
                            std::fs::remove_file(path).map_err(|e| e.to_string())?;
                            editor.document = editor.document.fork("before\r\n\r\n```mermaid\r\nflowchart LR\r\nA[中文😀] --> B[保存]\r\nstyle A fill:#ff0000\r\n```\r\n\r\nafter\r\n\r\n```mermaid\r\nflowchart LR\r\nA[**literal** --> B\r\n```\r\n");
                            editor.move_to(0, false, cx);
                        }
                        43 => {
                            check_eq!(geometry.display.diagrams.len(),1);
                            check_eq!(geometry.diagrams_painted,1);
                            check!(geometry.math_error.is_none());
                            check!(!editor.document.dirty());
                            check!(geometry.display.code_blocks[0].diagnostic.is_some());
                            let original = editor.document.source.text().to_owned();
                            let original_start = geometry.display.diagrams[0].source.range.start;
                            editor.replace_and_mark_text_in_range(None,"前",Some(1..1),window,cx);
                            check_eq!(editor.document.source.text(),original);
                            check_eq!(editor.display.diagrams.len(),1);
                            check_eq!(editor.display.diagrams[0].source.range.start,original_start+"前".len());
                            // The committed source is unchanged, but the temporary
                            // composition has different text/mapping than this frame.
                            check!(editor.bounds_for_range(0..0,geometry.bounds,window,cx).is_none());
                            check!(editor.character_index_for_point(geometry.bounds.origin,window,cx).is_none());
                            editor.escape(&Escape,window,cx);
                            check_eq!(editor.document.source.text(),original);
                            check!(!editor.document.dirty());
                            let view = &geometry.display.diagrams[0];
                            let line = geometry.lines.iter().find(|line| line.diagram == Some(0)).ok_or("missing Mermaid geometry")?;
                            check!(line.line_height > px(LINE_HEIGHT));
                            let position = geometry.position(view.display.start) + point(line.width / 2.,line.line_height / 2.);
                            editor.mouse_down(&MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() },window,cx);
                            editor.mouse_up(&MouseUpEvent { button: MouseButton::Left, position, ..Default::default() },window,cx);
                            check_eq!(editor.document.head,view.source.anchor);
                        }
                        44 => {
                            check!(geometry.display.diagrams.is_empty());
                            check!(geometry.display.text.contains("A[中文😀] --> B[保存]"));
                            let raw = editor.document.source.text().to_owned();
                            editor.move_to(raw.find("中文").ok_or("missing diagram label")?,false,cx);
                            editor.replace_and_mark_text_in_range(None,"zhong",Some(5..5),window,cx);
                            check_eq!(editor.document.source.text(),raw);
                            editor.replace_text_in_range(None,"中",window,cx);
                            editor.undo(&Undo,window,cx);
                            check_eq!(editor.document.source.text(),raw);
                            let end = raw.find("style A").ok_or("missing Mermaid style")? + "style A fill:#ff0000".len();
                            editor.move_to(end,false,cx);
                            editor.enter(&Enter,window,cx);
                            check!(editor.document.source.text().contains("style A fill:#ff0000\r\n\r\n```"));
                            editor.undo(&Undo,window,cx);
                            check_eq!(editor.document.source.text(),raw);
                            editor.move_to(0,false,cx);
                        }
                        45 => {
                            let view = geometry.display.diagrams.first().ok_or("missing refolded diagram")?;
                            editor.document.select(view.source.range.start,false,true);
                            editor.right(&Right,window,cx);
                            check_eq!(editor.document.head,view.source.anchor);
                            check!(editor.display.diagrams.is_empty());
                            editor.move_to(0,false,cx);
                        }
                        46 => {
                            let raw = editor.document.source.text().to_owned();
                            let view = geometry.display.diagrams.first().ok_or("missing selected diagram")?;
                            let start = geometry.display.source_at(view.display.start,Affinity::Downstream);
                            let end = geometry.display.source_at(view.display.end,Affinity::Upstream);
                            check!(raw[start..end].starts_with("```mermaid"));
                            check!(raw[start..end].ends_with("```\r\n"));
                            editor.document.select(0,false,true);
                            editor.document.select(end,true,true);
                            editor.refresh(cx);
                            check_eq!(editor.display.diagrams.len(),1);
                            editor.replace_text_in_range(None,"replaced",window,cx);
                            check!(!editor.document.source.text().contains("A[中文😀]"));
                            editor.undo(&Undo,window,cx);
                            check_eq!(editor.document.source.text(),raw);
                            check!(!editor.document.dirty());
                            editor.move_to(0,false,cx);
                        }
                        47 => {
                            let raw = editor.document.source.text().to_owned();
                            editor.toggle_source(&ToggleSource,window,cx);
                            check!(editor.display.diagrams.is_empty());
                            check_eq!(editor.display.text,raw.replace("\r\n","\n"));
                            check!(!editor.document.dirty());
                            editor.toggle_source(&ToggleSource,window,cx);
                            let view = editor.display.diagrams.first().ok_or("missing source-mode return")?.clone();
                            editor.document.select(view.source.range.end,false,true);
                            editor.backspace(&Backspace,window,cx);
                            check_eq!(editor.document.source.text(),raw);
                            check_eq!(editor.document.head,view.source.anchor);
                            editor.document = editor.document.fork("- item\r\n\r\n  > ~~~mermaid\r\n  > flowchart LR\r\n  > A[容器] --> B\r\n  > ~~~\r\n");
                            editor.move_to(0,false,cx);
                        }
                        48 => {
                            check_eq!(geometry.display.diagrams.len(),1);
                            check_eq!(geometry.diagrams_painted,1);
                            let view = &geometry.display.diagrams[0];
                            check!(view.source.indent >= 2);
                            editor.move_to(view.source.anchor,false,cx);
                            editor.toggle_block_source(&ToggleBlockSource,window,cx);
                            check!(editor.display.diagrams.is_empty());
                            check!(editor.display.text.contains("  > ~~~mermaid"));
                            check_eq!(editor.document.block_source.as_ref().ok_or("missing Mermaid scope")?.range.start,0);
                        }
                        49 => {
                            let raw = editor.document.source.text().to_owned();
                            let end = raw.find("A[容器]").ok_or("missing nested diagram")? + "A[容器]".len();
                            editor.move_to(end,false,cx);
                            editor.enter(&Enter,window,cx);
                            check!(editor.document.source.text().contains("A[容器]\r\n --> B"));
                            editor.undo(&Undo,window,cx);
                            check_eq!(editor.document.source.text(),raw);
                            editor.escape(&Escape,window,cx);
                        }
                        50 => {
                            check_eq!(geometry.display.diagrams.len(),1);
                            check!(!editor.document.dirty());
                            let raw = editor.document.source.text().to_owned();
                            let path = std::env::temp_dir().join(format!("ryne-mermaid-smoke-{}.md",std::process::id()));
                            editor.document.save(&path).map_err(|e| e.to_string())?;
                            check_eq!(editor.document.open_with(&path).map_err(|e| e.to_string())?.source.text(),raw);
                            std::fs::remove_file(path).map_err(|e| e.to_string())?;
                            editor.document = editor.document.fork("before\n\n```mermaid\nflowchart LR\nA[**literal** --> B\n```\n");
                            editor.move_to(0,false,cx);
                        }
                        51 => {
                            check!(geometry.display.diagrams.is_empty());
                            check_eq!(geometry.display.code_blocks.len(),1);
                            check!(geometry.display.code_blocks[0].diagnostic.is_some());
                            check!(geometry.display.text.contains("**literal**"));
                            check!(!editor.document.dirty());
                            editor.document = editor.document.fork("### heading\r\n\r\nparagraph **bold**\r\n\r\n- item\r\n  - child\r\n\r\n```rs\r\nlet value = 1;\r\n```\r\n\r\n| a | b |\r\n|---|---|\r\n| c | d |\r\n");
                            editor.move_to(5, false, cx);
                        }
                        52 | 55 | 58 | 61 => {
                            let block = crate::block::at(&editor.document.parse, editor.document.head).ok_or("missing focused block")?;
                            check_eq!(block.label, match stage - 1 { 52 => "H3", 55 => "•", 58 => "▦", _ => "</>" });
                            let control = geometry.block_control.ok_or("missing focused block control")?;
                            check_eq!(control.origin, geometry.block_origin(&block).ok_or("missing block origin")?);
                            let first = geometry.lines.iter().find(|line| match stage - 1 {
                                58 => line.cell.is_some(),
                                61 => line.code.is_some(),
                                _ => line.start == geometry.display.position_at(block.range.start),
                            }).ok_or("missing first block line")?;
                            check_eq!(control.center().y, geometry.bounds.top() + px(geometry.page.top) + first.top + first.line_height / 2. - geometry.scroll);
                            if block.is_list() {
                                check!(control.left()>=geometry.bounds.left()+px(geometry.page.left+geometry.page.width)-px(32.));
                            } else {
                                check!(control.right() <= geometry.position(geometry.display.position_at(block.anchor)).x);
                            }
                            check!(!editor.document.dirty());
                            editor.update_block_hover(control.center(),cx);
                        }
                        53 | 56 | 59 => {
                            check!(editor.block_menu.is_some());
                            check!(!editor.dragging);
                            let menu = geometry.block_menu.ok_or("missing painted block menu")?;
                            check!(geometry.bounds.contains(&menu.origin));
                            check!(menu.bottom() <= geometry.bounds.bottom());
                            let (snapshot, block) = editor.block_menu.clone().ok_or("missing menu target")?;
                            editor.block_action(snapshot, &block, match stage - 1 { 53 => BlockAction::Source, 56 => BlockAction::Copy, _ => BlockAction::Select }, cx);
                        }
                        54 => {
                            check!(editor.document.block_source.is_some());
                            check!(editor.display.text.starts_with("### heading"));
                            check!(!editor.document.dirty());
                            editor.escape(&Escape,window,cx);
                            let offset = editor.document.source.text().find("item").ok_or("missing item")?;
                            editor.move_to(offset,false,cx);
                        }
                        57 => {
                            let block = crate::block::at(&editor.document.parse, editor.document.head).ok_or("missing copied block")?;
                            check_eq!(cx.read_from_clipboard().and_then(|item| item.text()).ok_or("empty clipboard")?, editor.document.source.text()[block.range.clone()]);
                            check!(editor.document.selection().is_empty());
                            check!(!editor.document.dirty());
                            let offset = editor.document.source.text().find("c | d").ok_or("missing table")?;
                            editor.move_to(offset,false,cx);
                        }
                        60 => {
                            let table = crate::table::tables(&editor.document.parse).into_iter().next().ok_or("missing table selection")?;
                            check_eq!(editor.document.selection(), table.range);
                            check_eq!(editor.focused_block().ok_or("missing selected table control")?.label,"▦");
                            check!(!editor.document.dirty());
                            let offset = editor.document.source.text().find("value").ok_or("missing code")?;
                            editor.move_to(offset,false,cx);
                        }
                        62 => {
                            check!(editor.block_menu.is_some());
                            editor.escape(&Escape,window,cx);
                            check!(editor.block_menu.is_none());
                            check!(editor.document.block_source.is_none());
                            editor.document.composition = Some(Composition { range: editor.document.head..editor.document.head, text: "临".into(), selection: editor.document.head..editor.document.head });
                            editor.refresh(cx);
                        }
                        63 => {
                            check!(geometry.block_control.is_none());
                            check!(!editor.document.dirty());
                            editor.document.composition = None;
                            editor.document.set_source_mode(true);
                            editor.refresh(cx);
                        }
                        64 => {
                            check!(geometry.block_control.is_none());
                            editor.document.set_source_mode(false);
                            let block = crate::block::at(&editor.document.parse, editor.document.head).ok_or("missing stale target")?;
                            let snapshot = editor.document.source.id();
                            let head = editor.document.head;
                            editor.insert(head..head,"x",cx);
                            editor.block_action(snapshot,&block,BlockAction::Source,cx);
                            check!(editor.document.block_source.is_none());
                            editor.undo(&Undo,window,cx);
                            check!(!editor.document.dirty());
                        }
                        65 => {
                            check!(geometry.block_control.is_some());
                            check!(!editor.document.dirty());
                            editor.document = editor.document.fork("1000. ### nested heading\n      continuation\n");
                            editor.move_to(12,false,cx);
                        }
                        66 => {
                            let block = editor.focused_block().ok_or("missing nested heading")?;
                            check_eq!(block.label,"H3");
                            let control = geometry.block_control.ok_or("missing nested heading control")?;
                            let marker = geometry.list_markers.first().ok_or("missing long list marker")?.1;
                            check_eq!(control.right()+px(32.),marker.left());
                            check_eq!(control.center().y,marker.center().y);
                            editor.block_action(geometry.snapshot,&block,BlockAction::Source,cx);
                        }
                        67 => {
                            check!(geometry.block_control.is_some());
                            check!(editor.display.text.starts_with("1000. ### nested heading"));
                            check!(!editor.document.dirty());
                            editor.document = editor.document.fork("paragraph\n\n## hover heading\n\n```rs\nliteral\n```\n");
                            editor.move_to(0,false,cx);
                        }
                        68 => {
                            let offset = editor.document.source.text().find("hover heading").ok_or("missing hover heading")?;
                            let position = geometry.position(geometry.display.position_at(offset)) + point(px(4.),px(8.));
                            editor.update_block_hover(position,cx);
                            check_eq!(editor.active_block().ok_or("missing hovered heading")?.label,"H2");
                            check_eq!(editor.document.head,0);
                            check!(!editor.document.dirty());
                        }
                        69 => {
                            let control = geometry.block_control.ok_or("missing hover-only control")?;
                            editor.update_block_hover(control.center(),cx);
                            check!(editor.block_menu.is_some());
                            editor.update_block_hover(control.center(),cx);
                            check!(editor.block_menu.is_some());
                            check_eq!(editor.document.head,0);
                        }
                        70 => {
                            let control = geometry.block_control.ok_or("missing hover control")?;
                            let menu = geometry.block_menu.ok_or("missing hover menu")?;
                            editor.update_block_hover(point(control.center().x,control.bottom()+px(2.)),cx);
                            check!(editor.block_menu.is_some());
                            editor.update_block_hover(menu.center(),cx);
                            check!(editor.block_menu.is_some());
                            editor.update_block_hover(geometry.bounds.bottom_right()-point(px(8.),px(8.)),cx);
                            check!(editor.block_menu.is_none());
                            check_eq!(editor.document.head,0);
                            let offset = editor.document.source.text().find("literal").ok_or("missing hover code")?;
                            let position = geometry.position(geometry.display.position_at(offset))+point(px(4.),px(8.));
                            editor.update_block_hover(position,cx);
                            let block = editor.active_block().ok_or("missing code hover target")?;
                            let header = geometry.block_origin(&block).ok_or("missing code panel origin")?+point(px(50.),px(12.));
                            editor.update_block_hover(header,cx);
                        }
                        71 => {
                            check_eq!(editor.active_block().ok_or("missing hovered code")?.label,"</>");
                            check!(geometry.block_control.is_some());
                            editor.editor_hover(&false,window,cx);
                            check!(editor.hovered_block.is_none());
                            check!(editor.block_menu.is_none());
                            check!(!editor.document.dirty());
                            editor.document = editor.document.fork("");
                            editor.refresh(cx);
                            for count in 1..=3 {
                                editor.replace_text_in_range(None, "#", window, cx);
                                check_eq!(editor.display.text, "#".repeat(count));
                                check_eq!(editor.document.head, count);
                            }
                        }
                        72 => {
                            check_eq!(geometry.lines[0].shaped.text.as_str(), "###");
                            editor.replace_and_mark_text_in_range(None, "标题", Some(2..2), window, cx);
                            check_eq!(editor.document.source.text(), "###");
                            check!(editor.display.text.starts_with("###"), "composition keeps the input prefix visible");
                        }
                        73 => {
                            editor.enter(&Enter, window, cx);
                            check!(editor.document.composition.is_none());
                            check_eq!(editor.document.source.text(), "### 标题");
                            check_eq!(editor.display.text, "标题");
                            check_eq!(editor.document.head, "### 标题".len());
                            check_eq!(editor.selected_text_range(false, window, cx).ok_or("missing heading selection")?.range, 6..6);
                        }
                        74 => {
                            let (font_size, height) = heading_metrics(3);
                            check_eq!(geometry.lines[0].shaped.unwrapped_layout.font_size, px(font_size));
                            check_eq!(geometry.lines[0].line_height, px(height));
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), "###");
                            check_eq!(editor.display.text, "###");
                            editor.redo(&Redo, window, cx);
                            check_eq!(editor.document.source.text(), "### 标题");
                            check_eq!(editor.display.text, "标题");
                            editor.document = editor.document.fork("");
                            editor.refresh(cx);
                            for _ in 0..2 { editor.replace_text_in_range(None, "*", window, cx); }
                            check_eq!(editor.document.source.text(), "**");
                        }
                        75 => {
                            let snapshot = editor.document.source.id();
                            editor.replace_and_mark_text_in_range(None, "中文", Some(2..2), window, cx);
                            check_eq!(editor.document.source.id(), snapshot);
                            check_eq!(editor.document.source.text(), "**");
                            check_eq!(editor.display.text, "**中文");
                        }
                        76 => {
                            editor.enter(&Enter, window, cx);
                            check_eq!(editor.document.source.text(), "**中文**");
                            check_eq!(editor.document.head, "**中文".len());
                            check_eq!(editor.display.text, "**中文**");
                            check_eq!(editor.selected_text_range(false, window, cx).ok_or("missing inline selection")?.range, 4..4);
                        }
                        77 => {
                            check!(geometry.display.spans.iter().any(|span| span.style.bold && !span.style.marker));
                            let snapshot = editor.document.source.id();
                            for _ in 0..2 { editor.replace_text_in_range(None, "*", window, cx); }
                            check_eq!(editor.document.source.id(), snapshot);
                            check_eq!(editor.display.text, "中文");
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), "**");
                            editor.redo(&Redo, window, cx);
                            check_eq!(editor.document.source.text(), "**中文**");
                            editor.document = editor.document.fork("\r\n\r\ntail");
                            editor.refresh(cx);
                            for input in ["`", "`", "`", "rust"] { editor.replace_text_in_range(None, input, window, cx); }
                            check_eq!(editor.document.source.text(), "```rust\r\n\r\ntail");
                            check_eq!(editor.document.head, 7);
                            check!(editor.display.text.starts_with("```rust\n"));
                        }
                        78 => {
                            check_eq!(geometry.lines[0].shaped.text.as_str(), "```rust");
                            editor.enter(&Enter, window, cx);
                            check_eq!(editor.document.source.text(), "```rust\r\n\r\n```\r\n\r\ntail");
                            check_eq!(editor.document.head, 9);
                            check_eq!(editor.display.code_blocks.len(), 1);
                            check!(!editor.display.text.contains("```"));
                        }
                        79 => {
                            check_eq!(geometry.display.code_blocks.len(), 1);
                            editor.enter(&Enter, window, cx);
                            for input in ["*", "a"] { editor.replace_text_in_range(None, input, window, cx); }
                            check_eq!(editor.document.source.text(), "```rust\r\n\r\n*a\r\n```\r\n\r\ntail");
                            for _ in 0..4 { editor.undo(&Undo, window, cx); }
                            check_eq!(editor.document.source.text(), "```rust\r\n\r\ntail");
                            editor.redo(&Redo, window, cx);
                        }
                        80 => {
                            check_eq!(editor.document.source.text(), "```rust\r\n\r\n```\r\n\r\ntail");
                            check_eq!(geometry.display.code_blocks.len(), 1);
                            editor.document = editor.document.fork("");
                            editor.refresh(cx);
                        }
                        81 => {
                            let zone = geometry.append_zone.ok_or("missing invisible append block")?;
                            check!(geometry.bounds.contains(&zone.center()));
                            check!(geometry.append_control.is_none());
                            check!(!editor.document.dirty());
                            editor.update_block_hover(zone.center(), cx);
                        }
                        82 => {
                            let control = geometry.append_control.ok_or("missing append icon after hover")?;
                            check!(geometry.append_menu.is_none());
                            check_eq!(editor.document.head, 0);
                            check!(!editor.document.dirty());
                            editor.update_block_hover(control.center(), cx);
                        }
                        83 => {
                            let menu = geometry.append_menu.ok_or("missing append menu")?;
                            check!(menu.top() >= geometry.bounds.top());
                            check!(menu.bottom() <= geometry.bounds.bottom());
                            editor.update_block_hover(menu.center(), cx);
                            check!(editor.append_menu.is_some());
                            check!(!editor.document.dirty());
                            editor.escape(&Escape, window, cx);
                            check!(editor.append_menu.is_none());
                            let control = geometry.append_control.ok_or("missing append icon")?;
                            editor.update_block_hover(control.center(), cx);
                        }
                        84 => {
                            check!(geometry.append_menu.is_some());
                            editor.append_block(geometry.snapshot, ryne_extensions::InsertBlock::Code, window, cx);
                            check_eq!(editor.document.source.text(), "```\n\n```");
                            check_eq!(editor.document.head, 4);
                            check!(editor.append_menu.is_none());
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), "");
                            check!(!editor.document.dirty());
                        }
                        85 => {
                            editor.redo(&Redo, window, cx);
                            check_eq!(editor.document.source.text(), "```\n\n```");
                            check_eq!(editor.document.head, 4);
                            editor.toggle_source(&ToggleSource, window, cx);
                        }
                        86 => {
                            check!(geometry.append_zone.is_none());
                            check!(geometry.append_control.is_none());
                            editor.toggle_source(&ToggleSource, window, cx);
                            editor.replace_and_mark_text_in_range(None, "中文", Some(2..2), window, cx);
                        }
                        87 => {
                            check!(geometry.append_zone.is_none());
                            check!(geometry.append_control.is_none());
                            editor.escape(&Escape, window, cx);
                            editor.dragging = true;
                            editor.refresh(cx);
                        }
                        88 => {
                            check!(geometry.append_zone.is_none());
                            editor.dragging = false;
                            editor.document = editor.document.fork(&"paragraph\n\n".repeat(80));
                            editor.refresh(cx);
                            editor.scroll = px(100000.);
                            editor.ensure_caret = false;
                            cx.notify();
                        }
                        89 => {
                            check!(geometry.scroll > px(0.));
                            let zone = geometry.append_zone.ok_or("missing append block at scrolled document end")?;
                            check!(geometry.bounds.contains(&zone.center()));
                            editor.update_block_hover(zone.center(), cx);
                        }
                        90 => {
                            let control = geometry.append_control.ok_or("missing scrolled append icon")?;
                            check!(geometry.bounds.contains(&control.center()));
                            editor.update_block_hover(control.center(), cx);
                        }
                        91 => {
                            let menu = geometry.append_menu.ok_or("missing scrolled append menu")?;
                            check!(menu.bottom() <= geometry.bounds.bottom());
                            editor.document.replace(0..0, "prefix").map_err(|error| format!("{error:?}"))?;
                            let source = editor.document.source.text().to_owned();
                            editor.append_block(geometry.snapshot, ryne_extensions::InsertBlock::Table, window, cx);
                            check_eq!(editor.document.source.text(), source);
                            editor.refresh(cx);
                        }
                        92 => {
                            editor.document = editor.document.fork("# 中英 Heading😀\r\n\r\n正文 **bold** and emoji😀\r\n\r\n## 二级\r\n\r\n- 第一项\r\n- 第二项\r\n\r\n```rust\r\nlet x = 1;\r\n\r\n```\r\n");
                            editor.scroll = px(0.);
                            editor.move_to(editor.document.source.text().find("正文").ok_or("missing typography sample")?, false, cx);
                        }
                        93 => {
                            check_eq!(geometry.page.width.min(760.), geometry.page.width);
                            let centered = (geometry.page.left * 2. + geometry.page.width - f32::from(geometry.bounds.size.width)).abs() < 0.1;
                            check!(centered);
                            check_eq!(editor.fonts.body.family.as_ref(), "Helvetica Neue");
                            check_eq!(editor.fonts.code.family.as_ref(), "Menlo");
                            let blank = geometry.rows.iter().find(|row| row.range.is_empty() && row.line_height == px(14.)).ok_or("missing compact separator row")?;
                            let separator_point = geometry.position_in_row(blank, blank.range.start) + point(px(0.), blank.line_height / 2.);
                            check_eq!(geometry.hit_caret(separator_point).0, blank.range.start);
                            for row in &geometry.rows {
                                if row.range.is_empty() || geometry.lines[row.line].diagram.is_some() { continue; }
                                let position = geometry.position_in_row(row, row.range.start) + point(px(0.), row.line_height / 2.);
                                check_eq!(geometry.hit_caret(position).0, row.range.start);
                            }
                            let bytes = editor.document.source.text().to_owned();
                            editor.toggle_source(&ToggleSource, window, cx);
                            check_eq!(editor.document.source.text(), bytes);
                            check!(!editor.document.dirty());
                        }
                        94 => {
                            check!(geometry.source_mode);
                            check_eq!(geometry.lines[0].shaped.unwrapped_layout.font_size, px(theme::CODE.0));
                            check!(geometry.lines.iter().all(|line| line.line_height >= px(theme::CODE.1)));
                            check_eq!(geometry.page.width, f32::from(geometry.bounds.size.width) - 48.);
                            check!(!editor.document.dirty());
                            editor.toggle_source(&ToggleSource, window, cx);
                            editor.toggle_block_source(&ToggleBlockSource, window, cx);
                        }
                        95 => {
                            let line = geometry.lines.iter().find(|line| line.shaped.text.contains("正文")).ok_or("missing local source row")?;
                            check_eq!(line.shaped.unwrapped_layout.font_size, px(theme::CODE.0));
                            check!(geometry.lines.iter().any(|line| line.shaped.text.contains("中英") && line.shaped.unwrapped_layout.font_size == px(32.)));
                            check!(!editor.document.dirty());
                        }
                        96 => {
                            editor.document = editor.document.fork(&"paragraph 中😀 mixed words for wrapping and navigation. ".repeat(200));
                            editor.refresh(cx);
                            editor.scroll = px(600.);
                            editor.ensure_caret = false;
                        }
                        97 => {
                            let row = geometry.rows.iter().find(|row| px(geometry.page.top) + row.top + row.line_height > geometry.scroll).ok_or("missing scroll anchor")?;
                            let source = geometry.display.source_at(row.range.start, Affinity::Downstream);
                            typography_anchor = Some((source, geometry.position(row.range.start).y - geometry.bounds.top()));
                            window.resize(size(px(640.), px(680.)));
                        }
                        98..=100 => {
                            let (source, y) = typography_anchor.ok_or("missing typography anchor")?;
                            let position = geometry.position(geometry.display.position_at(source));
                            check!((position.y - geometry.bounds.top() - y).abs() < px(1.));
                            check!(!editor.document.dirty());
                            check_eq!(geometry.hit_caret(position + point(px(0.), px(1.))).0, geometry.display.position_at(source));
                            if stage - 1 == 98 {
                                window.resize(size(px(1440.), px(680.)));
                            } else if stage - 1 == 99 {
                                check_eq!(geometry.page.width, 760.);
                                window.resize(size(px(960.), px(680.)));
                            }
                        }
                        101 => {
                            editor.document = editor.document.fork("# Heading中文\n\nbody text $\\frac{a}{b}$ trailing\n\n```rust\nlet x = 1;\n\n```\n\n| header | second |\n| --- | --- |\n| cell | 中文 |\n\n");
                            editor.scroll = px(0.);
                            editor.move_to(editor.document.source.text().find("body").ok_or("missing caret body")?, false, cx);
                        }
                        102 => {
                            let mut sizes = Vec::new();
                            for row in &geometry.rows {
                                let line = &geometry.lines[row.line];
                                let caret = geometry.caret_bounds(row.range.start, Affinity::Downstream);
                                let origin = geometry.position_in_row(row, row.range.start);
                                let font_size = line.mixed.as_ref().map_or(line.shaped.unwrapped_layout.font_size, |mixed| px(mixed.font_size));
                                check_eq!(caret.size.height, font_size.max(px(1.)).min(row.line_height));
                                check_eq!(caret.center().x, origin.x);
                                check!(caret.top() >= origin.y);
                                check!(caret.bottom() <= origin.y + row.line_height + px(0.01));
                                if !row.range.is_empty() { check!(caret.size.height < row.line_height); }
                                sizes.push(caret.size.height);
                            }
                            for size in [32., theme::BODY.0, theme::CODE.0, theme::TABLE.0] {
                                check!(sizes.contains(&px(size)));
                            }
                            check!(geometry.lines.iter().any(|line| line.mixed.is_some()));
                            let offset = geometry.display.position_at(editor.caret());
                            let utf16 = editor.editable_text()[..editor.caret()].encode_utf16().count();
                            let platform = editor.bounds_for_range(utf16..utf16, geometry.bounds, window, cx).ok_or("missing IME caret bounds")?;
                            check_eq!(platform, geometry.caret_bounds(offset, editor.caret_affinity));
                            check!(!editor.document.dirty());
                            editor.toggle_source(&ToggleSource, window, cx);
                        }
                        103 => {
                            for row in &geometry.rows {
                                check_eq!(geometry.caret_bounds(row.range.start, Affinity::Downstream).size.height, px(theme::CODE.0));
                            }
                            editor.caret_visible = false;
                            editor.move_to(editor.document.head, false, cx);
                            check!(editor.caret_visible);
                            editor.replace_and_mark_text_in_range(None, "中文", Some(2..2), window, cx);
                            editor.sync_caret_blink(window, cx);
                            check!(editor.caret_visible);
                            check!(editor.caret_blink_task.is_none());
                            check!(!editor.document.dirty());
                        }
                        104 => {
                            check!(editor.caret_visible);
                            check!(editor.caret_blink_task.is_none());
                            editor.escape(&Escape, window, cx);
                            editor.select_all(&SelectAll, window, cx);
                            editor.sync_caret_blink(window, cx);
                            check!(editor.caret_blink_task.is_none());
                            check!(!editor.document.dirty());
                            editor.move_to(0, false, cx);
                            editor.dragging = true;
                            editor.sync_caret_blink(window, cx);
                            check!(editor.caret_visible);
                            check!(editor.caret_blink_task.is_none());
                        }
                        105 => {
                            editor.dragging = false;
                            let other = cx.focus_handle();
                            window.focus(&other);
                            editor.sync_caret_blink(window, cx);
                            check!(editor.caret_blink_task.is_none());
                            window.focus(&editor.focus);
                            editor.reset_caret_blink();
                            editor.sync_caret_blink(window, cx);
                            check!(editor.caret_visible);
                            check_eq!(editor.caret_blink_task.is_some(), window.is_window_active());
                            check!(!editor.document.dirty());
                            editor.document = editor.document.fork(&"wrapped caret text 中文 ".repeat(80));
                            editor.refresh(cx);
                        }
                        106 => {
                            let boundary = geometry.rows.windows(2).find(|rows| rows[0].range.end == rows[1].range.start).ok_or("missing wrapped caret boundary")?;
                            let offset = boundary[0].range.end;
                            let upstream = geometry.caret_bounds(offset, Affinity::Upstream);
                            let downstream = geometry.caret_bounds(offset, Affinity::Downstream);
                            check!(upstream.top() < downstream.top());
                            check_eq!(upstream.size.height, px(theme::BODY.0));
                            check_eq!(downstream.size.height, px(theme::BODY.0));
                            check!(!editor.document.dirty());
                            // Blinking/resetting adds no history; the first undo still reverts input.
                            editor.insert(0..0, "x", cx);
                            editor.reset_caret_blink();
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), "wrapped caret text 中文 ".repeat(80));
                            check!(!editor.document.dirty());
                        }
                        107 => {
                            let runs = geometry.display.spans.iter().map(|span| TextRun {
                                len: span.display.len(), font: editor.fonts.body.clone(), color: rgb(theme::INK).into(),
                                background_color: None, underline: None, strikethrough: None,
                            }).collect::<Vec<_>>();
                            let range = 0..geometry.display.text.len();
                            let layouts = shape_inline(&geometry.display, &runs, range.clone(), theme::BODY, px(160.), window);
                            check!(layouts.iter().all(|layout| layout.mixed.is_none()));
                            check_eq!(layouts.iter().map(|layout| layout.shaped.text.as_ref()).collect::<String>(), geometry.display.text);
                            check!(layouts.iter().any(|layout| !layout.shaped.wrap_boundaries.is_empty()));
                            editor.document = editor.document.fork("**a中😀e\u{301}z** ~~strike中😀~~ `code中😀`\n\n$\\frac{a}{b}$\n\n```rust\nlet xy = 1;\n```\n\n| left | right |\n| --- | ---: |\n| a😀b | c中d |\n");
                            let offset = editor.document.source.text().find('😀').ok_or("missing inline caret")?;
                            editor.move_to(offset, false, cx);
                        }
                        108..=117 => {
                            let offset = geometry.display.position_at(editor.caret());
                            let row = geometry.row_for(offset, editor.caret_affinity).ok_or("missing caret row")?;
                            let line = &geometry.lines[row.line];
                            // A caret in plain/styled text keeps the original full shaped line.
                            check!(line.mixed.is_none());
                            check_eq!(line.x_for_index(offset - line.start), line.shaped.unwrapped_layout.x_for_index(offset - line.start));
                            let cursor = geometry.caret_bounds(offset, editor.caret_affinity);
                            check_eq!(geometry.display.source_at(offset, editor.caret_affinity), editor.caret());
                            check_eq!(geometry.hit_caret(cursor.center()).0, offset);
                            let utf16 = editor.editable_text()[..editor.caret()].encode_utf16().count();
                            check_eq!(editor.bounds_for_range(utf16..utf16, geometry.bounds, window, cx).ok_or("missing caret IME bounds")?, cursor);
                            check!(!editor.document.dirty());
                            let raw = editor.document.source.text();
                            if (112..=114).contains(&(stage - 1)) {
                                // The caret used to switch to shaping that lost the code font
                                // when adjacent runs shared the same foreground color.
                                let code = geometry.display.position_at(raw.find("code中").ok_or("missing code font sample")?);
                                let glyph_run = line.shaped.unwrapped_layout.runs.iter().find(|run| run.glyphs.iter().any(|glyph| glyph.index == code - line.start)).ok_or("missing inline code glyph")?;
                                check_eq!(glyph_run.font_id, window.text_system().resolve_font(&editor.fonts.code));
                            }
                            let target = match stage - 1 {
                                108 => Some(raw.find("strike").ok_or("missing strike start")?),
                                109 => Some(raw.find("strike").ok_or("missing strike middle")? + 3),
                                110 => Some(raw.find("~~ `").ok_or("missing strike end")?),
                                111 => Some(raw.find("code中").ok_or("missing inline code start")?),
                                112 => Some(raw.find("code中").ok_or("missing inline code middle")? + 4),
                                113 => Some(raw.find("`\n").ok_or("missing inline code end")?),
                                114 => Some(raw.find("xy").ok_or("missing block code caret")? + 1),
                                115 => Some(raw.find("c中d").ok_or("missing table caret")? + 1),
                                _ => None,
                            };
                            if let Some(offset) = target { editor.move_to(offset, false, cx); }
                            else if stage - 1 == 116 { editor.toggle_source(&ToggleSource, window, cx); }
                            else { editor.select_all(&SelectAll, window, cx); }
                        }
                        118 => {
                            check!(geometry.lines.iter().all(|line| line.mixed.is_none()));
                            check!(!editor.document.dirty());
                        }
                        119..=125 => {
                            if stage - 1 == 119 {
                                editor.document = editor.document.fork("# 多言語😀\r\n\r\n- 한국어 · Français · Español\r\n");
                                editor.move_to(2, false, cx);
                                editor.replace_text_in_range(None, "x", window, cx);
                                editor.undo(&Undo, window, cx);
                                editor.replace_and_mark_text_in_range(None, "にほん", Some(3..3), window, cx);
                            }
                            let source = editor.document.source.clone();
                            let selection = editor.document.selection();
                            let dirty = editor.document.dirty();
                            let undo = editor.document.history_len(false);
                            let redo = editor.document.history_len(true);
                            let composition = editor.document.composition.as_ref().ok_or("missing multilingual composition")?;
                            let composition = (composition.range.clone(), composition.text.clone(), composition.selection.clone());
                            let display = editor.display.clone();
                            let language = Language::ALL[(stage - 1 - 119) as usize];
                            editor.select_language(Preference(Some(language)), cx);
                            check_eq!(editor.locale.language, language);
                            check_eq!(editor.document.source.id(), source.id());
                            check_eq!(editor.document.source.text(), source.text());
                            check_eq!(editor.document.selection(), selection);
                            check_eq!(editor.document.dirty(), dirty);
                            check_eq!(editor.document.history_len(false), undo);
                            check_eq!(editor.document.history_len(true), redo);
                            check_eq!(editor.display, display);
                            let current = editor.document.composition.as_ref().ok_or("composition lost during language change")?;
                            check_eq!((current.range.clone(), current.text.clone(), current.selection.clone()), composition);
                            let path = std::env::temp_dir().join(format!("ryne-language-smoke-{}", std::process::id()));
                            check_eq!(Preference::read(&path), Preference(Some(language)));
                            // Draw every native language choice, including the longest labels.
                            editor.language_menu = true;
                            cx.notify();
                        }
                        126 => {
                            editor.select_language(Preference(None), cx);
                            check_eq!(editor.locale.language, Language::En);
                            editor.escape(&Escape, window, cx);
                            editor.redo(&Redo, window, cx);
                            check!(editor.document.source.text().starts_with("# x多言語"));
                            editor.undo(&Undo, window, cx);
                            check!(editor.document.source.text().starts_with("# 多言語"));
                            check!(!editor.document.dirty());
                            std::fs::remove_file(std::env::temp_dir().join(format!("ryne-language-smoke-{}", std::process::id()))).map_err(|e| e.to_string())?;
                            editor.document = editor.document.fork(&format!("# {}\n", "标题换行测试".repeat(20)));
                            editor.move_to(2, false, cx);
                        }
                        127..=132 => {
                            let level = stage - 127;
                            let control = geometry.block_control.ok_or("missing wrapped heading control")?;
                            let first = geometry.rows.first().ok_or("missing first heading row")?;
                            check!(geometry.rows.iter().filter(|row| row.line == first.line).count() > 1);
                            check_eq!(first.line_height, px(theme::heading(level).1));
                            check_eq!(control.center().y, geometry.bounds.top() + px(geometry.page.top) + first.top + first.line_height / 2. - geometry.scroll);
                            check!(!editor.document.dirty());
                            if level < 6 {
                                editor.document = editor.document.fork(&format!("{} {}\n", "#".repeat(usize::from(level + 1)), "标题换行测试".repeat(20)));
                                editor.move_to(usize::from(level + 2), false, cx);
                            }
                        }
                        133 => {
                            editor.document = editor.document.fork("> | A | B |\r\n> | --- | ---: |\r\n> | 中😀 | value |\r\n> | next | x |\r\n\r\noutside\r\n");
                            editor.hovered_render = None;
                            let end = editor.document.source.text().len()-2;
                            editor.move_to(end,false,cx);
                            window.focus(&editor.focus);
                        }
                        134 => {
                            check_eq!(geometry.render_regions.len(),6);
                            check!(geometry.render_buttons.is_empty());
                            let region=geometry.render_regions.iter().find(|r|r.controls[0].target.part==[1,1]).ok_or("missing cell region")?;
                            let selection=editor.document.selection();let key=editor.document.source.id();
                            editor.mouse_move(&MouseMoveEvent {position:region.bounds.center(),..Default::default()},window,cx);
                            check_eq!(editor.document.selection(),selection);check_eq!(editor.document.source.id(),key);check!(!editor.document.dirty());
                        }
                        135 => {
                            check_eq!(geometry.render_buttons.len(),1);
                            let region=geometry.render_regions.iter().find(|r|r.controls[0].target.part==[1,1]).ok_or("missing cell region")?;
                            check!(region.bounds.contains(&geometry.render_buttons[0].center()));
                            let cell=region.controls[0].focus.clone();
                            let offset=geometry.display.position_at(cell.start);
                            let line=geometry.lines.iter().find(|l|l.start==offset).ok_or("missing cell text layout")?;
                            check!(line.width <= region.bounds.size.width-px(48.));
                            let selection=editor.document.selection();
                            editor.open_render_menu(geometry.render_key(window.viewport_size()),region.key(),window,cx);
                            check!(editor.render_menu.is_some());check_eq!(editor.document.selection(),selection);check_eq!(editor.document.history_len(false),0);
                        }
                        136 => {
                            check_eq!(geometry.render_menu_bounds.len(),1);
                            let menu=editor.render_menu.as_ref().ok_or("missing cell popup")?;
                            check_eq!(menu.rows.len(),11);
                            check!(matches!(menu.rows.first(),Some(RenderMenuRow::Submenu {children,..}) if children.len()==3));
                            editor.right(&Right,window,cx);
                        }
                        137 => {
                            check_eq!(geometry.render_menu_bounds.len(),2);
                            editor.down(&Down,window,cx);
                            let selection=editor.document.selection();
                            let length=editor.document.source.text().len();
                            editor.enter(&Enter,window,cx);
                            check!(editor.document.source.text().contains("| --- | :---: |"));
                            check_eq!(editor.document.history_len(false),1);
                            let delta=editor.document.source.text().len() as isize-length as isize;
                            check_eq!(editor.document.selection(),selection.start.saturating_add_signed(delta)..selection.end.saturating_add_signed(delta));
                        }
                        138 => {
                            check!(editor.render_menu.is_none());
                            editor.undo(&Undo,window,cx);
                            check!(!editor.document.dirty());
                            editor.hovered_render=None;
                            let at=editor.document.source.text().find("中😀").ok_or("missing cell text")?;
                            editor.move_to(at,false,cx);
                        }
                        139 => {
                            check_eq!(geometry.render_buttons.len(),1);
                            check!(editor.hovered_render.is_none());
                            editor.render_shortcut(&KeyDownEvent {keystroke:Keystroke {key:"down".into(),modifiers:Modifiers {control:true,platform:cfg!(target_os="macos"),..Default::default()},..Default::default()},is_held:false},window,cx);
                            check_eq!(editor.document.history_len(false),1);
                        }
                        140 => {
                            check_eq!(editor.display.tables[0].source.rows.len(),4);
                            let old=geometry.render_key(window.viewport_size());
                            let region=geometry.render_regions[0].key();
                            editor.undo(&Undo,window,cx);
                            editor.open_render_menu(old,region,window,cx);
                            check!(editor.render_menu.is_none());check!(!editor.document.dirty());
                        }
                        141 => {
                            check_eq!(geometry.render_buttons.len(),1);
                            let region=geometry.render_regions.iter().find(|r|r.controls[0].target.part==[1,0]).ok_or("missing focused cell")?;
                            editor.open_render_menu(geometry.render_key(window.viewport_size()),region.key(),window,cx);
                        }
                        142 => {
                            let menu=editor.render_menu.as_mut().ok_or("missing focused popup")?;
                            menu.selected=menu.rows.iter().position(|r|matches!(r,RenderMenuRow::Action(i) if i.request.command=="table.delete-column")).ok_or("missing column action")?;
                            editor.enter(&Enter,window,cx);
                            check_eq!(editor.document.history_len(false),1);
                        }
                        143 => {
                            check_eq!(editor.display.tables[0].source.align.len(),1);
                            check!(editor.document.source.text().contains("> | value |"));
                            editor.undo(&Undo,window,cx);
                            check!(!editor.document.dirty());
                            editor.toggle_source(&ToggleSource,window,cx);
                        }
                        144 => {
                            check!(geometry.render_regions.is_empty());check!(geometry.render_buttons.is_empty());
                            editor.toggle_source(&ToggleSource,window,cx);
                            let at=editor.document.source.text().find("中😀").ok_or("missing cell text")?;
                            editor.move_to(at,false,cx);
                        }
                        145 => {
                            check_eq!(geometry.render_buttons.len(),1);
                            editor.replace_and_mark_text_in_range(None,"候选",Some(2..2),window,cx);
                            check!(!editor.document.dirty());
                        }
                        146 => {
                            check!(editor.document.composition.is_some());
                            check!(geometry.render_buttons.is_empty());check!(geometry.render_menu_bounds.is_empty());
                            check_eq!(geometry.render_regions.len(),6);
                            editor.escape(&Escape,window,cx);
                            check!(!editor.document.dirty());
                        }
                        147 => {
                            editor.dragging=true;editor.refresh(cx);
                        }
                        148 => {
                            check!(geometry.render_buttons.is_empty());check!(geometry.render_menu_bounds.is_empty());
                            editor.dragging=false;editor.refresh(cx);
                        }
                        149 => {
                            editor.document=editor.document.fork("9. parent\r\n   - child\r\n10. second\r\n    - other\r\n\r\noutside\r\n");
                            editor.hovered_render=None;
                            let end=editor.document.source.text().len()-2;editor.move_to(end,false,cx);
                        }
                        150 => {
                            check_eq!(geometry.render_regions.len(),4);check!(geometry.render_buttons.is_empty());
                            let source=editor.document.source.id();let selection=editor.document.selection();let display=editor.display.clone();
                            let at=editor.document.source.text().find("second").ok_or("missing list item")?;
                            let region=geometry.render_regions.iter().find(|r|r.controls[0].focus.start==at).ok_or("missing ordered item region")?;
                            editor.mouse_move(&MouseMoveEvent{position:region.trigger.center(),..Default::default()},window,cx);
                            check_eq!(editor.document.source.id(),source);check_eq!(editor.document.selection(),selection);check_eq!(editor.display,display);check!(!editor.document.dirty());
                        }
                        151 => {
                            check_eq!(geometry.render_buttons.len(),1);
                            let at=editor.document.source.text().find("second").ok_or("missing ordered item")?;
                            let region=geometry.render_regions.iter().find(|r|r.controls[0].focus.start==at).ok_or("missing list trigger")?;
                            let marker=geometry.list_markers.iter().find(|(r,_)|r.start==region.controls[0].target.owner.start).ok_or("missing numbered marker")?.1;
                            check_eq!(region.trigger.right()+px(4.),marker.left());check_eq!(region.trigger.center().y,marker.center().y);
                            let control=geometry.block_control.ok_or("missing whole-list type control")?;
                            let target=geometry.block_target.as_ref().ok_or("missing whole-list target")?;
                            check!(target.is_list());check_eq!(target.range.start,0);
                            check!(target.range.end>=region.controls[0].target.owner.end);
                            check!(target.range.start<region.controls[0].target.owner.start);
                            check!(control.left()>=geometry.bounds.left()+px(geometry.page.left+geometry.page.width)-px(32.));
                            let first=geometry.render_regions.iter().find(|r|r.controls[0].focus.start==3).ok_or("missing first list item")?;
                            check_eq!(control.center().y,first.trigger.center().y);
                            check!(control.left()>region.trigger.right());
                            let selection=editor.document.selection();editor.open_render_menu(geometry.render_key(window.viewport_size()),region.key(),window,cx);
                            check_eq!(editor.document.selection(),selection);check_eq!(editor.document.history_len(false),0);
                        }
                        152 => {
                            check_eq!(geometry.render_menu_bounds.len(),1);
                            let menu=editor.render_menu.as_mut().ok_or("missing list menu")?;check_eq!(menu.rows.len(),7);
                            menu.selected=menu.rows.iter().position(|r|matches!(r,RenderMenuRow::Action(i) if i.request.command=="list.move-up")).ok_or("missing move action")?;
                            editor.enter(&Enter,window,cx);
                        }
                        153 => {
                            check_eq!(editor.document.source.text(),"9. second\r\n   - other\r\n10. parent\r\n    - child\r\n\r\noutside\r\n");check_eq!(editor.document.history_len(false),1);
                            editor.undo(&Undo,window,cx);check!(!editor.document.dirty());
                            editor.hovered_render=None;let at=editor.document.source.text().find("child").ok_or("missing nested item")?;editor.move_to(at,false,cx);
                        }
                        154 => {
                            check_eq!(geometry.render_buttons.len(),1);
                            let target=geometry.block_target.as_ref().ok_or("missing nested whole-list target")?;
                            check!(target.is_list());check_eq!(target.range.start,0);
                            check_eq!(target.icon,Some("text_number_list_ltr"));
                            let control=geometry.block_control.ok_or("missing nested whole-list control")?;
                            let first=geometry.render_regions.iter().find(|r|r.controls[0].focus.start==3).ok_or("missing outer first item")?;
                            check_eq!(control.center().y,first.trigger.center().y);
                            check!(control.left()>=geometry.bounds.left()+px(geometry.page.left+geometry.page.width)-px(32.));
                            editor.render_shortcut(&KeyDownEvent{keystroke:Keystroke{key:"enter".into(),modifiers:Modifiers{alt:true,..Default::default()},..Default::default()},is_held:false},window,cx);
                            let menu=editor.render_menu.as_ref().ok_or("missing nested list menu")?;
                            let marker=editor.document.source.text().find("- child").ok_or("missing child marker")?;
                            check!(menu.rows.iter().all(|r|matches!(r,RenderMenuRow::Action(i) if i.request.target.part==[marker])));
                        }
                        155 => {
                            let menu=editor.render_menu.as_mut().ok_or("missing nested menu")?;
                            menu.selected=menu.rows.iter().position(|r|matches!(r,RenderMenuRow::Action(i) if i.request.command=="list.outdent")).ok_or("missing outdent action")?;
                            editor.enter(&Enter,window,cx);
                            check!(editor.document.source.text().contains("\r\n- child\r\n"));check_eq!(editor.document.history_len(false),1);
                        }
                        156 => {
                            editor.undo(&Undo,window,cx);check!(!editor.document.dirty());
                            editor.replace_and_mark_text_in_range(None,"候选",Some(2..2),window,cx);
                        }
                        157 => {
                            check!(geometry.render_buttons.is_empty());check!(geometry.render_menu_bounds.is_empty());check_eq!(geometry.render_regions.len(),4);
                            editor.escape(&Escape,window,cx);check!(!editor.document.dirty());editor.toggle_source(&ToggleSource,window,cx);
                        }
                        158 => {
                            check!(geometry.render_regions.is_empty());editor.toggle_source(&ToggleSource,window,cx);editor.dragging=true;editor.refresh(cx);
                        }
                        159 => {
                            check!(geometry.render_buttons.is_empty());editor.dragging=false;
                            editor.document=editor.document.fork("- ```rust\n  let x = 1;\n  ```\n- second\n\noutside\n");
                            editor.hovered_render=None;let end=editor.document.source.text().len()-1;editor.move_to(end,false,cx);
                        }
                        160 => {
                            check_eq!(geometry.render_regions.len(),2);
                            let at=editor.document.source.text().find("```rust").ok_or("missing list code fence")?;
                            let region=geometry.render_regions.iter().find(|r|r.controls[0].focus.start==at).ok_or("missing panel-first item region")?;
                            editor.mouse_move(&MouseMoveEvent{position:region.trigger.center(),..Default::default()},window,cx);
                        }
                        161 => {
                            check_eq!(geometry.render_buttons.len(),1);
                            let region=geometry.render_regions.iter().find(|r|r.controls[0].focus.start==2).ok_or("missing code list trigger")?;
                            editor.open_render_menu(geometry.render_key(window.viewport_size()),region.key(),window,cx);check!(editor.render_menu.is_some());check!(!editor.document.dirty());
                            editor.escape(&Escape,window,cx);
                            editor.document=editor.document.fork("- first\n  - child\n- second\n\noutside\n");editor.hovered_render=None;editor.move_to(2,false,cx);
                        }
                        162 => {
                            check_eq!(geometry.render_buttons.len(),1);
                            let target=geometry.block_target.clone().ok_or("missing unordered list block")?;
                            check!(target.is_list());
                            check_eq!(&editor.document.source.text()[target.range.clone()],"- first\n  - child\n- second\n\n");
                            let source=editor.document.source.id();let selection=editor.document.selection();
                            let control=geometry.block_control.ok_or("missing unordered list icon")?;
                            editor.update_block_hover(control.center(),cx);
                            check_eq!(editor.block_menu.as_ref().map(|(_,b)|b),Some(&target));
                            check_eq!(editor.document.source.id(),source);check_eq!(editor.document.selection(),selection);check!(!editor.document.dirty());
                            let region=geometry.render_regions.iter().find(|r|r.controls[0].focus.start==2).ok_or("missing unordered item")?;
                            editor.open_render_menu(geometry.render_key(window.viewport_size()),region.key(),window,cx);
                            editor.enter(&Enter,window,cx);
                        }
                        163 => {
                            check_eq!(editor.document.source.text(),"- first\n  - child\n- \n- second\n\noutside\n");check_eq!(editor.document.history_len(false),1);
                            editor.undo(&Undo,window,cx);check!(!editor.document.dirty());
                        }
                        164 => {
                            editor.document = editor.document.fork("# heading\r\n\r\n- first\r\n  - child\r\n- second\r\n\r\n| A | B |\r\n| --- | ---: |\r\n| x | y |\r\n");
                            editor.move_to(2, false, cx);
                        }
                        165..=192 => {
                            let language = Language::ALL[(stage - 1 - 165) as usize / 4];
                            let phase = (stage - 1 - 165) % 4;
                            let source = editor.document.source.clone();
                            let selection = editor.document.selection();
                            let undo = editor.document.history_len(false);
                            let redo = editor.document.history_len(true);
                            match phase {
                                0 => {
                                    editor.select_language(Preference(Some(language)), cx);
                                    check!(editor.block_menu.is_none());
                                    check!(editor.render_menu.is_none());
                                    // Validate the labels emitted by the real feature providers,
                                    // including disabled actions and the alignment submenu.
                                    let controls = editor.document.render_controls()?;
                                    check!(controls.iter().any(|control| control.provider == "list"));
                                    check!(controls.iter().any(|control| control.provider == "table"));
                                    for control in controls {
                                        for action in control.actions {
                                            check!(language.render_label(action.label, "MISSING") != "MISSING");
                                            if let Some(submenu) = action.submenu {
                                                check!(language.render_label(submenu.label, "MISSING") != "MISSING");
                                            }
                                        }
                                    }
                                    let block = block::at(&editor.document.parse, 2).ok_or("missing localized heading")?;
                                    editor.hovered_block = Some((source.id(), block.clone()));
                                    editor.open_block_menu(source.id(), block, cx);
                                }
                                1 => {
                                    let bounds = geometry.block_menu.ok_or("missing translated block menu")?;
                                    for command in [BlockAction::Source, BlockAction::Select, BlockAction::Copy] {
                                        check!(menu_text_width(language.block_action(command, false), 13., window) + px(24.) <= bounds.size.width);
                                    }
                                    let region = geometry.render_regions.iter().find(|r| r.controls[0].provider == "table").ok_or("missing localized table controls")?;
                                    editor.open_render_menu(geometry.render_key(window.viewport_size()), region.key(), window, cx);
                                    editor.right(&Right, window, cx);
                                }
                                2 => {
                                    check_eq!(geometry.render_menu_bounds.len(), 2);
                                    let menu = editor.render_menu.as_ref().ok_or("missing translated table menu")?;
                                    for row in &menu.rows {
                                        match row {
                                            RenderMenuRow::Action(item) => check!(render_action_width(&item.action, language, window) <= geometry.render_menu_bounds[0].size.width),
                                            RenderMenuRow::Submenu { children, .. } => {
                                                for item in children {
                                                    check!(render_action_width(&item.action, language, window) <= geometry.render_menu_bounds[1].size.width);
                                                }
                                            }
                                        }
                                    }
                                    let region = geometry.render_regions.iter().find(|r| r.controls[0].provider == "list").ok_or("missing localized list controls")?;
                                    editor.open_render_menu(geometry.render_key(window.viewport_size()), region.key(), window, cx);
                                }
                                _ => {
                                    check_eq!(geometry.render_menu_bounds.len(), 1);
                                    let menu = editor.render_menu.as_ref().ok_or("missing translated list menu")?;
                                    for row in &menu.rows {
                                        if let RenderMenuRow::Action(item) = row {
                                            let required = render_action_width(&item.action, language, window);
                                            if required > geometry.render_menu_bounds[0].size.width {
                                                return Err(format!("translated list menu too narrow: {language:?}, {}, required {required:?}, actual {:?}", item.action.label, geometry.render_menu_bounds[0].size.width));
                                            }
                                        }
                                    }
                                }
                            }
                            check_eq!(editor.document.source.id(), source.id());
                            check_eq!(editor.document.source.text(), source.text());
                            check_eq!(editor.document.selection(), selection);
                            check_eq!(editor.document.history_len(false), undo);
                            check_eq!(editor.document.history_len(true), redo);
                            check!(!editor.document.dirty());
                            check_eq!(editor.locale.language, language);
                        }
                        193 => {
                            editor.select_language(Preference(None), cx);
                            std::fs::remove_file(std::env::temp_dir().join(format!("ryne-language-smoke-{}", std::process::id()))).map_err(|e| e.to_string())?;
                            println!("Menu localization passed: block titles/actions, list/table provider labels and alignment submenu in all seven languages; measured translated text, icons and shortcut space; unchanged source, selection, dirty state and undo/redo");
                        }
                        _ => {
                            if window.is_window_active() {
                                check!(blink_seen.load(std::sync::atomic::Ordering::Relaxed));
                            }
                            eprintln!("GUI smoke idle blink observed: {}", blink_seen.load(std::sync::atomic::Ordering::Relaxed));
                            println!("List controls passed: ordered/unordered/nested and panel-first regions, leading SVG trigger and marker spacing, one whole-list type icon at the upper right across nested items, hover without reflow/selection changes, focused innermost target, clicked menu, whole-subtree movement across 9->10, outdent, empty sibling insertion, undo, source/IME/drag suppression");
                            println!("Cell controls passed: real region geometry, reserved text space, hover/focus SVG trigger, clicked popup, grouped alignment submenu, keyboard activation and shortcuts, row/column edits with atomic undo, unchanged outside selection, stale frame rejection, source/IME/drag suppression and preedit layout reservation");
                            println!("GUI smoke passed: seven interface languages, persisted selection, system reset and unchanged Unicode/CRLF source, selection, history and IME composition; continuous caret text/strike/code shaping, natural spacing, Unicode/IME hit mapping, inline/code/right-aligned table/source geometry; font-sized caret, IME bounds, wrapped affinity and blink focus/selection/composition/drag lifecycle; typography, semantic spacing, source fonts, resize anchoring and caret mapping; invisible append block, hover menu, viewport positioning, source/IME/drag suppression, stale menu rejection and atomic insertion undo; inline completion, heading, block menu, Mermaid, math, highlighting, table, list and code regressions");
                            cx.quit();
                        }
                    }
                    Ok(())
                });
                let error = match result {
                    Ok(Ok(())) => None,
                    Ok(Err(error)) => Some(error),
                    Err(error) => Some(error.to_string()),
                };
                if let Some(error) = error {
                    eprintln!("GUI smoke failed at stage {stage}: {error}");
                    std::process::exit(1);
                }

            }
        }).detach();
    }
}
