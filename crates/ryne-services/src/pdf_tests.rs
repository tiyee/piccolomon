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
    let display = Display {
        text: format!("{}FINAL-TOKEN", "body line\n".repeat(200)),
        ..Default::default()
    };
    let pages = layout_pages(&display).unwrap();
    assert!(pages.len() > 3);
    assert!(pages.last().unwrap().contains("FINAL-TOKEN"));
    for (name, exporter) in exporters() {
        let bytes = exporter.export(&display).unwrap();
        assert_eq!(page_count(&bytes), pages.len(), "{name}");
    }
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
