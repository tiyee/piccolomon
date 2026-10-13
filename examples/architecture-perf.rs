//! Repeatable headless timings; debug builds are for comparison, not latency budgets.
mod perf_support;
use perf_support::{corpus, measure};
use std::{hint::black_box, sync::Arc};
fn main() -> Result<(), String> {
    if std::env::args().nth(1).as_deref() == Some("services") {
        measure("mermaid_render", || {
            black_box(
                ryne_services::mermaid::render(
                    "flowchart LR\nA[中文] --> B[保存]",
                    ryne_services::mermaid::control(),
                )
                .expect("diagram"),
            );
        });
        let services = ryne_session::SessionServices {
            features: Arc::new(ryne_features::MarkdownFeatures::new(
                Arc::new(ryne_services::math::FormularyRenderer),
                vec![ryne_features::diagrams::FencedPreview {
                    id: "mermaid",
                    language: "mermaid",
                    renderer: Arc::new(ryne_services::mermaid::MermanRenderer),
                }],
            )?),
            files: Arc::new(ryne_services::files::AtomicFileStore),
        };
        let mut document = ryne_session::EditorSession::new(
            "```mermaid\nflowchart LR\nA[中文] --> B[保存]\n```\n\ntail",
            services,
        );
        document.select(document.source.text().len(), false, false);
        let (_, request) = document.diagram_views(false, &[]);
        document.complete_job(request.ok_or("missing preview job")?.run());
        measure("diagram_host_warm_refresh", || {
            black_box(document.diagram_views(false, &[]));
        });
        let (views, _) = document.diagram_views(false, &[]);
        println!(
            "{{\"cached_diagrams\":{},\"cached_pixel_bytes\":{}}}",
            views.len(),
            views
                .iter()
                .filter_map(|(_, v)| v.as_ref().ok())
                .map(|r| r.bytes)
                .sum::<usize>()
        );
        return Ok(());
    }
    let formulas = Arc::new(ryne_services::math::FormularyRenderer);
    let services = ryne_session::SessionServices {
        features: Arc::new(ryne_features::MarkdownFeatures::new(formulas, vec![])?),
        files: Arc::new(ryne_services::files::AtomicFileStore),
    };
    let source = corpus();
    println!(
        "{{\"fixture_bytes\":{},\"profile\":\"debug\",\"warmup\":8}}",
        source.len()
    );
    let mut document = ryne_session::EditorSession::new(&source, services.clone());
    let end = source.len();
    document.select(end, false, false);
    measure("input_publish_and_undo", || {
        document.insert_text(end..end, "中").expect("input");
        black_box(document.source.id());
        document.undo(false);
    });
    measure("parse_and_new_session", || {
        black_box(ryne_session::EditorSession::new(&source, services.clone()));
    });
    measure("projection", || {
        black_box(document.display());
    });
    let mut highlighter = ryne_services::highlight::CodeHighlighter::default();
    measure("projection_warm_highlight", || {
        black_box(document.highlighted_display(&mut highlighter));
    });
    measure("selection_without_content_edit", || {
        document.select(end - 1, false, false);
        document.select(end, false, false);
    });
    Ok(())
}
