use std::sync::Arc;
fn main() {
    let formulas = Arc::new(ryne_services::math::FormularyRenderer::default());
    let services = ryne_session::SessionServices {
        features: Arc::new(
            match ryne_features::MarkdownFeatures::new(
                formulas.clone(),
                vec![ryne_features::diagrams::FencedPreview {
                    id: "mermaid",
                    language: "mermaid",
                    renderer: Arc::new(ryne_services::mermaid::MermanRenderer),
                }],
            ) {
                Ok(features) => features.with_language_detector(Arc::new(
                    ryne_services::language::TreeSitterLanguageDetector,
                )),
                Err(error) => {
                    eprintln!("初始化失败：{error}");
                    std::process::exit(1)
                }
            },
        ),
        files: Arc::new(ryne_services::files::AtomicFileStore),
    };
    ryne_ui::run(
        services,
        ryne_ui::DesktopServices {
            pdf: Arc::new(ryne_services::pdf::PdfExporter),
            pdf_v2: Arc::new(ryne_services::pdf_v2::KrillaPdfExporter),
            updates: Arc::new(ryne_services::updates::GithubUpdates),
        },
        formulas,
        Box::new(ryne_services::highlight::CodeHighlighter::default()),
    );
}
