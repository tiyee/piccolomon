use super::*;
#[derive(Clone, Copy)]
enum PdfVersion {
    V1,
    V2,
}
impl PdfVersion {
    fn file_name(self, stem: &str) -> String {
        match self {
            Self::V1 => format!("{stem}.pdf"),
            Self::V2 => format!("{stem}-v2.pdf"),
        }
    }
}
impl Editor {
    pub(super) fn export_pdf(
        &mut self,
        _: &ExportPdf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_pdf_with(PdfVersion::V1, window, cx);
    }
    pub(super) fn export_pdf_v2(
        &mut self,
        _: &ExportPdfV2,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_pdf_with(PdfVersion::V2, window, cx);
    }
    fn export_pdf_with(
        &mut self,
        version: PdfVersion,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.exporting || self.file_operation_pending || self.close_prompt {
            return;
        }
        self.commit_composition(cx);
        if self.document.files.saving {
            self.status = Text::SavingWait.into();
            cx.notify();
            return;
        }
        self.exporting = true;
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
        let picker = cx.prompt_for_new_path(&directory, Some(&version.file_name(&name)));
        let snapshot = self.document.export_snapshot();
        let exporter = match version {
            PdfVersion::V1 => self.desktop.pdf.clone(),
            PdfVersion::V2 => self.desktop.pdf_v2.clone(),
        };
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
                            this.exporting = false;
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
                            write_pdf(&path, &bytes).map_err(|e| e.to_string())
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
                this.exporting = false;
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
fn write_pdf(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let parent = path.parent().unwrap_or(std::path::Path::new("."));
    let temporary = parent.join(format!(
        ".ryne-pdf-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| std::fs::rename(&temporary, path));
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn atomic_pdf_write_replaces_output_and_rejects_source_aliases() {
        let path = std::env::temp_dir().join(format!("ryne-pdf-write-{}.pdf", std::process::id()));
        write_pdf(&path, b"first").unwrap();
        write_pdf(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert!(same_file(Some(&path), &path));
        assert!(!same_file(None, &path));
        std::fs::remove_file(path).unwrap();
    }
}
