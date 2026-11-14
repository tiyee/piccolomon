//! Export a Markdown file through the same immutable snapshot and PDF service as the menu.
use std::sync::Arc;
fn document(text: &str) -> Result<ryne_session::EditorSession, String> {
    Ok(ryne_session::EditorSession::new(
        text,
        ryne_session::SessionServices {
            features: Arc::new(ryne_features::MarkdownFeatures::new(
                Arc::new(ryne_services::math::FormularyRenderer::default()),
                vec![ryne_features::diagrams::FencedPreview {
                    id: "mermaid",
                    language: "mermaid",
                    renderer: Arc::new(ryne_services::mermaid::MermanRenderer),
                }],
            )?),
            files: Arc::new(ryne_services::files::AtomicFileStore),
        },
    ))
}
fn desktop_services() -> Result<ryne_ui::DesktopServices, String> {
    Ok(ryne_ui::DesktopServices {
        directories: Arc::new(ryne_services::directory::LocalDirectory),
        exports: ryne_ui::ExportRegistry::new(vec![ryne_ui::ExportDescriptor {
            id: "pdf",
            label: "export.pdf",
            fallback: "Export PDF",
            file_suffix: "",
            exporter: Arc::new(ryne_services::pdf::PdfExporter),
        }])?,
        artifacts: Arc::new(ryne_services::files::AtomicArtifactStore),
        updates: Arc::new(ryne_services::updates::GithubUpdates),
    })
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    const USAGE: &str = "Usage: export-pdf INPUT.md OUTPUT.pdf";
    let mut args = std::env::args_os().skip(1);
    let input = args.next().ok_or(USAGE)?;
    let output = args.next().ok_or(USAGE)?;
    if args.next().is_some() {
        return Err(USAGE.into());
    }
    let source = std::fs::read_to_string(input)?;
    let display = document(&source)?.export_snapshot().render()?;
    let services = desktop_services()?;
    let format = services
        .exports
        .get("pdf")
        .ok_or("Export backend missing")?;
    services.artifacts.write(
        std::path::Path::new(&output),
        &format.exporter.export(&display)?,
    )?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folded_export_keeps_editor_state_and_literal_fallbacks() {
        let source = "# 标题 Title\r\n\r\n**bold** `literal *code*` $x^2$\r\n\r\n- parent\r\n  - child\r\n\r\n| A | B |\r\n|:---|---:|\r\n| 中文 | abc |\r\n\r\n```rust\r\nlet literal = \"**raw**\";\r\n```\r\n\r\n```mermaid\r\nflowchart LR\r\n A --> B\r\n```\r\n\r\n```mermaid\r\nnot a diagram\r\n```\r\n";
        for newline in ["\n", "\r\n", "\r"] {
            let source = source.replace("\r\n", newline);
            let mut session = document(&source).unwrap();
            session.set_source_mode(newline == "\r\n");
            session.select(8, false, false);
            let snapshot = session.source.id();
            let selection = session.selection();
            let display = session.export_snapshot().render().unwrap();
            assert_eq!(session.source.id(), snapshot);
            assert_eq!(session.source.text(), source);
            assert_eq!(session.selection(), selection);
            assert_eq!(session.history_len(false), 0);
            assert_eq!(session.history_len(true), 0);
            assert_eq!(session.source_mode, newline == "\r\n");
            assert!(!session.dirty());
            assert!(session.files.path.is_none());
            assert!(display.text.starts_with("标题 Title"));
            assert!(display.text.contains("literal *code*"));
            assert!(display.text.contains("**raw**"));
            assert!(display.text.contains("not a diagram"));
            assert_eq!(display.tables.len(), 1);
            assert_eq!(display.lists.len(), 2);
            assert_eq!(display.formulas.len(), 1);
            assert_eq!(display.diagrams.len(), 1);
            let services = desktop_services().unwrap();
            assert_eq!(services.exports.formats().len(), 1);
            assert_eq!(
                services.exports.get("pdf").unwrap().file_name("document"),
                "document.pdf"
            );
            let bytes = services
                .exports
                .get("pdf")
                .unwrap()
                .exporter
                .export(&display)
                .unwrap();
            assert!(bytes.starts_with(b"%PDF-1.7"));
            assert!(bytes.ends_with(b"%%EOF"));
            assert_eq!(session.source.id(), snapshot);
            assert_eq!(session.source.text(), source);
            assert_eq!(session.selection(), selection);
            assert_eq!(session.history_len(false), 0);
            assert_eq!(session.history_len(true), 0);
            assert_eq!(session.source_mode, newline == "\r\n");
            assert!(!session.dirty());
            assert!(session.files.path.is_none());
        }
    }
    #[test]
    fn export_paginates_long_paragraphs_code_and_table_cells() {
        let source = format!(
            "# Long document\n\n{}\n\n```text\n{}\n```\n\n| Heading | Number |\n|---|---:|\n| {} | 123 |\n\nFINAL-TOKEN\n",
            "正文 text with spaces. ".repeat(800),
            "literal code **raw** and long lines ".repeat(120),
            "cell content ".repeat(800)
        );
        let session = document(&source).unwrap();
        let display = session.export_snapshot().render().unwrap();
        let services = desktop_services().unwrap();
        let bytes = services
            .exports
            .get("pdf")
            .unwrap()
            .exporter
            .export(&display)
            .unwrap();
        let structure = String::from_utf8_lossy(&bytes)
            .split_ascii_whitespace()
            .collect::<String>();
        assert!(structure.matches("/Type/Page/").count() > 3);
        assert!(display.text.contains("FINAL-TOKEN"));
        assert_eq!(session.source.text(), source);
    }
}
