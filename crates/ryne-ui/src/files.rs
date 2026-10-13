use super::*;
impl Editor {
    pub(super) fn open(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_composition(cx);
        if self.document.files.saving {
            self.status = Text::SavingWait.into();
            cx.notify();
            return;
        }
        let services = self.document.services();
        let opening_key = self.document.source.id();
        let confirmation = self.document.dirty().then(|| {
            window.prompt(
                PromptLevel::Warning,
                self.locale.language.text(Text::OpenOther),
                Some(self.locale.language.text(Text::Unsaved)),
                &[
                    self.locale.language.text(Text::Cancel),
                    self.locale.language.text(Text::DiscardOpen),
                ],
                cx,
            )
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Some(confirmation) = confirmation
                && confirmation.await.ok() != Some(1)
            {
                return;
            }
            let picker = this.update_in(cx, |this, _, cx| {
                cx.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: Some(this.locale.language.text(Text::OpenMarkdown).into()),
                })
            });
            let Ok(picker) = picker else {
                return;
            };
            match picker.await {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.first() {
                        let path = path.clone();
                        let services = services.clone();
                        let document = cx
                            .background_executor()
                            .spawn(async move { Document::open(&path, services) })
                            .await;
                        let _ = this.update_in(cx, |this, window, cx| {
                            if this.document.source.id() != opening_key
                                || this.document.composition.is_some()
                                || this.document.files.saving
                            {
                                this.status = Text::ChangedWhileOpening.into();
                                cx.notify();
                                return;
                            }
                            match document {
                                Ok(document) => {
                                    this.document = document;
                                    this.scroll = px(0.);
                                    this.preferred_x = None;
                                    this.caret_affinity = Affinity::Downstream;
                                    this.ensure_caret = true;
                                    this.status = Text::Opened.into();
                                }
                                Err(error) => {
                                    this.status =
                                        this.locale.language.error(Text::OpenFailed, error)
                                }
                            }
                            window.focus(&this.focus);
                            this.refresh(cx);
                        });
                    }
                }
                Ok(Err(error)) => {
                    let _ = this.update_in(cx, |this, _, cx| {
                        this.status = this.locale.language.error(Text::OpenFailed, error);
                        cx.notify();
                    });
                }
                _ => (),
            }
        })
        .detach();
    }
    pub(super) fn save_to(
        &mut self,
        path: PathBuf,
        close_after: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_after_save |= close_after;
        self.status = Text::Saving.into();
        if let Some(request) = self.document.queue_save(path) {
            self.run_save(request, window, cx)
        }
        cx.notify();
    }
    pub(super) fn run_save(
        &mut self,
        request: ryne_session::SaveRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task = cx.background_executor().spawn(async move { request.run() });
        cx.spawn_in(window, async move |this, cx| {
            let receipt = task.await;
            let _ = this.update_in(cx, |editor, window, cx| {
                let completion = editor.document.complete_save(receipt);
                if !completion.accepted {
                    return;
                }
                editor.status = completion.error.as_ref().map_or_else(
                    || {
                        if completion.clean {
                            Text::Saved.into()
                        } else {
                            Text::SavedEarlier.into()
                        }
                    },
                    |e| editor.locale.language.error(Text::SaveFailed, e),
                );
                if let Some(request) = completion.next {
                    editor.run_save(request, window, cx)
                } else if editor.close_after_save {
                    editor.close_after_save = false;
                    if completion.error.is_none()
                        && completion.clean
                        && editor.document.composition.is_none()
                    {
                        editor.allow_close = true;
                        window.remove_window();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_composition(cx);
        if let Some(path) = self.document.files.path.clone() {
            self.save_to(path, false, window, cx);
        } else {
            self.save_as(&SaveAs, window, cx);
        }
    }
    pub(super) fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_composition(cx);
        let directory = self
            .document
            .files
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or(PathBuf::from(".")));
        let document_id = self.document.source.id().document_id;
        let picker =
            cx.prompt_for_new_path(&directory, Some(self.locale.language.text(Text::Untitled)));
        cx.spawn_in(window, async move |this, cx| match picker.await {
            Ok(Ok(Some(path))) => {
                let _ = this.update_in(cx, |this, window, cx| {
                    if this.document.source.id().document_id == document_id {
                        this.commit_composition(cx);
                        this.save_to(path, false, window, cx);
                    }
                });
            }
            Ok(Err(error)) => {
                let _ = this.update_in(cx, |this, _, cx| {
                    this.status = this.locale.language.error(Text::SaveFailed, error);
                    cx.notify();
                });
            }
            _ => (),
        })
        .detach();
    }
    pub(super) fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.commit_composition(cx);
        if self.document.files.saving {
            self.status = Text::SavingWait.into();
            cx.notify();
            return false;
        }
        if self.allow_close || !self.document.dirty() {
            return true;
        }
        if self.close_prompt {
            return false;
        }
        self.close_prompt = true;
        let closing_document = self.document.source.id().document_id;
        let answer = window.prompt(
            PromptLevel::Warning,
            self.locale.language.text(Text::SaveChanges),
            None,
            &[
                self.locale.language.text(Text::Cancel),
                self.locale.language.text(Text::Discard),
                self.locale.language.text(Text::Save),
            ],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = answer.await.ok();
            let path = this
                .update_in(cx, |this, _, cx| {
                    this.close_prompt = false;
                    cx.notify();
                    this.document.files.path.clone()
                })
                .ok()
                .flatten();
            if answer == Some(1) {
                let _ = this.update_in(cx, |this, window, _| {
                    if this.document.source.id().document_id == closing_document {
                        this.allow_close = true;
                        window.remove_window();
                    }
                });
            } else if answer == Some(2) {
                let path = if let Some(path) = path {
                    Some(path)
                } else {
                    let picker = this
                        .update_in(cx, |this, _, cx| {
                            cx.prompt_for_new_path(
                                &std::env::current_dir().unwrap_or(PathBuf::from(".")),
                                Some(this.locale.language.text(Text::Untitled)),
                            )
                        })
                        .ok();
                    if let Some(picker) = picker {
                        picker.await.ok().and_then(Result::ok).flatten()
                    } else {
                        None
                    }
                };
                if let Some(path) = path {
                    let _ = this.update_in(cx, |this, window, cx| {
                        if this.document.source.id().document_id == closing_document {
                            this.commit_composition(cx);
                            this.save_to(path, true, window, cx);
                        }
                    });
                }
            }
        })
        .detach();
        false
    }
    pub(super) fn quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        if self.request_close(window, cx) {
            window.remove_window();
        }
    }
}
