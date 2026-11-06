//! Recovery of committed source for a document that has not yet been saved.
mod store;
#[cfg(test)]
mod tests;

use super::*;
use std::{io, sync::Arc, time::Duration};
use store::{Draft, Slot};

#[derive(Default)]
pub(super) struct Recovery {
    pub directory: Option<PathBuf>,
    active: Option<Arc<Draft>>,
    written: Option<SnapshotId>,
    task: Option<Task<()>>,
}

impl Editor {
    pub(super) fn is_draft_path(&self, path: &std::path::Path) -> bool {
        self.recovery
            .directory
            .as_ref()
            .is_some_and(|directory| store::reserved_path(directory, path))
    }

    pub(super) fn clear_draft(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(active) = &self.recovery.active
            && let Err(e) = active.retire()
        {
            self.draft_error(e, cx);
            return false;
        }
        self.recovery.task = None;
        self.recovery.written = None;
        self.recovery.active = None;
        true
    }

    fn draft_error(&mut self, e: io::Error, cx: &mut Context<Self>) {
        self.status = self.locale.language.error(Text::DraftError, e);
        cx.notify();
    }

    pub(super) fn track_draft(&mut self, cx: &mut Context<Self>) {
        let Some(directory) = &self.recovery.directory else {
            return;
        };
        if !self.document.dirty() {
            if self.recovery.active.is_some() {
                self.clear_draft(cx);
            }
            return;
        }
        // If Save As wrote an older revision while typing continued, keep this
        // draft until the current text has actually been saved or discarded.
        if self.document.files.path.is_some() && self.recovery.active.is_none() {
            return;
        }
        if self.recovery.task.is_some() || self.recovery.written == Some(self.document.source.id())
        {
            return;
        }
        let active = self
            .recovery
            .active
            .get_or_insert_with(|| Arc::new(Draft::new(None)))
            .clone();
        let directory = directory.clone();
        // Coalesce at most one timer/write at a time. Continuous edits do not
        // push the deadline out. Read the latest committed snapshot at expiry.
        self.recovery.task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let Ok(Some(source)) = this.update(cx, |editor, _| {
                editor
                    .recovery
                    .active
                    .as_ref()
                    .filter(|a| Arc::ptr_eq(a, &active))
                    .map(|_| editor.document.source.clone())
            }) else {
                return;
            };
            let snapshot = source.id();
            let writer = active.clone();
            let result = cx
                .background_executor()
                .spawn(async move { writer.write(&directory, source.text()) })
                .await;
            let _ = this.update(cx, |editor, cx| {
                if !editor
                    .recovery
                    .active
                    .as_ref()
                    .is_some_and(|a| Arc::ptr_eq(a, &active))
                {
                    return;
                }
                editor.recovery.task = None;
                match result {
                    Ok(()) => {
                        editor.recovery.written = Some(snapshot);
                        editor.track_draft(cx);
                    }
                    Err(e) => editor.draft_error(e, cx),
                }
            });
        }));
    }

    fn can_recover(&self, snapshot: SnapshotId) -> bool {
        self.can_replace(snapshot)
            && self.default_document.is_some()
            && !self.document.dirty()
            && self.document.files.path.is_none()
            && !self.operations.blocks_document_replacement()
    }

    pub(super) fn offer_drafts(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = self.recovery.directory.clone() else {
            return;
        };
        let snapshot = self.document.source.id();
        let services = self.document.services();
        cx.spawn_in(window, async move |this, cx| {
            let directory = dir.clone();
            let ids = cx
                .background_executor()
                .spawn(async move { store::candidates(&directory) })
                .await;
            let ids = match ids {
                Ok(ids) => ids,
                Err(e) => {
                    let _ = this.update_in(cx, |editor, _, cx| editor.draft_error(e, cx));
                    return;
                }
            };
            for id in ids {
                let directory = dir.clone();
                let candidate = cx
                    .background_executor()
                    .spawn(async move { Slot::read(&directory, id) })
                    .await;
                let (slot, text) = match candidate {
                    Ok(Some(candidate)) => candidate,
                    Ok(None) => continue,
                    Err(e) => {
                        let _ = this.update_in(cx, |editor, _, cx| editor.draft_error(e, cx));
                        continue;
                    }
                };
                let prompt = this.update_in(cx, |editor, window, cx| {
                    if !editor.can_recover(snapshot) {
                        return None;
                    }
                    let language = editor.locale.language;
                    let preview = text.chars().take(120).collect::<String>();
                    Some(window.prompt(
                        PromptLevel::Info,
                        language.text(Text::RecoverDraft),
                        Some(&preview),
                        &[
                            PromptButton::cancel(language.text(Text::Later)),
                            PromptButton::new(language.text(Text::DiscardDraft)),
                            PromptButton::ok(language.text(Text::Recover)),
                        ],
                        cx,
                    ))
                });
                let Ok(Some(prompt)) = prompt else { return };
                let answer = prompt.await.ok();
                if !this
                    .update_in(cx, |editor, _, _| editor.can_recover(snapshot))
                    .unwrap_or(false)
                {
                    return;
                }
                match answer {
                    Some(2) => {
                        let document = cx
                            .background_executor()
                            .spawn(async move { Document::recovered(&text, services) })
                            .await;
                        let _ = this.update_in(cx, |editor, window, cx| {
                            if editor.can_recover(snapshot) {
                                editor.recovery.active = Some(Arc::new(Draft::new(Some(slot))));
                                editor.recovery.written = Some(document.source.id());
                                editor.install_document(document, Text::Recovered, window, cx);
                            }
                        });
                        return;
                    }
                    Some(1) => {
                        if let Err(e) = slot.discard() {
                            let _ = this.update_in(cx, |editor, _, cx| editor.draft_error(e, cx));
                        }
                    }
                    _ => (),
                }
            }
        })
        .detach();
    }
}
