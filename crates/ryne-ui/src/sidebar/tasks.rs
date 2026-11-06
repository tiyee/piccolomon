use super::*;
use std::panic::{catch_unwind, AssertUnwindSafe};

pub(super) enum Notice {
    Batch(Vec<DirectoryEntry>),
    End(Result<ScanEnd, String>),
}

impl Editor {
    pub(in crate::sidebar) fn pump_directory(&mut self, cx: &mut Context<Self>) {
        if self.sidebar.request.is_some() {
            return;
        }
        let manual = self.sidebar.choice.clone();
        let root = manual.clone().or_else(|| self.sidebar.browser.root.clone());
        let Some(root) = root else { return };
        let (path, limit) = if manual.is_some() {
            (PathBuf::new(), SCAN_LIMIT)
        } else {
            if !self.sidebar.prefs.visible || self.sidebar.pane() == Pane::Outline {
                return;
            }
            let Some(next) = self.sidebar.browser.next(self.sidebar.pane() == Pane::List) else {
                return;
            };
            next
        };
        let generation = self.sidebar.browser.generation;
        let picker = self.sidebar.picker;
        self.sidebar.ticket += 1;
        let id = self.sidebar.ticket;
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = async_channel::bounded(4);
        self.sidebar.request = Some(Request {
            channel: rx.clone(),
            id,
            path: path.clone(),
            cancel: cancel.clone(),
        });
        #[cfg(test)]
        let peak = self.sidebar.queue_peak.clone();
        let service = self.desktop.directories.clone();
        let scan_root = root.clone();
        let scan_path = path.clone();
        let current = self.sidebar.browser.current.clone();

        cx.background_executor()
            .spawn(async move {
                let stopped = || cancel.load(Ordering::Relaxed) || tx.is_closed();
                let result = catch_unwind(AssertUnwindSafe(|| {
                    service.enumerate(
                        ScanRequest {
                            root: &scan_root,
                            relative: &scan_path,
                            current: current.as_deref(),
                            limit,
                        },
                        &stopped,
                        &mut |entries| {
                            entries.chunks(BATCH).all(|chunk| {
                                let sent = !stopped()
                                    && tx.send_blocking(Notice::Batch(chunk.to_vec())).is_ok();
                                #[cfg(test)]
                                peak.fetch_max(tx.len(), Ordering::Relaxed);
                                sent
                            })
                        },
                    )
                }))
                .map_err(|_| "directory service panicked".to_owned())
                .and_then(|r| r.map_err(|e| e.to_string()));
                let _ = tx.send(Notice::End(result)).await;
            })
            .detach();
        cx.spawn(async move |this, cx| {
            let mut staged = Vec::new();
            let mut overflow = false;
            while let Ok(notice) = rx.recv().await {
                let terminal = matches!(notice, Notice::End(_));
                let updated = this.update(cx, |editor, cx| {
                    let state = &mut editor.sidebar;
                    let Some(req) = &state.request else { return };
                    if req.id != id {
                        return;
                    }
                    let live = !req.cancel.load(Ordering::Relaxed)
                        && state.browser.generation == generation
                        && (manual.is_none() || state.picker == picker);
                    if terminal {
                        state.request = None;
                    }
                    if !live {
                        if terminal {
                            cx.notify();
                        }
                        return;
                    }
                    match notice {
                        Notice::Batch(batch) if manual.is_some() => {
                            let remaining = ENTRY_LIMIT.saturating_sub(staged.len());
                            overflow |= batch.len() > remaining;
                            staged.extend(batch.into_iter().take(remaining));
                        }
                        Notice::Batch(batch) => {
                            for entry in batch {
                                if state
                                    .saved_revisions
                                    .get(&path.join(&entry.name))
                                    .is_some_and(|rev| rev.version > id)
                                {
                                    continue;
                                }
                                state.browser.insert(&path, entry);
                            }
                        }
                        Notice::End(result) if manual.is_some() => {
                            state.choice = None;
                            match result {
                                Ok(end) => {
                                    let same = state.browser.root.as_ref() == Some(&root);
                                    if same {
                                        state.refresh_anchors = state.scroll_anchors();
                                    }
                                    state.browser.root(Some(root.clone()), true, same);
                                    state.reset_root(same, cx);
                                    for e in staged.drain(..) {
                                        state.browser.insert(Path::new(""), e);
                                    }
                                    state.browser.partial |= overflow;
                                    state.browser.finish(Path::new(""), Ok(end));
                                    state.prefs.directory = Some(true);
                                    state.prefs.visible = true;
                                    editor.persist_sidebar();
                                }
                                Err(error) => {
                                    editor.status =
                                        editor.locale.language.error(Text::DirectoryFailed, &error);
                                    editor.sidebar.message = Some(error);
                                }
                            }
                        }
                        Notice::End(result) => state.browser.finish(&path, result),
                    }
                    cx.notify();
                });
                if updated.is_err() || terminal {
                    break;
                }
            }
        })
        .detach();
    }
    pub(super) fn refresh_saved_metadata(
        &mut self,
        absolute: PathBuf,
        relative: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let state = &mut self.sidebar;
        state.ticket += 1;
        let version = state.ticket;
        let generation = state.browser.generation;
        state.saved_revisions.insert(
            relative.clone(),
            SavedRevision {
                version,
                reading_metadata: true,
            },
        );
        state.summary_cache.retain(|(k, _)| k.path != relative);
        let service = self.desktop.directories.clone();
        let task = cx.background_executor().spawn(async move {
            catch_unwind(AssertUnwindSafe(|| service.metadata(&absolute)))
                .ok()
                .and_then(Result::ok)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |e, cx| {
                if e.sidebar.browser.generation == generation
                    && let Some(rev) = e.sidebar.saved_revisions.get_mut(&relative)
                    && rev.version == version
                {
                    rev.reading_metadata = false;
                    if let Some(entry) = result
                        && let Some(parent) = relative.parent()
                    {
                        e.sidebar.browser.insert(parent, entry);
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }
    pub(crate) fn open_folder(&mut self, _: &OpenFolder, _: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar_protected() {
            return;
        }
        if self.sidebar.choice.is_some() {
            self.sidebar.cancel_request();
        }
        self.sidebar.invalidate_choice();
        let id = self.sidebar.picker;
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(self.locale.language.text(Text::OpenFolder).into()),
        });
        cx.spawn(async move |this, cx| {
            let result = picker.await;
            let _ = this.update(cx, |e, cx| {
                if id != e.sidebar.picker {
                    return;
                }
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(root) = paths
                            .into_iter()
                            .next()
                            .and_then(|p| std::path::absolute(p).ok())
                        {
                            e.sidebar.cancel_request();
                            e.sidebar.choice = Some(root);
                            e.pump_directory(cx);
                        }
                    }
                    Ok(Err(error)) => {
                        e.status = e.locale.language.error(Text::DirectoryFailed, error)
                    }
                    _ => (),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(in crate::sidebar) fn summary_for(
        &mut self,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> String {
        let state = &mut self.sidebar;
        if state.browser.relative_current() == Some(path) && self.document.dirty() {
            let id = self.document.source.id();
            if state
                .draft_preview
                .as_ref()
                .is_none_or(|(key, _)| *key != id)
            {
                state.draft_preview =
                    Some((id, browser::summary(self.document.source.text().as_bytes())));
            }
            return state
                .draft_preview
                .as_ref()
                .map(|(_, s)| s.clone())
                .unwrap_or_default();
        }
        let Some(entry) = state.browser.entries.get(path) else {
            return String::new();
        };
        let key = SummaryKey {
            generation: state.browser.generation,
            path: path.to_owned(),
            bytes: entry.bytes,
            modified: entry.modified,
            revision: state.saved_revisions.get(path).map_or(0, |r| r.version),
        };
        if let Some(index) = state.summary_cache.iter().position(|(k, _)| k == &key)
            && let Some(item) = state.summary_cache.remove(index)
        {
            let text = item.1.clone();
            state.summary_cache.push_back(item);
            return text;
        }
        if state.summaries.len() >= 2 || state.summaries.contains(&key) {
            return String::new();
        }
        let Some(root) = state.browser.root.as_ref() else {
            return String::new();
        };
        let absolute = root.join(path);
        let cancelled = state.summary_cancel.clone();
        let service = self.desktop.directories.clone();
        let task = cx.background_executor().spawn(async move {
            if cancelled.load(Ordering::Relaxed) {
                return String::new();
            }
            catch_unwind(AssertUnwindSafe(|| service.prefix(&absolute)))
                .ok()
                .and_then(Result::ok)
                .map_or_else(String::new, |bytes| browser::summary(&bytes))
        });
        self.track_summary(key, task, cx);
        String::new()
    }

    pub(super) fn track_summary(
        &mut self,
        key: SummaryKey,
        read: impl Future<Output = String> + 'static,
        cx: &mut Context<Self>,
    ) {
        self.sidebar.summaries.push(key.clone());
        cx.spawn(async move |this, cx| {
            let text = read.await;
            let _ = this.update(cx, |editor, cx| {
                let state = &mut editor.sidebar;
                state.summaries.retain(|k| k != &key);
                if state.browser.generation == key.generation
                    && state
                        .saved_revisions
                        .get(&key.path)
                        .map_or(0, |r| r.version)
                        == key.revision
                    && state
                        .browser
                        .entries
                        .get(&key.path)
                        .is_some_and(|e| e.modified == key.modified && e.bytes == key.bytes)
                {
                    state.summary_cache.push_back((key, text));
                    let mut bytes = state
                        .summary_cache
                        .iter()
                        .map(|(_, s)| s.len())
                        .sum::<usize>();
                    while state.summary_cache.len() > 256 || bytes > 1024 * 1024 {
                        if let Some((_, old)) = state.summary_cache.pop_front() {
                            bytes -= old.len()
                        } else {
                            break;
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
