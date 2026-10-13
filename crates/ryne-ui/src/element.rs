use super::*;
pub(super) struct EditorElement {
    pub(super) editor: Entity<Editor>,
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
        .id(label)
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
        let button = |label: &'static str| {
            div()
                .px_3()
                .py_1()
                .rounded_md()
                .cursor_pointer()
                .text_size(px(13.))
                .bg(rgb(0xecebe7))
                .hover(|s| s.bg(rgb(0xe0dfd9)))
                .child(label)
        };
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
                    let language = std::mem::take(&mut editor.language_menu);
                    let append = editor.append_menu.take().is_some();
                    let render = editor.render_menu.take().is_some();
                    if editor.block_menu.take().is_some() || append || language || render {
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
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::toggle_source))
            .on_action(cx.listener(Self::toggle_block_source))
            .on_action(cx.listener(Self::escape))
            .on_action(cx.listener(Self::quit))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .gap_2()
                    .px_5()
                    .py_3()
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
                            .flex_wrap()
                            .justify_end()
                            .child(
                                button(self.locale.language.text(Text::Language)).on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.language_menu = !this.language_menu;
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(button(self.locale.language.text(Text::Open)).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| this.open(&Open, window, cx)),
                            ))
                            .child(button(self.locale.language.text(Text::Save)).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| this.save(&Save, window, cx)),
                            ))
                            .child(
                                button(if self.document.block_source.is_some() {
                                    self.locale.language.text(Text::FoldSource)
                                } else {
                                    self.locale.language.text(Text::BlockSource)
                                })
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, window, cx| {
                                        this.toggle_block_source(&ToggleBlockSource, window, cx)
                                    }),
                                ),
                            )
                            .child(
                                button(if self.document.source_mode {
                                    self.locale.language.text(Text::Preview)
                                } else {
                                    self.locale.language.text(Text::Source)
                                })
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, window, cx| {
                                        this.toggle_source(&ToggleSource, window, cx)
                                    }),
                                ),
                            ),
                    ),
            )
            .when(self.language_menu, |view| {
                view.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .px_5()
                        .py_2()
                        .child(
                            button(self.locale.language.text(Text::System))
                                .when(self.locale.preference.0.is_none(), |s| s.bg(rgb(0xdde8ef)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.select_language(Preference(None), cx);
                                    }),
                                ),
                        )
                        .children(Language::ALL.into_iter().map(|language| {
                            button(language.native_name())
                                .when(self.locale.preference.0 == Some(language), |s| {
                                    s.bg(rgb(0xdde8ef))
                                })
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.select_language(Preference(Some(language)), cx);
                                    }),
                                )
                        })),
                )
            })
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
