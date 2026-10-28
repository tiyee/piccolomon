use super::*;
static CACHE_SMOKE_COMMANDS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
struct CacheSmokeCommand;
/// Startup-only fixture for the real application's smoke run.
pub fn smoke_features() -> Vec<std::sync::Arc<dyn ryne_extensions::FeatureModule + Send + Sync>> {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--smoke-test")) {
        vec![std::sync::Arc::new(CacheSmokeCommand)]
    } else {
        Vec::new()
    }
}
impl ryne_extensions::FeatureModule for CacheSmokeCommand {
    fn manifest(&self) -> ryne_extensions::FeatureManifest {
        ryne_extensions::FeatureManifest::builtin("smoke-command")
    }
    fn register(&self, r: &mut ryne_extensions::Registrar) -> Result<(), String> {
        r.command(
            ryne_extensions::Contribution {
                id: "smoke.command",
                slot: ryne_extensions::Slot::Command("edit"),
                exclusive: false,
                priority: 10,
            },
            std::sync::Arc::new(Self),
        )
    }
}
impl ryne_extensions::CommandHandler for CacheSmokeCommand {
    fn plan(
        &self,
        _: &ryne_extensions::EditIntent,
        _: &ryne_extensions::EditingState,
        _: &ryne_extensions::FeatureServices<'_>,
    ) -> Result<Option<ryne_extensions::CommandPlan>, String> {
        CACHE_SMOKE_COMMANDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(None)
    }
}

pub(super) const SAMPLE: &str = "# Ryne（如言）\n\n点击 **粗体**、*斜体*、~~删除线~~、`inline code` 或 [链接](https://example.org)。\n\n光标进入时展开 Markdown 标记，离开后收起。\n试试 **粗体中的 *斜体***，也可以直接输入 Markdown。\n\n| 功能 | 状态 | 示例 |\n| :--- | :---: | ---: |\n| 表格编辑 | 可用 | `code` |\n| Tab 切换单元格 | Shift+Enter 换行 | 123 |\n\n- 无序列表：点击正文编辑\n  - 嵌套列表与 **粗体**\n- Enter 续项，空项 Enter 退出\n\n1. 有序列表\n2. Tab 缩进，Shift+Tab 反缩进\n\n```rust\nfn main() {\n    println!(\"Hello, Markdown\");\n}\n```\n\nEnter 分段 · Shift+Enter 硬换行\n支持跨行选择、粘贴和撤销，长段落随窗口宽度自动折行。\n\n```mermaid\nflowchart LR\n    A[打开文件] --> B[编辑 Markdown] --> C[保存]\n```\n\n行内公式：$E = mc^2$，以及 $\\frac{a_1}{\\sqrt{b}}$。\n\nCmd+O 打开 · Cmd+S 保存 · Cmd+Z 撤销\nCmd+Shift+M 切换全文源码\n\n";
impl Editor {
    pub(super) fn smoke_pointer(
        &mut self,
        target: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let geometry = self
            .viewport
            .geometry
            .as_ref()
            .ok_or("missing pointer layout")?;
        let position = match target {
            0 => geometry.position(0) + point(px(5.), px(15.)),
            1 => {
                geometry.position(
                    self.viewport
                        .display
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
            if !editor.caret_state.caret_visible {
                observed.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let _blink_observer = blink_observer;
            let mut typography_anchor: Option<(usize, Pixels)> = None;
            let mut cached_layout: Option<std::sync::Arc<text_layout::TextLayout>> = None;
            let mut cached_projection: Option<std::sync::Arc<ryne_projection::IndexedDisplay>> = None;
            let mut cached_controls: Option<std::sync::Arc<Vec<ryne_extensions::RenderControl>>> = None;
            let mut quote_control: Option<Bounds<Pixels>> = None;
            let mut second_source=String::new();
            let mut third_source=None;
            let code_language_only = std::env::var_os("RYNE_SMOKE_CODE_LANGUAGE_ONLY").is_some();
            let performance_only=std::env::var_os("RYNE_SMOKE_PERFORMANCE_ONLY").is_some();
            let first_batch_only=std::env::var_os("RYNE_SMOKE_FIRST_BATCH_ONLY").is_some();
            let second_batch_only=std::env::var_os("RYNE_SMOKE_SECOND_BATCH_ONLY").is_some();
            let mut p1_state = crate::p1_smoke::State::default();
            let p1_only=std::env::var_os("RYNE_SMOKE_P1_REPAIR_ONLY").is_some();
            let third_batch_only=std::env::var_os("RYNE_SMOKE_THIRD_BATCH_ONLY").is_some();
            let low_risk_only=std::env::var_os("RYNE_SMOKE_LOW_RISK_ONLY").is_some();
            let stages: Vec<u16> = if low_risk_only {
                std::iter::once(0).chain(372..374).collect()
            } else if p1_only {
                std::iter::once(0).chain(295..372).collect()
            } else if third_batch_only {
                std::iter::once(0).chain(287..296).collect()
            } else if second_batch_only {
                std::iter::once(0).chain(279..288).collect()
            } else if first_batch_only {
                std::iter::once(0).chain(271..280).collect()
            } else if performance_only {
                std::iter::once(0).chain(266..272).collect()
            } else if code_language_only {
                std::iter::once(0).chain(259..267).collect()
            } else {
                (0..375).collect()
            };
            for stage in stages {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(if stage>=295 {20} else {500}))
                    .await;
                if matches!(stage, 272 | 277) && !performance_only {
                    // No editor input/refresh here: the scheduled host task must
                    // finish automatic highlighting by itself.
                    for _ in 0..200 {
                        let pending = this.update_in(cx, |editor, _, _| editor.viewport.highlight_task.is_some()).unwrap_or(false);
                        if !pending { break; }
                        cx.background_executor().timer(std::time::Duration::from_millis(50)).await;
                    }
                }
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
                        let ready = editor.viewport.geometry.as_ref().is_some_and(|geometry| {
                            geometry.is_current(editor)
                                && geometry.display.text == editor.viewport.display.text
                                && geometry.display.spans == editor.viewport.display.spans
                                && geometry.display.formulas == editor.viewport.display.formulas
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
                    let geometry = editor.viewport.geometry.as_ref().ok_or("native frame was not laid out")?.clone();
                    check!(geometry.is_current(editor));
                    check_eq!(geometry.snapshot, editor.document.source.id());
                    check_eq!(geometry.display.text, editor.viewport.display.text);
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
                            check!(editor.viewport.display.text.starts_with("Ryne（如言）\n"));
                            check_eq!(editor.document.head, 2);
                            check!(!editor.viewport.display.text.contains("**粗体**"));
                            let source = editor.document.source.text().find("粗体").ok_or("missing bold sample")?;
                            let position = geometry.position(editor.viewport.display.position_at(source));
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
                            editor.viewport.dragging = false;
                            check_eq!(editor.document.head, source);
                            check!(editor.viewport.display.text.contains("**粗体**"));
                            check_eq!(editor.document.source.text(), SAMPLE);
                        }
                        1 => {
                            let offset = editor.document.head;
                            let position = geometry.position(editor.viewport.display.position_at(offset));
                            check_eq!(geometry.hit_caret(position).0, editor.viewport.display.position_at(offset));
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
                            check!(!editor.viewport.display.text.contains("**粗体**"));
                            let revision = editor.document.source.revision();
                            editor.toggle_source(&ToggleSource, window, cx);
                            check_eq!(editor.viewport.display.text, SAMPLE);
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
                            editor.viewport.dragging = false;
                            check_eq!(editor.document.head, editor.document.source.text().find("标题").ok_or("missing heading sample")?);
                            check!(editor.viewport.display.text.starts_with("# **标题** #\n"));
                            editor.toggle_source(&ToggleSource, window, cx);
                        }
                        5 => {
                            check_eq!(geometry.lines[0].shaped.unwrapped_layout.font_size, px(theme::CODE.0));
                            check!(geometry.lines[0].line_height >= px(theme::CODE.1));
                            check!(editor.viewport.display.text.starts_with("# **标题** #\n"));
                            editor.toggle_source(&ToggleSource, window, cx);
                            check!(editor.viewport.display.text.starts_with("**标题**\n"));
                            editor.document = editor.document.fork("# 标题 #\r\n\r\n正文");
                            editor.move_to(0, false, cx);
                        }
                        6 => {
                            editor.smoke_pointer(0, window, cx)?;
                            check!(editor.document.heading_reveal.is_none(), "hover must keep heading markers folded");
                            check!(editor.viewport.display.text.starts_with("标题\n"));
                            check_eq!(editor.document.head, 2);
                            check!(!editor.document.dirty());
                        }
                        7 => {
                            let position = geometry.position(0) + point(px(1.), px(15.));
                            editor.mouse_down(&MouseDownEvent {
                                button: MouseButton::Left, position, click_count: 1, ..Default::default()
                            }, window, cx);
                            editor.viewport.dragging = false;
                            check_eq!(editor.document.head, 2, "click resolves against the folded heading");
                            check!(editor.document.heading_reveal.is_some(), "click reveals heading markers");
                            check!(editor.viewport.display.text.starts_with("# 标题 #\n"));
                        }
                        8 => {
                            let position = geometry.position(0) + point(px(1.), px(15.));
                            editor.mouse_down(&MouseDownEvent {
                                button: MouseButton::Left, position, click_count: 1, ..Default::default()
                            }, window, cx);
                            editor.viewport.dragging = false;
                            check_eq!(editor.document.head, 0, "revealed heading marker can be selected");
                            editor.smoke_pointer(1, window, cx)?;
                            check!(editor.document.heading_reveal.is_some(), "moving the pointer away keeps clicked heading editable");
                            editor.smoke_pointer(2, window, cx)?;
                        }
                        9 => {
                            check!(editor.document.heading_reveal.is_some(), "leaving editor must not fold a clicked heading");
                            let position = geometry.position(editor.viewport.display.position_at(editor.document.source.text().find("正文").ok_or("missing body")?)) + point(px(1.), px(8.));
                            editor.mouse_down(&MouseDownEvent {
                                button: MouseButton::Left, position, click_count: 1, ..Default::default()
                            }, window, cx);
                            editor.viewport.dragging = false;
                            check!(editor.document.heading_reveal.is_none(), "clicking another block folds heading markers");
                            check!(editor.viewport.display.text.starts_with("标题\n"));
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
                            for row in geometry.rows.iter() {
                                check_eq!(geometry.display.snap(row.range.start), row.range.start);
                                check_eq!(geometry.display.snap(row.range.end), row.range.end);
                            }
                            editor.move_to(boundary, false, cx);
                            editor.end(&End, window, cx);
                            check_eq!(editor.document.head, second.range.end);
                            check_eq!(editor.caret_state.caret_affinity, Affinity::Upstream);
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
                            check!(editor.viewport.display.text.contains("**第一行\n第二行**"));
                            let start = editor.document.head;
                            let end = editor.document.source.text().find("** 后记").ok_or("missing inline end")?;
                            let point = geometry.position(geometry.display.position_at(end)) + point(px(0.), px(8.));
                            editor.viewport.dragging = true;
                            editor.mouse_move(&MouseMoveEvent { position: point, ..Default::default() }, window, cx);
                            editor.viewport.dragging = false;
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
                            check_eq!(geometry.display.text, editor.viewport.display.text);
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
                            editor.viewport.dragging = false;
                            check_eq!(editor.document.head, offset);
                            let original = editor.document.source.text().to_owned();
                            editor.replace_and_mark_text_in_range(None, "文", Some(1..1), window, cx);
                            editor.enter(&Enter, window, cx);
                            check!(editor.document.source.text().contains("**文中😀**"));
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), original);
                            let anchor = editor.viewport.display.code_blocks[0].source.anchor;
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
                            check_eq!(editor.viewport.display.code_blocks[0].source.language, "rust");
                            editor.document = editor.document.fork("```rust\n```");
                            editor.move_to(0, false, cx);
                        }
                        18 => {
                            check_eq!(geometry.display.text, "\n");
                            check_eq!(geometry.lines[0].code, Some(0));
                            let position = geometry.position(0) + point(px(0.), px(8.));
                            editor.mouse_down(&MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }, window, cx);
                            editor.viewport.dragging = false;
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
                            editor.viewport.dragging = false;
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
                            editor.viewport.dragging = false;
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
                            editor.viewport.dragging = false;
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
                            editor.viewport.dragging = false;
                            if editor.document.head != caret {
                                return Err(format!("table click expected {caret}, got {}, point {position:?}, hit {:?}", editor.document.head, geometry.hit_caret(position)));
                            }
                            editor.replace_and_mark_text_in_range(None, "文", Some(1..1), window, cx);
                            editor.enter(&Enter, window, cx);
                            check!(editor.document.source.text().contains("中文😀"));
                            check_eq!(editor.viewport.display.tables[0].rows.len(), 3);
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), original);
                            editor.replace_text_in_range(None, "|", window, cx);
                            check!(editor.document.source.text().contains("中\\|😀"));
                            check_eq!(editor.viewport.display.tables.len(), 1);
                            editor.undo(&Undo, window, cx);
                            editor.hard_break(&HardBreak, window, cx);
                            check!(editor.document.source.text().contains("中<br>😀"));
                            check!(editor.viewport.display.text.contains("中\n😀"));
                            editor.undo(&Undo, window, cx);
                            let table = &editor.viewport.display.tables[0];
                            let anchor = table.source.rows[2].cells[1].content.start;
                            editor.move_to(anchor, false, cx);
                            editor.enter(&Enter, window, cx);
                            check_eq!(editor.viewport.display.tables[0].rows.len(), 4);
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), original);
                            editor.redo(&Redo, window, cx);
                            check_eq!(editor.viewport.display.tables[0].rows.len(), 4);
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
                            check_eq!(editor.viewport.display.tables[0].source.align[1], editable_markdown_parser::Alignment::Center);
                            editor.document = editor.document.fork("| | |\n| --- | --- |\n| | |\n");
                            editor.move_to(0, false, cx);
                        }
                        31 => {
                            check_eq!(geometry.display.text, "\t\n\t\n");
                            let table = &geometry.display.tables[0];
                            let anchor = table.source.rows[0].cells[1].content.start;
                            let position = geometry.position(table.rows[0][1].start) + point(px(0.), px(8.));
                            editor.mouse_down(&MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }, window, cx);
                            editor.viewport.dragging = false;
                            check_eq!(editor.document.head, anchor);
                            let original = editor.document.source.text().to_owned();
                            editor.replace_text_in_range(None, "😀", window, cx);
                            check!(editor.viewport.display.text.starts_with("\t😀\n"));
                            check_eq!(editor.viewport.display.tables.len(), 1);
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), original);
                            let anchor = editor.viewport.display.tables[0].source.rows[1].cells[1].content.start;
                            editor.move_to(anchor, false, cx);
                            editor.tab(&Tab, window, cx);
                            check_eq!(editor.viewport.display.tables[0].rows.len(), 3);
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
                            check_eq!(editor.viewport.display.tables.len(), 1);
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
                                return Err(format!("table source frame stale: source_mode={}, document_tables={}, geometry_tables={}, source_equal={}", editor.document.source_mode, editor.viewport.display.tables.len(), geometry.display.tables.len(), geometry.display.text == editor.viewport.display.text));
                            }
                            check_eq!(geometry.display.text, editor.document.source.text());
                            editor.escape(&Escape, window, cx);
                            check_eq!(editor.viewport.display.tables.len(), 1);
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
                            editor.viewport.dragging = false;
                            check_eq!(editor.document.head, offset);
                            let original = editor.document.source.text().to_owned();
                            editor.move_to(offset - 1, false, cx);
                            editor.move_to(offset + "中😀\"".len(), true, cx);
                            editor.replace_text_in_range(None, "42", window, cx);
                            check!(editor.viewport.display.spans.iter().any(|span| span.style.syntax == Some(Class::Constant)));
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
                            check_eq!(editor.viewport.display.code_blocks[0].source.language, "rust");
                            check!(editor.viewport.display.spans.iter().any(|span| span.style.syntax == Some(ryne_projection::Class::String)));
                            let path = std::env::temp_dir().join(format!("ryne-highlight-smoke-{}.md", std::process::id()));
                            editor.document.save(&path).map_err(|error| error.to_string())?;
                            let reopened = editor.document.open_with(&path).map_err(|error| error.to_string())?;
                            check_eq!(reopened.source.text(), editor.document.source.text());
                            std::fs::remove_file(path).map_err(|error| error.to_string())?;
                            editor.document = editor.document.fork("");
                            editor.move_to(0, false, cx);
                            editor.replace_text_in_range(None, "```\r\nfn main() { let s = \"中😀\"; }\r\n```\r\n", window, cx);
                            check!(editor.document.source.text().starts_with("```\r\n"));
                            check!(editor.viewport.display.code_blocks[0].source.language.is_empty());
                            check!(editor.viewport.display.spans.iter().any(|span| span.style.syntax == Some(ryne_projection::Class::Keyword)));
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
                            editor.viewport.dragging = false;
                            check_eq!(editor.document.head, formula.source.start+1);
                            check!(editor.viewport.display.text.contains("$\\frac{x_1}{\\sqrt{y}}$"));
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
                            check_eq!(editor.viewport.display.tables.len(), 1);
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
                            let rows = shape_inline(&geometry.display, &runs, 0..end, (18., LINE_HEIGHT), px(70.), window)?;
                            check!(rows.len() >= 3);
                            for row in &rows {
                                check_eq!(geometry.display.snap(row.start), row.start);
                            }
                            check_eq!(rows.iter().filter_map(|r| r.mixed.as_ref()).flat_map(|m| &m.pieces)
                                .filter(|p| matches!(p.content, InlineContent::Math(_))).count(), 2);
                            editor.toggle_source(&ToggleSource, window, cx);
                            check!(editor.viewport.display.formulas.is_empty());
                            check!(editor.viewport.display.text.contains("$\\frac{x_1}{\\sqrt{y}}$"));
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
                            check_eq!(editor.viewport.display.diagrams.len(),1);
                            check_eq!(editor.viewport.display.diagrams[0].source.range.start,original_start+"前".len());
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
                            check!(editor.viewport.display.diagrams.is_empty());
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
                            check_eq!(editor.viewport.display.diagrams.len(),1);
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
                            check!(editor.viewport.display.diagrams.is_empty());
                            check_eq!(editor.viewport.display.text,raw.replace("\r\n","\n"));
                            check!(!editor.document.dirty());
                            editor.toggle_source(&ToggleSource,window,cx);
                            let view = editor.viewport.display.diagrams.first().ok_or("missing source-mode return")?.clone();
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
                            check!(editor.viewport.display.diagrams.is_empty());
                            check!(editor.viewport.display.text.contains("  > ~~~mermaid"));
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
                            check_eq!(ryne_extensions::BlockMetadata::from(block.kind).label, match stage - 1 { 52 => "H3", 55 => "•", 58 => "▦", _ => "</>" });
                            let control = geometry.block_control.ok_or("missing focused block control")?;
                            check_eq!(control.origin, geometry.block_origin(&block).ok_or("missing block origin")?);
                            let first = geometry.lines.iter().find(|line| match stage - 1 {
                                58 => line.cell.is_some(),
                                61 => line.code.is_some(),
                                _ => line.start == geometry.display.position_at(block.range.start),
                            }).ok_or("missing first block line")?;
                            check_eq!(control.center().y, geometry.bounds.top() + px(geometry.page.top) + first.top + first.line_height / 2. - geometry.scroll);
                            check!(control.left() >= geometry.bounds.left());
                            check!(control.right() <= geometry.position(geometry.display.position_at(block.anchor)).x);
                            check!(!editor.document.dirty());
                            editor.update_block_hover(control.center(),cx);
                            if stage - 1 == 61 && editor.menus.block_menu.is_none() {
                                return Err(format!(
                                    "code menu hover rejected: current={}, epoch={}/{}, append={}, active={:?}, target={:?}, dragging={}",
                                    geometry.is_current(editor), geometry.frame_epoch, editor.viewport.frame_epoch,
                                    geometry.append_zone.is_some_and(|zone| zone.contains(&control.center())),
                                    editor.active_block(), geometry.block_target, editor.viewport.dragging,
                                ));
                            }
                        }
                        53 | 56 | 59 => {
                            check!(editor.menus.block_menu.is_some());
                            check!(!editor.viewport.dragging);
                            let menu = geometry.block_menu.ok_or("missing painted block menu")?;
                            check!(geometry.bounds.contains(&menu.origin));
                            check!(menu.bottom() <= geometry.bounds.bottom());
                            let (snapshot, block) = editor.menus.block_menu.clone().ok_or("missing menu target")?;
                            editor.block_action(snapshot, &block, match stage - 1 { 53 => BlockAction::Source, 56 => BlockAction::Copy, _ => BlockAction::Select }, cx);
                        }
                        54 => {
                            check!(editor.document.block_source.is_some());
                            check!(editor.viewport.display.text.starts_with("### heading"));
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
                            check_eq!(ryne_extensions::BlockMetadata::from(editor.focused_block().ok_or("missing selected table control")?.kind).label,"▦");
                            check!(!editor.document.dirty());
                            let offset = editor.document.source.text().find("value").ok_or("missing code")?;
                            editor.move_to(offset,false,cx);
                        }
                        62 => {
                            check!(editor.menus.block_menu.is_some());
                            editor.escape(&Escape,window,cx);
                            check!(editor.menus.block_menu.is_none());
                            check!(editor.document.block_source.is_none());
                            editor.document.update_composition(Some(editor.document.head..editor.document.head), "临", Some(0..0))?;
                            editor.refresh(cx);
                        }
                        63 => {
                            check!(geometry.block_control.is_none());
                            check!(!editor.document.dirty());
                            editor.document.cancel_composition();
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
                            check_eq!(ryne_extensions::BlockMetadata::from(block.kind).label,"H3");
                            let control = geometry.block_control.ok_or("missing nested heading control")?;
                            let marker = geometry.list_markers.first().ok_or("missing long list marker")?.1;
                            check_eq!(control.right()+px(32.),marker.left());
                            check_eq!(control.center().y,marker.center().y);
                            editor.block_action(geometry.snapshot,&block,BlockAction::Source,cx);
                        }
                        67 => {
                            check!(geometry.block_control.is_some());
                            check!(editor.viewport.display.text.starts_with("1000. ### nested heading"));
                            check!(!editor.document.dirty());
                            editor.document = editor.document.fork("paragraph\n\n## hover heading\n\n```rs\nliteral\n```\n");
                            editor.move_to(0,false,cx);
                        }
                        68 => {
                            let offset = editor.document.source.text().find("hover heading").ok_or("missing hover heading")?;
                            let position = geometry.position(geometry.display.position_at(offset)) + point(px(4.),px(8.));
                            editor.update_block_hover(position,cx);
                            check_eq!(ryne_extensions::BlockMetadata::from(editor.active_block().ok_or("missing hovered heading")?.kind).label,"H2");
                            check_eq!(editor.document.head,0);
                            check!(!editor.document.dirty());
                        }
                        69 => {
                            let control = geometry.block_control.ok_or("missing hover-only control")?;
                            editor.update_block_hover(control.center(),cx);
                            check!(editor.menus.block_menu.is_some());
                            editor.update_block_hover(control.center(),cx);
                            check!(editor.menus.block_menu.is_some());
                            check_eq!(editor.document.head,0);
                        }
                        70 => {
                            let control = geometry.block_control.ok_or("missing hover control")?;
                            let menu = geometry.block_menu.ok_or("missing hover menu")?;
                            editor.update_block_hover(point(control.center().x,control.bottom()+px(2.)),cx);
                            check!(editor.menus.block_menu.is_some());
                            editor.update_block_hover(menu.center(),cx);
                            check!(editor.menus.block_menu.is_some());
                            editor.update_block_hover(geometry.bounds.bottom_right()-point(px(8.),px(8.)),cx);
                            check!(editor.menus.block_menu.is_none());
                            check_eq!(editor.document.head,0);
                            let offset = editor.document.source.text().find("literal").ok_or("missing hover code")?;
                            let position = geometry.position(geometry.display.position_at(offset))+point(px(4.),px(8.));
                            editor.update_block_hover(position,cx);
                            let block = editor.active_block().ok_or("missing code hover target")?;
                            let header = geometry.block_origin(&block).ok_or("missing code panel origin")?+point(px(50.),px(12.));
                            editor.update_block_hover(header,cx);
                        }
                        71 => {
                            check_eq!(ryne_extensions::BlockMetadata::from(editor.active_block().ok_or("missing hovered code")?.kind).label,"</>");
                            check!(geometry.block_control.is_some());
                            editor.editor_hover(&false,window,cx);
                            check!(editor.menus.hovered_block.is_none());
                            check!(editor.menus.block_menu.is_none());
                            check!(!editor.document.dirty());
                            editor.document = editor.document.fork("");
                            editor.refresh(cx);
                            for count in 1..=3 {
                                editor.replace_text_in_range(None, "#", window, cx);
                                check_eq!(editor.viewport.display.text, "#".repeat(count));
                                check_eq!(editor.document.head, count);
                            }
                        }
                        72 => {
                            check_eq!(geometry.lines[0].shaped.text.as_str(), "###");
                            editor.replace_and_mark_text_in_range(None, "标题", Some(2..2), window, cx);
                            check_eq!(editor.document.source.text(), "###");
                            check!(editor.viewport.display.text.starts_with("###"), "composition keeps the input prefix visible");
                        }
                        73 => {
                            editor.enter(&Enter, window, cx);
                            check!(editor.document.composition().is_none());
                            check_eq!(editor.document.source.text(), "### 标题");
                            check_eq!(editor.viewport.display.text, "标题");
                            check_eq!(editor.document.head, "### 标题".len());
                            check_eq!(editor.selected_text_range(false, window, cx).ok_or("missing heading selection")?.range, 6..6);
                        }
                        74 => {
                            let (font_size, height) = heading_metrics(3);
                            check_eq!(geometry.lines[0].shaped.unwrapped_layout.font_size, px(font_size));
                            check_eq!(geometry.lines[0].line_height, px(height));
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), "###");
                            check_eq!(editor.viewport.display.text, "###");
                            editor.redo(&Redo, window, cx);
                            check_eq!(editor.document.source.text(), "### 标题");
                            check_eq!(editor.viewport.display.text, "标题");
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
                            check_eq!(editor.viewport.display.text, "**中文");
                        }
                        76 => {
                            editor.enter(&Enter, window, cx);
                            check_eq!(editor.document.source.text(), "**中文**");
                            check_eq!(editor.document.head, "**中文".len());
                            check_eq!(editor.viewport.display.text, "**中文**");
                            check_eq!(editor.selected_text_range(false, window, cx).ok_or("missing inline selection")?.range, 4..4);
                        }
                        77 => {
                            check!(geometry.display.spans.iter().any(|span| span.style.bold && !span.style.marker));
                            let snapshot = editor.document.source.id();
                            for _ in 0..2 { editor.replace_text_in_range(None, "*", window, cx); }
                            check_eq!(editor.document.source.id(), snapshot);
                            check_eq!(editor.viewport.display.text, "中文");
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), "**");
                            editor.redo(&Redo, window, cx);
                            check_eq!(editor.document.source.text(), "**中文**");
                            editor.document = editor.document.fork("\r\n\r\ntail");
                            editor.refresh(cx);
                            for input in ["`", "`", "`", "rust"] { editor.replace_text_in_range(None, input, window, cx); }
                            check_eq!(editor.document.source.text(), "```rust\r\n\r\ntail");
                            check_eq!(editor.document.head, 7);
                            check!(editor.viewport.display.text.starts_with("```rust\n"));
                        }
                        78 => {
                            check_eq!(geometry.lines[0].shaped.text.as_str(), "```rust");
                            editor.enter(&Enter, window, cx);
                            check_eq!(editor.document.source.text(), "```rust\r\n\r\n```\r\n\r\ntail");
                            check_eq!(editor.document.head, 9);
                            check_eq!(editor.viewport.display.code_blocks.len(), 1);
                            check!(!editor.viewport.display.text.contains("```"));
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
                            let control = geometry.append_control.ok_or_else(|| format!("missing append icon after hover: hovered={}, focused={}, scroll={:?}, zone={:?}, epoch={}/{}", editor.menus.append_hovered, editor.focus.is_focused(window), editor.viewport.scroll, geometry.append_zone, editor.viewport.frame_epoch, geometry.frame_epoch))?;
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
                            check!(editor.menus.append_menu.is_some());
                            check!(!editor.document.dirty());
                            editor.escape(&Escape, window, cx);
                            check!(editor.menus.append_menu.is_none());
                            let control = geometry.append_control.ok_or("missing append icon")?;
                            editor.update_block_hover(control.center(), cx);
                        }
                        84 => {
                            check!(geometry.append_menu.is_some());
                            editor.append_block(geometry.snapshot, ryne_extensions::InsertBlock::Code, window, cx);
                            check_eq!(editor.document.source.text(), "```\n\n```");
                            check_eq!(editor.document.head, 4);
                            check!(editor.menus.append_menu.is_none());
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
                            editor.viewport.dragging = true;
                            editor.refresh(cx);
                        }
                        88 => {
                            check!(geometry.append_zone.is_none());
                            editor.viewport.dragging = false;
                            editor.document = editor.document.fork(&"paragraph\n\n".repeat(80));
                            editor.refresh(cx);
                            editor.viewport.scroll = px(100000.);
                            editor.viewport.ensure_caret = false;
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
                            editor.viewport.scroll = px(0.);
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
                            for row in geometry.rows.iter() {
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
                            editor.viewport.scroll = px(600.);
                            editor.viewport.ensure_caret = false;
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
                            editor.viewport.scroll = px(0.);
                            editor.move_to(editor.document.source.text().find("body").ok_or("missing caret body")?, false, cx);
                        }
                        102 => {
                            let mut sizes = Vec::new();
                            for row in geometry.rows.iter() {
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
                            check_eq!(platform, geometry.caret_bounds(offset, editor.caret_state.caret_affinity));
                            check!(!editor.document.dirty());
                            editor.toggle_source(&ToggleSource, window, cx);
                        }
                        103 => {
                            for row in geometry.rows.iter() {
                                check_eq!(geometry.caret_bounds(row.range.start, Affinity::Downstream).size.height, px(theme::CODE.0));
                            }
                            editor.caret_state.caret_visible = false;
                            editor.move_to(editor.document.head, false, cx);
                            check!(editor.caret_state.caret_visible);
                            editor.replace_and_mark_text_in_range(None, "中文", Some(2..2), window, cx);
                            editor.sync_caret_blink(window, cx);
                            check!(editor.caret_state.caret_visible);
                            check!(editor.caret_state.caret_blink_task.is_none());
                            check!(!editor.document.dirty());
                        }
                        104 => {
                            check!(editor.caret_state.caret_visible);
                            check!(editor.caret_state.caret_blink_task.is_none());
                            editor.escape(&Escape, window, cx);
                            editor.select_all(&SelectAll, window, cx);
                            editor.sync_caret_blink(window, cx);
                            check!(editor.caret_state.caret_blink_task.is_none());
                            check!(!editor.document.dirty());
                            editor.move_to(0, false, cx);
                            editor.viewport.dragging = true;
                            editor.sync_caret_blink(window, cx);
                            check!(editor.caret_state.caret_visible);
                            check!(editor.caret_state.caret_blink_task.is_none());
                        }
                        105 => {
                            editor.viewport.dragging = false;
                            let other = cx.focus_handle();
                            window.focus(&other);
                            editor.sync_caret_blink(window, cx);
                            check!(editor.caret_state.caret_blink_task.is_none());
                            window.focus(&editor.focus);
                            editor.reset_caret_blink();
                            editor.sync_caret_blink(window, cx);
                            check!(editor.caret_state.caret_visible);
                            check_eq!(editor.caret_state.caret_blink_task.is_some(), window.is_window_active());
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
                            let layouts = shape_inline(&geometry.display, &runs, range.clone(), theme::BODY, px(160.), window)?;
                            check!(layouts.iter().all(|layout| layout.mixed.is_none()));
                            check_eq!(layouts.iter().map(|layout| layout.shaped.text.as_ref()).collect::<String>(), geometry.display.text);
                            check!(layouts.iter().any(|layout| !layout.shaped.wrap_boundaries.is_empty()));
                            editor.document = editor.document.fork("**a中😀e\u{301}z** ~~strike中😀~~ `code中😀`\n\n$\\frac{a}{b}$\n\n```rust\nlet xy = 1;\n```\n\n| left | right |\n| --- | ---: |\n| a😀b | c中d |\n");
                            let offset = editor.document.source.text().find('😀').ok_or("missing inline caret")?;
                            editor.move_to(offset, false, cx);
                        }
                        108..=117 => {
                            let offset = geometry.display.position_at(editor.caret());
                            let row = geometry.row_for(offset, editor.caret_state.caret_affinity).ok_or("missing caret row")?;
                            let line = &geometry.lines[row.line];
                            // A caret in plain/styled text keeps the original full shaped line.
                            check!(line.mixed.is_none());
                            check_eq!(line.x_for_index(offset - line.start), line.shaped.unwrapped_layout.x_for_index(offset - line.start));
                            let cursor = geometry.caret_bounds(offset, editor.caret_state.caret_affinity);
                            check_eq!(geometry.display.source_at(offset, editor.caret_state.caret_affinity), editor.caret());
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
                            let composition = editor.document.composition().ok_or("missing multilingual composition")?;
                            let composition = (composition.range.clone(), composition.text.clone(), composition.selection.clone());
                            let display = editor.viewport.display.clone();
                            let language = Language::ALL[(stage - 1 - 119) as usize];
                            let menus = cx.get_menus().ok_or("missing native app menus")?;
                            let language_menu = menus[0].items.iter().find_map(|item| match item {
                                OwnedMenuItem::Submenu(menu) if menu.name.as_ref() == editor.locale.language.text(Text::Language) => Some(menu),
                                _ => None,
                            }).ok_or("missing Ryne language submenu")?;
                            let action = language_menu.items.iter().find_map(|item| match item {
                                OwnedMenuItem::Action { action, .. } => action.as_any().downcast_ref::<SelectLanguage>()
                                    .filter(|action| action.preference == Preference(Some(language))).cloned(),
                                _ => None,
                            }).ok_or("missing native language action")?;
                            check!(window.is_action_available(&action, cx));
                            editor.change_language(&action, window, cx);
                            check_eq!(editor.locale.language, language);
                            check_eq!(editor.document.source.id(), source.id());
                            check_eq!(editor.document.source.text(), source.text());
                            check_eq!(editor.document.selection(), selection);
                            check_eq!(editor.document.dirty(), dirty);
                            check_eq!(editor.document.history_len(false), undo);
                            check_eq!(editor.document.history_len(true), redo);
                            check_eq!(editor.viewport.display, display);
                            let current = editor.document.composition().ok_or("composition lost during language change")?;
                            check_eq!((current.range.clone(), current.text.clone(), current.selection.clone()), composition);
                            let path = std::env::temp_dir().join(format!("ryne-language-smoke-{}", std::process::id()));
                            check_eq!(Preference::read(&path), Preference(Some(language)));
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
                            let level = (stage - 127) as u8;
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
                            editor.menus.hovered_render = None;
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
                            check!(editor.menus.render_menu.is_some());check_eq!(editor.document.selection(),selection);check_eq!(editor.document.history_len(false),0);
                        }
                        136 => {
                            check_eq!(geometry.render_menu_bounds.len(),1);
                            let menu=editor.menus.render_menu.as_ref().ok_or("missing cell popup")?;
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
                            check!(editor.menus.render_menu.is_none());
                            editor.undo(&Undo,window,cx);
                            check!(!editor.document.dirty());
                            editor.menus.hovered_render=None;
                            let at=editor.document.source.text().find("中😀").ok_or("missing cell text")?;
                            editor.move_to(at,false,cx);
                        }
                        139 => {
                            check_eq!(geometry.render_buttons.len(),1);
                            check!(editor.menus.hovered_render.is_none());
                            editor.render_shortcut(&KeyDownEvent {keystroke:Keystroke {key:"down".into(),modifiers:Modifiers {control:true,platform:cfg!(target_os="macos"),..Default::default()},..Default::default()},is_held:false},window,cx);
                            check_eq!(editor.document.history_len(false),1);
                        }
                        140 => {
                            check_eq!(editor.viewport.display.tables[0].source.rows.len(),4);
                            let old=geometry.render_key(window.viewport_size());
                            let region=geometry.render_regions[0].key();
                            editor.undo(&Undo,window,cx);
                            editor.open_render_menu(old,region,window,cx);
                            check!(editor.menus.render_menu.is_none());check!(!editor.document.dirty());
                        }
                        141 => {
                            check_eq!(geometry.render_buttons.len(),1);
                            let region=geometry.render_regions.iter().find(|r|r.controls[0].target.part==[1,0]).ok_or("missing focused cell")?;
                            editor.open_render_menu(geometry.render_key(window.viewport_size()),region.key(),window,cx);
                        }
                        142 => {
                            let menu=editor.menus.render_menu.as_mut().ok_or("missing focused popup")?;
                            menu.selected=menu.rows.iter().position(|r|matches!(r,RenderMenuRow::Action(i) if i.request.command=="table.delete-column")).ok_or("missing column action")?;
                            editor.enter(&Enter,window,cx);
                            check_eq!(editor.document.history_len(false),1);
                        }
                        143 => {
                            check_eq!(editor.viewport.display.tables[0].source.align.len(),1);
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
                            check!(editor.document.composition().is_some());
                            check!(geometry.render_buttons.is_empty());check!(geometry.render_menu_bounds.is_empty());
                            check_eq!(geometry.render_regions.len(),6);
                            editor.escape(&Escape,window,cx);
                            check!(!editor.document.dirty());
                        }
                        147 => {
                            editor.viewport.dragging=true;editor.refresh(cx);
                        }
                        148 => {
                            check!(geometry.render_buttons.is_empty());check!(geometry.render_menu_bounds.is_empty());
                            editor.viewport.dragging=false;editor.refresh(cx);
                        }
                        149 => {
                            window.resize(size(px(640.), px(680.)));
                            editor.document=editor.document.fork("9. parent\r\n   - child\r\n10. second\r\n    - other\r\n\r\noutside\r\n");
                            editor.menus.hovered_render=None;
                            let end=editor.document.source.text().len()-2;editor.move_to(end,false,cx);
                        }
                        150 => {
                            check_eq!(geometry.render_regions.len(),4);check!(geometry.render_buttons.is_empty());
                            let source=editor.document.source.id();let selection=editor.document.selection();let display=editor.viewport.display.clone();
                            let at=editor.document.source.text().find("second").ok_or("missing list item")?;
                            let region=geometry.render_regions.iter().find(|r|r.controls[0].focus.start==at).ok_or("missing ordered item region")?;
                            editor.mouse_move(&MouseMoveEvent{position:region.trigger.center(),..Default::default()},window,cx);
                            check_eq!(editor.document.source.id(),source);check_eq!(editor.document.selection(),selection);check_eq!(editor.viewport.display,display);check!(!editor.document.dirty());
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
                            check!(control.left()>=geometry.bounds.left());
                            let first=geometry.render_regions.iter().find(|r|r.controls[0].focus.start==3).ok_or("missing first list item")?;
                            check_eq!(control.center().y,first.trigger.center().y);
                            check!(control.right()+px(4.)<=first.trigger.left());
                            let source=editor.document.source.id();let selection=editor.document.selection();let display=editor.viewport.display.clone();
                            editor.update_block_hover(control.center(),cx);
                            check_eq!(editor.menus.block_menu.as_ref().map(|(_,b)|b),Some(target));
                            check_eq!(editor.document.source.id(),source);check_eq!(editor.document.selection(),selection);check_eq!(editor.viewport.display,display);check!(!editor.document.dirty());
                            let selection=editor.document.selection();editor.open_render_menu(geometry.render_key(window.viewport_size()),region.key(),window,cx);
                            check_eq!(editor.document.selection(),selection);check_eq!(editor.document.history_len(false),0);
                        }
                        152 => {
                            check_eq!(geometry.render_menu_bounds.len(),1);
                            let menu=editor.menus.render_menu.as_mut().ok_or("missing list menu")?;check_eq!(menu.rows.len(),7);
                            menu.selected=menu.rows.iter().position(|r|matches!(r,RenderMenuRow::Action(i) if i.request.command=="list.move-up")).ok_or("missing move action")?;
                            editor.enter(&Enter,window,cx);
                        }
                        153 => {
                            check_eq!(editor.document.source.text(),"9. second\r\n   - other\r\n10. parent\r\n    - child\r\n\r\noutside\r\n");check_eq!(editor.document.history_len(false),1);
                            editor.undo(&Undo,window,cx);check!(!editor.document.dirty());
                            editor.menus.hovered_render=None;let at=editor.document.source.text().find("child").ok_or("missing nested item")?;editor.move_to(at,false,cx);
                        }
                        154 => {
                            check_eq!(geometry.render_buttons.len(),1);
                            let target=geometry.block_target.as_ref().ok_or("missing nested whole-list target")?;
                            check!(target.is_list());check_eq!(target.range.start,0);
                            check_eq!(ryne_extensions::BlockMetadata::from(target.kind).icon,Some("text_number_list_ltr"));
                            let control=geometry.block_control.ok_or("missing nested whole-list control")?;
                            let first=geometry.render_regions.iter().find(|r|r.controls[0].focus.start==3).ok_or("missing outer first item")?;
                            check_eq!(control.center().y,first.trigger.center().y);
                            check!(control.left()>=geometry.bounds.left());
                            check!(control.right()+px(4.)<=first.trigger.left());
                            editor.render_shortcut(&KeyDownEvent{keystroke:Keystroke{key:"enter".into(),modifiers:Modifiers{alt:true,..Default::default()},..Default::default()},is_held:false},window,cx);
                            let menu=editor.menus.render_menu.as_ref().ok_or("missing nested list menu")?;
                            let marker=editor.document.source.text().find("- child").ok_or("missing child marker")?;
                            check!(menu.rows.iter().all(|r|matches!(r,RenderMenuRow::Action(i) if i.request.target.part==[marker])));
                        }
                        155 => {
                            let menu=editor.menus.render_menu.as_mut().ok_or("missing nested menu")?;
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
                            check!(geometry.render_regions.is_empty());editor.toggle_source(&ToggleSource,window,cx);editor.viewport.dragging=true;editor.refresh(cx);
                        }
                        159 => {
                            check!(geometry.render_buttons.is_empty());editor.viewport.dragging=false;
                            editor.document=editor.document.fork("- ```rust\n  let x = 1;\n  ```\n- second\n\noutside\n");
                            editor.menus.hovered_render=None;let end=editor.document.source.text().len()-1;editor.move_to(end,false,cx);
                        }
                        160 => {
                            check_eq!(geometry.render_regions.iter().filter(|r| r.controls[0].provider == "list").count(),2);
                            let at=editor.document.source.text().find("```rust").ok_or("missing list code fence")?;
                            let region=geometry.render_regions.iter().find(|r|r.controls[0].focus.start==at).ok_or("missing panel-first item region")?;
                            editor.mouse_move(&MouseMoveEvent{position:region.trigger.center(),..Default::default()},window,cx);
                        }
                        161 => {
                            check_eq!(geometry.render_buttons.len(),2);
                            let region=geometry.render_regions.iter().find(|r|r.controls[0].focus.start==2).ok_or("missing code list trigger")?;
                            editor.open_render_menu(geometry.render_key(window.viewport_size()),region.key(),window,cx);check!(editor.menus.render_menu.is_some());check!(!editor.document.dirty());
                            editor.escape(&Escape,window,cx);
                            editor.document=editor.document.fork("- first\n  - child\n- second\n\noutside\n");editor.menus.hovered_render=None;editor.move_to(2,false,cx);
                        }
                        162 => {
                            check_eq!(geometry.render_buttons.len(),1);
                            let target=geometry.block_target.clone().ok_or("missing unordered list block")?;
                            check!(target.is_list());
                            check_eq!(&editor.document.source.text()[target.range.clone()],"- first\n  - child\n- second\n\n");
                            let source=editor.document.source.id();let selection=editor.document.selection();
                            let control=geometry.block_control.ok_or("missing unordered list icon")?;
                            editor.update_block_hover(control.center(),cx);
                            check_eq!(editor.menus.block_menu.as_ref().map(|(_,b)|b),Some(&target));
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
                            window.resize(size(px(960.), px(680.)));
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
                                    check!(editor.menus.block_menu.is_none());
                                    check!(editor.menus.render_menu.is_none());
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
                                    editor.menus.hovered_block = Some((source.id(), block.clone()));
                                    editor.open_block_menu(source.id(), block, cx);
                                }
                                1 => {
                                    let bounds = geometry.block_menu.ok_or("missing translated block menu")?;
                                    for command in [BlockAction::Source, BlockAction::Select, BlockAction::Copy] {
                                        check!(menu_text_width(language.block_action(command, false), 13., window) + px(24.) <= bounds.size.width);
                                    }
                                    let region = geometry.render_regions.iter().find(|r| r.controls[0].provider == "table").ok_or("missing localized table controls")?;
                                    editor.open_render_menu(geometry.render_key(window.viewport_size()), region.key(), window, cx);
                                    check!(editor.menus.render_menu.is_some());
                                    editor.right(&Right, window, cx);
                                    check!(editor.menus.render_menu.as_ref().is_some_and(|m| m.submenu.is_some()));
                                }
                                2 => {
                                    if geometry.render_menu_bounds.len() != 2 {
                                        return Err(format!("translated submenu missing: language={language:?}, bounds={}, menu={:?}, frame={:?}, triggers={:?}", geometry.render_menu_bounds.len(), editor.menus.render_menu.as_ref().map(|m| (&m.key, &m.region, m.selected, m.submenu)), geometry.render_key(window.viewport_size()), geometry.render_buttons));
                                    }
                                    let menu = editor.menus.render_menu.as_ref().ok_or("missing translated table menu")?;
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
                                    if geometry.render_menu_bounds.len() != 1 {
                                        return Err(format!("translated list menu missing: language={language:?}, bounds={}, menu={:?}, frame={:?}, triggers={:?}", geometry.render_menu_bounds.len(), editor.menus.render_menu.as_ref().map(|m| (&m.key, &m.region, m.selected, m.submenu)), geometry.render_key(window.viewport_size()), geometry.render_buttons));
                                    }
                                    let menu = editor.menus.render_menu.as_ref().ok_or("missing translated list menu")?;
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
                        194..=200 => {
                            if stage - 1 == 194 {
                                editor.document = editor.document.fork(Language::En.sample());
                                editor.default_document = Some(welcome::DefaultDocument::new(&editor.document, Language::En));
                            } else {
                                check_eq!(geometry.display.code_blocks.len() + geometry.display.diagrams.len(), 5);
                                check_eq!(geometry.display.diagrams.len(), 1);
                                check_eq!(geometry.display.tables.len(), 1);
                            }
                            let language = Language::ALL[(stage - 1 - 194) as usize];
                            editor.select_language(Preference(Some(language)), cx);
                            check_eq!(editor.document.source.text(), language.sample());
                            check!(!editor.document.dirty());
                            check_eq!(editor.document.history_len(false), 0);
                            check_eq!(editor.document.history_len(true), 0);
                            check!(editor.viewport.geometry.is_none(), "guide change must invalidate native geometry");
                            check_eq!(editor.viewport.scroll, px(0.));
                        }
                        201 => {
                            check_eq!(editor.document.source.text(), Language::Es.sample());
                            check_eq!(geometry.display.code_blocks.len() + geometry.display.diagrams.len(), 5);
                            check_eq!(geometry.display.diagrams.len(), 1);
                            check_eq!(geometry.display.tables.len(), 1);
                            editor.document.set_source_mode(true);
                            editor.select_language(Preference(None), cx);
                            check_eq!(editor.document.source.text(), Language::En.sample());
                            check!(editor.document.source_mode);
                        }
                        202 => {
                            check_eq!(geometry.display.text, Language::En.sample());
                            editor.document.insert_text(0..0, "中😀\r\n").map_err(|e| e.to_string())?;
                            let source = editor.document.source.clone();
                            let selection = editor.document.selection();
                            editor.select_language(Preference(Some(Language::Ja)), cx);
                            check_eq!(editor.document.source.id(), source.id());
                            check_eq!(editor.document.selection(), selection);
                            check!(editor.document.dirty());
                            check_eq!(editor.document.history_len(false), 1);
                            editor.document.undo(false);
                            check_eq!(editor.document.source.text(), Language::En.sample());
                            editor.select_language(Preference(Some(Language::Fr)), cx);
                            check_eq!(editor.document.source.text(), Language::En.sample());
                            check_eq!(editor.document.history_len(true), 1);
                            editor.refresh(cx);
                        }
                        203 => {
                            editor.select_language(Preference(None), cx);
                            check_eq!(editor.document.source.text(), Language::En.sample());
                            check_eq!(editor.document.history_len(true), 1);
                            std::fs::remove_file(std::env::temp_dir().join(format!("ryne-language-smoke-{}", std::process::id()))).map_err(|e| e.to_string())?;
                            println!("Default guides passed: native layout in all seven languages, format panels, language/system switching, source-mode preservation, fresh geometry, clean history and protection of edited guides and redo");
                        }
                        204 => {
                            let snapshot = editor.document.source.id();
                            let selection = editor.document.selection();
                            let undo = editor.document.history_len(false);
                            editor.change_language(&SelectLanguage { preference: Preference(None) }, window, cx);
                            check_eq!(editor.document.source.id(), snapshot);
                            check_eq!(editor.document.selection(), selection);
                            check_eq!(editor.document.history_len(false), undo);
                            let menus = cx.get_menus().ok_or("missing native app menus")?;
                            check_eq!(menus.len(), 2);
                            check_eq!(menus[0].name.as_ref(), "Ryne");
                            check_eq!(menus[1].name.as_ref(), editor.locale.language.text(Text::File));
                            check!(menus[1].items.iter().any(|item| matches!(item,
                                OwnedMenuItem::Action { name, action, .. }
                                    if name.as_str() == editor.locale.language.text(Text::ExportPdf)
                                        && action.as_any().downcast_ref::<menus::ExportRegistered>().is_some_and(|a| a.id == "pdf"))));
                            check!(menus[1].items.iter().any(|item| matches!(item,
                                OwnedMenuItem::Action { name, action, .. }
                                    if name.as_str() == editor.locale.language.text(Text::ExportPdfV2)
                                        && action.as_any().downcast_ref::<menus::ExportRegistered>().is_some_and(|a| a.id == "pdf-v2"))));
                            editor.escape(&Escape, window, cx);
                            editor.install_document(editor.document.fork("clean document"), Text::Opened, window, cx);
                            editor.new_document(&New, window, cx);
                        }
                        205 => {
                            check!(!editor.operations.file_operation_pending.is_pending());
                            check_eq!(editor.document.source.text(), "");
                            check!(!editor.document.dirty());
                            check!(editor.document.files.path.is_none());
                            check!(editor.default_document.is_none());
                            let path = std::env::temp_dir().join(format!("ryne-menu-smoke-{}.md", std::process::id()));
                            std::fs::write(&path, "# 原始\r\n\r\n```text\r\n**literal**\r\n```\r\n").map_err(|e| e.to_string())?;
                            editor.open_recent(&OpenRecent { path }, window, cx);
                        }
                        206 => {
                            check!(!editor.operations.file_operation_pending.is_pending());
                            check_eq!(editor.document.source.text(), "# 原始\r\n\r\n```text\r\n**literal**\r\n```\r\n");
                            let path = editor.document.files.path.clone().ok_or("missing opened recent path")?;
                            check_eq!(editor.recent.paths.first(), Some(&std::fs::canonicalize(&path).map_err(|e| e.to_string())?));
                            std::fs::write(&path, "# Reloaded 中文\r\n\r\n- item\r\n").map_err(|e| e.to_string())?;
                            editor.reload(&Reload, window, cx);
                        }
                        207 => {
                            check!(!editor.operations.file_operation_pending.is_pending());
                            check_eq!(editor.document.source.text(), "# Reloaded 中文\r\n\r\n- item\r\n");
                            check!(!editor.document.dirty());
                            check_eq!(editor.document.history_len(false), 0);
                            let path = editor.document.files.path.clone().ok_or("missing reload path")?;
                            editor.open_recent(&OpenRecent { path }, window, cx);
                            editor.replace_text_in_range(None, "new edit", window, cx);
                        }
                        208 => {
                            check!(!editor.operations.file_operation_pending.is_pending());
                            check!(editor.document.source.text().contains("new edit"));
                            check!(editor.document.dirty());
                            let current = editor.document.source.id();
                            check!(editor.can_replace(current));
                            editor.document.update_composition(Some(0..0), "预编辑", Some(0..0))?;
                            check!(!editor.can_replace(current));
                            editor.document.cancel_composition();
                            editor.document.files.saving = true;
                            check!(!editor.can_replace(current));
                            editor.document.files.saving = false;
                            let path = editor.document.files.path.clone().ok_or("missing smoke cleanup path")?;
                            std::fs::remove_file(path).map_err(|e| e.to_string())?;
                            editor.clear_recent(&ClearRecent, window, cx);
                            check!(editor.recent.paths.is_empty());
                            println!("Application menus passed: localized native menus, language preference without document changes, blank new document, recent open, CRLF reload, concurrent-edit rejection, IME/save guards and clear history");
                        }
                        209 => {
                            editor.document = editor.document.fork("- [ ] 待办😀\r\n  1. [X] 已完成\r\n- [ ] \r\n\r\noutside\r\n");
                            editor.move_to(editor.document.source.text().find("outside").ok_or("missing task outside")?, false, cx);
                        }
                        210 => {
                            check!(!editor.viewport.display.text.contains("[ ]"));
                            check!(!editor.viewport.display.text.contains("[X]"));
                            let tasks = geometry.render_regions.iter().filter(|r| matches!(r.controls[0].placement, ryne_extensions::RenderControlPlacement::Marker { .. })).collect::<Vec<_>>();
                            check_eq!(tasks.len(), 3);
                            for task in &tasks {
                                check!(geometry.render_buttons.contains(&task.trigger));
                                let control = &task.controls[0];
                                let start = geometry.display.position_at(control.focus.start);
                                let row = geometry.row_for(start, Affinity::Downstream).ok_or("missing task text row")?;
                                check_eq!(task.trigger.center().y, geometry.bounds.top() + px(geometry.page.top) + row.top + row.line_height / 2. - geometry.scroll);
                                check!(task.trigger.right() < geometry.position(start).x);
                            }
                            let selection = editor.document.selection();
                            let key = geometry.render_key(window.viewport_size());
                            let id = tasks[0].key();
                            editor.activate_render_control(key.clone(), id.clone(), window, cx);
                            check!(editor.document.source.text().starts_with("- [x] 待办😀"));
                            check_eq!(editor.document.selection(), selection);
                            check_eq!(editor.document.history_len(false), 1);
                            // The old frame must not toggle again after its source revision changes.
                            editor.activate_render_control(key, id, window, cx);
                            check_eq!(editor.document.history_len(false), 1);
                        }
                        211 => {
                            let task = geometry.render_regions.iter().find(|r| matches!(r.controls[0].placement, ryne_extensions::RenderControlPlacement::Marker { icon: "checkbox_checked" })).ok_or("missing checked task SVG")?;
                            editor.activate_render_control(geometry.render_key(window.viewport_size()), task.key(), window, cx);
                            check!(editor.document.source.text().starts_with("- [ ] 待办😀"));
                            check_eq!(editor.document.history_len(false), 2);
                            editor.undo(&Undo, window, cx);
                            editor.undo(&Undo, window, cx);
                            check!(!editor.document.dirty());
                            editor.document.update_composition(Some(6..6), "预编辑", Some(0..0))?;
                            editor.refresh(cx);
                        }
                        212 => {
                            check!(geometry.render_buttons.is_empty());
                            let task = geometry.render_regions.iter().find(|r| matches!(r.controls[0].placement, ryne_extensions::RenderControlPlacement::Marker { .. })).ok_or("missing preedit task marker")?;
                            editor.activate_render_control(geometry.render_key(window.viewport_size()), task.key(), window, cx);
                            check!(!editor.document.dirty());
                            editor.escape(&Escape, window, cx);
                            editor.toggle_source(&ToggleSource, window, cx);
                        }
                        213 => {
                            check!(geometry.render_regions.is_empty());
                            check!(editor.viewport.display.text.contains("[ ]"));
                            check!(editor.viewport.display.text.contains("[X]"));
                            editor.toggle_source(&ToggleSource, window, cx);
                            editor.viewport.dragging = true;
                            editor.refresh(cx);
                        }
                        214 => {
                            check!(geometry.render_buttons.is_empty());
                            editor.viewport.dragging = false;
                            editor.refresh(cx);
                            println!("Task checkboxes passed: embedded checked/unchecked SVGs, nested real-window marker geometry, toggle with preserved selection and atomic undo, stale frame and source/IME/drag protection");
                        }
                        215 => {
                            editor.document = editor.document.fork("# first\r\n\r\nsecond😀\r\n");
                            editor.move_to(2, false, cx);
                        }
                        216 => {
                            let control = geometry.block_control.ok_or("missing first block control")?;
                            editor.update_block_hover(control.center(), cx);
                        }
                        217 => {
                            let (snapshot, block) = editor.menus.block_menu.clone().ok_or("missing first block menu")?;
                            check!(!block::can_insert_above(&editor.document.parse, &block));
                            let source = editor.document.source.clone();
                            editor.block_action(snapshot, &block, BlockAction::AddAbove, cx);
                            check!(editor.menus.block_insert.is_none());
                            editor.block_action(snapshot, &block, BlockAction::AddBelow, cx);
                            check_eq!(editor.menus.block_insert, Some(false));
                            check_eq!(editor.document.source.id(), source.id());
                            check!(!editor.document.dirty());
                        }
                        218 => {
                            let picker = geometry.block_menu.ok_or("missing adjacent type picker")?;
                            check!(geometry.bounds.contains(&picker.origin));
                            check!(picker.right() <= geometry.bounds.right());
                            check!(picker.bottom() <= geometry.bounds.bottom());
                            let zone = geometry.append_zone.ok_or("missing picker-overlapped append zone")?;
                            let overlap = point(picker.center().x, zone.center().y);
                            check!(picker.contains(&overlap));
                            editor.update_block_hover(overlap, cx);
                            check!(editor.menus.block_menu.is_some());
                            check!(!editor.menus.append_hovered);
                            editor.update_block_hover(picker.center(), cx);
                            check!(editor.menus.block_menu.is_some());
                            check_eq!(editor.menus.block_insert, Some(false));
                            editor.escape(&Escape, window, cx);
                            check!(editor.menus.block_menu.is_some());
                            check!(editor.menus.block_insert.is_none());
                            let (snapshot, block) = editor.menus.block_menu.clone().ok_or("missing retained insertion target")?;
                            editor.block_action(snapshot, &block, BlockAction::AddBelow, cx);
                            editor.insert_adjacent_block(snapshot, &block, false, ryne_extensions::InsertBlock::Code, window, cx);
                            check_eq!(editor.document.history_len(false), 1);
                            check!(editor.menus.block_menu.is_none());
                            check!(editor.document.source.text().contains("```\r\n\r\n```"));
                        }
                        219 => {
                            check_eq!(geometry.display.code_blocks.len(), 1);
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), "# first\r\n\r\nsecond😀\r\n");
                            check!(!editor.document.dirty());
                            let caret = editor.document.source.text().find("second").ok_or("missing second block")?;
                            editor.move_to(caret, false, cx);
                        }
                        220 => {
                            let control = geometry.block_control.ok_or("missing second block control")?;
                            editor.update_block_hover(control.center(), cx);
                        }
                        221 => {
                            let (snapshot, block) = editor.menus.block_menu.clone().ok_or("missing second block menu")?;
                            check!(block::can_insert_above(&editor.document.parse, &block));
                            editor.block_action(snapshot, &block, BlockAction::AddAbove, cx);
                            check_eq!(editor.menus.block_insert, Some(true));
                        }
                        222 => {
                            check!(geometry.block_menu.is_some());
                            let (snapshot, block) = editor.menus.block_menu.clone().ok_or("missing above insertion target")?;
                            let selection = editor.document.selection();
                            editor.document.update_composition(Some(selection.clone()), "临", Some(0..0))?;
                            editor.insert_adjacent_block(snapshot, &block, true, ryne_extensions::InsertBlock::Table, window, cx);
                            check!(!editor.document.dirty());
                            editor.document.cancel_composition();
                            editor.menus.hovered_block = Some((snapshot, block.clone()));
                            editor.open_block_menu(snapshot, block.clone(), cx);
                            editor.block_action(snapshot, &block, BlockAction::AddAbove, cx);
                            editor.insert_adjacent_block(snapshot, &block, true, ryne_extensions::InsertBlock::Table, window, cx);
                            check_eq!(editor.document.history_len(false), 1);
                            check!(editor.document.source.text().find("| --- | --- |").ok_or("missing inserted table")? < editor.document.source.text().find("second").ok_or("missing original second block")?);
                            let inserted = editor.document.source.id();
                            editor.insert_adjacent_block(snapshot, &block, true, ryne_extensions::InsertBlock::Heading(2), window, cx);
                            check_eq!(editor.document.source.id(), inserted);
                        }
                        223 => {
                            check_eq!(geometry.display.tables.len(), 1);
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), "# first\r\n\r\nsecond😀\r\n");
                            check!(!editor.document.dirty());
                            println!("Adjacent block insertion passed: first-block below-only, shared type picker geometry and hover, Escape back, above/below source transactions, CRLF, undo, stale target and IME protection");
                        }
                        224 => {
                            editor.document = editor.document.fork("plain😀\r\n\r\n$\\frac{a_1}{\\sqrt{b}}$\r\n\r\n- $x^2$\r\n\r\n| f |\r\n| --- |\r\n| $x_i$ |\r\n");
                            editor.move_to(5, false, cx);
                        }
                        225 => {
                            check_eq!(editor.viewport.display.formulas.len(), 3);
                            let before = editor.viewport.display.formulas.clone();
                            let source = editor.document.source.clone();
                            let selection = editor.document.selection();
                            let menus = cx.get_menus().ok_or("missing native menus")?;
                            let action = menus[0].items.iter().find_map(|item| match item {
                                OwnedMenuItem::Submenu(menu) if menu.name.as_ref() == editor.locale.language.text(Text::MathFont) => menu.items.iter().find_map(|item| match item {
                                    OwnedMenuItem::Action { action, .. } => action.as_any().downcast_ref::<SelectMathFont>().filter(|action| action.font == MathFont::StixTwo).cloned(),
                                    _ => None,
                                }),
                                _ => None,
                            }).ok_or("missing STIX action")?;
                            check!(window.is_action_available(&action, cx));
                            editor.change_math_font(&action, window, cx);
                            check_eq!(editor.formulas.font(), MathFont::StixTwo);
                            check!(!geometry.is_current(editor));
                            check_eq!(editor.document.source.id(), source.id());
                            check_eq!(editor.document.source.text(), source.text());
                            check_eq!(editor.document.selection(), selection);
                            check_eq!(editor.document.history_len(false), 0);
                            check!(!editor.document.dirty());
                            for (old, new) in before.iter().zip(&editor.viewport.display.formulas) {
                                check!(old.formula.svg != new.formula.svg);
                                check!(new.formula.path.starts_with("math:stix-two-math:"));
                            }
                        }
                        226 => {
                            check_eq!(geometry.display.formulas, editor.viewport.display.formulas);
                            editor.replace_and_mark_text_in_range(None, "にほん", Some(3..3), window, cx);
                            let composition = editor.document.composition().ok_or("missing preedit")?;
                            let composition = (composition.range.clone(), composition.text.clone(), composition.selection.clone());
                            let display = editor.viewport.display.clone();
                            let source = editor.document.source.id();
                            editor.change_math_font(&SelectMathFont { font: MathFont::LeteSans }, window, cx);
                            check_eq!(editor.formulas.font(), MathFont::StixTwo);
                            check_eq!(editor.viewport.display, display);
                            check_eq!(editor.document.source.id(), source);
                            let current = editor.document.composition().ok_or("lost preedit")?;
                            check_eq!(current.range, composition.0);
                            check_eq!(current.text, composition.1);
                            check_eq!(current.selection, composition.2);
                            editor.document.cancel_composition();
                            editor.refresh(cx);
                            check_eq!(editor.formulas.font(), MathFont::LeteSans);
                        }
                        227 => {
                            check!(geometry.display.formulas.iter().all(|f| f.formula.path.starts_with("math:lete-sans-math:")));
                            editor.viewport.dragging = true;
                            let display = editor.viewport.display.clone();
                            editor.change_math_font(&SelectMathFont { font: MathFont::StixTwo }, window, cx);
                            check_eq!(editor.formulas.font(), MathFont::LeteSans);
                            check_eq!(editor.viewport.display, display);
                            editor.viewport.dragging = false;
                            editor.refresh(cx);
                            check_eq!(editor.formulas.font(), MathFont::StixTwo);
                        }
                        228 => {
                            editor.change_math_font(&SelectMathFont { font: MathFont::LeteSans }, window, cx);
                            check!(!editor.document.dirty());
                            check_eq!(editor.document.history_len(false), 0);
                            check_eq!(editor.document.history_len(true), 0);
                            let path = std::env::temp_dir().join(format!("ryne-math-font-smoke-{}", std::process::id()));
                            check_eq!(math_fonts::MathFontPreference::load(Some(path.clone())).font, MathFont::LeteSans);
                            std::fs::remove_file(path).map_err(|e| e.to_string())?;
                            println!("Math font menu passed: native action availability, Lete/STIX live metrics and SVG identity, fresh geometry, persisted preference, IME/drag deferral, unchanged Unicode/CRLF source, selection, dirty and history");
                        }
                        229 => {
                            editor.document = editor.document.fork("#标题😀\r\n\r\n\r\n* __正文__\r\n\r\n~~~rust\r\n__literal__\r\n~~~\r\n");
                            editor.move_to("#标题😀".len(), false, cx);
                        }
                        230 => {
                            check!(window.is_action_available(&FormatSource, cx));
                            let before = editor.document.source.id();
                            editor.format_source(&FormatSource, window, cx);
                            check!(!geometry.is_current(editor));
                            check!(editor.document.source.id() != before);
                            check_eq!(editor.document.source.text(), "# 标题😀\r\n\r\n- **正文**\r\n\r\n```rust\r\n__literal__\r\n```\r\n");
                            check_eq!(editor.document.history_len(false), 1);
                            check!(editor.document.dirty());
                        }
                        231 => {
                            check_eq!(geometry.snapshot, editor.document.source.id());
                            let before = editor.document.source.id();
                            editor.format_source(&FormatSource, window, cx);
                            check_eq!(editor.document.source.id(), before);
                            check_eq!(editor.document.history_len(false), 1);
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), "#标题😀\r\n\r\n\r\n* __正文__\r\n\r\n~~~rust\r\n__literal__\r\n~~~\r\n");
                            check!(!editor.document.dirty());
                        }
                        232 => {
                            editor.redo(&Redo, window, cx);
                            let source = editor.document.source.id();
                            let selection = editor.document.selection();
                            editor.document.update_composition(Some(selection), "未", Some(0..0))?;
                            editor.format_source(&FormatSource, window, cx);
                            check!(editor.document.composition().is_some());
                            check_eq!(editor.document.source.id(), source);
                            editor.document.cancel_composition();
                            editor.viewport.dragging = true;
                            editor.format_source(&FormatSource, window, cx);
                            check_eq!(editor.document.source.id(), source);
                            editor.viewport.dragging = false;
                            editor.refresh(cx);
                            println!("Source formatting passed: toolbar action availability, explicit heading repair, marker/fence rules, CRLF and literal code, fresh geometry, atomic undo/redo, idempotence and IME/drag protection");
                        }
                        233 => {
                            editor.document = editor.document.fork("plain text 中文😀\n\n| A | B |\n| --- | --- |\n| $x^2$ | text |\n\n```rust\nlet x = 1;\n```\n");
                            editor.viewport.scroll = px(0.);
                            editor.move_to(0, false, cx);
                            window.resize(size(px(960.), px(680.)));
                        }
                        234 => {
                            cached_layout = Some(geometry.text_layout.clone());
                            cached_projection = Some(editor.viewport.display.clone());
                            cached_controls = Some(editor.viewport.render_controls.clone());
                            editor.caret_state.caret_visible = !editor.caret_state.caret_visible;
                            cx.notify();
                        }
                        235 => {
                            check!(cached_layout.as_ref().is_some_and(|old| std::sync::Arc::ptr_eq(old, &geometry.text_layout)), "blink rebuilt text layout");
                            editor.move_to(1, false, cx);
                            editor.menus.hovered_render = editor.viewport.render_controls.first().map(|c| c.id.clone());
                            cx.notify();
                        }
                        236 => {
                            check!(cached_projection.as_ref().is_some_and(|old| std::sync::Arc::ptr_eq(old, &editor.viewport.display)), "caret rebuilt projection");
                            check!(cached_controls.as_ref().is_some_and(|old| std::sync::Arc::ptr_eq(old, &editor.viewport.render_controls)), "caret rebuilt controls");
                            check!(cached_layout.as_ref().is_some_and(|old| std::sync::Arc::ptr_eq(old, &geometry.text_layout)), "caret/hover rebuilt text layout");
                            window.resize(size(px(640.), px(680.)));
                        }
                        237 => {
                            check!(cached_layout.as_ref().is_some_and(|old| !std::sync::Arc::ptr_eq(old, &geometry.text_layout)), "resize reused stale layout");
                            cached_layout = Some(geometry.text_layout.clone());
                            editor.fonts.body.weight = FontWeight::MEDIUM;
                            cx.notify();
                        }
                        238 => {
                            check!(cached_layout.as_ref().is_some_and(|old| !std::sync::Arc::ptr_eq(old, &geometry.text_layout)), "font change reused stale layout");
                            cached_layout = Some(geometry.text_layout.clone());
                            let control = std::sync::Arc::make_mut(&mut editor.viewport.render_controls).first_mut().ok_or("missing spacing control")?;
                            control.trailing_space += 8.;
                            editor.viewport.control_index = text_layout::ControlIndex::new(&editor.viewport.render_controls);
                            cx.notify();
                        }
                        239 => {
                            check!(cached_layout.as_ref().is_some_and(|old| !std::sync::Arc::ptr_eq(old, &geometry.text_layout)), "control spacing reused stale layout");
                            cached_layout = Some(geometry.text_layout.clone());
                            editor.insert(0..0, "x", cx);
                            check!(!geometry.is_current(editor));
                        }
                        240 => {
                            check!(cached_layout.as_ref().is_some_and(|old| !std::sync::Arc::ptr_eq(old, &geometry.text_layout)), "edit reused stale layout");
                            cached_layout = Some(geometry.text_layout.clone());
                            editor.replace_and_mark_text_in_range(None, "未定😀", Some(3..3), window, cx);
                            check!(!geometry.is_current(editor));
                        }
                        241 => {
                            check!(cached_layout.as_ref().is_some_and(|old| !std::sync::Arc::ptr_eq(old, &geometry.text_layout)), "IME reused stale layout");
                            cached_layout = Some(geometry.text_layout.clone());
                            editor.escape(&Escape, window, cx);
                        }
                        242 => {
                            check!(cached_layout.as_ref().is_some_and(|old| !std::sync::Arc::ptr_eq(old, &geometry.text_layout)), "cancelled IME retained temporary layout");
                            cached_layout = Some(geometry.text_layout.clone());
                            editor.toggle_source(&ToggleSource, window, cx);
                        }
                        243 => {
                            check!(cached_layout.as_ref().is_some_and(|old| !std::sync::Arc::ptr_eq(old, &geometry.text_layout)), "source mode reused rendered layout");
                            check_eq!(editor.document.history_len(false), 1);
                            check!(editor.document.source.text().starts_with("xplain text"));
                            editor.undo(&Undo, window, cx);
                            check!(!editor.document.dirty());
                            window.resize(size(px(960.), px(680.)));
                            println!("Layout cache passed: blink/caret/hover reuse; resize/font/control-space/edit/IME/source-mode invalidation; unchanged document/history and shared source geometry");
                        }
                        244 => {
                            struct RejectIme;
                            impl ryne_extensions::Validator for RejectIme {
                                fn validate(&self, _: &ryne_extensions::EditingState, plan: &ryne_extensions::CommandPlan) -> Result<(), String> {
                                    if matches!(plan, ryne_extensions::CommandPlan::Edit { .. }) { Err("injected IME rejection".into()) } else { Ok(()) }
                                }
                            }
                            editor.document = editor.document.fork("start");
                            editor.document.add_validator(std::sync::Arc::new(RejectIme));
                            editor.move_to(5, false, cx);
                            editor.replace_and_mark_text_in_range(None, "中😀", Some(3..3), window, cx);
                            editor.enter(&Enter, window, cx);
                            check_eq!(editor.document.source.text(), "start");
                            check_eq!(editor.document.composition().ok_or("rejected confirmation lost preedit")?.text, "中😀");
                            check_eq!(editor.document.history_len(false), 0);
                            check!(!editor.request_close(window, cx));
                            check!(editor.document.composition().is_some());
                        }
                        245 => {
                            check!(geometry.is_current(editor));
                            check!(geometry.display.text.contains("中😀"));
                            editor.document.cancel_composition();
                            editor.document = editor.document.fork("e\u{301}😀");
                            editor.move_to(editor.document.source.text().len(), false, cx);
                            editor.backspace(&Backspace, window, cx);
                            check_eq!(editor.document.source.text(), "e\u{301}");
                            editor.backspace(&Backspace, window, cx);
                            check_eq!(editor.document.source.text(), "");
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), "e\u{301}");
                            println!("Session boundary checks passed: rejected IME confirmation retains temporary text and geometry, close is blocked, generic deletion respects graphemes and undo");
                        }
                        246 => {
                            let raw = format!("> 中😀 {}\r\n> continuation\r\n>\r\n> > nested\r\n>\r\n> ```text\r\n> > **literal**\r\n> ```\r\n\r\noutside", "wrapped quote text ".repeat(12));
                            editor.document = editor.document.fork(&raw);
                            editor.move_to(2, false, cx);
                        }
                        247 => {
                            check_eq!(geometry.text_layout.quote_panels.len(), 1);
                            check_eq!(geometry.display.quotes.len(), 2);
                            check!(geometry.display.text.starts_with("中😀"));
                            check!(geometry.display.text.contains("> **literal**"));
                            let panel = &geometry.text_layout.quote_panels[0];
                            let first = geometry.lines.first().ok_or("missing quote line")?;
                            check_eq!(first.inset, px(32.));
                            check!(!first.shaped.wrap_boundaries.is_empty());
                            check!(panel.icon.right() < first.inset);
                            let control = geometry.block_control.ok_or("missing quote type control")?;
                            quote_control = Some(control);
                            check_eq!(ryne_extensions::BlockMetadata::from(geometry.block_target.as_ref().ok_or("missing quote target")?.kind).icon, Some("quote_bubble"));
                            check!(control.right() < geometry.bounds.left() + px(geometry.page.left) + panel.icon.left());
                            check!(panel.bounds.size.height > first.shaped.size(first.line_height).height);
                            let offset = geometry.display.text.find('😀').ok_or("missing quote emoji")?;
                            let position = geometry.position(offset);
                            let hit = geometry.hit_caret(position + point(px(1.), px(8.)));
                            check_eq!(geometry.display.source_at(hit.0, hit.1), editor.document.source.text().find('😀').ok_or("missing source emoji")?);
                            check!(!editor.document.dirty());
                            check_eq!(editor.document.history_len(false), 0);
                            editor.move_to(2, false, cx);
                            editor.insert(2..2, "新", cx);
                        }
                        248 => {
                            check!(geometry.display.text.starts_with("新中😀"));
                            check_eq!(geometry.text_layout.quote_panels.len(), 1);
                            check_eq!(editor.document.history_len(false), 1);
                            editor.undo(&Undo, window, cx);
                            check!(!editor.document.dirty());
                            editor.toggle_source(&ToggleSource, window, cx);
                        }
                        249 => {
                            check!(geometry.text_layout.quote_panels.is_empty());
                            check!(geometry.display.text.starts_with("> 中😀"));
                            check!(!editor.document.dirty());
                            check_eq!(editor.document.history_len(false), 0);
                            editor.toggle_source(&ToggleSource, window, cx);
                            editor.toggle_block_source(&ToggleBlockSource, window, cx);
                        }
                        250 => {
                            check!(geometry.text_layout.quote_panels.is_empty());
                            check!(geometry.display.text.starts_with("> 中😀"));
                            check!(!editor.document.dirty());
                            editor.toggle_block_source(&ToggleBlockSource, window, cx);
                        }
                        251 => {
                            check_eq!(geometry.text_layout.quote_panels.len(), 1);
                            check_eq!(editor.document.history_len(false), 0);
                            check!(!editor.document.dirty());
                            editor.move_to(editor.document.source.text().find("nested").ok_or("missing nested quote")?, false, cx);
                        }
                        252 => {
                            check_eq!(geometry.block_control, quote_control);
                            let block = geometry.block_target.as_ref().ok_or("missing outer quote control")?.clone();
                            check_eq!(block.kind, ryne_syntax::block::BlockKind::Quote);
                            check_eq!(block.range.start, 0);
                            check!(editor.document.source.text()[block.range.clone()].contains("continuation"));
                            check!(editor.document.source.text()[block.range.clone()].contains("nested"));
                            check_eq!(geometry.text_layout.quote_panels.len(), 1);
                            editor.block_action(editor.document.source.id(), &block, BlockAction::Select, cx);
                            check_eq!(editor.selected(), block.range);
                            check!(!editor.document.dirty());
                            check_eq!(editor.document.history_len(false), 0);
                            println!("Quote presentation passed: one outer block/panel across lines and nested prose, quote_bubble type icon and quoter body icon, stable menu origin, complete block selection, wrapping/literal code, Unicode mapping, editing/undo and local/full source without dirty/history changes");
                        }
                        253 => {
                            editor.document = editor.document.fork("> first");
                            editor.move_to(7, false, cx);
                            editor.enter(&Enter, window, cx);
                            check_eq!(editor.document.source.text(), "> first\n> ");
                            check_eq!(editor.document.history_len(false), 1);
                        }
                        254 => {
                            check_eq!(geometry.text_layout.quote_panels.len(), 1);
                            let panel = &geometry.text_layout.quote_panels[0];
                            let last = geometry.lines.last().ok_or("missing empty quote row")?;
                            check_eq!(last.inset, px(32.));
                            check!(panel.bounds.bottom() >= last.top + last.line_height);
                            let caret = geometry.position(geometry.display.text.len());
                            let hit = geometry.hit_caret(caret + point(px(1.), px(8.)));
                            check_eq!(geometry.display.source_at(hit.0, hit.1), editor.document.source.text().len());
                            check!(geometry.display.quote_at(geometry.display.text.len()).is_some());
                            editor.enter(&Enter, window, cx);
                            check_eq!(editor.document.source.text(), "> first\n");
                        }
                        255 => {
                            let last = geometry.lines.last().ok_or("missing row after quote exit")?;
                            check_eq!(last.inset, px(0.));
                            check!(geometry.text_layout.quote_panels[0].bounds.bottom() <= last.top);
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), "> first\n> ");
                        }
                        256 => {
                            check_eq!(geometry.lines.last().ok_or("missing restored quote row")?.inset, px(32.));
                            let last = geometry.lines.last().ok_or("missing restored quote row")?;
                            check!(geometry.text_layout.quote_panels[0].bounds.bottom() >= last.top + last.line_height);
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), "> first");
                            check!(!editor.document.dirty());
                        }
                        257 => {
                            editor.replace_and_mark_text_in_range(None, "中", Some(1..1), window, cx);
                            editor.enter(&Enter, window, cx);
                            check_eq!(editor.document.source.text(), "> first中");
                            check_eq!(editor.document.history_len(false), 1);
                            editor.undo(&Undo, window, cx);
                            check_eq!(editor.document.source.text(), "> first");
                            check!(!editor.document.dirty());
                            println!("Quote Enter passed: continuation prefix, background and inset on the empty new row, shared caret/hit mapping, empty-row exit, atomic undo and IME confirmation without an extra newline");
                        }
                        258 => {
                            editor.document = editor.document.fork("> ```\r\n> fn main() { let s = \"中😀\"; }\r\n> ```\r\n");
                            let at = editor.document.source.text().find("中😀").ok_or("missing language body")?;
                            editor.move_to(at, false, cx);
                        }
                        259 => {
                            let region = geometry.render_regions.iter().find(|r| r.controls[0].provider == "code").ok_or("missing code language header")?;
                            check!(matches!(&region.controls[0].placement, ryne_extensions::RenderControlPlacement::Header { label } if label == "auto"));
                            check!(geometry.render_buttons.contains(&region.trigger));
                            let first = geometry.lines.iter().find(|l| l.code.is_some()).ok_or("missing code body")?;
                            check!(region.trigger.bottom() <= geometry.bounds.top() + px(geometry.page.top) + first.top - geometry.scroll);
                            let lines = geometry.rows.iter().map(|r| r.range.clone()).collect::<Vec<_>>();
                            let selection = editor.document.selection();
                            editor.open_render_menu(geometry.render_key(window.viewport_size()), region.key(), window, cx);
                            check_eq!(editor.document.selection(), selection);
                            check_eq!(editor.document.history_len(false), 0);
                            check_eq!(geometry.rows.iter().map(|r| r.range.clone()).collect::<Vec<_>>(), lines);
                        }
                        260 => {
                            check!(!geometry.render_menu_bounds.is_empty());
                            let menu = editor.menus.render_menu.as_mut().ok_or("missing code language menu")?;
                            check!(menu.rows.iter().any(|r| matches!(r, RenderMenuRow::Action(i) if i.action.checked && i.request.command == "code.language-auto")));
                            check!(geometry.block_control.is_none());
                            check!(geometry.block_target.is_none());
                            check!(menu.rows.iter().all(|r| !matches!(r, RenderMenuRow::Action(i) if i.action.fallback == "<empty>")));
                            menu.selected = menu.rows.iter().position(|r| matches!(r, RenderMenuRow::Action(i) if i.request.command == "code.language-python")).ok_or("missing Python choice")?;
                            editor.enter(&Enter, window, cx);
                            check!(editor.document.source.text().starts_with("> ```python\r\n"));
                            check_eq!(editor.document.history_len(false), 1);
                        }
                        261 => {
                            let region = geometry.render_regions.iter().find(|r| r.controls[0].provider == "code").ok_or("missing updated language header")?;
                            check!(matches!(&region.controls[0].placement, ryne_extensions::RenderControlPlacement::Header { label } if label == "Python"));
                            editor.open_render_menu(geometry.render_key(window.viewport_size()), region.key(), window, cx);
                            let menu = editor.menus.render_menu.as_mut().ok_or("missing auto language menu")?;
                            menu.selected = menu.rows.iter().position(|r| matches!(r, RenderMenuRow::Action(i) if i.request.command == "code.language-auto")).ok_or("missing auto choice")?;
                            editor.enter(&Enter, window, cx);
                            check!(editor.document.source.text().starts_with("> ```\r\n"));
                        }
                        262 => {
                            check!(geometry.block_control.is_some());
                            check!(editor.viewport.display.spans.iter().any(|s| s.style.syntax == Some(ryne_projection::Class::Keyword)));
                            editor.undo(&Undo, window, cx);
                            editor.undo(&Undo, window, cx);
                            check!(!editor.document.dirty());
                            check!(editor.document.source.text().starts_with("> ```\r\n"));
                            editor.toggle_source(&ToggleSource, window, cx);
                        }
                        263 => {
                            check!(geometry.render_regions.is_empty());
                            editor.toggle_source(&ToggleSource, window, cx);
                            editor.replace_and_mark_text_in_range(None, "预编辑", Some(2..2), window, cx);
                        }
                        264 => {
                            check!(geometry.render_buttons.is_empty());
                            editor.escape(&Escape, window, cx);
                            check!(!editor.document.dirty());
                            println!("Code language menu passed: header geometry, empty-source auto, removed empty choice, popup hides block controls, keyboard selection, literal CRLF source edits, atomic undo, source/IME suppression");
                        }
                        265 => {
                            if code_language_only {println!("Code language GUI smoke passed");cx.quit();return Ok(());}
                            let unit="> quote **bold**\r\n> next\r\n\r\n- item 中😀\r\n  - nested\r\n\r\n| a | b |\r\n| --- | --- |\r\n| 中 | x |\r\n\r\n```rust\r\nfn main() {}\r\n```\r\n\r\n";
                            let source=unit.repeat(80);
                            editor.document=editor.document.fork(&source);
                            editor.viewport.scroll=px(0.);editor.viewport.ensure_caret=false;
                            editor.refresh(cx);
                        }
                        266 => {
                            let layout=&geometry.text_layout;
                            let top=geometry.scroll-px(geometry.page.top);let bottom=top+geometry.bounds.size.height;
                            check!(layout.visibility.lines.visible(top,bottom).len()<geometry.lines.len());
                            check!(geometry.render_regions.len()<editor.viewport.render_controls.len());
                            let quads=geometry.selection_quads(0..geometry.display.text.len(),rgb(0).into());
                            check!(quads.len()<=layout.visibility.rows.visible(top,bottom).len());
                            cached_layout=Some(layout.clone());
                            editor.viewport.scroll=geometry.height/2.;editor.viewport.ensure_caret=false;cx.notify();
                        }
                        267 => {
                            check!(cached_layout.as_ref().is_some_and(|old|std::sync::Arc::ptr_eq(old,&geometry.text_layout)));
                            let top=geometry.scroll-px(geometry.page.top);let bottom=top+geometry.bounds.size.height;
                            let visible=geometry.text_layout.visibility.rows.visible(top,bottom);
                            check!(!visible.is_empty());check!(visible.len()<geometry.rows.len());
                            let row=&geometry.rows[visible[visible.len()/2]];
                            let source=geometry.display.source_at(row.range.start,Affinity::Downstream);
                            let units=editor.document.editable_byte_to_utf16(source).map_err(|e|format!("{e:?}"))?;
                            check_eq!(editor.document.editable_utf16_to_byte(units).map_err(|e|format!("{e:?}"))?,source);
                            editor.move_to(source,false,cx);
                            editor.replace_and_mark_text_in_range(None,"中😀",Some(3..3),window,cx);
                        }
                        268 => {
                            check!(editor.document.composition().is_some());
                            let range=editor.marked_text_range(window,cx).ok_or("missing preedit UTF16 range")?;
                            check_eq!(range.end-range.start,3);
                            check!(editor.bounds_for_range(range,Bounds::default(),window,cx).is_some());
                            check_eq!(editor.document.history_len(false),0);
                            editor.document.cancel_composition();editor.refresh(cx);
                        }
                        269 => {
                            check!(!editor.document.dirty());check_eq!(editor.document.history_len(false),0);
                            println!("P1/P2 GUI checks passed: indexed CRLF/Unicode mappings, viewport line/control/selection clipping, scroll layout reuse and preedit geometry without source/history changes");
                        }
                        270 => {
                            if performance_only {
                                println!("P1/P2 performance GUI smoke passed");cx.quit();return Ok(());
                            }
                            let raw=(0..128).map(|i|format!("```\r\nfn first_batch_{i}() {{}}\r\n```\r\n\r\n")).collect::<String>();
                            editor.document=editor.document.fork(&raw);
                            editor.document.select(raw.len(),false,false);
                            editor.viewport.scroll=px(0.);editor.viewport.ensure_caret=false;
                            editor.refresh(cx);
                            check!(editor.viewport.highlight_task.is_some());
                        }
                        271 => {
                            check!(editor.viewport.highlight_task.is_none());
                            check_eq!(editor.viewport.display.code_blocks.len(),128);
                            for block in &editor.viewport.display.code_blocks {
                                let start=block.source.rows.first().ok_or("missing auto-code body")?.content.start;
                                check!(editor.viewport.display.spans.iter().any(|s|s.source.contains(&start)&&s.style.syntax.is_some()));
                            }
                            check!(!editor.document.dirty());check_eq!(editor.document.history_len(false),0);
                            cached_projection=Some(editor.viewport.display.clone());
                            cx.notify();
                        }
                        272 => {
                            check!(cached_projection.as_ref().is_some_and(|p|std::sync::Arc::ptr_eq(p,&editor.viewport.display)));
                            let raw="- [ ] one\r\n  - nested\r\n\r\n| a | b |\r\n| --- | --- |\r\n| x | y |\r\n\r\n```rust\r\nfn main() {}\r\n```\r\n";
                            editor.document=editor.document.fork(raw);
                            editor.refresh(cx);
                            check!(!editor.viewport.render_controls.is_empty());
                            editor.document.set_block_source(Some(0..raw.len()));
                            editor.refresh(cx);
                        }
                        273 => {
                            check!(editor.viewport.render_controls.is_empty());
                            editor.document.set_block_source(None);editor.refresh(cx);
                        }
                        274 => {
                            check!(!editor.viewport.render_controls.is_empty());
                            check!(!editor.document.dirty());check_eq!(editor.document.history_len(false),0);
                            let raw=(0..80).map(|i|format!("```\nfn preedit_batch_{i}() {{}}\n```\n\n")).collect::<String>();
                            editor.document=editor.document.fork(&raw);
                            editor.document.select(raw.len(),false,false);editor.refresh(cx);
                            editor.replace_and_mark_text_in_range(None,"临时",Some(2..2),window,cx);
                        }
                        275 => {
                            check!(editor.document.composition().is_some());
                            check!(editor.viewport.highlight_task.is_none());
                            check!(!editor.document.dirty());check_eq!(editor.document.history_len(false),0);
                            editor.escape(&Escape,window,cx);
                        }
                        276 => {
                            check!(editor.document.composition().is_none());
                            check!(editor.viewport.highlight_task.is_none());
                            check!(!editor.document.dirty());check_eq!(editor.document.history_len(false),0);
                            let raw=(0..80).map(|i|format!("```\nfn obsolete_batch_{i}() {{}}\n```\n\n")).collect::<String>();
                            editor.document=editor.document.fork(&raw);editor.refresh(cx);
                            check!(editor.viewport.highlight_task.is_some());
                            editor.viewport.dragging=true;editor.refresh(cx);
                            check!(editor.viewport.highlight_task.is_none());
                            editor.viewport.dragging=false;
                            editor.document=editor.document.fork("replacement 中😀\r\n");editor.refresh(cx);
                        }
                        277 => {
                            check_eq!(editor.document.source.text(),"replacement 中😀\r\n");
                            check!(editor.viewport.highlight_task.is_none());
                            check!(!editor.document.dirty());check_eq!(editor.document.history_len(false),0);
                            println!("First-batch GUI checks passed: automatic 128-block completion without input, stable idle projection, local source controls restored, IME/drag deferral and obsolete-task cancellation without source/history changes");
                            if first_batch_only {cx.quit();return Ok(());}
                        }
                        278 => {
                            let raw=(0..80).map(|i|format!("before {i} 中😀\n\n```rust\nfn item_{i}() {{}}\n```\n\n| a | b |\n| --- | --- |\n| $a+a+a+a+a+a+a+a+a+a+a+a+a+a+a+a+a+a+a+a+a+a+a+a$ {} | 中😀 $x^2$ |\n\ntail {i}\n\n","wrapped words ".repeat(30))).collect::<String>();
                            editor.document=editor.document.fork(&raw);editor.document.select(raw.len(),false,false);
                            editor.viewport.scroll=px(0.);editor.viewport.ensure_caret=false;editor.refresh(cx);
                        }
                        279 => {
                            let mut max_candidates=0;
                            check_eq!(geometry.display.formulas.len(),160);
                            for row in geometry.rows.iter().step_by((geometry.rows.len()/48).max(1)) {
                                for fraction in [0.1,0.5,0.9] {
                                    for x in [0.,100.,400.,760.] {
                                        let position=geometry.bounds.origin+point(px(geometry.page.left+x),px(geometry.page.top)+row.top+row.line_height*fraction-geometry.scroll);
                                        max_candidates=max_candidates.max(geometry.hit_candidate_count(position));
                                        #[cfg(debug_assertions)]
                                        check_eq!(geometry.hit_caret(position),geometry.hit_caret_linear(position));
                                        let expected=geometry.display.formulas.iter().find(|formula| {
                                            let a=geometry.position_with_affinity(formula.display.start,Affinity::Downstream);
                                            let b=geometry.position_with_affinity(formula.display.end,Affinity::Upstream);
                                            position.x>=a.x&&position.x<b.x&&position.y>=a.y&&position.y<a.y+geometry.line_height(formula.display.start)
                                        });
                                        check_eq!(geometry.formula_at_point(position).map(|f|&f.source),expected.map(|f|&f.source));
                                    }
                                }
                            }
                            check!(max_candidates<=12);check!(geometry.rows.len()>1000);
                            let mut overflow_hits=0;
                            for formula in geometry.display.formulas.iter().step_by(20) {
                                let a=geometry.position_with_affinity(formula.display.start,Affinity::Downstream);
                                let b=geometry.position_with_affinity(formula.display.end,Affinity::Upstream);
                                let row=geometry.row_for(formula.display.start,Affinity::Downstream).ok_or("missing formula row")?;
                                let cell=geometry.lines[row.line].cell.ok_or("formula not in cell")?;
                                let position=point(geometry.bounds.left()+px(geometry.page.left)+cell.right()+px(2.),a.y+row.line_height/2.);
                                if position.x>=a.x && position.x<b.x {
                                    check_eq!(geometry.formula_at_point(position).map(|f|&f.source),Some(&formula.source));
                                    overflow_hits+=1;
                                }
                            }
                            check!(overflow_hits>0);
                            println!("Second-batch hit candidates: max={max_candidates}, total_rows={}",geometry.rows.len());
                            cached_layout=Some(geometry.text_layout.clone());
                            editor.viewport.scroll=geometry.height;editor.viewport.ensure_caret=false;cx.notify();
                        }
                        280 => {
                            check!(cached_layout.as_ref().is_some_and(|p|std::sync::Arc::ptr_eq(p,&geometry.text_layout)));
                            let at=editor.document.source.text().rfind("tail 79").ok_or("missing final paragraph")?;
                            let position=geometry.position(geometry.display.position_at(at))+point(px(8.),px(12.));
                            let selection=editor.document.selection();
                            editor.mouse_move(&MouseMoveEvent {position,..Default::default()},window,cx);
                            check_eq!(editor.document.selection(),selection);
                            check!(editor.menus.hovered_block.as_ref().is_some_and(|(_,b)|b.range.contains(&at)));
                            check!(!editor.document.dirty());
                            second_source="plain 中😀 e\u{301}\n\n".repeat(5000)+"tail 中😀 👩‍👩‍👦 🇨🇳 e\u{301}";
                            editor.document=editor.document.fork(&second_source);editor.document.select(second_source.len(),false,false);
                            editor.refresh(cx);
                        }
                        281 => {
                            for _ in 0..20 {
                                let p=editor.viewport.display.position_at(editor.document.head);
                                let next=editor.viewport.display.text.grapheme_indices(true).map(|(p,_)|p).take_while(|&q|q<p).last().unwrap_or(0);
                                let expected=editor.viewport.display.source_at(next,Affinity::Downstream);
                                editor.horizontal(false,false,cx);check_eq!(editor.document.head,expected);
                            }
                            for _ in 0..20 {editor.horizontal(true,true,cx);}
                            check_eq!(editor.document.head,second_source.len());
                            check!(!editor.document.selection().is_empty());
                            editor.move_to(second_source.len(),false,cx);
                            editor.backspace(&Backspace,window,cx);
                            check_eq!(editor.document.source.text(),second_source.trim_end_matches("e\u{301}"));
                            check!(editor.document.dirty());check_eq!(editor.document.history_len(false),1);
                        }
                        282 => {
                            check!(editor.viewport.window_title.as_ref().is_some_and(|s|s.contains('•')));
                            editor.undo(&Undo,window,cx);
                            check!(!geometry.is_current(editor));
                            check_eq!(editor.document.source.text(),second_source);
                            check!(!editor.document.dirty());check_eq!(editor.document.history_len(false),0);
                        }
                        283 => {
                            check!(editor.viewport.window_title.as_ref().is_some_and(|s|!s.contains('•')));
                            editor.replace_and_mark_text_in_range(None,"临时",Some(2..2),window,cx);
                            check!(!geometry.is_current(editor));
                        }
                        284 => {
                            let range=editor.marked_text_range(window,cx).ok_or("missing indexed preedit range")?;
                            check!(editor.bounds_for_range(range,Bounds::default(),window,cx).is_some());
                            editor.escape(&Escape,window,cx);
                        }
                        285 => {
                            check_eq!(editor.document.source.text(),second_source);
                            check!(!editor.document.dirty());check_eq!(editor.document.history_len(false),0);
                            println!("Second-batch GUI checks passed: indexed hit oracle, bounded candidates, scrolled code-tail hover, Unicode horizontal/extended navigation, grapheme deletion, exact dirty/title state, undo and preedit geometry");
                            if second_batch_only {cx.quit();return Ok(());}
                        }
                        286 => {
                            CACHE_SMOKE_COMMANDS.store(0,std::sync::atomic::Ordering::Relaxed);
                            let raw="# heading\r\n\r\nplain 中😀 tail and **bold**\r\n\r\n- first\r\n  - nested\r\n\r\n| a | b |\r\n| --- | --- |\r\n| x | y |\r\n\r\n```rust\r\nlet x = 1;\r\n```\r\n";
                            editor.document=editor.document.fork(raw);third_source=Some(editor.document.source.clone());editor.viewport.scroll=px(0.);
                            editor.move_to(raw.find("plain").ok_or("missing plain")?,false,cx);
                        }
                        287 => {
                            cached_projection=Some(editor.viewport.display.clone());
                            cached_controls=Some(editor.viewport.render_controls.clone());
                            cached_layout=Some(geometry.text_layout.clone());
                            for _ in 0..5 {editor.horizontal(true,false,cx);}
                            for _ in 0..5 {editor.horizontal(false,true,cx);}
                        }
                        288 => {
                            check!(CACHE_SMOKE_COMMANDS.load(std::sync::atomic::Ordering::Relaxed)>0);
                            check!(cached_projection.as_ref().is_some_and(|p|std::sync::Arc::ptr_eq(p,&editor.viewport.display)));
                            check!(cached_controls.as_ref().is_some_and(|p|std::sync::Arc::ptr_eq(p,&editor.viewport.render_controls)));
                            check!(cached_layout.as_ref().is_some_and(|p|std::sync::Arc::ptr_eq(p,&geometry.text_layout)));
                            check!(!editor.document.dirty());check_eq!(editor.document.history_len(false),0);
                            editor.move_to(3,false,cx);editor.document.reveal_clicked_heading();editor.refresh(cx);
                        }
                        289 => {
                            check!(!cached_projection.as_ref().is_some_and(|p|std::sync::Arc::ptr_eq(p,&editor.viewport.display)));
                            check!(!cached_layout.as_ref().is_some_and(|p|std::sync::Arc::ptr_eq(p,&geometry.text_layout)));
                            editor.document.set_block_source(Some(0..editor.document.source.text().len()));editor.refresh(cx);
                        }
                        290 => {
                            check!(editor.viewport.render_controls.is_empty());
                            check_eq!(editor.viewport.display.text,editor.document.source.text().replace("\r\n","\n"));
                            editor.document.set_block_source(None);editor.move_to(editor.document.source.text().len(),false,cx);
                            check!(!geometry.is_current(editor));
                            editor.replace_and_mark_text_in_range(None,"临时😀",Some(4..4),window,cx);
                        }
                        291 => {
                            check!(editor.document.composition().is_some());
                            let range=editor.marked_text_range(window,cx).ok_or("missing third-batch preedit")?;
                            check!(editor.bounds_for_range(range,Bounds::default(),window,cx).is_some());
                            editor.escape(&Escape,window,cx);
                        }
                        292 => {
                            check!(editor.document.composition().is_none());check!(!editor.document.dirty());check_eq!(editor.document.history_len(false),0);
                            check!(!editor.viewport.render_controls.is_empty());
                            cached_projection=Some(editor.viewport.display.clone());cached_controls=Some(editor.viewport.render_controls.clone());cached_layout=Some(geometry.text_layout.clone());
                            editor.refresh(cx);
                        }
                        293 => {
                            check!(cached_projection.as_ref().is_some_and(|p|std::sync::Arc::ptr_eq(p,&editor.viewport.display)));
                            check!(cached_controls.as_ref().is_some_and(|p|std::sync::Arc::ptr_eq(p,&editor.viewport.render_controls)));
                            check!(cached_layout.as_ref().is_some_and(|p|std::sync::Arc::ptr_eq(p,&geometry.text_layout)));
                            check_eq!(third_source.as_ref().map(|s|s.id()),Some(editor.document.source.id()));
                            check_eq!(third_source.as_ref().map(|s|s.text()),Some(editor.document.source.text()));
                            println!("Third-batch GUI checks passed: actual command extension dispatched; projection/control/layout reuse across navigation and selection; heading/local-source invalidation, stale-frame rejection and preedit recovery without source/history changes");
                            if third_batch_only {cx.quit();return Ok(());}
                        }
                        294..=369 => { p1_state.step(stage-1,editor,&geometry,window,cx)?; }
                        370 => {
                            println!("P1 repair GUI passed: guarded table/formula layout recovery, temporary IME mapping, source/local suppression, full-snapshot block invalidation, and 20 redraws each with menu open, menu closed and no block; unchanged source/history/projection/controls/layout");
                            if p1_only {cx.quit();return Ok(());}
                        }
                        371 => {
                            editor.document=editor.document.fork(&crate::low_risk_smoke::sample());
                            editor.viewport.scroll=px(0.);
                            editor.refresh(cx);
                        }
                        372 => {
                            crate::low_risk_smoke::verify(editor,&geometry,window)?;
                            println!("Low-risk layout checks passed: 180 formulas in one Unicode paragraph at four widths; ordered formula placement/metrics; ordinary/table vertical navigation matches linear oracle with wrapped and empty cells, formula heights, and table boundaries; source/history unchanged");
                            if low_risk_only {cx.quit();return Ok(());}
                        }
                        _ => {
                            if performance_only {
                                println!("P1/P2 performance GUI smoke passed");
                                cx.quit();
                                return Ok(());
                            }
                            if window.is_window_active() {
                                check!(blink_seen.load(std::sync::atomic::Ordering::Relaxed));
                            }
                            eprintln!("GUI smoke idle blink observed: {}", blink_seen.load(std::sync::atomic::Ordering::Relaxed));
                            println!("List controls passed: ordered/unordered/nested and panel-first regions, leading SVG trigger and marker spacing, one whole-list type icon at the upper left clear of item triggers across nested items, hover without reflow/selection changes, focused innermost target, clicked menu, whole-subtree movement across 9->10, outdent, empty sibling insertion, undo, source/IME/drag suppression");
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
