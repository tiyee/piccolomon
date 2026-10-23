use super::*;
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = ryne, no_json)]
pub(super) struct ExportRegistered {
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = ryne, no_json)]
pub(super) struct OpenRecent {
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = ryne, no_json)]
pub(super) struct SelectLanguage {
    pub preference: Preference,
}

#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = ryne, no_json)]
pub(super) struct SelectMathFont {
    pub font: MathFont,
}

pub(super) fn application_menus(
    language: Language,
    preference: Preference,
    math_font: MathFont,
    paths: &[PathBuf],
    exports: &[ExportDescriptor],
) -> Vec<Menu> {
    let text = |key| language.text(key);
    let language_choice = |name: &str, choice: Preference| {
        MenuItem::action(
            if preference == choice {
                format!("✓ {name}")
            } else {
                name.to_owned()
            },
            SelectLanguage { preference: choice },
        )
    };
    let mut languages = vec![language_choice(text(Text::System), Preference(None))];
    languages.extend(
        Language::ALL
            .into_iter()
            .map(|language| language_choice(language.native_name(), Preference(Some(language)))),
    );
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
                MenuItem::submenu(Menu {
                    name: text(Text::Language).into(),
                    items: languages,
                }),
                MenuItem::submenu(Menu {
                    name: text(Text::MathFont).into(),
                    items: MathFont::ALL
                        .into_iter()
                        .map(|font| {
                            MenuItem::action(
                                if font == math_font {
                                    format!("✓ {}", font.name())
                                } else {
                                    font.name().to_owned()
                                },
                                SelectMathFont { font },
                            )
                        })
                        .collect(),
                }),
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
            ]
            .into_iter()
            .chain(exports.iter().map(|format| {
                MenuItem::action(
                    language.render_label(format.label, format.fallback),
                    ExportRegistered {
                        id: format.id.into(),
                    },
                )
            }))
            .collect(),
        },
    ]
}
impl Editor {
    pub(super) fn sync_application_menus(&self, cx: &App) {
        cx.set_menus(application_menus(
            self.locale.language,
            self.locale.preference,
            self.math_font.font,
            &self.recent.paths,
            self.desktop.exports.formats(),
        ));
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
    pub(super) fn change_language(
        &mut self,
        action: &SelectLanguage,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_language(action.preference, cx);
    }
    pub(super) fn change_math_font(
        &mut self,
        action: &SelectMathFont,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menus.top_menu = None;
        self.menus.top_submenu = None;
        if let Err(error) = self.math_font.select(action.font) {
            self.status = self
                .locale
                .language
                .error(Text::MathFontPreferenceFailed, error);
        }
        self.refresh(cx);
        self.sync_application_menus(cx);
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
        if self.operations.checking_updates {
            return;
        }
        self.operations.checking_updates = true;
        self.status = Text::CheckingUpdates.into();
        let checker = self.desktop.updates.clone();
        let task = cx
            .background_executor()
            .spawn(async move { checker.check(env!("CARGO_PKG_VERSION")) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let Ok((answer, url)) = this.update_in(cx, |this, window, cx| {
                this.operations.checking_updates = false;
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
        let menus = application_menus(
            self.locale.language,
            self.locale.preference,
            self.math_font.font,
            &self.recent.paths,
            self.desktop.exports.formats(),
        );
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
                            this.menus.top_menu = if this.menus.top_menu == Some(index) {
                                None
                            } else {
                                Some(index)
                            };
                            this.menus.top_submenu = None;
                            cx.notify();
                        }),
                    ),
            );
        }
        if let Some(index) = self.menus.top_menu
            && let Some(menu) = menus.into_iter().nth(index)
        {
            let mut items = Vec::new();
            for (item_index, item) in menu.items.into_iter().enumerate() {
                match item {
                    MenuItem::Submenu(submenu) => {
                        items.push(
                            div()
                                .id(("app-submenu", item_index))
                                .px_3()
                                .py_2()
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(0xecebe7)))
                                .child(format!("{} ▸", submenu.name))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.menus.top_submenu =
                                            if this.menus.top_submenu == Some(item_index) {
                                                None
                                            } else {
                                                Some(item_index)
                                            };
                                        cx.notify();
                                    }),
                                )
                                .into_any_element(),
                        );
                        if self.menus.top_submenu == Some(item_index) {
                            items.extend(
                                submenu
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
                            this.menus.top_menu = None;
                            this.menus.top_submenu = None;
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
    fn test_exports() -> Vec<ExportDescriptor> {
        struct Unused;
        impl ryne_extensions::PdfExporter for Unused {
            fn export(&self, _: &Display) -> Result<Vec<u8>, String> {
                Err("menu test".into())
            }
        }
        [
            ("pdf", "export.pdf", "Export PDF"),
            ("pdf-v2", "export.pdf-v2", "Export PDF v2"),
        ]
        .into_iter()
        .map(|(id, label, fallback)| ExportDescriptor {
            id,
            label,
            fallback,
            file_suffix: "",
            exporter: std::sync::Arc::new(Unused),
        })
        .collect()
    }
    #[::core::prelude::v1::test]
    fn custom_export_appears_without_a_new_ui_action_type() {
        let mut exports = test_exports();
        exports[0].id = "custom-pdf";
        exports[0].label = "custom.label";
        exports[0].fallback = "Custom PDF";
        exports.truncate(1);
        let menus = application_menus(
            Language::En,
            Preference(None),
            MathFont::default(),
            &[],
            &exports,
        );
        let MenuItem::Action { name, action, .. } = menus[1].items.last().unwrap() else {
            panic!("missing export")
        };
        assert_eq!(name.as_ref(), "Custom PDF");
        assert_eq!(
            action
                .as_any()
                .downcast_ref::<ExportRegistered>()
                .unwrap()
                .id,
            "custom-pdf"
        );
    }
    #[::core::prelude::v1::test]
    fn math_font_submenu_localizes_routes_and_marks_exactly_one_choice() {
        for language in Language::ALL {
            for selected in MathFont::ALL {
                let menus =
                    application_menus(language, Preference(None), selected, &[], &test_exports());
                let MenuItem::Submenu(menu) = &menus[0].items[2] else {
                    panic!("math font submenu")
                };
                assert_eq!(menu.name.as_ref(), language.text(Text::MathFont));
                assert_eq!(menu.items.len(), 2);
                for (item, font) in menu.items.iter().zip(MathFont::ALL) {
                    let MenuItem::Action { name, action, .. } = item else {
                        panic!("font action")
                    };
                    assert_eq!(
                        action
                            .as_any()
                            .downcast_ref::<SelectMathFont>()
                            .unwrap()
                            .font,
                        font
                    );
                    assert_eq!(name.starts_with("✓ "), font == selected);
                    assert!(name.ends_with(font.name()));
                }
            }
        }
    }
    #[::core::prelude::v1::test]
    fn language_submenu_routes_every_preference_and_marks_current_choice() {
        let preferences = std::iter::once(Preference(None))
            .chain(
                Language::ALL
                    .into_iter()
                    .map(|language| Preference(Some(language))),
            )
            .collect::<Vec<_>>();
        for language in Language::ALL {
            for preference in &preferences {
                let menus = application_menus(
                    language,
                    *preference,
                    MathFont::default(),
                    &[],
                    &test_exports(),
                );
                let MenuItem::Submenu(submenu) = &menus[0].items[1] else {
                    panic!("language submenu must be under Ryne");
                };
                assert_eq!(submenu.name.as_ref(), language.text(Text::Language));
                assert_eq!(submenu.items.len(), preferences.len());
                for (item, choice) in submenu.items.iter().zip(&preferences) {
                    let MenuItem::Action { name, action, .. } = item else {
                        panic!("language choice must dispatch an action");
                    };
                    let action = action
                        .as_any()
                        .downcast_ref::<SelectLanguage>()
                        .expect("language action");
                    assert_eq!(action.preference, *choice);
                    let label = choice
                        .0
                        .map_or(language.text(Text::System), Language::native_name);
                    assert_eq!(
                        name.as_ref(),
                        if choice == preference {
                            format!("✓ {label}")
                        } else {
                            label.to_owned()
                        }
                    );
                }
                assert!(menus[1].items.iter().any(|item| matches!(item,
                    MenuItem::Action { name, action, .. }
                        if name.as_ref() == language.text(Text::Open) && action.as_any().is::<Open>())));
                assert!(menus[1].items.iter().any(|item| matches!(item,
                    MenuItem::Action { name, action, .. }
                        if name.as_ref() == language.text(Text::Save) && action.as_any().is::<Save>())));
            }
        }
    }
    #[::core::prelude::v1::test]
    fn native_menus_follow_language_and_recent_paths() {
        let path = PathBuf::from("/tmp/中文.md");
        for language in Language::ALL {
            let menus = application_menus(
                language,
                Preference(None),
                MathFont::default(),
                std::slice::from_ref(&path),
                &test_exports(),
            );
            assert_eq!(menus[0].name.as_ref(), "Ryne");
            assert_eq!(menus[1].name.as_ref(), language.text(Text::File));
            for (key, v2) in [(Text::ExportPdf, false), (Text::ExportPdfV2, true)] {
                let entry = menus[1].items.iter().find(|item| {
                    matches!(item, MenuItem::Action { name, .. } if name.as_ref() == language.text(key))
                }).expect("localized export action");
                let MenuItem::Action { action, .. } = entry else {
                    unreachable!()
                };
                let action = action.as_any().downcast_ref::<ExportRegistered>().unwrap();
                assert_eq!(action.id, if v2 { "pdf-v2" } else { "pdf" });
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
