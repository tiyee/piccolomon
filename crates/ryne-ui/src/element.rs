use super::*;

// GPUI tests exercise the custom caption and its native event targets on every host.
const NATIVE_MAC_TITLEBAR: bool = cfg!(all(target_os = "macos", not(test)));
pub(super) const WINDOW_HEADER_HEIGHT: f32 = 34.;

pub(super) struct EditorElement {
    pub(super) editor: Entity<Editor>,
}

pub(super) struct ModeTooltip(pub(super) &'static str);

impl Render for ModeTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::palette(cx);
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(rgb(colors.paper))
            .border_1()
            .border_color(rgb(colors.border))
            .text_color(rgb(colors.ink))
            .font_family(".SystemUIFont")
            .text_size(px(12.))
            .child(self.0)
    }
}

fn window_control_button(
    colors: theme::Palette,
    id: &'static str,
    area: WindowControlArea,
    icon: &'static str,
    label: &'static str,
) -> impl IntoElement {
    let close = area == WindowControlArea::Close;
    div()
        .id(id)
        .debug_selector(|| id.to_owned())
        .group("window-control")
        .w(px(30.))
        .h_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .hover(move |style| style.bg(rgb(if close { 0xc42b1c } else { colors.hover })))
        .tooltip(move |_, cx| cx.new(|_| ModeTooltip(label)).into())
        // Let GPUI's Windows non-client handlers perform the operation. In
        // particular Max toggles restore (zoom_window only maximizes on Windows),
        // and Close sends WM_CLOSE through on_window_should_close / request_close.
        // Mouse handlers that stop propagation here would bypass those handlers.
        .window_control_area(area)
        .child(
            svg()
                .path(icon)
                .size(px(16.))
                .text_color(rgb(colors.ink))
                .group_hover("window-control", move |style| {
                    style.text_color(rgb(if close { 0xffffff } else { colors.ink }))
                }),
        )
}

impl Editor {
    fn window_header(
        &self,
        name: &str,
        sidebar_visible: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = theme::palette(cx);
        // Paint the two pane backgrounds behind the native controls and drag targets.
        let background = div()
            .absolute()
            .inset_0()
            .flex()
            .children(
                self.sidebar
                    .surface(colors)
                    .map(|pane| pane.debug_selector(|| "sidebar-titlebar".into())),
            )
            .child(
                div()
                    .debug_selector(|| "document-titlebar".into())
                    .flex_1()
                    .when(!NATIVE_MAC_TITLEBAR, |pane| {
                        pane.border_b_1().border_color(rgb(colors.border))
                    }),
            );
        div()
            .relative()
            .w_full()
            .debug_selector(|| "window-header".into())
            .h(px(WINDOW_HEADER_HEIGHT))
            .flex_shrink_0()
            .flex()
            .items_center()
            .child(background)
            .when(NATIVE_MAC_TITLEBAR && !window.is_fullscreen(), |row| {
                row.child(div().w(px(72.)).h_full().flex_shrink_0())
            })
            .child(self.titlebar_document(name, sidebar_visible, window, cx))
            .when(
                cfg!(any(target_os = "windows", test)) && !window.is_fullscreen(),
                |row| row.child(self.windows_caption_controls(window, colors)),
            )
    }

    fn titlebar_document(
        &self,
        name: &str,
        sidebar_visible: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = theme::palette(cx);
        let (stem, extension) = name
            .rsplit_once('.')
            .filter(|(stem, extension)| !stem.is_empty() && !extension.is_empty())
            .unwrap_or((name, ""));
        let stem_line_height = px(24.);
        let extension_line_height = px(18.);
        let text_system = window.text_system();
        let stem_font = text_system.resolve_font(&font(".SystemUIFont").bold());
        let extension_font = text_system.resolve_font(&font(".SystemUIFont"));
        // GPUI's flex layout synthesizes baselines from box bottoms. Align the
        // actual text baselines by including each font's descent and leading.
        let stem_depth =
            stem_line_height - text_system.baseline_offset(stem_font, px(16.), stem_line_height);
        let extension_depth = extension_line_height
            - text_system.baseline_offset(extension_font, px(12.), extension_line_height);
        let baseline_depth = stem_depth.max(extension_depth);
        div()
            .flex()
            .items_center()
            .flex_1()
            .min_w_0()
            .h_full()
            .gap_2()
            .pl(px(12.))
            .pr(px(12.))
            .child(
                div()
                    .id("window-drag")
                    .debug_selector(|| "window-drag".into())
                    .window_control_area(WindowControlArea::Drag)
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        svg()
                            .path("icon:clip")
                            .size(px(16.))
                            .flex_shrink_0()
                            .text_color(rgb(colors.ink)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .debug_selector(|| "document-name".into())
                            .min_w_0()
                            .max_w(px(220.))
                            .child(
                                div()
                                    .min_w_0()
                                    .text_size(px(16.))
                                    .line_height(stem_line_height)
                                    .pb(baseline_depth - stem_depth)
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(colors.heading_ink))
                                    .text_ellipsis()
                                    .child(stem.to_owned()),
                            )
                            .when(!extension.is_empty(), |row| {
                                row.child(
                                    div()
                                        .text_size(px(12.))
                                        .line_height(extension_line_height)
                                        .pb(baseline_depth - extension_depth)
                                        .text_color(rgb(colors.muted))
                                        .flex_shrink_0()
                                        .child(format!(".{extension}")),
                                )
                            }),
                    ),
            )
            .when(NATIVE_MAC_TITLEBAR, |row| {
                row.child(self.editor_shortcuts(sidebar_visible, cx))
            })
    }

    fn editor_shortcuts(&self, sidebar_visible: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::palette(cx);
        div()
            .flex_shrink_0()
            .flex()
            .gap_2()
            .items_center()
            .child(
                div()
                    .id("toggle-sidebar")
                    .debug_selector(|| "toggle-sidebar".into())
                    .size(px(28.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .tooltip({
                        let label = self.locale.language.text(Text::Sidebar);
                        move |_, cx| cx.new(|_| ModeTooltip(label)).into()
                    })
                    .child(
                        svg()
                            .path(if sidebar_visible {
                                "icon:panel_left_expand"
                            } else {
                                "icon:panel_right_expand"
                            })
                            .size(px(18.))
                            .text_color(rgb(colors.ink)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|s, _, w, cx| {
                            cx.stop_propagation();
                            s.sidebar_toggle(&ToggleSidebar, w, cx)
                        }),
                    ),
            )
            .child(
                div()
                    .id("format-source")
                    .debug_selector(|| "format-source".into())
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
                            .text_color(rgb(colors.ink)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.format_source(&FormatSource, window, cx);
                        }),
                    )
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
            )
            .child(
                div()
                    .id("toggle-source")
                    .debug_selector(|| "toggle-source".into())
                    .size(px(28.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .cursor_pointer()
                    .tooltip({
                        let label = self.locale.language.text(if self.document.source_mode {
                            Text::SourceMode
                        } else {
                            Text::ViewMode
                        });
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
                            .text_color(rgb(colors.ink)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.toggle_source(&ToggleSource, window, cx)
                        }),
                    ),
            )
    }

    fn windows_menu_bar(
        &self,
        sidebar_visible: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = theme::palette(cx);
        div()
            .debug_selector(|| "window-menu-bar".into())
            .w_full()
            .h(px(34.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_between()
            .pr(px(12.))
            .pb(px(1.))
            .border_b_1()
            .border_color(rgb(colors.border))
            .child(self.window_menu(window, cx))
            .child(self.editor_shortcuts(sidebar_visible, cx))
    }

    fn windows_caption_controls(
        &self,
        window: &Window,
        colors: theme::Palette,
    ) -> impl IntoElement {
        let maximized = window.is_maximized();
        div()
            .flex_shrink_0()
            .h_full()
            .flex()
            .child(window_control_button(
                colors,
                "window-minimize",
                WindowControlArea::Min,
                "icon:window_minimize",
                self.locale.language.text(Text::MinimizeWindow),
            ))
            .child(window_control_button(
                colors,
                "window-maximize",
                WindowControlArea::Max,
                if maximized {
                    "icon:window_restore"
                } else {
                    "icon:window_maximize"
                },
                self.locale.language.text(if maximized {
                    Text::RestoreWindow
                } else {
                    Text::MaximizeWindow
                }),
            ))
            .child(window_control_button(
                colors,
                "window-close",
                WindowControlArea::Close,
                "icon:window_close",
                self.locale.language.text(Text::CloseWindow),
            ))
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
    pub(super) label: Option<ShapedLine>,
}

pub(super) use ryne_extensions::BlockCommand as BlockAction;

pub(super) fn block_type_icon(
    colors: theme::Palette,
    block: &ryne_extensions::BlockMetadata,
    color: u32,
) -> AnyElement {
    match block.icon {
        Some(name) => svg()
            .path(format!("icon:{name}"))
            .w(px(16.))
            .h(px(16.))
            .flex_shrink_0()
            // GPUI Svg paints only when its own style has a color.
            .text_color(rgb(color))
            .group_hover("block-control", move |style| {
                style.text_color(rgb(colors.ink))
            })
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
    colors: theme::Palette,
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
        .hover(move |style| style.bg(rgb(colors.hover)))
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
    colors: theme::Palette,
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
                    .hover(move |style| style.bg(rgb(colors.code_bg)))
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
                color: rgb(theme::Palette::LIGHT.ink).into(),
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
        if !self.caret_state.focus_observed {
            self.caret_state.focus_observed = true;
            if NATIVE_MAC_TITLEBAR {
                let editor = cx.entity().downgrade();
                // AppKit's initial layout can reset the native button frames.
                // Reapplying the current title after the first frame asks GPUI
                // to restore traffic_light_position without resizing the window.
                window.on_next_frame(move |window, cx| {
                    if let Some(editor) = editor.upgrade()
                        && let Some(title) = &editor.read(cx).viewport.window_title
                    {
                        window.set_window_title(title);
                    }
                });
            }
            cx.observe_window_appearance(window, |_, window, cx| {
                cx.update_global::<theme::Settings, _>(|settings, _| {
                    settings.system = window.appearance();
                });
                cx.refresh_windows();
            })
            .detach();
            cx.on_focus(&self.focus, window, |editor, _, cx| {
                editor.reset_caret_blink();
                cx.notify();
            })
            .detach();
            cx.on_blur(&self.focus, window, |editor, _, cx| {
                editor.reset_caret_blink();
                editor.menus.append_hovered = false;
                editor.menus.append_menu = None;
                editor.menus.block_menu = None;
                editor.menus.hovered_block = None;
                editor.menus.hovered_render = None;
                editor.menus.render_menu = None;
                cx.notify();
            })
            .detach();
            cx.observe_window_activation(window, |editor, window, cx| {
                editor.reset_caret_blink();
                if !window.is_window_active() {
                    editor.menus.render_menu = None;
                    editor.menus.hovered_render = None;
                }
                cx.notify();
            })
            .detach();
        }
        let colors = theme::palette(cx);
        self.sync_caret_blink(window, cx);
        let name = self
            .document
            .files
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.locale.language.text(Text::Untitled).into());
        let dock = self.sidebar_geometry(window);
        let sidebar = self.sidebar_view(cx);
        let title = format!(
            "{}{} — Ryne",
            name,
            if self.document.dirty() { " •" } else { "" }
        );
        if self.viewport.window_title.as_ref() != Some(&title) {
            window.set_window_title(&title);
            self.viewport.window_title = Some(title);
        }
        let root = div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(colors.paper))
            .text_color(rgb(colors.ink))
            .font_family(".SystemUIFont")
            .text_size(px(13.))
            .key_context("RyneWindow")
            .on_action(cx.listener(Self::open_folder))
            .on_action(cx.listener(Self::sidebar_toggle))
            .on_mouse_move(cx.listener(Self::sidebar_resize_move))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| s.sidebar_resize_end(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| s.sidebar_resize_end(cx)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|editor, _, _, cx| {
                    let top = editor.menus.top_menu.take().is_some();
                    editor.menus.top_submenu = None;
                    let append = editor.menus.append_menu.take().is_some();
                    let render = editor.menus.render_menu.take().is_some();
                    if editor.menus.block_menu.take().is_some() || append || render || top {
                        cx.notify();
                    }
                }),
            )
            .on_action(cx.listener(Self::about))
            .on_action(cx.listener(Self::change_language))
            .on_action(cx.listener(Self::change_math_font))
            .on_action(cx.listener(Self::change_theme))
            .on_action(cx.listener(Self::check_updates))
            .on_action(cx.listener(Self::hide))
            .on_action(cx.listener(Self::new_document))
            .on_action(cx.listener(Self::open_recent))
            .on_action(cx.listener(Self::clear_recent))
            .on_action(cx.listener(Self::reload))
            .on_action(cx.listener(Self::export_pdf))
            .on_action(cx.listener(Self::export_registered))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::quit))
            .child(self.window_header(&name, sidebar.is_some(), window, cx))
            .when(!NATIVE_MAC_TITLEBAR, |view| {
                view.child(self.windows_menu_bar(sidebar.is_some(), window, cx))
            });
        let right = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                div()
                    .id("editor")
                    // GPUI's automatic focus handler prevents the default mouse-down
                    // behavior. Limit it to the document: a focus-tracking window
                    // root also consumes Windows non-client titlebar presses,
                    // suppressing native dragging and all three window buttons.
                    .track_focus(&self.focus)
                    .key_context("MarkdownEditor")
                    .capture_key_down(cx.listener(Self::render_shortcut))
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
                    .on_action(cx.listener(Self::format_source))
                    .on_action(cx.listener(Self::toggle_source))
                    .on_action(cx.listener(Self::toggle_block_source))
                    .on_action(cx.listener(Self::escape))
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
                    .on_scroll(cx.listener(Self::scroll))
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
                    .border_color(rgb(colors.border))
                    .text_size(px(12.))
                    .text_color(rgb(colors.muted))
                    .child(self.status.render(self.locale.language))
                    .child(if self.document.source_mode {
                        self.locale.language.text(Text::SourceMode)
                    } else if self.document.block_source.is_some() {
                        self.locale.language.text(Text::BlockMode)
                    } else {
                        self.locale.language.text(Text::LivePreview)
                    }),
            );
        let body = div().relative().flex().flex_1().min_h_0().w_full();
        let body = match sidebar {
            Some(panel) if dock > 0. => body.child(panel).child(right),
            Some(panel) => body
                .child(right)
                .child(div().absolute().top_0().bottom_0().left_0().child(panel)),
            None => body.child(right),
        };
        root.child(body)
    }
}
