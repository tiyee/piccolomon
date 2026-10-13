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
fn desktop_services() -> ryne_ui::DesktopServices {
    ryne_ui::DesktopServices {
        pdf: Arc::new(ryne_services::pdf::PdfExporter),
        pdf_v2: Arc::new(ryne_services::pdf_v2::KrillaPdfExporter),
        updates: Arc::new(ryne_services::updates::GithubUpdates),
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    const USAGE: &str = "Usage: export-pdf INPUT.md OUTPUT.pdf [--v2]";
    let mut args = std::env::args_os().skip(1);
    let input = args.next().ok_or(USAGE)?;
    let output = args.next().ok_or(USAGE)?;
    let v2 = match args.next() {
        None => false,
        Some(option) if option == "--v2" => true,
        Some(_) => return Err(USAGE.into()),
    };
    if args.next().is_some() {
        return Err(USAGE.into());
    }
    let source = std::fs::read_to_string(input)?;
    let display = document(&source)?.export_snapshot().render()?;
    let services = desktop_services();
    let exporter = if v2 { services.pdf_v2 } else { services.pdf };
    std::fs::write(output, exporter.export(&display)?)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folded_export_keeps_editor_state_and_literal_fallbacks() {
        let source = "# 标题 Title\r\n\r\n**bold** `literal *code*` $x^2$\r\n\r\n- parent\r\n  - child\r\n\r\n| A | B |\r\n|:---|---:|\r\n| 中文 | abc |\r\n\r\n```rust\r\nlet literal = \"**raw**\";\r\n```\r\n\r\n```mermaid\r\nflowchart LR\r\n A --> B\r\n```\r\n\r\n```mermaid\r\nnot a diagram\r\n```\r\n";
        let mut session = document(source).unwrap();
        session.set_source_mode(true);
        session.select(8, false, false);
        let snapshot = session.source.id();
        let selection = session.selection();
        let display = session.export_snapshot().render().unwrap();
        assert_eq!(session.source.id(), snapshot);
        assert_eq!(session.source.text(), source);
        assert_eq!(session.selection(), selection);
        assert_eq!(session.history_len(false), 0);
        assert_eq!(session.history_len(true), 0);
        assert!(session.source_mode);
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
        let services = desktop_services();
        for exporter in [services.pdf, services.pdf_v2] {
            let bytes = exporter.export(&display).unwrap();
            assert!(bytes.starts_with(b"%PDF-1.7"));
            assert!(bytes.ends_with(b"%%EOF"));
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
        let services = desktop_services();
        for exporter in [services.pdf, services.pdf_v2] {
            let bytes = exporter.export(&display).unwrap();
            let structure = String::from_utf8_lossy(&bytes)
                .split_ascii_whitespace()
                .collect::<String>();
            assert!(structure.matches("/Type/Page/").count() > 3);
        }
        assert!(display.text.contains("FINAL-TOKEN"));
        assert_eq!(session.source.text(), source);
    }
}
