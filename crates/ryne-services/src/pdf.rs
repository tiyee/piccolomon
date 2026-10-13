//! Paginated A4 export from a folded, immutable editor projection.
//! SVG text is shaped with system fallback fonts and embedded into the PDF.
use base64::{Engine, engine::general_purpose::STANDARD};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref};
use resvg::usvg::{self, fontdb};
use ryne_projection::{Display, Formula, Style, TableView};
use std::{
    collections::HashMap,
    ops::Range,
    sync::{Arc, LazyLock},
};
use unicode_segmentation::UnicodeSegmentation;

pub struct PdfExporter;
pub(super) const WIDTH: f32 = 595.28;
pub(super) const HEIGHT: f32 = 841.89;
const MARGIN: f32 = 48.;
const BODY_WIDTH: f32 = WIDTH - MARGIN * 2.;
pub(super) static FONTS: LazyLock<Arc<fontdb::Database>> = LazyLock::new(|| {
    let mut fonts = fontdb::Database::new();
    fonts.load_system_fonts();
    fonts.set_sans_serif_family(if cfg!(target_os = "macos") {
        "Helvetica Neue"
    } else {
        "Arial"
    });
    fonts.set_monospace_family(if cfg!(target_os = "macos") {
        "Menlo"
    } else {
        "Consolas"
    });
    Arc::new(fonts)
});
#[derive(Clone)]
struct Piece {
    text: String,
    style: Style,
    width: f32,
    formula: Option<Arc<Formula>>,
}
type Line = Vec<Piece>;
struct Layout<'a> {
    display: &'a Display,
    pages: Vec<String>,
    page: String,
    y: f32,
    widths: HashMap<(char, bool, bool, bool), f32>,
}
impl ryne_extensions::PdfExporter for PdfExporter {
    fn export(&self, display: &Display) -> Result<Vec<u8>, String> {
        encode(&layout_pages(display)?)
    }
}
/// Both encoders consume the same print layout, independently of live GUI geometry.
pub(super) fn layout_pages(display: &Display) -> Result<Vec<String>, String> {
    if display.text.len() > 4 * 1024 * 1024 {
        return Err("PDF export exceeds 4 MiB of text".into());
    }
    let mut layout = Layout {
        display,
        pages: Vec::new(),
        page: String::new(),
        y: MARGIN,
        widths: HashMap::new(),
    };
    layout.document()?;
    layout.finish_page()?;
    Ok(layout.pages)
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
impl Layout<'_> {
    fn finish_page(&mut self) -> Result<(), String> {
        if self.pages.len() >= 500 {
            return Err("PDF export exceeds 500 pages".into());
        }
        let number = self.pages.len() + 1;
        self.pages.push(format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}"><rect width="100%" height="100%" fill="white"/>{}<text x="{}" y="{}" font-family="sans-serif" font-size="9" fill="#727780" text-anchor="middle">{number}</text></svg>"##, std::mem::take(&mut self.page), WIDTH / 2., HEIGHT - 24.));
        self.y = MARGIN;
        Ok(())
    }
    fn room(&mut self, height: f32) -> Result<(), String> {
        if height > HEIGHT - MARGIN * 2. {
            return Err("PDF element exceeds a page".into());
        }
        if self.y + height > HEIGHT - MARGIN {
            self.finish_page()?;
        }
        Ok(())
    }
    fn glyph_width(&mut self, ch: char, style: Style) -> Result<f32, String> {
        if matches!(ch, '\u{200d}' | '\u{fe0e}' | '\u{fe0f}') {
            return Ok(0.);
        }
        if ch.is_control() && ch != '\t' {
            return Err(format!(
                "Cannot export control character U+{:04X}",
                ch as u32
            ));
        }
        let key = (
            ch,
            style.code || style.code_block || style.source,
            style.bold || style.heading > 0,
            style.italic,
        );
        if let Some(width) = self.widths.get(&key) {
            return Ok(*width);
        }
        let family = if key.1 {
            fontdb::Family::Monospace
        } else {
            fontdb::Family::SansSerif
        };
        let preferred = FONTS.query(&fontdb::Query {
            families: &[family],
            weight: fontdb::Weight(if key.2 { 700 } else { 400 }),
            style: if key.3 {
                fontdb::Style::Italic
            } else {
                fontdb::Style::Normal
            },
            ..Default::default()
        });
        let width = preferred
            .into_iter()
            .chain(FONTS.faces().map(|f| f.id))
            .find_map(|id| {
                FONTS
                    .with_face_data(id, |data, index| {
                        let face = ttf_parser::Face::parse(data, index).ok()?;
                        let glyph = face.glyph_index(if ch == '\t' { ' ' } else { ch })?;
                        let advance =
                            face.glyph_hor_advance(glyph)? as f32 / face.units_per_em() as f32;
                        Some(advance * if ch == '\t' { 4. } else { 1. })
                    })
                    .flatten()
            })
            .ok_or_else(|| format!("No PDF font for U+{:04X}", ch as u32))?;
        self.widths.insert(key, width);
        Ok(width)
    }
    fn wrap(&mut self, range: Range<usize>, size: f32, width: f32) -> Result<Vec<Line>, String> {
        let text = &self.display.text[range.clone()];
        let mut lines = Vec::new();
        let mut line = Vec::new();
        let mut used = 0.;
        for (offset, grapheme) in text.grapheme_indices(true) {
            if grapheme == "\n" {
                lines.push(std::mem::take(&mut line));
                used = 0.;
                continue;
            }
            let position = range.start + offset;
            let style = self
                .display
                .spans
                .iter()
                .find(|s| s.display.contains(&position))
                .map_or(Style::default(), |s| s.style);
            let formula = self
                .display
                .formulas
                .iter()
                .find(|f| f.display.start == position)
                .map(|f| f.formula.clone());
            let advance = if let Some(formula) = &formula {
                formula.width * size
            } else {
                let mut advance = 0.;
                for ch in grapheme.chars() {
                    advance += self.glyph_width(ch, style)? * size;
                }
                advance
            };
            if !line.is_empty() && used + advance > width - 2. {
                lines.push(std::mem::take(&mut line));
                used = 0.;
            }
            if advance > width {
                return Err("PDF column is too narrow for its content".into());
            }
            line.push(Piece {
                text: grapheme.replace('\t', "    "),
                style,
                width: advance,
                formula,
            });
            used += advance;
        }
        lines.push(line);
        Ok(lines)
    }
    fn line_height(line: &Line, size: f32, normal: f32) -> f32 {
        let ascent = line
            .iter()
            .filter_map(|p| p.formula.as_ref())
            .map(|f| f.ascent * size)
            .fold(size, f32::max);
        let descent = line
            .iter()
            .filter_map(|p| p.formula.as_ref())
            .map(|f| f.descent * size)
            .fold(0., f32::max);
        normal.max(ascent + descent + 8.)
    }
    fn draw(&mut self, line: &Line, mut x: f32, size: f32, height: f32) {
        let ascent = line
            .iter()
            .filter_map(|p| p.formula.as_ref())
            .map(|f| f.ascent * size)
            .fold(size, f32::max);
        let baseline = self.y + (height - size).min(6.) + ascent;
        let mut index = 0;
        while index < line.len() {
            let piece = &line[index];
            if let Some(formula) = &piece.formula {
                let encoded = STANDARD.encode(&formula.svg);
                self.page.push_str(&format!(r#"<image x="{x}" y="{}" width="{}" height="{}" href="data:image/svg+xml;base64,{encoded}"/>"#, baseline - formula.ascent * size, piece.width, (formula.ascent + formula.descent) * size));
                x += piece.width;
                index += 1;
                continue;
            }
            let mut text = piece.text.clone();
            let mut width = piece.width;
            index += 1;
            while index < line.len()
                && line[index].formula.is_none()
                && line[index].style == piece.style
            {
                text.push_str(&line[index].text);
                width += line[index].width;
                index += 1;
            }
            let style = piece.style;
            if style.code && !style.code_block {
                self.page.push_str(&format!(
                    r##"<rect x="{x}" y="{}" width="{width}" height="{}" rx="2" fill="#f0f1ee"/>"##,
                    baseline - size,
                    size + 4.
                ));
            }
            let family = if style.code || style.code_block || style.source {
                "monospace"
            } else {
                "sans-serif"
            };
            let color = style
                .syntax
                .map_or(if style.link { 0x306fa6 } else { 0x30343b }, |c| c.color());
            self.page.push_str(&format!(r##"<text x="{x}" y="{baseline}" xml:space="preserve" font-family="{family}" font-size="{size}" font-weight="{}" font-style="{}" fill="#{color:06x}">{}</text>"##, if style.bold || style.heading > 0 { 700 } else { 400 }, if style.italic { "italic" } else { "normal" }, escape(&text)));
            if style.strike || style.link {
                let y = baseline - if style.strike { size * 0.35 } else { -2. };
                self.page.push_str(&format!(
                    r##"<path d="M{x} {y}h{width}" stroke="#{color:06x}" stroke-width="0.7"/>"##
                ));
            }
            x += width;
        }
    }
    fn table(&mut self, table: &TableView) -> Result<(), String> {
        let inset = (table.source.inset as f32 * 2. / 3.).min(BODY_WIDTH / 3.);
        let width = (BODY_WIDTH - inset) / table.source.align.len() as f32;
        for (row_index, row) in table.rows.iter().enumerate() {
            let mut cells = Vec::new();
            for cell in row {
                cells.push(self.wrap(cell.clone(), 11., width - 16.)?);
            }
            let count = cells.iter().map(Vec::len).max().unwrap_or(1);
            for n in 0..count {
                let height = cells
                    .iter()
                    .filter_map(|c| c.get(n))
                    .map(|l| Self::line_height(l, 11., 19.))
                    .fold(19., f32::max);
                self.room(height + 8.)?;
                let top = self.y;
                for (column, cell) in cells.iter().enumerate() {
                    let x = MARGIN + inset + column as f32 * width;
                    self.page.push_str(&format!(r##"<rect x="{x}" y="{top}" width="{width}" height="{}" fill="{}" stroke="#d9ddd6" stroke-width="0.6"/>"##, height + 8., if row_index == 0 { "#f0f1ee" } else { "white" }));
                    if let Some(line) = cell.get(n) {
                        let used: f32 = line.iter().map(|p| p.width).sum();
                        let offset = match table.source.align[column] {
                            editable_markdown_parser::Alignment::Center => (width - used) / 2.,
                            editable_markdown_parser::Alignment::Right => width - used - 8.,
                            _ => 8.,
                        };
                        self.y = top + 4.;
                        self.draw(line, x + offset, 11., height);
                    }
                }
                self.y = top + height + 8.;
            }
        }
        self.y += 12.;
        Ok(())
    }
    fn document(&mut self) -> Result<(), String> {
        let mut start = 0;
        let mut previous_code = None;
        while start < self.display.text.len() {
            if let Some(table) = self
                .display
                .tables
                .iter()
                .find(|t| t.range.start == start)
                .cloned()
            {
                self.table(&table)?;
                start = table.range.end;
                continue;
            }
            if let Some(diagram) = self
                .display
                .diagrams
                .iter()
                .find(|d| d.display.start == start)
            {
                let d = &diagram.diagram;
                let scale = (BODY_WIDTH / d.width)
                    .min((HEIGHT - MARGIN * 2.) / d.height)
                    .min(1.);
                let height = d.height * scale;
                self.room(height)?;
                let mut pixels = d.pixels.to_vec();
                for pixel in pixels.as_chunks_mut::<4>().0 {
                    pixel.swap(0, 2);
                    for channel in 0..3 {
                        pixel[channel] =
                            ((pixel[channel] as u16 * pixel[3] as u16 + 127) / 255) as u8;
                    }
                }
                let pixmap = resvg::tiny_skia::Pixmap::from_vec(
                    pixels,
                    resvg::tiny_skia::IntSize::from_wh(d.pixel_width, d.pixel_height)
                        .ok_or("Invalid diagram size")?,
                )
                .ok_or("Invalid diagram pixels")?;
                let encoded = STANDARD.encode(pixmap.encode_png().map_err(|e| e.to_string())?);
                self.page.push_str(&format!(r#"<image x="{MARGIN}" y="{}" width="{}" height="{height}" href="data:image/png;base64,{encoded}"/>"#, self.y, d.width * scale));
                self.y += height + 12.;
                start = diagram.display.end
                    + usize::from(
                        self.display.text.as_bytes().get(diagram.display.end) == Some(&b'\n'),
                    );
                continue;
            }
            let end = self.display.text[start..]
                .find('\n')
                .map_or(self.display.text.len(), |n| start + n);
            let code = self.display.code_row(start);
            let heading = self
                .display
                .spans
                .iter()
                .find(|s| s.display.start <= end && start < s.display.end && s.style.heading > 0)
                .map_or(0, |s| s.style.heading);
            let size = if code.is_some() {
                10.
            } else {
                match heading {
                    1 => 22.,
                    2 => 18.,
                    3 => 15.,
                    4 => 14.,
                    5 => 13.,
                    _ => 12.,
                }
            };
            let list = self.display.list_row(start).cloned();
            let inset = code
                .map_or(
                    list.as_ref()
                        .map_or(0., |r| r.source.item.inset as f32 * 2. / 3.),
                    |i| 12. + self.display.code_blocks[i].source.indent as f32 * 12.,
                )
                .min(BODY_WIDTH / 2.);
            if code.is_some() && previous_code != code {
                self.room(40.)?;
                let label = &self.display.code_blocks[code.unwrap_or(0)].source.language;
                self.page.push_str(&format!(r##"<text x="{}" y="{}" font-family="sans-serif" font-size="9" fill="#727780">{}</text>"##, MARGIN + inset, self.y + 12., escape(label)));
                self.y += 22.;
            }
            previous_code = code;
            if start == end {
                self.room(10.)?;
                self.y += 10.;
            } else {
                let lines = self.wrap(
                    start..end,
                    size,
                    BODY_WIDTH - inset - if code.is_some() { 12. } else { 0. },
                )?;
                for (n, line) in lines.iter().enumerate() {
                    let height = Self::line_height(line, size, size * 1.65);
                    self.room(height)?;
                    if code.is_some() {
                        self.page.push_str(&format!(r##"<rect x="{}" y="{}" width="{}" height="{height}" fill="#f0f1ee"/>"##, MARGIN + inset - 8., self.y, BODY_WIDTH - inset + 8.));
                    }
                    if let Some(row) = &list
                        && row.source.first
                        && n == 0
                    {
                        self.page.push_str(&format!(r##"<text x="{}" y="{}" font-family="sans-serif" font-size="{size}" fill="#727780" text-anchor="end">{}</text>"##, MARGIN + inset - 7., self.y + size + 6., escape(&row.source.item.label)));
                    }
                    self.draw(line, MARGIN + inset, size, height);
                    self.y += height;
                }
            }
            start = end + usize::from(end < self.display.text.len());
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_formulas_reserve_both_maximum_ascent_and_descent() {
        let piece = |ascent, descent| Piece {
            text: String::new(),
            style: Style::default(),
            width: 10.,
            formula: Some(Arc::new(Formula {
                path: String::new(),
                width: 1.,
                ascent,
                descent,
                svg: Vec::new(),
            })),
        };
        let line = vec![piece(3., 0.2), piece(0.5, 2.)];
        assert!(Layout::line_height(&line, 12., 20.) >= 12. * (3. + 2.) + 8.);
    }
    #[test]
    fn unsupported_control_characters_fail_instead_of_disappearing() {
        let display = Display {
            text: "visible\0hidden".into(),
            ..Default::default()
        };
        let result = ryne_extensions::PdfExporter::export(&PdfExporter, &display);
        assert!(result.unwrap_err().contains("U+0000"));
    }
}
