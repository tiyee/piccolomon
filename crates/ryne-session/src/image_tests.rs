use super::*;
use ryne_projection::{Diagram, DiagramOutcome};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct Images(AtomicUsize);
impl DiagramRenderer for Images {
    fn generation(&self) -> u64 {
        0
    }
    fn snapshot(&self) -> Option<Arc<dyn DiagramRenderer>> {
        Some(Arc::new(Self::default()))
    }
    fn render(&self, url: &str, _: CancelControl) -> DiagramOutcome {
        self.0.fetch_add(1, Ordering::Relaxed);
        if url.contains("broken") {
            return Err("failed image".into());
        }
        Ok(Arc::new(Diagram {
            pixels: vec![0, 0, 255, 255].into(),
            pixel_width: 1,
            pixel_height: 1,
            width: 160.,
            height: 90.,
            bytes: 4,
        }))
    }
}
fn document(raw: &str, renderer: Arc<dyn DiagramRenderer>) -> EditorSession {
    let services = SessionServices {
        features: Arc::new(
            ryne_features::MarkdownFeatures::new(
                Arc::new(ryne_services::math::FormularyRenderer::default()),
                vec![],
            )
            .unwrap()
            .with_images(renderer)
            .unwrap(),
        ),
        files: Arc::new(ryne_services::files::AtomicFileStore),
    };
    EditorSession::new(raw, services)
}

#[test]
fn real_file_and_embedded_svg_previews_survive_editing_and_pdf_export() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("图片 # 100%.svg");
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"120\" height=\"60\"><rect width=\"120\" height=\"60\" fill=\"red\"/><!--{}--></svg>",
        "x".repeat(9000)
    );
    std::fs::write(&path, &svg).unwrap();
    let file = url::Url::from_file_path(path).unwrap();
    let data = format!("data:image/svg+xml;base64,{}", STANDARD.encode(&svg));
    let raw =
        format!("before😀\r\n\r\n> ![本地]({file})\r\n\r\n- ![内嵌]({data})\r\n\r\nafter\r\n");
    let mut doc = document(&raw, Arc::new(ryne_services::images::ImageRenderer));
    let display = render(&mut doc);
    display.validate(&raw).unwrap();
    assert_eq!(display.diagrams.len(), 2, "{:?}", display.image_failures);
    assert_eq!(doc.source.text(), raw);
    assert!(!doc.dirty());
    assert_eq!(doc.history_len(false), 0);
    let anchor = display.diagrams[1].source.anchor;
    doc.select(anchor, false, false);
    assert_eq!(render(&mut doc).diagrams.len(), 1);
    doc.set_source_mode(true);
    assert!(render(&mut doc).diagrams.is_empty());
    doc.set_source_mode(false);
    doc.select(0, false, false);
    let at = raw.find("内嵌").unwrap();
    doc.replace(at..at + "内嵌".len(), "SVG 图片").unwrap();
    doc.select(0, false, false);
    assert_eq!(render(&mut doc).diagrams.len(), 2);
    doc.undo(false);
    assert_eq!(doc.source.text(), raw);
    let exported = doc.export_snapshot().render().unwrap();
    assert_eq!(exported.diagrams.len(), 2);
    let exporter = &ryne_services::pdf::PdfExporter as &dyn ryne_extensions::PdfExporter;
    assert!(exporter.export(&exported).unwrap().starts_with(b"%PDF-"));
    assert_eq!(doc.source.text(), raw);
    assert!(!doc.dirty());
}
fn render(doc: &mut EditorSession) -> Display {
    for _ in 0..33 {
        let (views, request) = doc.diagram_views(false, &[]);
        if let Some(request) = request {
            doc.complete_job(request.run());
        } else {
            return doc.project(doc, None, &views);
        }
    }
    panic!("image jobs did not converge")
}
#[test]
fn image_previews_preserve_source_selection_history_and_container_mapping() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw = "before😀\n\n![**图片** $x$](https://example.org/a.png \"标题\")\n\n> - ![ref][img]\n\n[img]: https://example.org/a.png\n\nafter\n".replace('\n', newline);
        let renderer = Arc::new(Images::default());
        let mut doc = document(&raw, renderer.clone());
        let source = doc.source.id();
        let selection = doc.selection();
        let display = render(&mut doc);
        display.validate(&raw).unwrap();
        assert_eq!(display.diagrams.len(), 2);
        assert!(display.formulas.is_empty());
        assert!(!display.text.contains("图片"));
        assert!(display.text.contains("after"));
        assert_eq!(
            renderer.0.load(Ordering::Relaxed),
            1,
            "duplicate URLs share a resource"
        );
        assert_eq!(doc.selection(), selection);
        assert_eq!(doc.source.id(), source);
        assert!(!doc.dirty());
        assert_eq!(doc.history_len(false), 0);
        let image = display.diagrams[1].clone();
        let indexed = ryne_projection::IndexedDisplay::new(display);
        assert_eq!(
            indexed.source_at(image.display.start, ryne_parser::Affinity::Downstream),
            image.source.range.start
        );
        assert_eq!(
            indexed.source_at(image.display.end, ryne_parser::Affinity::Upstream),
            image.source.range.end
        );
        doc.select(image.source.anchor, false, false);
        let editing = render(&mut doc);
        assert_eq!(editing.diagrams.len(), 1);
        assert!(editing.text.contains("![ref][img]"));
        doc.set_source_mode(true);
        let literal = render(&mut doc);
        assert!(literal.diagrams.is_empty());
        assert!(literal.text.contains("![ref][img]"));
        doc.set_source_mode(false);
        doc.select(0, false, false);
        doc.set_block_source(Some(image.source.scope.clone()));
        assert_eq!(render(&mut doc).diagrams.len(), 1);
        doc.set_block_source(None);
        assert_eq!(render(&mut doc).diagrams.len(), 2);
        assert_eq!(doc.source.text(), raw);
        assert_eq!(doc.history_len(false), 0);

        // URL edits are ordinary source transactions; rendering never enters history.
        let at = raw.find("a.png").unwrap();
        doc.replace(at..at + 5, "b.png").unwrap();
        doc.select(0, false, false);
        assert_eq!(render(&mut doc).diagrams.len(), 2);
        assert_eq!(doc.history_len(false), 1);
        doc.undo(false);
        assert_eq!(doc.source.text(), raw);
        assert!(!doc.dirty());
        let path = std::env::temp_dir().join(format!("ryne-image-test-{}.md", std::process::id()));
        doc.save(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), raw.as_bytes());
        std::fs::remove_file(path).unwrap();
    }
}
#[test]
fn failures_and_unsupported_inline_images_remain_editable_without_retry_loops() {
    let raw = "before\n\n![failed](https://example.org/broken.png)\n\ntext ![inline](https://example.org/a.png)\n\n```\n![literal](https://example.org/a.png)\n```\n\n![incomplete](\n";
    let renderer = Arc::new(Images::default());
    let mut doc = document(raw, renderer.clone());
    for _ in 0..5 {
        let display = render(&mut doc);
        display.validate(raw).unwrap();
        assert!(display.diagrams.is_empty());
        assert_eq!(display.image_failures.len(), 1);
        assert!(!display.text.contains("![failed]"));
        assert!(display.text.contains("https://example.org/broken.png"));
        for marker in ["![inline]", "![literal]", "![incomplete]"] {
            assert!(display.text.contains(marker));
        }
    }
    assert_eq!(renderer.0.load(Ordering::Relaxed), 1);
    assert_eq!(doc.source.text(), raw);
    assert!(!doc.dirty());
    doc.set_source_mode(true);
    assert!(render(&mut doc).image_failures.is_empty());
}
#[test]
fn image_objects_start_on_layout_line_boundaries_including_indented_and_nested_paragraphs() {
    for prefix in ["", "  ", "   ", "> ", "> - ", "- ", "1. "] {
        let raw = format!("before\n\n{prefix}![alt](https://example.org/a.png)\n\nafter");
        let mut doc = document(&raw, Arc::new(Images::default()));
        let display = render(&mut doc);
        display.validate(&raw).unwrap();
        let view = display.diagrams.first().expect(prefix);
        assert!(
            view.display.start == 0 || display.text.as_bytes()[view.display.start - 1] == b'\n',
            "{prefix:?}"
        );
        assert_eq!(doc.source.text(), raw);
        assert!(!doc.dirty());
    }
}
#[test]
fn image_jobs_respect_frozen_selection_stale_results_and_detached_export() {
    let raw = "before\n\n![说明](https://example.org/a.png)\n\nafter";
    let mut doc = document(raw, Arc::new(Images::default()));
    assert!(doc.diagram_views(true, &[]).1.is_none());
    let request = doc.diagram_views(false, &[]).1.unwrap();
    doc.select(raw.find("![").unwrap(), false, false);
    doc.diagram_views(false, &[]); // cancels work when entering the image source
    doc.complete_job(request.run());
    assert!(!doc.jobs.pending());
    assert!(render(&mut doc).diagrams.is_empty());
    doc.select(0, false, false);
    let display = render(&mut doc);
    let range = display.diagrams[0].source.range.clone();
    assert_eq!(
        doc.diagram_views(true, std::slice::from_ref(&range))
            .0
            .len(),
        1
    );
    // Composition before the image shifts only the temporary projection.
    doc.update_composition(None, "中文😀", None).unwrap();
    let (views, request) = doc.diagram_views(true, std::slice::from_ref(&range));
    assert!(request.is_none());
    let (temporary, views) = doc.projection_state(&views);
    let temporary_display = doc.project(&temporary, None, &views);
    temporary_display.validate(temporary.source.text()).unwrap();
    assert_eq!(temporary_display.diagrams.len(), 1);
    assert_eq!(doc.source.text(), raw);
    assert!(doc.cancel_composition());
    let snapshot = doc.export_snapshot();
    doc.replace(0..raw.len(), "replacement").unwrap();
    assert!(render(&mut doc).diagrams.is_empty());
    let exported = snapshot.render().unwrap();
    assert_eq!(exported.diagrams.len(), 1);
    let exporter = &ryne_services::pdf::PdfExporter as &dyn ryne_extensions::PdfExporter;
    assert!(exporter.export(&exported).unwrap().starts_with(b"%PDF-"));
    assert_eq!(doc.source.text(), "replacement");
    assert_eq!(doc.history_len(false), 1);
}

#[test]
fn failed_image_addresses_fold_truncate_and_reopen_without_losing_source() {
    use unicode_segmentation::UnicodeSegmentation;
    for newline in ["\n", "\r\n", "\r"] {
        for count in [255, 256, 257, 1024] {
            let prefix = "https://broken.example/";
            let address = format!(
                "{prefix}{}",
                "中e\u{301}👨‍👩‍👧‍👦"
                    .graphemes(true)
                    .cycle()
                    .take(count - prefix.len())
                    .collect::<String>()
            );
            let raw =
                format!("before😀{newline}{newline}> - ![alt]({address}){newline}{newline}after");
            let renderer = Arc::new(Images::default());
            let mut doc = document(&raw, renderer.clone());
            let display = render(&mut doc);
            display.validate(&raw).unwrap();
            assert_eq!(
                display.quotes.len(),
                1,
                "failed preview must retain its quote panel"
            );
            assert!(
                !display.lists.is_empty(),
                "failed preview must retain its list marker"
            );
            let view = &display.image_failures[0];
            let expected = address.graphemes(true).take(256).collect::<String>()
                + if count > 256 { "…" } else { "" };
            assert_eq!(&display.text[view.display.clone()], expected);
            assert!(!display.text.contains("![alt]"));
            assert_eq!(display.position_at(view.source.anchor), view.display.start);
            assert_eq!(
                display.source_at(view.display.start, ryne_parser::Affinity::Downstream),
                view.source.range.start
            );
            assert_eq!(
                display.source_at(view.display.end, ryne_parser::Affinity::Upstream),
                view.source.range.end
            );
            let scope = view.source.scope.clone();
            let range = view.source.range.clone();
            doc.select(view.source.anchor, false, false);
            // Frozen selection retains the folded failure until the interaction ends.
            let (views, job) = doc.diagram_views(true, std::slice::from_ref(&range));
            assert!(job.is_none());
            assert_eq!(doc.project(&doc, None, &views).image_failures.len(), 1);
            let editing = render(&mut doc);
            assert!(editing.image_failures.is_empty());
            assert!(editing.text.contains(&format!("![alt]({address})")));
            doc.select(0, false, false);
            assert_eq!(render(&mut doc).image_failures.len(), 1);
            assert_eq!(
                renderer.0.load(Ordering::Relaxed),
                usize::from(address.len() <= 8192)
            );
            doc.set_block_source(Some(scope));
            assert!(render(&mut doc).image_failures.is_empty());
            doc.set_block_source(None);
            doc.set_source_mode(true);
            assert!(render(&mut doc).image_failures.is_empty());
            assert_eq!(doc.source.text(), raw);
            assert!(!doc.dirty());
            assert_eq!(doc.history_len(false), 0);
        }
    }
    // Provider-level URL errors must also unfold on focus, before any job runs.
    let raw = "before\n\n![bad](data:invalid)\n\nafter";
    let mut doc = document(raw, Arc::new(Images::default()));
    let view = render(&mut doc).image_failures[0].clone();
    doc.select(view.source.anchor, false, false);
    assert!(render(&mut doc).image_failures.is_empty());
    assert_eq!(doc.source.text(), raw);
}

#[test]
fn failed_reference_image_mapping_stays_atomic_at_equal_byte_length() {
    let raw = "before\n\n![x][r]\n\n[r]: brokené\n\nafter";
    let mut doc = document(raw, Arc::new(Images::default()));
    let display = render(&mut doc);
    let view = &display.image_failures[0];
    assert_eq!(view.source.range.len(), view.display.len());
    display.validate(raw).unwrap();
    for at in view.source.range.clone() {
        assert_eq!(display.position_at(at), view.display.start);
    }
    for mode in 0..4 {
        let mut invalid = display.clone();
        match mode {
            0 => invalid.image_failures[0].source.anchor = raw.len() + 1,
            1 => invalid.image_failures[0].display.end = invalid.image_failures[0].display.start,
            2 => {
                invalid
                    .spans
                    .iter_mut()
                    .find(|s| s.style.image_failure)
                    .unwrap()
                    .style
                    .image_failure = false
            }
            _ => invalid.image_failures.clear(),
        }
        assert!(invalid.validate(raw).is_err());
    }
}
