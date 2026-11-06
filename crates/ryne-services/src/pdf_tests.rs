//! Run the same export contracts against both independent PDF encoders.
use crate::print_layout::layout_pages;
use ryne_extensions::PdfExporter;
use ryne_projection::Display;

fn exporters() -> [(&'static str, &'static dyn PdfExporter); 2] {
    [
        ("svg2pdf", &crate::pdf::PdfExporter),
        ("krilla", &crate::pdf_v2::KrillaPdfExporter),
    ]
}

fn page_count(bytes: &[u8]) -> usize {
    assert!(bytes.starts_with(b"%PDF-1.7"));
    assert!(bytes.ends_with(b"%%EOF"));
    // Check the actual byte offset, before decoding binary font/image streams.
    let marker = b"\nstartxref\n";
    let start = bytes
        .windows(marker.len())
        .rposition(|w| w == marker)
        .unwrap();
    let offset: usize = std::str::from_utf8(&bytes[start + marker.len()..])
        .unwrap()
        .split_ascii_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(bytes[offset..].starts_with(b"xref\n"));
    String::from_utf8_lossy(bytes)
        .split_ascii_whitespace()
        .collect::<String>()
        .matches("/Type/Page/")
        .count()
}

#[test]
fn both_backends_export_one_empty_a4_page() {
    for (name, exporter) in exporters() {
        let bytes = exporter.export(&Display::default()).unwrap();
        assert_eq!(page_count(&bytes), 1, "{name}");
        let structure = String::from_utf8_lossy(&bytes)
            .split_ascii_whitespace()
            .collect::<String>();
        assert!(structure.contains("/MediaBox[00595.28841.89]"), "{name}");
    }
}

#[test]
fn both_backends_encode_every_page_of_a_long_document() {
    // The PDFKit text check uses the CJK fonts installed on macOS. Keep the
    // portable pagination contract usable on hosts with only Latin fonts.
    let last_line = if cfg!(target_os = "macos") {
        "末页中文 FINAL-TOKEN"
    } else {
        "FINAL-TOKEN"
    };
    let display = Display {
        text: format!("{}{last_line}", "body line\n".repeat(200)),
        ..Default::default()
    };
    let pages = layout_pages(&display).unwrap();
    assert!(pages.len() > 3);
    assert!(pages.last().unwrap().contains("FINAL-TOKEN"));
    for (name, exporter) in exporters() {
        let bytes = exporter.export(&display).unwrap();
        assert_eq!(page_count(&bytes), pages.len(), "{name}");
        #[cfg(target_os = "macos")]
        assert_last_page_text(name, &bytes);
    }
}

/// Read the encoded page tree, font mappings and content streams using the
/// system PDF reader, independently of either exporter or the SVG layout.
#[cfg(target_os = "macos")]
fn assert_last_page_text(name: &str, bytes: &[u8]) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("export.pdf");
    std::fs::write(&file, bytes).unwrap();
    let output = std::process::Command::new("/usr/bin/osascript")
        .args([
            "-l",
            "JavaScript",
            "-e",
            r#"
            ObjC.import('PDFKit');
            function run(argv) {
                const doc = $.PDFDocument.alloc.initWithURL($.NSURL.fileURLWithPath(argv[0]));
                return ObjC.unwrap(doc.pageAtIndex(doc.pageCount - 1).string);
            }
        "#,
        ])
        .arg(file)
        .output()
        .expect("run macOS PDFKit reader");
    assert!(
        output.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    // Readers may insert whitespace at font/run boundaries.
    assert!(
        text.split_whitespace()
            .collect::<String>()
            .contains("末页中文FINAL-TOKEN"),
        "{name}: unexpected last page: {text:?}"
    );
}

#[test]
fn both_backends_reject_invalid_text_and_document_limits() {
    for (text, message) in [
        ("visible\0hidden".into(), "control character"),
        ("a".repeat(4 * 1024 * 1024 + 1), "4 MiB"),
        ("\n".repeat(40_000), "500 pages"),
    ] {
        let display = Display {
            text,
            ..Default::default()
        };
        for (name, exporter) in exporters() {
            let error = exporter.export(&display).unwrap_err();
            assert!(error.contains(message), "{name}: {error}");
        }
    }
}
