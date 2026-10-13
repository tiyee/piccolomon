//! Krilla PDF backend over the shared immutable A4 print layout.
//! SVG is an intermediate drawing scene, not a screenshot: text stays embedded,
//! formula paths stay vector, and Mermaid retains its existing bitmap resource.
use crate::pdf::{FONTS, HEIGHT, WIDTH, layout_pages};
use krilla::{Document, geom::Size, page::PageSettings};
use krilla_svg::{SurfaceExt, SvgSettings};
use ryne_projection::Display;

pub struct KrillaPdfExporter;

impl ryne_extensions::PdfExporter for KrillaPdfExporter {
    fn export(&self, display: &Display) -> Result<Vec<u8>, String> {
        let pages = layout_pages(display)?;
        // This adapter owns usvg 0.47 values; the editor and v1 retain resvg/usvg
        // 0.45. Fontdb 0.23 is shared, so both encoders resolve the same font set.
        let mut options = usvg_pdf::Options {
            fontdb: FONTS.clone(),
            ..Default::default()
        };
        // Generated scenes only contain inline resources. Never resolve paths or
        // URLs from Markdown; formulas and diagrams are already snapshot values.
        options.image_href_resolver.resolve_string = Box::new(|_, _| None);
        let size = Size::from_wh(WIDTH, HEIGHT).ok_or("Invalid PDF page size")?;
        let mut document = Document::new();
        for svg in pages {
            let tree = usvg_pdf::Tree::from_str(&svg, &options).map_err(|e| e.to_string())?;
            let mut page = document.start_page_with(PageSettings::new(size));
            let mut surface = page.surface();
            surface
                .draw_svg(
                    &tree,
                    size,
                    SvgSettings {
                        embed_text: true,
                        ..Default::default()
                    },
                )
                .ok_or("Cannot draw PDF page")?;
            surface.finish();
            page.finish();
        }
        document
            .finish()
            .map_err(|e| format!("Krilla PDF export failed: {e:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ryne_extensions::PdfExporter;

    #[test]
    fn empty_document_exports_one_a4_page() {
        let bytes = KrillaPdfExporter.export(&Display::default()).unwrap();
        let structure = String::from_utf8_lossy(&bytes)
            .split_ascii_whitespace()
            .collect::<String>();
        assert_eq!(structure.matches("/Type/Page/").count(), 1);
        assert!(structure.contains("/MediaBox[00595.28841.89]"));
    }

    #[test]
    fn invalid_content_and_source_budget_fail_before_encoding() {
        for (text, message) in [
            ("visible\0hidden".into(), "control character"),
            ("a".repeat(4 * 1024 * 1024 + 1), "4 MiB"),
        ] {
            let display = Display {
                text,
                ..Default::default()
            };
            assert!(
                KrillaPdfExporter
                    .export(&display)
                    .unwrap_err()
                    .contains(message)
            );
        }
    }
}
