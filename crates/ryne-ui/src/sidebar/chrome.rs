use super::*;

pub(super) fn glyph(name: &'static str, size: f32, color: u32) -> Svg {
    svg()
        .path(name)
        .size(px(size))
        .flex_shrink_0()
        .text_color(rgb(color))
}
fn tool(id: &'static str, icon: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .size(px(30.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .cursor_pointer()
        .hover(|d| d.bg(rgb(theme::BORDER)))
        .tooltip(move |_, cx| cx.new(|_| element::ModeTooltip(label)).into())
        .child(glyph(icon, 16., theme::MUTED))
}
impl SidebarState {
    pub(super) fn folder_icon(&self, path: &Path, expanded: bool) -> &'static str {
        if self.request.as_ref().is_some_and(|r| {
            r.path == path
                && !r.cancel.load(Ordering::Relaxed)
                && (self.choice.is_none() || self.choice == self.browser.root)
        }) {
            "icon:folder_sync"
        } else if self.browser.known_empty(path) {
            "icon:folder_prohibited"
        } else if expanded {
            "icon:folder_open"
        } else {
            "icon:folder"
        }
    }

    pub(super) fn document_icon(&self, path: &Path, saving: Option<&Path>) -> &'static str {
        let matches_row = |absolute: &Path| {
            self.browser
                .root
                .as_ref()
                .and_then(|root| absolute.strip_prefix(root).ok())
                == Some(path)
        };
        let revision = self.saved_revisions.get(path);
        if self.opening.as_deref().is_some_and(matches_row)
            || saving.is_some_and(matches_row)
            || revision.is_some_and(|r| r.reading_metadata)
            || self.summaries.iter().any(|k| {
                k.generation == self.browser.generation
                    && k.revision == revision.map_or(0, |r| r.version)
                    && k.path == path
            })
        {
            "icon:folder_sync"
        } else if path.extension().is_some_and(|ext| {
            ["md", "markdown", "mdown", "mkd"]
                .iter()
                .any(|s| ext.eq_ignore_ascii_case(s))
        }) {
            "icon:text_bullet_list_square"
        } else {
            "icon:sidebar_document"
        }
    }
}
impl Editor {
    pub(super) fn sidebar_header(&self, pane: Pane, cx: &mut Context<Self>) -> Div {
        let lang = self.locale.language;
        let outline = pane == Pane::Outline;
        let files_pane = if self.sidebar.prefs.list {
            Pane::List
        } else {
            Pane::Tree
        };
        let target = if outline { files_pane } else { Pane::Outline };
        let switch = if target == Pane::Outline {
            tool(
                "outline-tab",
                "icon:reading_list",
                lang.text(Text::OutlineView),
            )
        } else {
            tool(
                "directory-tab",
                "icon:text_bullet_list_ltr",
                lang.text(if target == Pane::List {
                    Text::FileListView
                } else {
                    Text::FileTreeView
                }),
            )
        };
        div()
            .w_full()
            .h(px(40.))
            .flex_shrink_0()
            .px(px(5.))
            .flex()
            .items_center()
            .justify_between()
            .child(switch.on_click(cx.listener(move |s, _, w, cx| s.select_pane(target, w, cx))))
            .child(
                div()
                    .text_size(px(14.))
                    .text_color(rgb(theme::INK))
                    .child(lang.text(if outline {
                        Text::Outline
                    } else {
                        Text::Directory
                    })),
            )
            .child(
                tool(
                    "sidebar-search",
                    "icon:sidebar_search",
                    lang.text(if outline {
                        Text::FilterHeadings
                    } else {
                        Text::FilterFiles
                    }),
                )
                .on_click(cx.listener(|s, _, w, cx| s.search_sidebar(w, cx))),
            )
    }
    pub(super) fn search_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar_protected() {
            return;
        }
        let input = if self.sidebar.pane() == Pane::Outline {
            self.sidebar.heading_search = true;
            &self.sidebar.heading_filter
        } else {
            self.select_pane(Pane::List, window, cx);
            &self.sidebar.filter
        };
        window.focus(&input.read(cx).focus);
        cx.notify();
    }
    pub(super) fn sidebar_footer(&self, root: &Path, pane: Pane, cx: &mut Context<Self>) -> Div {
        let lang = self.locale.language;
        let name = root
            .file_name()
            .unwrap_or(root.as_os_str())
            .to_string_lossy()
            .into_owned();
        let list = pane == Pane::List;
        div()
            .relative()
            .w_full()
            .flex_shrink_0()
            .h(px(30.))
            .border_t_1()
            .border_color(rgb(theme::BORDER))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .px(px(58.))
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_ellipsis()
                            .text_size(px(12.))
                            .text_color(rgb(theme::MUTED))
                            .child(name),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        tool(
                            "footer-open-folder",
                            "icon:sidebar_plus",
                            lang.text(Text::OpenFolder),
                        )
                        .on_click(cx.listener(|s, _, w, cx| s.open_folder(&OpenFolder, w, cx))),
                    )
                    .child(
                        div()
                            .flex()
                            .child(
                                tool(
                                    "sidebar-more",
                                    "icon:more_vertical",
                                    lang.text(Text::FolderOptions),
                                )
                                .on_click(cx.listener(
                                    |s, _, _, cx| {
                                        if !s.sidebar_protected() {
                                            s.sidebar.folder_menu = !s.sidebar.folder_menu;
                                            cx.notify();
                                        }
                                    },
                                )),
                            )
                            .child(
                                tool(
                                    if list { "tree-view" } else { "list-view" },
                                    if list {
                                        "icon:sidebar_outline"
                                    } else {
                                        "icon:sidebar_cards"
                                    },
                                    lang.text(if list {
                                        Text::FileTreeView
                                    } else {
                                        Text::FileListView
                                    }),
                                )
                                .on_click(cx.listener(
                                    move |s, _, w, cx| {
                                        s.select_pane(
                                            if list { Pane::Tree } else { Pane::List },
                                            w,
                                            cx,
                                        )
                                    },
                                )),
                            ),
                    ),
            )
    }
    pub(super) fn folder_menu(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let lang = self.locale.language;
        let item = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .px_3()
                .py_1()
                .rounded_sm()
                .cursor_pointer()
                .hover(|s| s.bg(rgb(theme::CODE_BG)))
                .child(label)
        };
        div()
            .id("folder-options")
            .absolute()
            .right(px(8.))
            .bottom(px(31.))
            .occlude()
            .p_1()
            .rounded_md()
            .shadow_sm()
            .bg(rgb(theme::PAPER))
            .border_1()
            .border_color(rgb(theme::BORDER))
            .child(
                item("menu-open-folder", lang.text(Text::OpenFolder)).on_click(cx.listener(
                    |s, _, w, cx| {
                        s.sidebar.folder_menu = false;
                        s.open_folder(&OpenFolder, w, cx);
                        cx.notify();
                    },
                )),
            )
            .child(
                item("refresh-directory", lang.text(Text::Refresh)).on_click(cx.listener(
                    |s, _, _, cx| {
                        s.sidebar.folder_menu = false;
                        s.sidebar.refresh_anchors = s.sidebar.scroll_anchors();
                        let root = s.sidebar.browser.root.clone();
                        s.sidebar.browser.root(root, s.sidebar.browser.manual, true);
                        s.sidebar.reset_root(true, cx);
                        cx.notify();
                    },
                )),
            )
    }
}
