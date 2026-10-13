use super::*;
pub(super) struct EditorElement {
    pub(super) editor: Entity<Editor>,
}

struct ModeTooltip(&'static str);

impl Render for ModeTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(rgb(theme::PAPER))
            .border_1()
            .border_color(rgb(theme::BORDER))
            .text_color(rgb(theme::INK))
            .font_family(".SystemUIFont")
            .text_size(px(12.))
            .child(self.0)
    }
}
pub(super) struct Frame {
    pub(super) geometry: Geometry,
    pub(super) selections: Vec<PaintQuad>,
    pub(super) cursor: PaintQuad,
    pub(super) code_panels: Vec<CodePanel>,
    pub(super) list_labels: Vec<(Point<Pixels>, ShapedLine, Pixels)>,
    pub(super) table_quads: Vec<PaintQuad>,
    pub(super) controls: Vec<AnyElement>,
}

pub(super) struct CodePanel {
    pub(super) bounds: Bounds<Pixels>,
    pub(super) label: ShapedLine,
}

pub(super) use ryne_extensions::BlockCommand as BlockAction;

pub(super) fn block_type_icon(block: &ryne_extensions::BlockMetadata, color: u32) -> AnyElement {
    match block.icon {
        Some(name) => svg()
            .path(format!("icon:{name}"))
            .w(px(16.))
            .h(px(16.))
            .flex_shrink_0()
            // GPUI Svg paints only when its own style has a color.
            .text_color(rgb(color))
            .group_hover("block-control", |style| style.text_color(rgb(0x454b53)))
            .into_any_element(),
        None => div()
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(color))
            .child(block.label)
            .into_any_element(),
    }
}

pub(super) fn block_menu_item(
    label: &'static str,
    action: BlockAction,
    editor: Entity<Editor>,
    snapshot: SnapshotId,
    block: crate::block::Block,
) -> impl IntoElement {
    div()
        .id(match action {
            BlockAction::Source => "block-source",
            BlockAction::Select => "block-select",
            BlockAction::Copy => "block-copy",
            BlockAction::AddBelow => "block-add-below",
            BlockAction::AddAbove => "block-add-above",
        })
        .px_3()
        .py_2()
        .text_size(px(13.))
        .cursor_pointer()
        .rounded_md()
        .hover(|style| style.bg(rgb(0xeeede8)))
        .child(label)
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            editor.update(cx, |editor, cx| {
                editor.block_action(snapshot, &block, action, cx)
            });
        })
        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

/// Shared type picker for the document tail and adjacent-block insertion.
pub(super) fn insert_type_picker(
    editor: Entity<Editor>,
    snapshot: SnapshotId,
    target: Option<(crate::block::Block, bool)>,
    language: crate::i18n::Language,
    templates: &[(&'static str, ryne_extensions::InsertBlock)],
) -> impl IntoElement {
    let mut picker = div();
    for items in templates.chunks(2) {
        let mut row = div().flex();
        for &(label, kind) in items {
            let entity = editor.clone();
            let target = target.clone();
            row = row.child(
                div()
                    .id(label)
                    .flex_1()
                    .min_w_0()
                    .px_3()
                    .py_2()
                    .text_size(px(13.))
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(theme::CODE_BG)))
                    .child(language.insert_label(kind))
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        cx.stop_propagation();
                        entity.update(cx, |editor, cx| {
                            if let Some((block, before)) = &target {
                                editor.insert_adjacent_block(
                                    snapshot, block, *before, kind, window, cx,
                                );
                            } else {
                                editor.append_block(snapshot, kind, window, cx);
                            }
                        });
                    })
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
            );
        }
        picker = picker.child(row);
    }
    picker
}

/// Measure UI labels with the same system font used by menu elements. The
/// document font and reading-column width must not constrain translated menus.
pub(super) fn menu_text_width(text: &str, size: f32, window: &Window) -> Pixels {
    let width = window
        .text_system()
        .shape_line(
            text.to_owned().into(),
            px(size),
            &[TextRun {
                len: text.len(),
                font: font(".SystemUIFont"),
                color: rgb(theme::INK).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        )
        .width;
    // Native layout quantizes fractional widths. Round up so a translated
    // label cannot lose its final fraction of a pixel after popup layout.
    px(f32::from(width).ceil())
}

impl IntoElement for EditorElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = Frame;
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = gpui::Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Frame {
        self.prepaint_frame(bounds, window, cx)
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        frame: &mut Frame,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.paint_frame(bounds, frame, window, cx)
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focus_observed {
            self.focus_observed = true;
            cx.on_focus(&self.focus, window, |editor, _, cx| {
                editor.reset_caret_blink();
                cx.notify();
            })
            .detach();
            cx.on_blur(&self.focus, window, |editor, _, cx| {
                editor.reset_caret_blink();
                editor.append_hovered = false;
                editor.append_menu = None;
                editor.block_menu = None;
                editor.hovered_block = None;
                editor.hovered_render = None;
                editor.render_menu = None;
                cx.notify();
            })
            .detach();
            cx.observe_window_activation(window, |editor, window, cx| {
                editor.reset_caret_blink();
                if !window.is_window_active() {
                    editor.render_menu = None;
                    editor.hovered_render = None;
                }
                cx.notify();
            })
            .detach();
        }
        self.sync_caret_blink(window, cx);
        let name = self
            .document
            .files
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.locale.language.text(Text::Untitled).into());
        window.set_window_title(&format!(
            "{}{} — Ryne",
            name,
            if self.document.dirty() { " •" } else { "" }
        ));
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme::PAPER))
            .text_color(rgb(theme::INK))
            .font_family(".SystemUIFont")
            .text_size(px(13.))
            .key_context("MarkdownEditor")
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(Self::render_shortcut))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|editor, _, _, cx| {
                    let top = editor.top_menu.take().is_some();
                    editor.top_submenu = None;
                    let append = editor.append_menu.take().is_some();
                    let render = editor.render_menu.take().is_some();
                    if editor.block_menu.take().is_some() || append || render || top {
                        cx.notify();
                    }
                }),
            )
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::hard_break))
            .on_action(cx.listener(Self::tab))
            .on_action(cx.listener(Self::outdent))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::about))
            .on_action(cx.listener(Self::change_language))
            .on_action(cx.listener(Self::change_math_font))
            .on_action(cx.listener(Self::check_updates))
            .on_action(cx.listener(Self::hide))
            .on_action(cx.listener(Self::new_document))
            .on_action(cx.listener(Self::open_recent))
            .on_action(cx.listener(Self::clear_recent))
            .on_action(cx.listener(Self::reload))
            .on_action(cx.listener(Self::export_pdf))
            .on_action(cx.listener(Self::export_pdf_v2))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::format_source))
            .on_action(cx.listener(Self::toggle_source))
            .on_action(cx.listener(Self::toggle_block_source))
            .on_action(cx.listener(Self::escape))
            .on_action(cx.listener(Self::quit))
            .when(
                cfg!(target_os = "macos") && !window.is_fullscreen(),
                |view| {
                    // The transparent native titlebar hides its text and lets PAPER
                    // show through. Reserve its traffic-light/drag area above controls.
                    view.child(div().h(px(28.)).flex_shrink_0())
                },
            )
            .when(!cfg!(target_os = "macos"), |view| {
                view.child(self.window_menu(window, cx))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .gap_2()
                    .px_5()
                    .py(px(2.))
                    .border_b_1()
                    .border_color(rgb(theme::BORDER))
                    .child(
                        div().flex().gap_2().items_center().child("Ryne").child(
                            div()
                                .text_size(px(12.))
                                .text_color(rgb(0x888888))
                                .max_w(px(220.))
                                .text_ellipsis()
                                .child(name),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .id("format-source")
                                    .size(px(28.))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .tooltip({
                                        let label = self.locale.language.text(Text::FormatSource);
                                        move |_, cx| cx.new(|_| ModeTooltip(label)).into()
                                    })
                                    .child(
                                        svg()
                                            .path("icon:format")
                                            .size(px(18.))
                                            .text_color(rgb(theme::INK)),
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, window, cx| {
                                            cx.stop_propagation();
                                            this.format_source(&FormatSource, window, cx);
                                        }),
                                    )
                                    .on_mouse_up(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    }),
                            )
                            .child(
                                div()
                                    .id("toggle-source")
                                    .size(px(28.))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .tooltip({
                                        let label = self.locale.language.text(
                                            if self.document.source_mode {
                                                Text::SourceMode
                                            } else {
                                                Text::ViewMode
                                            },
                                        );
                                        move |_, cx| cx.new(|_| ModeTooltip(label)).into()
                                    })
                                    .child(
                                        svg()
                                            .path(if self.document.source_mode {
                                                "icon:code"
                                            } else {
                                                "icon:eye"
                                            })
                                            .size(px(18.))
                                            .text_color(rgb(theme::INK)),
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, window, cx| {
                                            this.toggle_source(&ToggleSource, window, cx)
                                        }),
                                    ),
                            ),
                    ),
            )
            .child(
                div()
                    .id("editor")
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .cursor(CursorStyle::IBeam)
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
                    .on_mouse_move(cx.listener(|this, event, window, cx| {
                        if !this.smoke_mode {
                            this.mouse_move(event, window, cx);
                        }
                    }))
                    .on_hover(cx.listener(|this, hovered, window, cx| {
                        if !this.smoke_mode {
                            this.editor_hover(hovered, window, cx);
                        }
                    }))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
                    .on_scroll_wheel(cx.listener(Self::scroll))
                    .child(EditorElement {
                        editor: cx.entity(),
                    }),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .px_5()
                    .py_2()
                    .border_t_1()
                    .border_color(rgb(theme::BORDER))
                    .text_size(px(12.))
                    .text_color(rgb(0x818181))
                    .child(self.status.render(self.locale.language))
                    .child(if self.document.source_mode {
                        self.locale.language.text(Text::SourceMode)
                    } else if self.document.block_source.is_some() {
                        self.locale.language.text(Text::BlockMode)
                    } else {
                        self.locale.language.text(Text::LivePreview)
                    }),
            )
    }
}
