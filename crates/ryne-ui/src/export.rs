use super::*;
impl Editor {
    pub(super) fn export_pdf(
        &mut self,
        _: &ExportPdf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_pdf_with("pdf", window, cx);
    }
    pub(super) fn export_registered(
        &mut self,
        action: &menus::ExportRegistered,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_pdf_with(&action.id, window, cx);
    }
    fn export_pdf_with(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.operations.exporting
            || self.operations.file_operation_pending
            || self.operations.close_prompt
        {
            return;
        }
        if !self.commit_composition(cx) {
            return;
        }
        if self.document.files.saving {
            self.status = Text::SavingWait.into();
            cx.notify();
            return;
        }
        let Some(format) = self.desktop.exports.get(id).cloned() else {
            self.status = self
                .locale
                .language
                .error(Text::ExportFailed, "Export backend is not registered");
            cx.notify();
            return;
        };
        self.operations.exporting = true;
        let source_path = self.document.files.path.clone();
        let directory = source_path
            .as_ref()
            .and_then(|p| p.parent())
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let name = source_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into());
        let picker = cx.prompt_for_new_path(&directory, Some(&format.file_name(&name)));
        let snapshot = self.document.export_snapshot();
        let exporter = format.exporter.clone();
        let artifacts = self.desktop.artifacts.clone();
        cx.spawn_in(window, async move |this, cx| {
            let path = match picker.await {
                Ok(Ok(Some(path))) => Some(path),
                Ok(Err(error)) => {
                    let _ = this.update_in(cx, |this, _, cx| {
                        this.status = this.locale.language.error(Text::ExportFailed, error);
                        cx.notify();
                    });
                    None
                }
                _ => None,
            };
            if let Some(mut path) = path {
                let chosen_path = path.clone();
                if path
                    .extension()
                    .is_none_or(|ext| !ext.eq_ignore_ascii_case("pdf"))
                {
                    path.set_extension("pdf");
                }
                // The native picker confirms the chosen name. Confirm again if
                // appending .pdf resolves to a different, already existing file.
                if path != chosen_path && path.exists() {
                    let destination = path.to_string_lossy().into_owned();
                    let answer = this
                        .update_in(cx, |this, window, cx| {
                            window.prompt(
                                PromptLevel::Warning,
                                this.locale.language.text(Text::OverwriteFile),
                                Some(&destination),
                                &[
                                    this.locale.language.text(Text::Cancel),
                                    this.locale.language.text(Text::ReplaceFile),
                                ],
                                cx,
                            )
                        })
                        .ok();
                    let confirmed = if let Some(answer) = answer {
                        answer.await.ok() == Some(1)
                    } else {
                        false
                    };
                    if !confirmed {
                        let _ = this.update_in(cx, |this, _, cx| {
                            this.operations.exporting = false;
                            cx.notify();
                        });
                        return;
                    }
                }
                if same_file(source_path.as_deref(), &path) {
                    let _ = this.update_in(cx, |this, _, cx| {
                        this.status = Text::ExportSameFile.into();
                        cx.notify();
                    });
                } else {
                    let _ = this.update_in(cx, |this, _, cx| {
                        this.status = Text::Exporting.into();
                        cx.notify();
                    });
                    let result = cx
                        .background_executor()
                        .spawn(async move {
                            let display = snapshot.render()?;
                            let bytes = exporter.export(&display)?;
                            artifacts.write(&path, &bytes).map_err(|e| e.to_string())
                        })
                        .await;
                    let _ = this.update_in(cx, |this, _, cx| {
                        this.status = match result {
                            Ok(()) => Text::Exported.into(),
                            Err(error) => this.locale.language.error(Text::ExportFailed, error),
                        };
                        cx.notify();
                    });
                }
            }
            let _ = this.update_in(cx, |this, _, cx| {
                this.operations.exporting = false;
                cx.notify();
            });
        })
        .detach();
    }
}
fn same_file(source: Option<&std::path::Path>, destination: &std::path::Path) -> bool {
    source.is_some_and(|source| {
        source == destination
            || std::fs::canonicalize(source)
                .ok()
                .zip(std::fs::canonicalize(destination).ok())
                .is_some_and(|(a, b)| a == b)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn source_alias_protection_is_preserved() {
        let path = std::env::temp_dir().join(format!("ryne-source-alias-{}", std::process::id()));
        std::fs::write(&path, b"source").unwrap();
        let alias = path
            .parent()
            .unwrap()
            .join(".")
            .join(path.file_name().unwrap());
        assert!(same_file(Some(&path), &alias));
        assert!(!same_file(None, &path));
        std::fs::remove_file(path).unwrap();
    }
}
