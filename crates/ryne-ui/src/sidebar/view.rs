use super::*;

const TREE_ROW: f32 = 32.;
const TREE_INDENT: f32 = 16.;
const OUTLINE_ICON: f32 = 14.;
const CARD: f32 = 90.;

impl SidebarState {
    pub(crate) fn surface(&self, colors: theme::Palette) -> Option<Div> {
        self.prefs.visible.then(|| {
            div()
                .w(px(self.prefs.width))
                .flex_shrink_0()
                .border_r_1()
                .border_color(rgb(colors.border))
                .bg(rgb(colors.code_bg))
        })
    }
}

fn note(colors: theme::Palette, text: impl Into<SharedString>) -> Div {
    div()
        .px_2()
        .py_1()
        .text_size(px(12.))
        .text_color(rgb(colors.muted))
        .child(text.into())
}
fn highlight(
    colors: theme::Palette,
    d: Stateful<Div>,
    selected: bool,
    focused: bool,
) -> Stateful<Div> {
    let accent = Hsla::from(rgb(colors.link));
    d.relative()
        .border_1()
        .border_color(rgb(colors.code_bg))
        .when(!selected, |d| d.hover(move |d| d.bg(rgb(colors.border))))
        .when(selected, |d| {
            d.bg(accent.opacity(0.13))
                .text_color(rgb(colors.heading_ink))
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(3.))
                        .bg(accent),
                )
        })
        .when(focused, |d| d.border_color(accent))
}
impl Editor {
    pub(super) fn sidebar_keys(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if event.prefer_character_input
            || self.sidebar_protected()
            || event.keystroke.modifiers.modified()
        {
            return;
        }
        let filter = if self.sidebar.pane() == Pane::Outline {
            self.sidebar.heading_filter.read(cx)
        } else {
            self.sidebar.filter.read(cx)
        };
        if filter.focus.is_focused(window) {
            if key == "escape" && filter.marked.is_none() {
                if self.sidebar.pane() == Pane::Outline {
                    self.sidebar.heading_search = false;
                    self.sidebar.heading_filter.update(cx, |f, cx| f.reset(cx));
                }
                window.focus(&self.sidebar.focus, cx);
                cx.stop_propagation();
                cx.notify();
            }
            return;
        }
        if key == "escape" && self.sidebar.folder_menu {
            self.sidebar.folder_menu = false;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if key == "escape" {
            if self.sidebar.overlay(window) {
                self.sidebar_toggle(&ToggleSidebar, window, cx)
            } else {
                window.focus(&self.focus, cx)
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let pane = self.sidebar.pane();
        if key == "/" {
            self.search_sidebar(window, cx);
            cx.stop_propagation();
            return;
        }
        if !matches!(key, "up" | "down" | "enter" | "left" | "right") {
            return;
        }
        let generation = self.sidebar.browser.generation;
        let id = self.document.source.id();
        let (count, at) = if pane == Pane::Outline {
            let headings = ryne_syntax::SemanticQuery::new(&self.document.parse).outline();
            let at = self
                .sidebar
                .outline_focus
                .filter(|(s, _)| *s == id)
                .map(|(_, i)| i)
                .or(self.sidebar.active);
            if self.sidebar.heading_needle.is_empty() {
                (headings.len(), at)
            } else {
                (
                    self.sidebar.heading_matches.len(),
                    at.and_then(|i| self.sidebar.heading_matches.binary_search(&i).ok()),
                )
            }
        } else {
            let rows = if pane == Pane::Tree {
                &self.sidebar.browser.tree
            } else {
                &self.sidebar.browser.list
            };
            let current = self.sidebar.browse_focus[pane.index()]
                .as_deref()
                .or(self.sidebar.browser.relative_current());
            (
                rows.len(),
                current.and_then(|p| rows.iter().position(|row| row == p)),
            )
        };
        if count == 0 {
            return;
        }
        let mut i = match key {
            "up" => at.unwrap_or(0).saturating_sub(1),
            "down" => at.map_or(0, |i| i + 1),
            _ => at.unwrap_or(0),
        }
        .min(count - 1);
        if pane == Pane::Outline {
            let heading = if self.sidebar.heading_needle.is_empty() {
                i
            } else {
                self.sidebar.heading_matches[i]
            };
            self.sidebar.outline_focus = Some((id, heading));
            if key == "enter" {
                self.activate_heading(id, heading, window, cx);
            }
        } else {
            let rows = if pane == Pane::Tree {
                &self.sidebar.browser.tree
            } else {
                &self.sidebar.browser.list
            };
            let path = rows[i].clone();
            let directory = self
                .sidebar
                .browser
                .entries
                .get(&path)
                .is_some_and(|e| e.kind == EntryKind::Directory);
            let expanded = self.sidebar.browser.expanded.contains(&path);
            if key == "enter"
                || directory && (key == "left" && expanded || key == "right" && !expanded)
            {
                self.activate_path(generation, pane, path, window, cx);
            } else {
                if key == "left"
                    && pane == Pane::Tree
                    && let Some(parent) = path.parent()
                    && let Some(j) = rows.iter().position(|p| p == parent)
                {
                    i = j
                }
                if key == "right"
                    && directory
                    && expanded
                    && i + 1 < count
                    && rows[i + 1].parent() == Some(path.as_path())
                {
                    i += 1
                }
                self.sidebar.browse_focus[pane.index()] = Some(rows[i].clone());
            }
        }
        self.sidebar.scroll[pane.index()].scroll_to_item(i, ScrollStrategy::Center);
        cx.stop_propagation();
        cx.notify();
    }
    fn browser_row(
        &mut self,
        index: usize,
        pane: Pane,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let colors = theme::palette(cx);
        let lang = self.locale.language;
        let id = self.document.source.id();
        let focused = self.sidebar.focus.is_focused(window);
        let row = div()
            .id(("sidebar-row", index))
            .debug_selector(move || format!("sidebar-row-{index}"))
            .w_full()
            .cursor_pointer()
            .overflow_hidden();
        if pane == Pane::Outline {
            let index = if self.sidebar.heading_needle.is_empty() {
                index
            } else {
                *self.sidebar.heading_matches.get(index)?
            };
            let heading = ryne_syntax::SemanticQuery::new(&self.document.parse)
                .outline()
                .get(index)?;
            let label = if heading.title.is_empty() {
                lang.text(Text::EmptyHeading).to_owned()
            } else {
                heading.title.graphemes(true).take(180).collect()
            };
            return Some(
                highlight(
                    colors,
                    row,
                    self.sidebar.active == Some(index),
                    focused && self.sidebar.outline_focus == Some((id, index)),
                )
                .h(px(TREE_ROW))
                .pl(px(8. + OUTLINE_ICON * heading.depth as f32))
                .when(
                    heading.level == 1 || self.sidebar.active == Some(index),
                    |d| d.font_weight(FontWeight::SEMIBOLD),
                )
                .pr_2()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    svg()
                        .path(format!("icon:text_header_{}", heading.level))
                        .size(px(OUTLINE_ICON))
                        .flex_shrink_0()
                        .text_color(rgb(if self.sidebar.active == Some(index) {
                            colors.heading_ink
                        } else {
                            colors.muted
                        })),
                )
                .child(div().flex_1().min_w_0().text_ellipsis().child(label))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |s, _, window, cx| {
                        cx.stop_propagation();
                        s.activate_heading(id, index, window, cx);
                    }),
                )
                .into_any_element(),
            );
        }
        let state = &self.sidebar;
        let generation = state.browser.generation;
        let path = if pane == Pane::Tree {
            state.browser.tree.get(index)?
        } else {
            state.browser.list.get(index)?
        }
        .clone();
        let preview = (pane == Pane::List).then(|| self.summary_for(&path, cx));
        let state = &self.sidebar;
        let entry = state.browser.entries.get(&path)?;
        let kind = entry.kind;
        let name = entry.name.to_string_lossy().into_owned();
        let selected = state.browser.relative_current() == Some(path.as_path());
        let row = highlight(
            colors,
            row,
            selected,
            focused && state.browse_focus[pane.index()].as_ref() == Some(&path),
        );
        let expanded = state.browser.expanded.contains(&path);
        let icon = match kind {
            EntryKind::Directory => state.folder_icon(&path, expanded),
            EntryKind::DirectoryLink => "icon:folder",
            EntryKind::Document => state.document_icon(&path, self.document.files.saving_path()),
        };
        let glyph = chrome::glyph(icon, 14., if selected { colors.ink } else { colors.muted });
        let row = if let Some(preview) = preview {
            let dir = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .or_else(|| {
                    state
                        .browser
                        .root
                        .as_deref()
                        .and_then(Path::file_name)
                        .map(Path::new)
                });
            let dir = dir
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let date = entry
                .modified
                .map(|time| modified_label(time, lang))
                .unwrap_or_default();
            row.h(px(CARD))
                .px_2()
                .py_1()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .gap_1()
                        .text_size(px(11.))
                        .text_color(rgb(colors.muted))
                        .child(div().min_w_0().text_ellipsis().child(dir))
                        .child(div().flex_shrink_0().child(date)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(glyph)
                        .child(div().min_w_0().text_ellipsis().child(name)),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .line_height(px(17.))
                        .line_clamp(2)
                        .text_color(rgb(colors.muted))
                        .child(preview),
                )
        } else {
            let error = state.browser.errors.contains_key(&path);
            row.h(px(TREE_ROW))
                .flex()
                .items_center()
                .gap_1()
                .pl(px(
                    TREE_INDENT * (1 + path.components().count().min(13)) as f32
                ))
                .pr(px(8.))
                .child(glyph)
                .child(div().text_ellipsis().child(name))
                .when(kind == EntryKind::DirectoryLink, |r| {
                    r.child(
                        div()
                            .flex_shrink_0()
                            .text_color(rgb(colors.muted))
                            .child("↗"),
                    )
                })
                .when(error, |r| r.child("!"))
        };
        Some(
            row.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |s, _, window, cx| {
                    cx.stop_propagation();
                    s.activate_path(generation, pane, path.clone(), window, cx)
                }),
            )
            .into_any_element(),
        )
    }
    fn update_browser_rows(&mut self) {
        if !self.sidebar.browser.dirty_rows {
            return;
        }
        // Preserve the first visible path and its fractional row offset while
        // incoming batches insert earlier rows in either sorted view.
        let state = &mut self.sidebar;
        let anchors = state.scroll_anchors();
        state.browser.rows();
        for (i, anchor) in anchors.into_iter().enumerate() {
            let (rows, height) = if i == 0 {
                (&state.browser.tree, TREE_ROW)
            } else {
                (&state.browser.list, CARD)
            };
            if let Some((path, offset)) = state.refresh_anchors[i].clone().or(anchor)
                && let Some(j) = rows.iter().position(|p| p == &path)
            {
                state.refresh_anchors[i] = None;
                state.scroll[i]
                    .0
                    .borrow()
                    .base_handle
                    .set_offset(point(px(0.), -px(j as f32 * height + offset)));
            }
        }
    }
    pub(crate) fn sidebar_view(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let colors = theme::palette(cx);
        self.update_browser_rows();
        self.pump_directory(cx);
        let surface = self.sidebar.surface(colors)?;
        let pane = self.sidebar.pane();
        let outline = pane == Pane::Outline;
        let language = self.locale.language;
        let root = self.sidebar.browser.root.clone();
        let protected = self.sidebar_protected();
        let id = self.document.source.id();
        let count = match pane {
            Pane::Tree => self.sidebar.browser.tree.len(),
            Pane::List => self.sidebar.browser.list.len(),
            Pane::Outline => {
                let needle = if self.sidebar.heading_search {
                    self.sidebar.heading_filter.read(cx).value.to_lowercase()
                } else {
                    String::new()
                };
                let headings = ryne_syntax::SemanticQuery::new(&self.document.parse).outline();
                if self.sidebar.outline_id != Some(id) || self.sidebar.heading_needle != needle {
                    self.sidebar.heading_matches.clear();
                    if !needle.is_empty() {
                        self.sidebar.heading_matches.extend(
                            headings
                                .iter()
                                .enumerate()
                                .filter(|(_, h)| h.title.to_lowercase().contains(&needle))
                                .map(|(i, _)| i),
                        );
                    }
                    self.sidebar.heading_needle = needle;
                    if self.sidebar.outline_id != Some(id) {
                        self.sidebar.active = None;
                    }
                    self.sidebar.outline_id = Some(id);
                    self.sidebar.outline_focus = None;
                }
                if self.sidebar.heading_needle.is_empty() {
                    headings.len()
                } else {
                    self.sidebar.heading_matches.len()
                }
            }
        };
        let mut panel = surface
            .id("sidebar")
            .debug_selector(|| "sidebar".into())
            .occlude()
            .relative()
            .h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .text_size(px(14.))
            .overflow_hidden()
            .when(!protected, |d| d.track_focus(&self.sidebar.focus))
            .on_key_down(cx.listener(Self::sidebar_keys))
            .child(self.sidebar_header(pane, cx));
        if pane == Pane::Tree
            && let Some(root) = root.as_ref()
        {
            panel = panel.child(
                div()
                    .h(px(TREE_ROW))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_1()
                    .pl(px(TREE_INDENT))
                    .child(chrome::glyph(
                        self.sidebar.folder_icon(Path::new(""), true),
                        14.,
                        colors.muted,
                    ))
                    .child(
                        div().text_ellipsis().child(
                            root.file_name()
                                .unwrap_or(root.as_os_str())
                                .to_string_lossy()
                                .into_owned(),
                        ),
                    ),
            );
        }
        if outline && self.sidebar.heading_search && !protected {
            self.sidebar.heading_filter.update(cx, |f, _| {
                f.placeholder = language.text(Text::FilterHeadings)
            });
            panel = panel.child(self.sidebar.heading_filter.clone());
        }
        if pane == Pane::List && root.is_some() && !protected {
            self.sidebar
                .filter
                .update(cx, |f, _| f.placeholder = language.text(Text::FilterFiles));
            panel = panel.child(self.sidebar.filter.clone());
        }
        let entity = cx.entity();
        panel = if count > 0 {
            let rows = uniform_list("sidebar-rows", count, move |range, window, cx| {
                entity.update(cx, |s, cx| {
                    range
                        .filter_map(|i| s.browser_row(i, pane, window, cx))
                        .collect()
                })
            })
            .track_scroll(&self.sidebar.scroll[pane.index()])
            .h_full();
            panel.child(div().min_h_0().flex_1().child(rows))
        } else {
            panel.child(
                div()
                    .flex_1()
                    .when(outline, |d| {
                        d.child(note(
                            colors,
                            language.text(if self.sidebar.heading_needle.is_empty() {
                                Text::EmptyOutline
                            } else {
                                Text::NoMatches
                            }),
                        ))
                    })
                    .when(!outline && root.is_none(), |d| {
                        d.flex().items_center().justify_center().p_3().child(
                            div()
                                .id("choose-folder")
                                .cursor_pointer()
                                .px_4()
                                .py_3()
                                .rounded_md()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap_2()
                                .text_size(px(12.))
                                .text_color(rgb(colors.muted))
                                .hover(move |d| d.bg(rgb(colors.border)))
                                .child(chrome::glyph("icon:folder_open", 32., colors.muted))
                                .child(div().text_center().child(language.text(Text::OpenFolder)))
                                .on_click(
                                    cx.listener(|s, _, w, cx| s.open_folder(&OpenFolder, w, cx)),
                                ),
                        )
                    })
                    .when(
                        !outline
                            && root.is_some()
                            && self.sidebar.request.is_none()
                            && self.sidebar.browser.errors.is_empty(),
                        |d| {
                            d.child(note(
                                colors,
                                language.text(
                                    if pane == Pane::List && !self.sidebar.browser.filter.is_empty()
                                    {
                                        Text::NoMatches
                                    } else {
                                        Text::EmptyFiles
                                    },
                                ),
                            ))
                        },
                    ),
            )
        };
        if !outline {
            if let Some(error) = self.sidebar.message.as_ref().cloned().or_else(|| {
                self.sidebar
                    .browser
                    .errors
                    .first_key_value()
                    .map(|(p, e)| format!("{} {e}", p.display()))
            }) {
                panel = panel.child(note(
                    colors,
                    format!("{} · {error}", language.text(Text::DirectoryFailed)),
                ));
            }
            if self.sidebar.request.is_some() {
                panel = panel.child(note(colors, language.text(Text::Scanning)));
            }
            if self.sidebar.browser.limited() {
                panel = panel.child(note(colors, language.text(Text::Partial)));
            }
            if let Some(root) = root.as_ref() {
                panel = panel.child(self.sidebar_footer(root, pane, cx));
                if self.sidebar.folder_menu {
                    panel = panel.child(self.folder_menu(cx));
                }
            }
        }
        Some(
            panel
                .child(
                    div()
                        .id("sidebar-divider")
                        .debug_selector(|| "sidebar-divider".into())
                        .absolute()
                        .right_0()
                        .top_0()
                        .bottom_0()
                        .w(px(4.))
                        .cursor(CursorStyle::ResizeLeftRight)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|s, _, _, cx| {
                                if !s.sidebar_protected() {
                                    s.sidebar.resizing = true;
                                }
                                cx.stop_propagation();
                            }),
                        ),
                )
                .into_any_element(),
        )
    }
}

/// Gregorian date of a UTC timestamp. Shift to a March-based 400-year cycle
/// so February's leap day is the last day of the cycle (civil_from_days).
pub(super) fn modified_label(time: SystemTime, language: Language) -> String {
    let seconds = match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        Err(e) => {
            -i64::try_from(e.duration().as_secs()).unwrap_or(i64::MAX)
                - i64::from(e.duration().subsec_nanos() != 0)
        }
    };
    let day = seconds.div_euclid(86400) + 719468;
    let era = day.div_euclid(146097);
    let day_of_era = day.rem_euclid(146097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
    let ordinal = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let march_month = (5 * ordinal + 2) / 153;
    let d = ordinal - (153 * march_month + 2) / 5 + 1;
    let m = if march_month < 10 {
        march_month + 3
    } else {
        march_month - 9
    };
    let y = year_of_era + era * 400 + i64::from(m <= 2);
    let h = seconds.rem_euclid(86400) / 3600;
    let minutes = seconds.rem_euclid(3600) / 60;
    match language {
        Language::Fr | Language::Es => format!("{d:02}/{m:02}/{y} {h:02}:{minutes:02} UTC"),
        _ => format!("{y}-{m:02}-{d:02} {h:02}:{minutes:02} UTC"),
    }
}

impl SidebarState {
    pub(super) fn scroll_anchors(&self) -> [Option<(PathBuf, f32)>; 2] {
        std::array::from_fn(|i| {
            let (rows, height) = if i == 0 {
                (&self.browser.tree, TREE_ROW)
            } else {
                (&self.browser.list, CARD)
            };
            let y = -f32::from(self.scroll[i].0.borrow().base_handle.offset().y);
            let index = (y / height) as usize;
            (y > 0.)
                .then(|| rows.get(index).map(|p| (p.clone(), y % height)))
                .flatten()
        })
    }
}
