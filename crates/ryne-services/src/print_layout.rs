//! Paginated A4 export from a folded, immutable editor projection.
//! SVG text is shaped with system fallback fonts and embedded into the PDF.
use base64::{engine::general_purpose::STANDARD, Engine};
use resvg::usvg::fontdb;
use ryne_projection::{Display, Formula, Style, TableView};
use std::{
    collections::HashMap,
    ops::Range,
    sync::{Arc, LazyLock},
};
use unicode_segmentation::UnicodeSegmentation;

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
struct PrintAlert {
    kind: ryne_syntax::alert::Kind,
    end: usize,
    top: f32,
    insertion: usize,
}
struct Layout<'a> {
    alert: Option<PrintAlert>,
    display: &'a Display,
    formula_order: Vec<usize>,
    pages: Vec<String>,
    page: String,
    y: f32,
    widths: HashMap<(char, bool, bool, bool), f32>,
    #[cfg(test)]
    linear_lookup: bool,
}
/// Both encoders consume the same print layout, independently of live GUI geometry.
pub(super) fn layout_pages(display: &Display) -> Result<Vec<String>, String> {
    if display.text.len() > 4 * 1024 * 1024 {
        return Err("PDF export exceeds 4 MiB of text".into());
    }
    display.validate_layout()?;
    let mut layout = Layout {
        display,
        alert: None,
        formula_order: ordered_formulas(display),
        pages: Vec::new(),
        page: String::new(),
        y: MARGIN,
        widths: HashMap::new(),
        #[cfg(test)]
        linear_lookup: false,
    };
    layout.document()?;
    layout.finish_page()?;
    Ok(layout.pages)
}
fn ordered_formulas(display: &Display) -> Vec<usize> {
    let mut ids = (0..display.formulas.len()).collect::<Vec<_>>();
    // Preserve the former first-match precedence even for an extension's
    // unordered resources or duplicate positions, without sorting per cell.
    ids.sort_by_key(|&i| (display.formulas[i].display.start, i));
    ids
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
        self.flush_alert();
        let number = self.pages.len() + 1;
        self.pages.push(format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}"><rect width="100%" height="100%" fill="white"/>{}<text x="{}" y="{}" font-family="sans-serif" font-size="9" fill="#727780" text-anchor="middle">{number}</text></svg>"##, std::mem::take(&mut self.page), WIDTH / 2., HEIGHT - 24.));
        self.y = MARGIN;
        if let Some(alert) = &mut self.alert {
            alert.top = MARGIN;
            alert.insertion = 0;
        }
        Ok(())
    }
    fn flush_alert(&mut self) {
        if let Some(alert) = &self.alert {
            let height = (self.y - alert.top).max(0.);
            let color = ryne_projection::alert::color(alert.kind, false);
            let bg = ryne_projection::alert::background(alert.kind, 0xffffff, false);
            let decoration = format!(
                r##"<rect x="{MARGIN}" y="{}" width="{BODY_WIDTH}" height="{height}" fill="#{bg:06x}"/><rect x="{MARGIN}" y="{}" width="3" height="{height}" fill="#{color:06x}"/>"##,
                alert.top, alert.top
            );
            self.page.insert_str(alert.insertion, &decoration);
        }
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
        let mut span_index = self
            .display
            .spans
            .partition_point(|s| s.display.end <= range.start);
        let mut formula_index = self
            .formula_order
            .partition_point(|&i| self.display.formulas[i].display.start < range.start);
        for (offset, grapheme) in text.grapheme_indices(true) {
            if grapheme == "\n" {
                lines.push(std::mem::take(&mut line));
                used = 0.;
                continue;
            }
            let position = range.start + offset;
            while self
                .display
                .spans
                .get(span_index)
                .is_some_and(|s| s.display.end <= position)
            {
                span_index += 1;
            }
            while self
                .formula_order
                .get(formula_index)
                .is_some_and(|&i| self.display.formulas[i].display.start < position)
            {
                formula_index += 1;
            }
            let style = self
                .display
                .spans
                .get(span_index)
                .filter(|s| s.display.contains(&position))
                .map_or(Style::default(), |s| s.style);
            let formula = self
                .formula_order
                .get(formula_index)
                .map(|&i| &self.display.formulas[i])
                .filter(|f| f.display.start == position)
                .map(|f| f.formula.clone());
            #[cfg(test)]
            let (style, formula) = if self.linear_lookup {
                (
                    self.display
                        .spans
                        .iter()
                        .find(|s| s.display.contains(&position))
                        .map_or(Style::default(), |s| s.style),
                    self.display
                        .formulas
                        .iter()
                        .find(|f| f.display.start == position)
                        .map(|f| f.formula.clone()),
                )
            } else {
                (style, formula)
            };
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
            let color = style.syntax.map_or(
                if style.image_failure {
                    0xc62828
                } else if style.link {
                    0x306fa6
                } else {
                    0x30343b
                },
                |c| c.color(),
            );
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
                            ryne_parser::Alignment::Center => (width - used) / 2.,
                            ryne_parser::Alignment::Right => width - used - 8.,
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
    fn list_marker(&mut self, row: &ryne_projection::ListView, inset: f32, size: f32) {
        // The editor projection now folds task syntax into its icon; preserve the state in print.
        let label = row.source.item.task.as_ref().map_or_else(
            || row.source.item.label.clone(),
            |task| {
                format!(
                    "{} [{}]",
                    row.source.item.label,
                    if task.checked { "x" } else { " " }
                )
            },
        );
        self.page.push_str(&format!(r##"<text x="{}" y="{}" font-family="sans-serif" font-size="{size}" fill="#727780" text-anchor="end">{}</text>"##, MARGIN + inset - 7., self.y + size + 6., escape(&label)));
    }
    fn document(&mut self) -> Result<(), String> {
        let display = self.display;
        let code_rows = display.code_row_index();
        let mut list_rows = std::collections::BTreeMap::new();
        for row in &display.lists {
            list_rows.entry(row.start).or_insert(row);
        }
        let mut tables = std::collections::BTreeMap::new();
        for table in &display.tables {
            tables.entry(table.range.start).or_insert(table);
        }
        let alerts: Vec<_> = display
            .quotes
            .iter()
            .filter_map(|q| {
                q.alert
                    .as_ref()
                    .filter(|a| a.folded)
                    .map(|a| (q.display.clone(), a.source.kind))
            })
            .collect();
        let mut alert_cursor = 0;
        let mut start = 0;
        let mut previous_code = None;
        while start < self.display.text.len() {
            if self.alert.as_ref().is_some_and(|a| start >= a.end) {
                self.flush_alert();
                self.alert = None;
            }
            if let Some((range, kind)) = alerts.get(alert_cursor).filter(|(r, _)| r.start == start)
            {
                self.room(56.)?;
                self.alert = Some(PrintAlert {
                    kind: *kind,
                    end: range.end,
                    top: self.y,
                    insertion: self.page.len(),
                });
                let color = ryne_projection::alert::color(*kind, false);
                let icon = ryne_projection::alert::svg(*kind)
                    .replace("currentColor", &format!("#{color:06x}"));
                let encoded = STANDARD.encode(icon.as_bytes());
                self.page.push_str(&format!(r#"<image x="{}" y="{}" width="12" height="12" href="data:image/svg+xml;base64,{encoded}"/>"#, MARGIN + 12., self.y + 7.));
                self.page.push_str(&format!(r##"<text x="{}" y="{}" font-family="sans-serif" font-size="12" font-weight="600" fill="#{color:06x}">{}</text>"##, MARGIN + 30., self.y + 18., escape(kind.label())));
                self.y += 30.;
                start += 1; // The folded type row is its original, mapped newline.
                alert_cursor += 1;
                continue;
            }
            if let Some(table) = tables.get(&start).copied() {
                self.table(table)?;
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
            if let Some(formula) = display
                .formulas
                .get(
                    display
                        .formulas
                        .partition_point(|f| f.display.start < start),
                )
                .filter(|f| f.block && f.display == (start..end))
            {
                let list = list_rows.get(&start).copied();
                let inset = list
                    .map_or(0., |r| r.source.item.inset as f32 * 2. / 3.)
                    .max(
                        display
                            .quotes
                            .iter()
                            .filter(|q| q.display.contains(&start))
                            .map(|q| q.inset as f32 * 2. / 3.)
                            .fold(0., f32::max),
                    )
                    .min(BODY_WIDTH - 1.);
                let f = &formula.formula;
                let size = 12.0_f32
                    .min((BODY_WIDTH - inset) / f.width)
                    .min((HEIGHT - MARGIN * 2. - 16.) / (f.ascent + f.descent));
                let height = (f.ascent + f.descent) * size + 16.;
                self.room(height)?;
                if let Some(row) = list.filter(|r| r.source.first) {
                    self.list_marker(row, inset, 12.);
                }
                let x = MARGIN + inset + (BODY_WIDTH - inset - f.width * size) / 2.;
                let encoded = STANDARD.encode(&f.svg);
                self.page.push_str(&format!(r#"<image x="{x}" y="{}" width="{}" height="{}" href="data:image/svg+xml;base64,{encoded}"/>"#,
                    self.y + 8., f.width * size, (f.ascent + f.descent) * size));
                self.y += height;
                start = end + usize::from(end < display.text.len());
                previous_code = None;
                continue;
            }
            let code = code_rows.code_row(start);
            let heading = self
                .display
                .spans
                .get(display.spans.partition_point(|s| s.display.end <= start)..)
                .unwrap_or_default()
                .iter()
                .take_while(|s| s.display.start <= end)
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
            let list = list_rows.get(&start).copied();
            let inset = code
                .map_or(
                    list.as_ref()
                        .map_or(0., |r| r.source.item.inset as f32 * 2. / 3.),
                    |i| 12. + self.display.code_blocks[i].source.indent as f32 * 12.,
                )
                .max(if self.alert.is_some() { 20. } else { 0. })
                .min(BODY_WIDTH / 2.);
            if code.is_some() && previous_code != code {
                self.room(40.)?;
                let label = &self.display.code_blocks[code.unwrap_or(0)].source.language;
                self.page.push_str(&format!(r##"<text x="{}" y="{}" font-family="sans-serif" font-size="9" fill="#727780">{}</text>"##, MARGIN + inset, self.y + 12., escape(label)));
                self.y += 22.;
            }
            previous_code = code;
            if start == end {
                if let Some(row) = &list
                    && row.source.first
                    && row.source.item.task.is_some()
                {
                    self.room(size * 1.65)?;
                    self.list_marker(row, inset, size);
                    self.y += size * 1.65;
                } else {
                    self.room(10.)?;
                    self.y += 10.;
                }
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
                        self.list_marker(row, inset, size);
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alert_title_and_decoration_survive_page_breaks() {
        use ryne_projection::{AlertView, QuoteView};
        use ryne_syntax::alert::{Alert, Kind};
        // This fixture starts at the validated display boundary: the empty first row
        // is the type line's retained newline, followed by selectable body text.
        let text = format!("\n{}", "Body remains selectable.\n".repeat(120));
        let mut display = Display::plain_source(&text);
        display.quotes.push(QuoteView {
            source: 0..text.len(),
            display: 0..text.len(),
            depth: 1,
            inset: 20,
            empty_tail: None,
            alert: Some(AlertView {
                folded: true,
                source: Alert {
                    owner: 0,
                    kind: Kind::Note,
                    source: 0..text.len(),
                    header: 0..1,
                    marker: 0..1,
                },
            }),
        });
        let pages = layout_pages(&display).unwrap();
        assert!(pages.len() >= 3);
        assert_eq!(
            pages
                .iter()
                .map(|p| p.matches(">Note</text>").count())
                .sum::<usize>(),
            1
        );
        assert_eq!(
            pages
                .iter()
                .map(|p| p.matches("Body remains selectable.").count())
                .sum::<usize>(),
            120
        );
        for page in pages {
            assert!(page.contains("fill=\"#0969da\""));
            assert!(page.contains("fill=\"#f0f6fd\""));
        }
    }
    #[test]
    fn ordered_print_lookup_produces_the_same_scenes_as_linear_lookup() {
        use ryne_extensions::FormulaRenderer;
        let formula = crate::math::FormularyRenderer::default()
            .render("x^2")
            .unwrap();
        let text = "alpha ** 中 e\u{301} \u{fffc} end\ttext\n".repeat(80);
        let mut display = Display {
            text,
            ..Default::default()
        };
        for (i, (start, ch)) in display.text.char_indices().enumerate() {
            let range = start..start + ch.len_utf8();
            display.spans.push(ryne_projection::Span {
                source: range.clone(),
                display: range.clone(),
                style: Style {
                    bold: i % 3 == 0,
                    italic: i % 5 == 0,
                    strike: i % 7 == 0,
                    code: i % 11 == 0,
                    ..Default::default()
                },
            });
            if ch == '\u{fffc}' {
                display.formulas.push(ryne_projection::MathView {
                    block: false,
                    source: range.clone(),
                    display: range,
                    formula: formula.clone(),
                });
            }
        }
        display.formulas.reverse();
        let mut duplicate = display.formulas[0].clone();
        duplicate.formula = Arc::new(Formula {
            path: "ignored duplicate".into(),
            width: 9999.,
            ascent: 1.,
            descent: 0.,
            svg: Vec::new(),
        });
        display.formulas.push(duplicate);
        let mut old = Layout {
            display: &display,
            alert: None,
            formula_order: ordered_formulas(&display),
            pages: Vec::new(),
            page: String::new(),
            y: MARGIN,
            widths: HashMap::new(),
            linear_lookup: true,
        };
        old.document().unwrap();
        old.finish_page().unwrap();
        // The public consumer now rejects malformed ordering. Keep the original
        // first-match oracle for the private lookup algorithm independently.
        assert!(layout_pages(&display).is_err());
        let mut optimized = Layout {
            display: &display,
            alert: None,
            formula_order: ordered_formulas(&display),
            pages: Vec::new(),
            page: String::new(),
            y: MARGIN,
            widths: HashMap::new(),
            linear_lookup: false,
        };
        optimized.document().unwrap();
        optimized.finish_page().unwrap();
        assert_eq!(optimized.pages, old.pages);
        // Independent subranges model cell wrapping and out-of-order callers.
        let range = display.text.find('中').unwrap()..display.text.find('\n').unwrap();
        let mut new = Layout {
            display: &display,
            alert: None,
            formula_order: ordered_formulas(&display),
            pages: Vec::new(),
            page: String::new(),
            y: MARGIN,
            widths: HashMap::new(),
            linear_lookup: false,
        };
        let old_lines = old.wrap(range.clone(), 11., 120.).unwrap();
        let new_lines = new.wrap(range, 11., 120.).unwrap();
        let signature = |lines: Vec<Line>| {
            lines
                .into_iter()
                .map(|line| {
                    line.into_iter()
                        .map(|p| (p.text, p.style, p.width, p.formula.map(|f| f.path.clone())))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(signature(old_lines), signature(new_lines));
    }
    #[test]
    fn folded_task_states_survive_print_including_empty_items() {
        use ryne_parser::Source;
        use ryne_syntax::ParseSnapshot;
        for (marker, status) in [("[ ]", "[ ]"), ("[x]", "[x]"), ("[X]", "[x]")] {
            for body in ["body", ""] {
                let parse = ParseSnapshot::new(Source::new(&format!("- {marker} {body}\n")));
                let row = ryne_syntax::list::rows(&parse).remove(0);
                let display = Display {
                    text: format!("{body}\n"),
                    lists: vec![ryne_projection::ListView {
                        source: row,
                        start: 0,
                    }],
                    ..Default::default()
                };
                let pages = layout_pages(&display).unwrap();
                assert!(pages[0].contains(status), "{marker} {body}");
                assert!(pages[0].contains(body));
            }
        }
    }
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
        let result = layout_pages(&display);
        assert!(result.unwrap_err().contains("U+0000"));
    }
}
