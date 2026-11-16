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
    pub(super) fn open_external_path(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.activate_window();
        self.replace_document(Replacement::Path(path), window, cx);
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
            && self.document.composition().is_none()
            && !self.document.files.saving
            && !self.operations.close_prompt
    }
    pub(super) fn install_document(
        &mut self,
        document: Document,
        status: Text,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.document = document;
        self.sidebar_installed(window, cx);
        self.menus.top_menu = None;
        self.menus.top_submenu = None;
        self.default_document = None;
        self.viewport.scroll = px(0.);
        self.viewport.geometry = None;
        self.viewport.dragging = false;
        self.caret_state.preferred_x = None;
        self.caret_state.caret_affinity = Affinity::Downstream;
        self.viewport.ensure_caret = true;
        self.operations.allow_close = false;
        self.operations.close_after_save = false;
        self.menus.hovered_block = None;
        self.menus.hovered_render = None;
        self.status = status.into();
        if let Some(path) = self.document.files.path.clone() {
            self.remember_file(&path, cx);
        }
        window.focus(&self.focus, cx);
        self.refresh(cx);
    }
    fn replace_document(
        &mut self,
        action: Replacement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.commit_composition(cx) {
            return;
        }
        if self.document.files.saving {
            self.status = Text::SavingWait.into();
            cx.notify();
            return;
        }
        if self.operations.blocks_document_replacement() {
            return;
        }
        let Some(operation) = self.operations.file_operation_pending.begin() else {
            return;
        };
        let snapshot = self.document.source.id();
        let services = self.document.services();
        cx.spawn_in(window, async move |editor, cx| {
            run_replacement(&editor, cx, operation, snapshot, services, action).await;
            let _ = editor.update_in(cx, |e, _, cx| {
                if e.operations.file_operation_pending.finish(operation) {
                    e.sidebar.opening = None;
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn replacement_ready(&mut self, snapshot: SnapshotId) -> bool {
        let ready = self.can_replace(snapshot);
        if !ready {
            self.status = Text::ChangedWhileOpening.into();
        }
        ready
    }

    fn accept_open(
        &mut self,
        receipt: background::Receipt<Document>,
        snapshot: SnapshotId,
        reload: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(opened) = self.operations.file_operation_pending.complete(receipt) else {
            return;
        };
        self.sidebar.opening = None;
        if !self.replacement_ready(snapshot) {
            cx.notify();
            return;
        }
        match opened {
            Err(error) => self.status = self.locale.language.error(Text::OpenFailed, error),
            Ok(document)
                if document
                    .files
                    .path
                    .as_deref()
                    .is_some_and(|p| self.is_draft_path(p)) =>
            {
                self.status = Text::DraftLocation.into()
            }
            Ok(document) if self.clear_draft(cx) => {
                let status = if reload {
                    Text::Reloaded
                } else if document.files.path.is_none() {
                    Text::NewCreated
                } else {
                    Text::Opened
                };
                self.install_document(document, status, window, cx);
            }
            Ok(_) => (),
        }
        cx.notify();
    }
    pub(super) fn save_to(
        &mut self,
        path: PathBuf,
        close_after: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_to_with_intent(
            path,
            close_after,
            ryne_session::SaveIntent::Save,
            window,
            cx,
        )
    }
    fn save_to_with_intent(
        &mut self,
        path: PathBuf,
        close_after: bool,
        intent: ryne_session::SaveIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_draft_path(&path) {
            self.status = Text::DraftLocation.into();
            cx.notify();
            return;
        }
        self.operations.close_after_save |= close_after;
        self.status = Text::Saving.into();
        if let Some(request) = self.document.queue_save_with_intent(path, intent) {
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
                editor.finish_save_receipt(receipt, window, cx);
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn finish_save_receipt(
        &mut self,
        receipt: ryne_session::SaveReceipt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let completion = self.document.complete_save(receipt);
        if !completion.accepted {
            return false;
        }
        self.status = completion.error.as_ref().map_or_else(
            || {
                if completion.clean {
                    Text::Saved.into()
                } else {
                    Text::SavedEarlier.into()
                }
            },
            |e| self.locale.language.error(Text::SaveFailed, e),
        );
        if completion.error.is_none()
            && let Some(path) = self.document.files.path.clone()
        {
            self.sidebar_saved(&path, completion.intent, cx);
            self.remember_file(&path, cx);
            self.track_draft(cx);
        }
        let can_continue =
            completion.error.is_none() && completion.clean && completion.next.is_none();
        if let Some(request) = completion.next {
            self.run_save(request, window, cx)
        } else if self.operations.close_after_save {
            self.operations.close_after_save = false;
            if completion.error.is_none()
                && completion.clean
                && self.document.composition().is_none()
                && self.clear_draft(cx)
            {
                self.operations.allow_close = true;
                window.remove_window();
            }
        }
        cx.notify();
        can_continue
    }
    pub(super) fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        if !self.commit_composition(cx) {
            return;
        }
        if let Some(path) = self.document.files.path.clone() {
            self.save_to(path, false, window, cx);
        } else {
            self.save_as(&SaveAs, window, cx);
        }
    }
    pub(super) fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        if !self.commit_composition(cx) {
            return;
        }
        if self.operations.exporting.is_pending() {
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
                        if !this.commit_composition(cx) {
                            return;
                        }
                        this.save_to_with_intent(
                            path,
                            false,
                            ryne_session::SaveIntent::SaveAs,
                            window,
                            cx,
                        );
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
        if !self.commit_composition(cx) {
            return false;
        }
        if self.operations.exporting.is_pending()
            || self.operations.file_operation_pending.is_pending()
        {
            self.status = Text::OperationWait.into();
            cx.notify();
            return false;
        }
        if self.document.files.saving {
            self.status = Text::SavingWait.into();
            cx.notify();
            return false;
        }
        if self.operations.allow_close || !self.document.dirty() {
            return self.clear_draft(cx);
        }
        if self.operations.close_prompt {
            return false;
        }
        self.operations.close_prompt = true;
        let closing_snapshot = self.document.source.id();
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
                    this.operations.close_prompt = false;
                    cx.notify();
                    this.document.files.path.clone()
                })
                .ok()
                .flatten();
            if answer == Some(1) {
                let _ = this.update_in(cx, |this, window, cx| {
                    if this.can_replace(closing_snapshot) && this.clear_draft(cx) {
                        this.operations.allow_close = true;
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
                        if this.document.source.id().document_id == closing_snapshot.document_id {
                            if !this.commit_composition(cx) {
                                return;
                            }
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

async fn run_replacement(
    editor: &WeakEntity<Editor>,
    cx: &mut AsyncWindowContext,
    operation: background::OperationToken,
    snapshot: SnapshotId,
    services: SessionServices,
    action: Replacement,
) -> Option<()> {
    let action = if matches!(action, Replacement::Picker) {
        let picker = editor
            .update_in(cx, |e, _, cx| {
                cx.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: Some(e.locale.language.text(Text::OpenMarkdown).into()),
                })
            })
            .ok()?;
        match picker.await.ok()? {
            Ok(Some(paths)) => Replacement::Path(paths.into_iter().next()?),
            Ok(None) => return None,
            Err(error) => {
                editor
                    .update_in(cx, |e, _, _| {
                        e.status = e.locale.language.error(Text::OpenFailed, error)
                    })
                    .ok()?;
                return None;
            }
        }
    } else {
        action
    };
    let opening = matches!(action, Replacement::Path(_));
    let prompt = editor
        .update_in(cx, |e, window, cx| {
            if !e.replacement_ready(snapshot) {
                return None;
            }
            let lang = e.locale.language;
            let title = match action {
                Replacement::New => Text::New,
                Replacement::Reload(_) => Text::Reload,
                _ => Text::OpenOther,
            };
            Some(e.document.dirty().then(|| {
                let mut buttons = vec![
                    lang.text(Text::Cancel),
                    lang.text(if opening {
                        Text::DiscardOpen
                    } else {
                        Text::Discard
                    }),
                ];
                if opening {
                    buttons.push(lang.text(Text::SaveOpen));
                }
                window.prompt(
                    PromptLevel::Warning,
                    lang.text(title),
                    Some(lang.text(Text::Unsaved)),
                    &buttons,
                    cx,
                )
            }))
        })
        .ok()??;
    if let Some(prompt) = prompt {
        match prompt.await.ok()? {
            1 => (),
            2 if opening => save_before_open(editor, cx, snapshot).await?,
            _ => return None,
        }
    }
    if !editor
        .update_in(cx, |e, _, _| e.replacement_ready(snapshot))
        .ok()?
    {
        return None;
    }
    let reload = matches!(action, Replacement::Reload(_));
    let path = match action {
        Replacement::New => None,
        Replacement::Path(p) | Replacement::Reload(p) => Some(p),
        Replacement::Picker => return None,
    };
    let receipt = cx
        .background_executor()
        .spawn(async move { operation.open(path, services) })
        .await;
    editor
        .update_in(cx, |e, w, cx| {
            e.accept_open(receipt, snapshot, reload, w, cx)
        })
        .ok()
}

/// The occupied replacement slot owns this save continuation. A queued save,
/// stale edit, failed write, or cancelled picker finishes the original request.
async fn save_before_open(
    editor: &WeakEntity<Editor>,
    cx: &mut AsyncWindowContext,
    snapshot: SnapshotId,
) -> Option<()> {
    let path = editor
        .update_in(cx, |e, _, _| e.document.files.path.clone())
        .ok()?;
    let path = if let Some(path) = path {
        path
    } else {
        let picker = editor
            .update_in(cx, |e, _, cx| {
                cx.prompt_for_new_path(
                    &std::env::current_dir().unwrap_or_default(),
                    Some(e.locale.language.text(Text::Untitled)),
                )
            })
            .ok()?;
        match picker.await.ok()? {
            Ok(path) => path?,
            Err(error) => {
                editor
                    .update_in(cx, |e, _, _| {
                        e.status = e.locale.language.error(Text::SaveFailed, error)
                    })
                    .ok()?;
                return None;
            }
        }
    };
    let request = editor
        .update_in(cx, |e, _, cx| {
            if !e.replacement_ready(snapshot) {
                return None;
            }
            if e.is_draft_path(&path) {
                e.status = Text::DraftLocation.into();
                return None;
            }
            e.status = Text::Saving.into();
            cx.notify();
            e.document.queue_save(path)
        })
        .ok()??;
    let receipt = cx
        .background_executor()
        .spawn(async move { request.run() })
        .await;
    editor
        .update_in(cx, |e, w, cx| {
            e.finish_save_receipt(receipt, w, cx) && e.replacement_ready(snapshot)
        })
        .ok()?
        .then_some(())
}

pub(super) fn file_url_to_path(value: &str) -> Option<PathBuf> {
    let url = url::Url::parse(value).ok()?;
    (url.scheme() == "file").then_some(())?;
    url.to_file_path().ok()
}
