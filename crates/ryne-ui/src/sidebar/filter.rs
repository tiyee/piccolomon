//! Separate platform text input for the list filter. All stored offsets are
//! bytes. Convert at the IME boundary and only hit-test the last shaped value.
use super::*;

pub(super) struct Filter {
    pub value: String,
    pub focus: FocusHandle,
    pub placeholder: &'static str,
    anchor: usize,
    head: usize,
    pub marked: Option<Range<usize>>,
    layout: Option<(Bounds<Pixels>, Pixels, ShapedLine)>,
    dragging: bool,
}
impl Filter {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            value: String::new(),
            focus: cx.focus_handle(),
            placeholder: "",
            anchor: 0,
            head: 0,
            marked: None,
            layout: None,
            dragging: false,
        }
    }
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        self.value.clear();
        self.anchor = 0;
        self.head = 0;
        self.marked = None;
        self.layout = None;
        cx.notify()
    }
    fn selection(&self) -> Range<usize> {
        self.head.min(self.anchor)..self.head.max(self.anchor)
    }
    fn bytes(&self, r: Range<usize>) -> Option<Range<usize>> {
        if r.start > r.end {
            return None;
        }
        Some(utf16_to_utf8(&self.value, r.start).ok()?..utf16_to_utf8(&self.value, r.end).ok()?)
    }
    fn units(&self, r: Range<usize>) -> Range<usize> {
        self.value[..r.start].encode_utf16().count()..self.value[..r.end].encode_utf16().count()
    }
    fn replace(&mut self, r: Option<Range<usize>>, text: &str) -> Option<Range<usize>> {
        let r = match r {
            Some(r) => self.bytes(r)?,
            None => self.marked.clone().unwrap_or_else(|| self.selection()),
        };
        let text = text.replace(['\n', '\r'], " ");
        self.value.replace_range(r.clone(), &text);
        self.head = r.start + text.len();
        self.anchor = self.head;
        self.marked = None;
        self.layout = None;
        Some(r.start..self.head)
    }
    fn hit(&self, position: Point<Pixels>) -> Option<usize> {
        let (bounds, offset, line) = self.layout.as_ref()?;
        (line.text.as_ref() == self.value)
            .then(|| line.closest_index_for_x(position.x - bounds.left() + *offset))
    }
    fn keys(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.marked.is_some() {
            return;
        }
        let key = event.keystroke.key.as_str();
        let mods = event.keystroke.modifiers;
        let selection = self.selection();
        let previous = self.value[..self.head]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i);
        let next = self.value[self.head..]
            .graphemes(true)
            .next()
            .map_or(self.head, |s| self.head + s.len());
        match (mods.secondary(), key) {
            (true, "a") => {
                self.anchor = 0;
                self.head = self.value.len();
            }
            (true, "v") => {
                if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                    self.replace(None, &text);
                }
            }
            (true, "c" | "x") => {
                if !selection.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        self.value[selection].to_owned(),
                    ));
                    if key == "x" {
                        self.replace(None, "");
                    }
                }
            }
            (false, "backspace" | "delete") => {
                if selection.is_empty() {
                    self.anchor = if key == "delete" { next } else { previous };
                }
                self.replace(None, "");
            }
            (false, "left" | "right" | "home" | "end") => {
                self.head = match key {
                    "home" => 0,
                    "end" => self.value.len(),
                    "left" if !mods.shift && !selection.is_empty() => selection.start,
                    "right" if !mods.shift && !selection.is_empty() => selection.end,
                    "left" => previous,
                    _ => next,
                };
                if !mods.shift {
                    self.anchor = self.head;
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}
impl EntityInputHandler for Filter {
    fn text_for_range(
        &mut self,
        r: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let bytes = self.bytes(r.clone())?;
        *actual = Some(r);
        Some(self.value[bytes].to_owned())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.units(self.selection()),
            reversed: self.head < self.anchor,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.clone().map(|r| self.units(r))
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify()
    }
    fn replace_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.replace(r, text).is_some() {
            cx.notify()
        }
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mark) = self.replace(r, text) else {
            return;
        };
        if let Some(s) = selected
            && s.start <= s.end
            && let (Ok(a), Ok(b)) = (utf16_to_utf8(text, s.start), utf16_to_utf8(text, s.end))
        {
            self.anchor = mark.start + a;
            self.head = mark.start + b;
        }
        self.marked = (!mark.is_empty()).then_some(mark);
        cx.notify()
    }
    fn bounds_for_range(
        &mut self,
        r: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let r = self.bytes(r)?;
        let (b, offset, line) = self.layout.as_ref()?;
        Some(Bounds::from_corners(
            point(b.left() - *offset + line.x_for_index(r.start), b.top()),
            point(b.left() - *offset + line.x_for_index(r.end), b.bottom()),
        ))
    }
    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let byte = self.hit(p)?;
        Some(self.units(byte..byte).start)
    }
}
impl Render for Filter {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::palette(cx);
        let entity = cx.entity();
        div()
            .id("sidebar-filter")
            .debug_selector(|| "sidebar-filter".into())
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .mx_2()
            .my_1()
            .px_2()
            .h(px(29.))
            .flex_shrink_0()
            .bg(rgb(colors.paper))
            .border_1()
            .border_color(rgb(colors.border))
            .rounded_md()
            .on_key_down(cx.listener(Self::keys))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|s, e: &MouseDownEvent, window, cx| {
                    window.focus(&s.focus);
                    if s.marked.is_none() {
                        s.head = s.hit(e.position).unwrap_or(s.head);
                        if !e.modifiers.shift {
                            s.anchor = s.head
                        }
                        s.dragging = true;
                    }
                    cx.stop_propagation();
                    cx.notify()
                }),
            )
            .on_mouse_move(cx.listener(|s, e: &MouseMoveEvent, _, cx| {
                if s.dragging
                    && e.pressed_button == Some(MouseButton::Left)
                    && let Some(hit) = s.hit(e.position)
                {
                    s.head = hit;
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|s, _, _, _| s.dragging = false),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        entity.update(cx, |s, _| {
                            let empty = s.value.is_empty();
                            let text: SharedString = if empty {
                                s.placeholder.into()
                            } else {
                                s.value.clone().into()
                            };
                            let line = window.text_system().shape_line(
                                text.clone(),
                                px(13.),
                                &[TextRun {
                                    len: text.len(),
                                    font: font(".SystemUIFont"),
                                    color: rgb(if empty { colors.muted } else { colors.ink })
                                        .into(),
                                    background_color: None,
                                    underline: None,
                                    strikethrough: None,
                                }],
                                None,
                            );
                            let offset =
                                (line.x_for_index(s.head) - bounds.size.width + px(3.)).max(px(0.));
                            s.layout = Some((bounds, offset, line));
                        });
                        let s = entity.read(cx);
                        window.handle_input(
                            &s.focus,
                            ElementInputHandler::new(bounds, entity.clone()),
                            cx,
                        );
                        let head = s.head;
                        let sel = s.selection();
                        let focused = s.focus.is_focused(window);
                        let marked = s.marked.clone();
                        if let Some((_, offset, line)) = s.layout.clone() {
                            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                                let origin = bounds.origin - point(offset, px(0.));
                                let x = |i| origin.x + line.x_for_index(i);
                                if focused && !sel.is_empty() {
                                    window.paint_quad(fill(
                                        Bounds::from_corners(
                                            point(x(sel.start), bounds.top()),
                                            point(x(sel.end), bounds.bottom()),
                                        ),
                                        Hsla::from(rgb(colors.link)).opacity(0.2),
                                    ));
                                }
                                let _ = line.paint(origin, bounds.size.height, window, cx);
                                if focused {
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(x(head), bounds.top() + px(5.)),
                                            size(px(1.), bounds.size.height - px(10.)),
                                        ),
                                        rgb(colors.ink),
                                    ));
                                }
                                if let Some(mark) = marked {
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(x(mark.start), bounds.bottom() - px(4.)),
                                            size(x(mark.end) - x(mark.start), px(1.)),
                                        ),
                                        rgb(colors.ink),
                                    ));
                                }
                            })
                        }
                    },
                )
                .size_full(),
            )
    }
}
