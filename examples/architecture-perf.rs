//! Repeatable headless timings; debug builds are for comparison, not latency budgets.
mod perf_support;
use perf_support::{corpus, measure, measure_samples};
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
                Arc::new(ryne_services::math::FormularyRenderer::default()),
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
    let formulas = Arc::new(ryne_services::math::FormularyRenderer::default());
    let services = ryne_session::SessionServices {
        features: Arc::new(ryne_features::MarkdownFeatures::new(formulas, vec![])?),
        files: Arc::new(ryne_services::files::AtomicFileStore),
    };
    if std::env::args().nth(1).as_deref() == Some("caret-cache") {
        for kib in [100usize, 1024] {
            let paragraph = "plain text 中文😀 and more words in a paragraph.\n\n";
            let source = paragraph.repeat((kib * 1024).div_ceil(paragraph.len()));
            let mut document = ryne_session::EditorSession::new(&source, services.clone());
            println!(
                "{{\"fixture_bytes\":{},\"profile\":\"{}\"}}",
                source.len(),
                if cfg!(debug_assertions) {
                    "debug"
                } else {
                    "release"
                }
            );
            // A single operation includes two moves. Large debug fixtures use
            // fewer samples; do not interpret their tails as a latency budget.
            let samples = if kib >= 1024 { 8 } else { 96 };
            measure_samples(
                &format!("caret_uncached_projection_{kib}kib"),
                samples,
                1,
                || {
                    document.select(1, false, false);
                    black_box(document.display());
                    document.select(2, false, false);
                    black_box(document.display());
                },
            );
            measure_samples(
                &format!("caret_cached_projection_{kib}kib"),
                samples,
                1,
                || {
                    document.select(1, false, false);
                    black_box(document.cached_project_batch(&document, None, &[]));
                    document.select(2, false, false);
                    black_box(document.cached_project_batch(&document, None, &[]));
                },
            );
        }
        let document = ryne_session::EditorSession::new(&"- item\n".repeat(800), services);
        measure("list_controls_800_uncached", || {
            black_box(document.render_controls().expect("controls"));
        });
        measure("list_controls_800_cached", || {
            black_box(
                document
                    .cached_render_control_batch(&document)
                    .expect("controls"),
            );
        });
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("list-controls") {
        for count in [100, 200, 400, 800] {
            let document =
                ryne_session::EditorSession::new(&"- item\n".repeat(count), services.clone());
            measure(&format!("list_controls_{count}"), || {
                black_box(document.render_controls().expect("list controls"));
            });
        }
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("scaling") {
        for rows in [100, 200, 400, 800] {
            let source = format!(
                "| a | b | c |\n| --- | --- | --- |\n{}\ntail",
                "| x | y | z |\n".repeat(rows)
            );
            let document = ryne_session::EditorSession::new(&source, services.clone());
            println!(
                "{{\"table_rows\":{rows},\"fixture_bytes\":{}}}",
                source.len()
            );
            measure(&format!("table_projection_{rows}"), || {
                black_box(document.display());
            });
        }
        let source = (0..257)
            .map(|i| format!("$x_{{{i}}}$\n\n"))
            .collect::<String>();
        let document = ryne_session::EditorSession::new(&source, services);
        let first = document.display();
        let second = document.display();
        let reused = first
            .formulas
            .iter()
            .zip(&second.formulas)
            .filter(|(a, b)| Arc::ptr_eq(&a.formula, &b.formula))
            .count();
        println!("{{\"formulas\":257,\"reused_resources\":{reused}}}");
        measure("formula_projection_257_warm", || {
            black_box(document.display());
        });
        return Ok(());
    }
    let source = corpus();
    println!(
        "{{\"fixture_bytes\":{},\"profile\":\"{}\",\"warmup\":8}}",
        source.len(),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
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
