#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

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
            )
            .and_then(|features| {
                features.with_images(
                    ryne_ui::smoke_image_renderer()
                        .unwrap_or_else(|| Arc::new(ryne_services::images::ImageRenderer)),
                )
            })
            .and_then(|mut features| {
                for fixture in ryne_ui::smoke_features() {
                    features = features.with_feature(fixture)?;
                }
                Ok(features)
            }) {
                Ok(features) => features,
                Err(error) => {
                    eprintln!("初始化失败：{error}");
                    std::process::exit(1)
                }
            },
        ),
        files: Arc::new(ryne_services::files::AtomicFileStore),
    };
    let exports = match ryne_ui::ExportRegistry::new(vec![ryne_ui::ExportDescriptor {
        id: "pdf",
        label: "export.pdf",
        fallback: "Export PDF",
        file_suffix: "",
        exporter: Arc::new(ryne_services::pdf::PdfExporter),
    }]) {
        Ok(exports) => exports,
        Err(error) => {
            eprintln!("初始化失败：{error}");
            std::process::exit(1);
        }
    };
    ryne_ui::run(
        services,
        ryne_ui::DesktopServices {
            directories: Arc::new(ryne_services::directory::LocalDirectory),
            exports,
            artifacts: Arc::new(ryne_services::files::AtomicArtifactStore),
            updates: Arc::new(ryne_services::updates::GithubUpdates),
        },
        formulas,
        Box::new(ryne_services::highlight::CodeHighlighter::default()),
    );
}
