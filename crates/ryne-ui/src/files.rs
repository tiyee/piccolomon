use super::*;
#[derive(Clone)]
pub(super) enum Replacement {
    New,
    Picker,
    Path(PathBuf),
    Reload(PathBuf),
}
impl Editor {
    pub(super) fn new_document(&mut self, _: &New, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_document(Replacement::New, window, cx);
    }
    pub(super) fn open(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_document(Replacement::Picker, window, cx);
    }
    pub(super) fn open_recent(
        &mut self,
        action: &OpenRecent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_document(Replacement::Path(action.path.clone()), window, cx);
    }
    pub(super) fn reload(&mut self, _: &Reload, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.document.files.path.clone() {
            self.replace_document(Replacement::Reload(path), window, cx);
        } else {
            self.status = Text::ReloadUnavailable.into();
            cx.notify();
        }
    }
    pub(super) fn can_replace(&self, snapshot: SnapshotId) -> bool {
        self.document.source.id() == snapshot
            && self.document.composition.is_none()
            && !self.document.files.saving
            && !self.close_prompt
    }
    pub(super) fn install_document(
        &mut self,
        document: Document,
        status: Text,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.document = document;
        self.top_menu = None;
        self.top_submenu = None;
        self.default_document = None;
        self.scroll = px(0.);
        self.geometry = None;
        self.dragging = false;
        self.preferred_x = None;
        self.caret_affinity = Affinity::Downstream;
        self.ensure_caret = true;
        self.allow_close = false;
        self.close_after_save = false;
        self.hovered_block = None;
        self.hovered_render = None;
        self.status = status.into();
        if let Some(path) = self.document.files.path.clone() {
            self.remember_file(&path, cx);
        }
        window.focus(&self.focus);
        self.refresh(cx);
    }
    fn replace_document(
        &mut self,
        action: Replacement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_composition(cx);
        if self.document.files.saving {
            self.status = Text::SavingWait.into();
            cx.notify();
            return;
        }
        if self.file_operation_pending || self.close_prompt || self.exporting {
            return;
        }
        self.file_operation_pending = true;
        let snapshot = self.document.source.id();
        let services = self.document.services();
        let title = match action {
            Replacement::New => Text::New,
            Replacement::Reload(_) => Text::Reload,
            _ => Text::OpenOther,
        };
        let confirmation = self.document.dirty().then(|| {
            window.prompt(
                PromptLevel::Warning,
                self.locale.language.text(title),
                Some(self.locale.language.text(Text::Unsaved)),
                &[
                    self.locale.language.text(Text::Cancel),
                    self.locale.language.text(
                        if matches!(action, Replacement::Picker | Replacement::Path(_)) {
                            Text::DiscardOpen
                        } else {
                            Text::Discard
                        },
                    ),
                ],
                cx,
            )
        });
        cx.spawn_in(window, async move |this, cx| {
            let confirmed = if let Some(confirmation) = confirmation {
                confirmation.await.ok() == Some(1)
            } else {
                true
            };
            let mut cancelled = !confirmed;
            if confirmed {
                let ready = this
                    .update_in(cx, |this, _, _| this.can_replace(snapshot))
                    .unwrap_or(false);
                if ready {
                    let reload = matches!(action, Replacement::Reload(_));
                    let path = match action {
                        Replacement::New => None,
                        Replacement::Path(path) | Replacement::Reload(path) => Some(path),
                        Replacement::Picker => {
                            let picker = this
                                .update_in(cx, |this, _, cx| {
                                    cx.prompt_for_paths(PathPromptOptions {
                                        files: true,
                                        directories: false,
                                        multiple: false,
                                        prompt: Some(
                                            this.locale.language.text(Text::OpenMarkdown).into(),
                                        ),
                                    })
                                })
                                .ok();
                            match picker {
                                Some(picker) => match picker.await {
                                    Ok(Ok(Some(paths))) => paths.into_iter().next(),
                                    Ok(Err(error)) => {
                                        let _ = this.update_in(cx, |this, _, cx| {
                                            this.status =
                                                this.locale.language.error(Text::OpenFailed, error);
                                            cx.notify();
                                        });
                                        cancelled = true;
                                        None
                                    }
                                    _ => {
                                        cancelled = true;
                                        None
                                    }
                                },
                                None => {
                                    cancelled = true;
                                    None
                                }
                            }
                        }
                    };
                    if !cancelled {
                        let document = cx
                            .background_executor()
                            .spawn(async move {
                                match path {
                                    Some(path) => Document::open(&path, services),
                                    None => Ok(Document::new("", services)),
                                }
                            })
                            .await;
                        let _ = this.update_in(cx, |this, window, cx| {
                            if !this.can_replace(snapshot) {
                                this.status = Text::ChangedWhileOpening.into();
                                cx.notify();
                                return;
                            }
                            match document {
                                Ok(document) => {
                                    let status = if reload {
                                        Text::Reloaded
                                    } else if document.files.path.is_none() {
                                        Text::NewCreated
                                    } else {
                                        Text::Opened
                                    };
                                    this.install_document(document, status, window, cx);
                                }
                                Err(error) => {
                                    this.status =
                                        this.locale.language.error(Text::OpenFailed, error);
                                    cx.notify();
                                }
                            }
                        });
                    }
                } else {
                    let _ = this.update_in(cx, |this, _, cx| {
                        this.status = Text::ChangedWhileOpening.into();
                        cx.notify();
                    });
                }
            }
            let _ = this.update_in(cx, |this, _, cx| {
                this.file_operation_pending = false;
                cx.notify();
            });
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
                if completion.error.is_none()
                    && let Some(path) = editor.document.files.path.clone()
                {
                    editor.remember_file(&path, cx);
                }
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
        if self.exporting {
            self.status = Text::OperationWait.into();
            cx.notify();
            return;
        }
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
        if self.exporting || self.file_operation_pending {
            self.status = Text::OperationWait.into();
            cx.notify();
            return false;
        }
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
