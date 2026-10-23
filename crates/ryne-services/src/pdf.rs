//! svg2pdf adapter over the shared immutable print layout.
use crate::print_layout::{layout_pages, FONTS, HEIGHT, WIDTH};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref};
use resvg::usvg;
use ryne_projection::Display;
use std::collections::HashMap;

pub struct PdfExporter;
impl ryne_extensions::PdfExporter for PdfExporter {
    fn export(&self, display: &Display) -> Result<Vec<u8>, String> {
        encode(&layout_pages(display)?)
    }
}
fn encode(pages: &[String]) -> Result<Vec<u8>, String> {
    let options = usvg::Options {
        fontdb: FONTS.clone(),
        ..Default::default()
    };
    let mut pdf = Pdf::new();
    let mut alloc = Ref::new(1);
    let catalog = alloc.bump();
    let page_tree = alloc.bump();
    pdf.catalog(catalog).pages(page_tree);
    let mut ids = Vec::new();
    for svg in pages {
        let tree = usvg::Tree::from_str(svg, &options).map_err(|e| e.to_string())?;
        let (chunk, root) = svg2pdf::to_chunk(&tree, svg2pdf::ConversionOptions::default())
            .map_err(|e| e.to_string())?;
        let mut refs = HashMap::new();
        let chunk = chunk.renumber(|old| *refs.entry(old).or_insert_with(|| alloc.bump()));
        let root = *refs.get(&root).ok_or("Missing PDF graphics reference")?;
        let page_id = alloc.bump();
        let content_id = alloc.bump();
        ids.push(page_id);
        let mut page = pdf.page(page_id);
        page.media_box(Rect::new(0., 0., WIDTH, HEIGHT))
            .parent(page_tree)
            .contents(content_id);
        page.resources().x_objects().pair(Name(b"Page"), root);
        page.finish();
        let mut content = Content::new();
        content
            .transform([WIDTH, 0., 0., HEIGHT, 0., 0.])
            .x_object(Name(b"Page"));
        pdf.stream(content_id, &content.finish());
        pdf.extend(&chunk);
    }
    pdf.pages(page_tree)
        .kids(ids.iter().copied())
        .count(ids.len() as i32);
    Ok(pdf.finish())
}
