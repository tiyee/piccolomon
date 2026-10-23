//! Generic render controls. Markdown command behavior stays in feature providers.
use super::*;
use ryne_extensions::{RenderAction, RenderActionRequest, RenderControl, RenderShortcut};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RenderFrameKey {
    document: SnapshotId,
    epoch: u64,
    bounds: Bounds<Pixels>,
    scroll: Pixels,
    viewport: Size<Pixels>,
}
#[derive(Clone)]
pub(super) struct RenderRegion {
    pub bounds: Bounds<Pixels>,
    pub trigger: Bounds<Pixels>,
    pub controls: Vec<RenderControl>,
}
pub(super) struct RenderViewState {
    available: bool,
    hovered: Option<String>,
    focused: bool,
    selection: Range<usize>,
    menu: Option<RenderMenu>,
}
impl RenderViewState {
    pub fn new(editor: &Editor, window: &Window) -> Self {
        Self {
            available: editor.render_available(),
            hovered: editor.menus.hovered_render.clone(),
            focused: editor.focus.is_focused(window),
            selection: editor.document.selection(),
            menu: editor.menus.render_menu.clone(),
        }
    }
}
impl RenderRegion {
    pub fn key(&self) -> String {
        self.controls
            .iter()
            .map(|c| format!("{}:{}", c.provider, c.id))
            .collect::<Vec<_>>()
            .join(";")
    }
    fn focused(&self, editor: &Editor) -> bool {
        self.contains_selection(&editor.document.selection())
    }
    fn contains_selection(&self, selection: &Range<usize>) -> bool {
        self.controls
            .iter()
            .any(|c| c.focus.start <= selection.start && selection.end <= c.focus.end)
    }
    fn specificity(&self) -> usize {
        self.controls
            .iter()
            .map(|c| c.focus.len())
            .min()
            .unwrap_or(usize::MAX)
    }
}
impl Geometry {
    pub(super) fn render_key(&self, viewport: Size<Pixels>) -> RenderFrameKey {
        RenderFrameKey {
            document: self.snapshot,
            epoch: self.frame_epoch,
            bounds: self.bounds,
            scroll: self.scroll,
            viewport,
        }
    }
}
#[derive(Clone)]
pub(super) struct RenderMenuItem {
    pub action: RenderAction,
    pub request: RenderActionRequest,
}
#[derive(Clone)]
pub(super) enum RenderMenuRow {
    Action(RenderMenuItem),
    Submenu {
        label: &'static str,
        fallback: &'static str,
        group: u8,
        children: Vec<RenderMenuItem>,
    },
}
impl RenderMenuRow {
    fn enabled(&self) -> bool {
        match self {
            Self::Action(i) => i.action.enabled,
            Self::Submenu { .. } => true,
        }
    }
    fn group(&self) -> u8 {
        match self {
            Self::Action(i) => i.action.group,
            Self::Submenu { group, .. } => *group,
        }
    }
}
#[derive(Clone)]
pub(super) struct RenderMenu {
    pub key: RenderFrameKey,
    pub region: String,
    pub rows: Vec<RenderMenuRow>,
    pub selected: usize,
    pub submenu: Option<usize>,
    pub child_selected: usize,
}
fn rows(controls: &[RenderControl]) -> Vec<RenderMenuRow> {
    let mut result = Vec::new();
    for c in controls {
        for action in &c.actions {
            let item = RenderMenuItem {
                action: action.clone(),
                request: RenderActionRequest {
                    provider: c.provider,
                    target: c.target.clone(),
                    command: action.command,
                },
            };
            if let Some(submenu) = action.submenu {
                if let Some(RenderMenuRow::Submenu { children, .. }) = result.iter_mut().find(
                    |r| matches!(r, RenderMenuRow::Submenu {label,..} if *label == submenu.label),
                ) {
                    children.push(item);
                } else {
                    result.push(RenderMenuRow::Submenu {
                        label: submenu.label,
                        fallback: submenu.fallback,
                        group: action.group,
                        children: vec![item],
                    });
                }
            } else {
                result.push(RenderMenuRow::Action(item));
            }
        }
    }
    result
}
impl Editor {
    fn render_available(&self) -> bool {
        !self.viewport.dragging
            && self.document.composition().is_none()
            && !self.document.source_mode
    }
    fn render_key_current(&self, key: &RenderFrameKey, window: &Window) -> bool {
        self.render_available()
            && self.viewport.geometry.as_ref().is_some_and(|g| {
                g.is_current(self)
                    && g.scroll == self.viewport.scroll
                    && g.render_key(window.viewport_size()) == *key
            })
    }
    pub(super) fn open_render_menu(
        &mut self,
        key: RenderFrameKey,
        region: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.render_key_current(&key, window) {
            return;
        }
        let Some(target) = self
            .viewport
            .geometry
            .as_ref()
            .and_then(|g| g.render_regions.iter().find(|r| r.key() == region))
        else {
            return;
        };
        let rows = rows(&target.controls);
        let selected = rows.iter().position(RenderMenuRow::enabled).unwrap_or(0);
        self.menus.render_menu = Some(RenderMenu {
            key,
            region,
            rows,
            selected,
            submenu: None,
            child_selected: 0,
        });
        self.menus.block_menu = None;
        self.menus.append_menu = None;
        self.menus.top_menu = None;
        self.menus.top_submenu = None;
        window.focus(&self.focus);
        cx.notify();
    }
    fn run_render_item(
        &mut self,
        key: RenderFrameKey,
        item: RenderMenuItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menus.render_menu = None;
        if !self.render_key_current(&key, window) {
            cx.notify();
            return;
        }
        if !item.action.enabled {
            cx.notify();
            return;
        }
        if let Err(error) = self.document.execute_render_action(&item.request) {
            self.status = self.locale.language.error(Text::RenderActionFailed, error);
        }
        self.caret_state.preferred_x = None;
        self.viewport.ensure_caret = true;
        window.focus(&self.focus);
        self.refresh(cx);
    }
    pub(super) fn activate_render_control(
        &mut self,
        key: RenderFrameKey,
        region: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.render_key_current(&key, window) {
            return;
        }
        let item = self.viewport.geometry.as_ref().and_then(|g| {
            let region = g.render_regions.iter().find(|r| r.key() == region)?;
            let control = region.controls.first()?;
            if !matches!(
                control.placement,
                ryne_extensions::RenderControlPlacement::Marker { .. }
            ) {
                return None;
            }
            Some(RenderMenuItem {
                action: control.actions.first()?.clone(),
                request: RenderActionRequest {
                    provider: control.provider,
                    target: control.target.clone(),
                    command: control.actions.first()?.command,
                },
            })
        });
        if let Some(item) = item {
            self.run_render_item(key, item, window, cx);
            // A structural toggle preserves the selection; do not scroll to a distant caret.
            self.viewport.ensure_caret = false;
        }
    }
    pub(super) fn update_render_hover(
        &mut self,
        point: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.render_available() {
            return false;
        }
        let Some(g) = &self.viewport.geometry else {
            return false;
        };
        if !g.is_current(self) {
            return false;
        }
        if self.menus.render_menu.is_some() {
            return true;
        } // Click/Escape dismiss; moving away does not close a clicked popup.
        let hovered = g
            .render_regions
            .iter()
            .filter(|r| r.bounds.contains(&point) || r.trigger.contains(&point))
            .min_by_key(|r| r.specificity())
            .map(RenderRegion::key);
        if self.menus.hovered_render != hovered {
            self.menus.hovered_render = hovered;
            cx.notify();
        }
        false
    }
    pub(super) fn render_menu_vertical(&mut self, down: bool, cx: &mut Context<Self>) -> bool {
        let Some(menu) = &mut self.menus.render_menu else {
            return false;
        };
        if let Some(row) = menu.submenu.and_then(|i| menu.rows.get(i)) {
            if let RenderMenuRow::Submenu { children, .. } = row {
                menu.child_selected = next_enabled(
                    menu.child_selected,
                    down,
                    &children
                        .iter()
                        .map(|c| c.action.enabled)
                        .collect::<Vec<_>>(),
                );
            }
        } else {
            menu.selected = next_enabled(
                menu.selected,
                down,
                &menu
                    .rows
                    .iter()
                    .map(RenderMenuRow::enabled)
                    .collect::<Vec<_>>(),
            );
        }
        cx.notify();
        true
    }
    pub(super) fn render_menu_horizontal(&mut self, right: bool, cx: &mut Context<Self>) -> bool {
        let Some(menu) = &mut self.menus.render_menu else {
            return false;
        };
        if right
            && matches!(
                menu.rows.get(menu.selected),
                Some(RenderMenuRow::Submenu { .. })
            )
        {
            menu.submenu = Some(menu.selected);
            menu.child_selected = 0;
        }
        if !right {
            menu.submenu = None;
        }
        cx.notify();
        true
    }
    pub(super) fn activate_render_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.document.composition().is_some() {
            return false;
        }
        let Some(menu) = self.menus.render_menu.clone() else {
            return false;
        };
        if let Some(RenderMenuRow::Submenu { children, .. }) =
            menu.submenu.and_then(|i| menu.rows.get(i))
        {
            if let Some(item) = children.get(menu.child_selected) {
                self.run_render_item(menu.key, item.clone(), window, cx);
            }
        } else {
            match menu.rows.get(menu.selected) {
                Some(RenderMenuRow::Action(item)) => {
                    self.run_render_item(menu.key, item.clone(), window, cx)
                }
                Some(RenderMenuRow::Submenu { .. }) => {
                    self.render_menu_horizontal(true, cx);
                }
                None => {}
            }
        }
        true
    }
    fn hover_render_row(&mut self, index: usize, child: bool, cx: &mut Context<Self>) {
        if let Some(menu) = &mut self.menus.render_menu {
            if child {
                menu.child_selected = index;
            } else {
                menu.selected = index;
                menu.submenu = matches!(menu.rows.get(index), Some(RenderMenuRow::Submenu { .. }))
                    .then_some(index);
            }
            cx.notify();
        }
    }
    pub(super) fn render_shortcut(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.render_available() {
            return;
        }
        let stroke = &event.keystroke;
        if stroke.key == "enter"
            && stroke.modifiers.alt
            && !stroke.modifiers.control
            && !stroke.modifiers.platform
            && !stroke.modifiers.shift
        {
            let target = self
                .viewport
                .geometry
                .as_ref()
                .filter(|g| g.is_current(self))
                .and_then(|g| {
                    g.render_regions
                        .iter()
                        .filter(|r| r.focused(self))
                        .min_by_key(|r| r.specificity())
                        .map(|r| (g.render_key(window.viewport_size()), r.key()))
                });
            if let Some((key, region)) = target {
                self.open_render_menu(key, region, window, cx);
                cx.stop_propagation();
            }
            return;
        }
        let item = if let Some(menu) = &self.menus.render_menu {
            menu.rows
                .iter()
                .flat_map(|row| match row {
                    RenderMenuRow::Action(i) => std::slice::from_ref(i),
                    RenderMenuRow::Submenu { children, .. } => children,
                })
                .find(|i| {
                    i.action
                        .shortcut
                        .is_some_and(|s| shortcut_matches(s, stroke))
                })
                .cloned()
                .map(|i| (menu.key.clone(), i))
        } else {
            self.viewport
                .geometry
                .as_ref()
                .filter(|g| g.is_current(self))
                .and_then(|g| {
                    g.render_regions
                        .iter()
                        .filter(|r| r.focused(self))
                        .min_by_key(|r| r.specificity())
                        .and_then(|r| {
                            r.controls.iter().find_map(|c| {
                                c.actions
                                    .iter()
                                    .find(|a| {
                                        a.shortcut.is_some_and(|s| shortcut_matches(s, stroke))
                                    })
                                    .map(|a| {
                                        (
                                            g.render_key(window.viewport_size()),
                                            RenderMenuItem {
                                                action: a.clone(),
                                                request: RenderActionRequest {
                                                    provider: c.provider,
                                                    target: c.target.clone(),
                                                    command: a.command,
                                                },
                                            },
                                        )
                                    })
                            })
                        })
                })
        };
        if let Some((key, item)) = item {
            cx.stop_propagation();
            self.run_render_item(key, item, window, cx);
        }
    }
}
fn shortcut_matches(s: RenderShortcut, stroke: &Keystroke) -> bool {
    let m = stroke.modifiers;
    stroke.key == s.key
        && m.shift == s.shift
        && m.alt == s.alt
        && !m.function
        && if cfg!(target_os = "macos") {
            m.control == s.control && m.platform == s.platform
        } else {
            m.control && !m.platform
        }
}
fn next_enabled(current: usize, down: bool, enabled: &[bool]) -> usize {
    if enabled.is_empty() {
        return 0;
    }
    (1..=enabled.len())
        .map(|step| {
            if down {
                (current + step) % enabled.len()
            } else {
                (current + enabled.len() - step % enabled.len()) % enabled.len()
            }
        })
        .find(|&i| enabled[i])
        .unwrap_or(current)
}
fn icon(name: &'static str, color: u32) -> AnyElement {
    svg()
        .path(format!("icon:{name}"))
        .size(px(16.))
        .text_color(rgb(color))
        .flex_shrink_0()
        .into_any_element()
}
fn shortcut_hint(shortcut: RenderShortcut) -> String {
    if cfg!(target_os = "macos") {
        shortcut.hint.to_string()
    } else {
        format!(
            "Ctrl+{}{}{}",
            if shortcut.alt { "Alt+" } else { "" },
            if shortcut.shift { "Shift+" } else { "" },
            shortcut.key
        )
    }
}
pub(super) fn render_action_width(
    action: &RenderAction,
    language: Language,
    window: &Window,
) -> Pixels {
    // Popup border/padding, row padding, optional icon and shortcut gap.
    menu_text_width(
        language.render_label(action.label, action.fallback),
        14.,
        window,
    ) + px(32.)
        + if action.checked || action.icon.is_some() {
            px(24.)
        } else {
            px(0.)
        }
        + action.shortcut.map_or(px(0.), |shortcut| {
            menu_text_width(&shortcut_hint(shortcut), 11., window) + px(8.)
        })
}
fn menu_item(
    item: RenderMenuItem,
    key: RenderFrameKey,
    selected: bool,
    entity: Entity<Editor>,
    index: usize,
    child: bool,
    language: Language,
) -> AnyElement {
    let enabled = item.action.enabled;
    let mut row = div()
        .id(SharedString::from(format!("render-action-{child}-{index}")))
        .min_h(px(32.))
        .py_1()
        .px_2()
        .flex()
        .gap_2()
        .items_center()
        .justify_between()
        .rounded_md()
        .cursor(if enabled {
            CursorStyle::PointingHand
        } else {
            CursorStyle::Arrow
        })
        .text_color(rgb(if enabled { 0x363c44 } else { 0xa4a4a4 }));
    if selected && enabled {
        row = row.bg(rgb(0xe5edf8));
    }
    let mut label = div().flex().flex_1().min_w_0().gap_2().items_center();
    if item.action.checked {
        label = label.child(icon("checkmark_square", 0x555e69));
    } else if let Some(name) = item.action.icon {
        label = label.child(icon(name, if enabled { 0x6f7781 } else { 0xb9bdc2 }));
    }
    label = label.child(language.render_label(item.action.label, item.action.fallback));
    row = row.child(label);
    if let Some(s) = item.action.shortcut {
        row = row.child(
            div()
                .flex_shrink_0()
                .text_size(px(11.))
                .text_color(rgb(if enabled { 0x8c9198 } else { 0xb5b8bc }))
                .child(shortcut_hint(s)),
        );
    }
    let hovered = entity.clone();
    row.on_hover(move |value, _, cx| {
        if *value {
            hovered.update(cx, |e, cx| e.hover_render_row(index, child, cx));
        }
    })
    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
        cx.stop_propagation();
        if enabled {
            entity.update(cx, |e, cx| {
                e.run_render_item(key.clone(), item.clone(), window, cx)
            });
        }
    })
    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    .into_any_element()
}
fn popup(id: &'static str, width: Pixels, height: Pixels) -> Stateful<Div> {
    div()
        .id(id)
        .w(width)
        .max_h(height)
        .overflow_y_scroll()
        .p_1()
        .bg(rgb(0xf8f8f7))
        .text_size(px(14.))
        .text_color(rgb(0x363c44))
        .border_1()
        .border_color(rgb(0xd1d2d4))
        .rounded_md()
        .shadow_md()
        .cursor_default()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
}
pub(super) fn prepaint_render_controls(
    editor: &RenderViewState,
    entity: &Entity<Editor>,
    geometry: &mut Geometry,
    controls: &mut Vec<AnyElement>,
    window: &mut Window,
    cx: &mut App,
) {
    let key = geometry.render_key(window.viewport_size());
    // Structural icons remain visible while dragging or composing; only activation is suppressed.
    for region in &geometry.render_regions {
        let Some(control) = region.controls.first() else {
            continue;
        };
        let ryne_extensions::RenderControlPlacement::Marker { icon: name } = control.placement
        else {
            continue;
        };
        let trigger = region.trigger;
        if !geometry.bounds.contains(&trigger.center()) {
            continue;
        }
        let mut button = div()
            .id(SharedString::from(format!(
                "render-marker-{}",
                region.key()
            )))
            .size(px(24.))
            .flex()
            .items_center()
            .justify_center()
            .child(icon(name, 0x6f7781));
        if editor.available {
            geometry.render_buttons.push(trigger);
            let entity = entity.clone();
            let key = key.clone();
            let id = region.key();
            button = button
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    cx.stop_propagation();
                    entity.update(cx, |e, cx| {
                        e.activate_render_control(key.clone(), id.clone(), window, cx)
                    });
                })
                .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        }
        let mut button = button.into_any_element();
        button.prepaint_as_root(
            trigger.origin,
            size(px(24.).into(), px(24.).into()),
            window,
            cx,
        );
        controls.push(button);
    }
    if !editor.available {
        return;
    }
    let focused = geometry
        .render_regions
        .iter()
        .filter(|r| r.contains_selection(&editor.selection))
        .min_by_key(|r| r.specificity())
        .map(RenderRegion::key);
    for region in &geometry.render_regions {
        if region.controls.iter().any(|c| {
            matches!(
                c.placement,
                ryne_extensions::RenderControlPlacement::Marker { .. }
            )
        }) {
            continue;
        }
        let id = region.key();
        let header = region.controls.iter().find_map(|c| match &c.placement {
            ryne_extensions::RenderControlPlacement::Header { label } => Some(label.clone()),
            _ => None,
        });
        let active = header.is_some()
            || editor.hovered.as_ref() == Some(&id)
            || (editor.focused && focused.as_ref() == Some(&id))
            || editor
                .menu
                .as_ref()
                .is_some_and(|m| m.region == id && m.key == key);
        if !active {
            continue;
        }
        let trigger = region.trigger;
        if trigger.left() < geometry.bounds.left()
            || trigger.right() > geometry.bounds.right()
            || trigger.top() < geometry.bounds.top()
            || trigger.bottom() > geometry.bounds.bottom()
        {
            continue;
        }
        geometry.render_buttons.push(trigger);
        let entity = entity.clone();
        let key = key.clone();
        let mut button = div()
            .id(SharedString::from(format!("render-trigger-{id}")))
            .w(trigger.size.width)
            .h(trigger.size.height)
            .flex()
            .items_center()
            .justify_center()
            .rounded_sm()
            .overflow_hidden()
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0xe1e3e6)))
            .child(match header {
                Some(label) => div()
                    .flex()
                    .gap_1()
                    .items_center()
                    .text_size(px(12.))
                    .text_color(rgb(theme::MUTED))
                    .child(label)
                    .child(
                        svg()
                            .path("icon:chevron_down")
                            .size(px(12.))
                            .flex_shrink_0()
                            .text_color(rgb(theme::MUTED)),
                    )
                    .into_any_element(),
                None => icon("more_vertical", 0x8b8f94),
            })
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                entity.update(cx, |e, cx| {
                    e.open_render_menu(key.clone(), id.clone(), window, cx)
                });
            })
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .into_any_element();
        button.prepaint_as_root(
            trigger.origin,
            size(trigger.size.width.into(), trigger.size.height.into()),
            window,
            cx,
        );
        controls.push(button);
    }
}
/// Popups are prepainted after every content control, matching paint and hit-test order.
pub(super) fn prepaint_render_menu(
    editor: &RenderViewState,
    entity: &Entity<Editor>,
    geometry: &mut Geometry,
    controls: &mut Vec<AnyElement>,
    window: &mut Window,
    cx: &mut App,
) {
    if !editor.available {
        return;
    }
    let key = geometry.render_key(window.viewport_size());
    let Some(menu) = editor.menu.as_ref().filter(|m| m.key == key) else {
        return;
    };
    let Some(anchor) = geometry
        .render_regions
        .iter()
        .find(|r| r.key() == menu.region && geometry.render_buttons.contains(&r.trigger))
        .map(|r| r.trigger)
    else {
        return;
    };
    let language = entity.read(cx).locale.language;
    let width = menu
        .rows
        .iter()
        .fold(px(280.), |width, row| {
            width.max(match row {
                RenderMenuRow::Action(item) => render_action_width(&item.action, language, window),
                RenderMenuRow::Submenu {
                    label, fallback, ..
                } => menu_text_width(language.render_label(label, fallback), 14., window) + px(56.),
            })
        })
        .min(geometry.bounds.size.width);
    let mut view = popup("render-menu", width, geometry.bounds.size.height);
    let mut last_group = None;
    for (index, row) in menu.rows.iter().enumerate() {
        if last_group.is_some_and(|g| g != row.group()) {
            view = view.child(div().h(px(1.)).my_1().bg(rgb(0xdedfe1)));
        }
        last_group = Some(row.group());
        view = view.child(match row {
            RenderMenuRow::Action(item) => menu_item(
                item.clone(),
                key.clone(),
                menu.selected == index,
                entity.clone(),
                index,
                false,
                language,
            ),
            RenderMenuRow::Submenu {
                label, fallback, ..
            } => {
                let click = entity.clone();
                let hover = entity.clone();
                div()
                    .id(SharedString::from(format!("render-submenu-{index}")))
                    .min_h(px(32.))
                    .py_1()
                    .px_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .rounded_md()
                    .cursor_pointer()
                    .when(menu.selected == index, |s| s.bg(rgb(0xe5edf8)))
                    .child(language.render_label(label, fallback))
                    .child(div().flex_shrink_0().child("›"))
                    .on_hover(move |value, _, cx| {
                        if *value {
                            hover.update(cx, |e, cx| e.hover_render_row(index, false, cx));
                        }
                    })
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        click.update(cx, |e, cx| e.hover_render_row(index, false, cx));
                    })
                    .into_any_element()
            }
        });
    }
    let mut view = view.into_any_element();
    let measured = view.layout_as_root(size(width.into(), AvailableSpace::MinContent), window, cx);
    let origin = point(
        (anchor.right() - measured.width).clamp(
            geometry.bounds.left(),
            (geometry.bounds.right() - measured.width).max(geometry.bounds.left()),
        ),
        (anchor.bottom() + px(4.))
            .min(geometry.bounds.bottom() - measured.height)
            .max(geometry.bounds.top()),
    );
    let parent_bounds = Bounds::new(origin, measured);
    geometry.render_menu_bounds.push(parent_bounds);
    view.prepaint_at(origin, window, cx);
    controls.push(view);
    if let Some(RenderMenuRow::Submenu { children, .. }) =
        menu.submenu.and_then(|i| menu.rows.get(i))
    {
        let child_width = children
            .iter()
            .fold(px(150.), |width, item| {
                width.max(render_action_width(&item.action, language, window))
            })
            .min(geometry.bounds.size.width);
        let mut child = popup("render-submenu", child_width, geometry.bounds.size.height)
            .children(children.iter().enumerate().map(|(i, item)| {
                menu_item(
                    item.clone(),
                    key.clone(),
                    menu.child_selected == i,
                    entity.clone(),
                    i,
                    true,
                    language,
                )
            }))
            .into_any_element();
        let measured = child.layout_as_root(
            size(child_width.into(), AvailableSpace::MinContent),
            window,
            cx,
        );
        let x = if parent_bounds.right() + measured.width <= geometry.bounds.right() {
            parent_bounds.right() - px(1.)
        } else {
            parent_bounds.left() - measured.width + px(1.)
        };
        let origin = point(
            x.clamp(
                geometry.bounds.left(),
                (geometry.bounds.right() - measured.width).max(geometry.bounds.left()),
            ),
            parent_bounds
                .top()
                .min(geometry.bounds.bottom() - measured.height)
                .max(geometry.bounds.top()),
        );
        geometry
            .render_menu_bounds
            .push(Bounds::new(origin, measured));
        child.prepaint_at(origin, window, cx);
        controls.push(child);
    }
}
