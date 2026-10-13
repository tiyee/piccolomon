use super::*;
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = ryne, no_json)]
pub(super) struct OpenRecent {
    pub path: PathBuf,
}

pub(super) fn application_menus(language: Language, paths: &[PathBuf]) -> Vec<Menu> {
    let text = |key| language.text(key);
    let mut recent = paths
        .iter()
        .map(|path| {
            MenuItem::action(
                path.to_string_lossy().into_owned(),
                OpenRecent { path: path.clone() },
            )
        })
        .collect::<Vec<_>>();
    if recent.is_empty() {
        recent.push(MenuItem::action(text(Text::NoRecent), NoRecent));
    }
    recent.push(MenuItem::separator());
    recent.push(MenuItem::action(text(Text::ClearRecent), ClearRecent));
    vec![
        Menu {
            name: "Ryne".into(),
            items: vec![
                MenuItem::action(text(Text::About), About),
                MenuItem::action(text(Text::Settings), Settings),
                MenuItem::action(text(Text::CheckUpdates), CheckUpdates),
                MenuItem::separator(),
                MenuItem::action(text(Text::Hide), Hide),
                MenuItem::separator(),
                MenuItem::action(text(Text::Quit), Quit),
            ],
        },
        Menu {
            name: text(Text::File).into(),
            items: vec![
                MenuItem::action(text(Text::New), New),
                MenuItem::action(text(Text::Open), Open),
                MenuItem::submenu(Menu {
                    name: text(Text::Recent).into(),
                    items: recent,
                }),
                MenuItem::action(text(Text::Reload), Reload),
                MenuItem::separator(),
                MenuItem::action(text(Text::Save), Save),
                MenuItem::action(text(Text::SaveAs), SaveAs),
                MenuItem::separator(),
                MenuItem::action(text(Text::ExportPdf), ExportPdf),
                MenuItem::action(text(Text::ExportPdfV2), ExportPdfV2),
            ],
        },
    ]
}
impl Editor {
    pub(super) fn sync_application_menus(&self, cx: &App) {
        cx.set_menus(application_menus(self.locale.language, &self.recent.paths));
    }
    pub(super) fn remember_file(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        self.recent.remember(path);
        if let Err(error) = self.recent.persist() {
            self.status = self.locale.language.error(Text::RecentFailed, error);
        }
        self.sync_application_menus(cx);
    }
    pub(super) fn clear_recent(&mut self, _: &ClearRecent, _: &mut Window, cx: &mut Context<Self>) {
        self.recent.paths.clear();
        if let Err(error) = self.recent.persist() {
            self.status = self.locale.language.error(Text::RecentFailed, error);
        }
        self.sync_application_menus(cx);
        cx.notify();
    }
    pub(super) fn about(&mut self, _: &About, window: &mut Window, cx: &mut Context<Self>) {
        let detail = format!(
            "{} {}",
            self.locale.language.text(Text::AboutDetail),
            env!("CARGO_PKG_VERSION")
        );
        drop(window.prompt(
            PromptLevel::Info,
            self.locale.language.text(Text::About),
            Some(&detail),
            &[self.locale.language.text(Text::Ok)],
            cx,
        ));
    }
    pub(super) fn settings(&mut self, _: &Settings, _: &mut Window, cx: &mut Context<Self>) {
        self.language_menu = true;
        cx.notify();
    }
    pub(super) fn hide(&mut self, _: &Hide, _: &mut Window, cx: &mut Context<Self>) {
        cx.hide();
    }
    pub(super) fn check_updates(
        &mut self,
        _: &CheckUpdates,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.checking_updates {
            return;
        }
        self.checking_updates = true;
        self.status = Text::CheckingUpdates.into();
        let checker = self.desktop.updates.clone();
        let task = cx
            .background_executor()
            .spawn(async move { checker.check(env!("CARGO_PKG_VERSION")) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let Ok((answer, url)) = this.update_in(cx, |this, window, cx| {
                this.checking_updates = false;
                let (title, detail, url) = match result {
                    Ok(ryne_extensions::UpdateStatus::Current) => (Text::UpToDate, None, None),
                    Ok(ryne_extensions::UpdateStatus::NoRelease) => (Text::NoRelease, None, None),
                    Ok(ryne_extensions::UpdateStatus::Available { version, url }) => {
                        (Text::UpdateAvailable, Some(version), Some(url))
                    }
                    Err(error) => (Text::UpdateFailed, Some(error), None),
                };
                this.status = detail
                    .as_ref()
                    .map_or_else(|| title.into(), |d| this.locale.language.error(title, d));
                let mut buttons = vec![this.locale.language.text(Text::Ok)];
                if url.is_some() {
                    buttons = vec![
                        this.locale.language.text(Text::Cancel),
                        this.locale.language.text(Text::ViewRelease),
                    ];
                }
                let answer = window.prompt(
                    if matches!(title, Text::UpdateFailed) {
                        PromptLevel::Warning
                    } else {
                        PromptLevel::Info
                    },
                    this.locale.language.text(title),
                    detail.as_deref(),
                    &buttons,
                    cx,
                );
                cx.notify();
                (answer, url)
            }) else {
                return;
            };
            if answer.await.ok() == Some(1)
                && let Some(url) = url
            {
                let _ = cx.update(|_, cx| cx.open_url(&url));
            }
        })
        .detach();
        cx.notify();
    }
}

impl Editor {
    /// GPUI 0.2.2 stores app menus on Windows without drawing a native menu bar.
    /// Present the same command tree inside the window on those platforms.
    pub(super) fn window_menu(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let menus = application_menus(self.locale.language, &self.recent.paths);
        let mut bar = div()
            .relative()
            .flex()
            .gap_1()
            .px_2()
            .h(px(30.))
            .items_center()
            .border_b_1()
            .border_color(rgb(theme::BORDER));
        for (index, menu) in menus.iter().enumerate() {
            bar = bar.child(
                div()
                    .id(("app-menu", index))
                    .px_3()
                    .py_1()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(0xecebe7)))
                    .child(menu.name.clone())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.top_menu = if this.top_menu == Some(index) {
                                None
                            } else {
                                Some(index)
                            };
                            this.recent_menu = false;
                            cx.notify();
                        }),
                    ),
            );
        }
        if let Some(index) = self.top_menu
            && let Some(menu) = menus.into_iter().nth(index)
        {
            let mut items = Vec::new();
            for item in menu.items {
                match item {
                    MenuItem::Submenu(recent) => {
                        items.push(
                            div()
                                .id("recent-submenu")
                                .px_3()
                                .py_2()
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(0xecebe7)))
                                .child(format!("{} ▸", recent.name))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.recent_menu = !this.recent_menu;
                                        cx.notify();
                                    }),
                                )
                                .into_any_element(),
                        );
                        if self.recent_menu {
                            items.extend(
                                recent
                                    .items
                                    .into_iter()
                                    .map(|item| window_menu_item(item, cx)),
                            );
                        }
                    }
                    item => items.push(window_menu_item(item, cx)),
                }
            }
            let width = 340_f32.min(f32::from(window.viewport_size().width) - 16.);
            bar = bar.child(
                deferred(
                    div()
                        .absolute()
                        .top(px(30.))
                        .left(px(8.))
                        .w(px(width))
                        .max_h(window.viewport_size().height - px(90.))
                        .id("app-menu-popup")
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .py_1()
                        .bg(rgb(theme::PAPER))
                        .border_1()
                        .border_color(rgb(theme::BORDER))
                        .shadow_lg()
                        .occlude()
                        .children(items),
                )
                .with_priority(10),
            );
        }
        bar.into_any_element()
    }
}
fn window_menu_item(item: MenuItem, cx: &mut Context<Editor>) -> AnyElement {
    match item {
        MenuItem::Action { name, action, .. } => {
            let enabled = !action.as_any().is::<NoRecent>();
            div()
                .id(name.clone())
                .px_3()
                .py_2()
                .text_ellipsis()
                .when(enabled, |s| {
                    s.cursor_pointer().hover(|s| s.bg(rgb(0xecebe7)))
                })
                .when(!enabled, |s| s.text_color(rgb(theme::MUTED)))
                .child(name)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        if enabled {
                            this.top_menu = None;
                            window.focus(&this.focus);
                            window.dispatch_action(action.boxed_clone(), cx);
                            cx.notify();
                        }
                    }),
                )
                .into_any_element()
        }
        _ => div()
            .h(px(1.))
            .my_1()
            .bg(rgb(theme::BORDER))
            .into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn native_menus_follow_language_and_recent_paths() {
        let path = PathBuf::from("/tmp/中文.md");
        for language in Language::ALL {
            let menus = application_menus(language, std::slice::from_ref(&path));
            assert_eq!(menus[0].name.as_ref(), "Ryne");
            assert_eq!(menus[1].name.as_ref(), language.text(Text::File));
            for (key, v2) in [(Text::ExportPdf, false), (Text::ExportPdfV2, true)] {
                let entry = menus[1].items.iter().find(|item| {
                    matches!(item, MenuItem::Action { name, .. } if name.as_ref() == language.text(key))
                }).expect("localized export action");
                let MenuItem::Action { action, .. } = entry else {
                    unreachable!()
                };
                if v2 {
                    assert!(action.as_any().is::<ExportPdfV2>());
                } else {
                    assert!(action.as_any().is::<ExportPdf>());
                }
            }
            let MenuItem::Submenu(recent) = &menus[1].items[2] else {
                panic!("recent submenu");
            };
            let MenuItem::Action { action, .. } = &recent.items[0] else {
                panic!("recent action");
            };
            assert_eq!(
                action.as_any().downcast_ref::<OpenRecent>().unwrap().path,
                path
            );
        }
    }
}
