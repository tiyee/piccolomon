//! Real adapter comparison: cargo run --locked -p ryne-services --example math-fonts
use resvg::tiny_skia::{Pixmap, PixmapPaint, Transform};
use ryne_extensions::MathFont;
use ryne_services::math::render_with_font;
use std::{fmt::Write, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/math-font-comparison"));
    std::fs::create_dir_all(&output)?;
    let formulas = [
        r"x_i^2 + a_{n+1}",
        r"\frac{-b \pm \sqrt{b^2-4ac}}{2a}",
        r"\sqrt[3]{\frac{a}{b}}",
        r"\int_0^\infty e^{-x^2}\,dx=\frac{\sqrt{\pi}}{2}",
        r"\sum_{n=1}^{N} n^2",
        r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
        r"\left(\frac{a}{b}\right)^2",
        r"\widehat{abc} + \overrightarrow{AB}",
    ];
    let mut html = String::from(
        "<!doctype html><meta charset='utf-8'><title>Ryne math font comparison</title><style>body{font-family:system-ui;margin:32px;background:#faf9f6}table{border-collapse:collapse}td,th{padding:12px 24px;border:1px solid #ddd;text-align:left}img{vertical-align:baseline}code{font-size:13px}small{color:#666}</style><h1>Lete Sans Math 0.63 / STIX Two Math 2.13</h1><p>math-core 0.5.5 → Formulary 0.3.0, actual Ryne adapter. SVG glyph paths, red baseline. PNGs at 1× / 2×. No layout engine changes.</p>",
    );
    let mut sheet = Pixmap::new(1600, 2600).ok_or("contact sheet allocation")?;
    sheet.fill(resvg::tiny_skia::Color::from_rgba8(250, 249, 246, 255));
    let mut sheet_y = 20;
    for size in [12., 14., 16., 18., 36.] {
        write!(
            html,
            "<h2>{size} px</h2><table><tr><th>LaTeX</th><th>Lete Sans Math</th><th>STIX Two Math</th></tr>"
        )?;
        for (row, latex) in formulas.iter().enumerate() {
            let escaped = latex
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            write!(html, "<tr><td><code>{escaped}</code></td>")?;
            let pair = MathFont::ALL
                .map(|font| render_with_font(latex, font).expect("validated comparison formula"));
            let ascent = pair
                .iter()
                .map(|f| f.ascent * size * 2.)
                .fold(0_f32, f32::max);
            let height = ascent
                + pair
                    .iter()
                    .map(|f| f.descent * size * 2.)
                    .fold(0_f32, f32::max);
            for (column, font) in MathFont::ALL.into_iter().enumerate() {
                let formula = render_with_font(latex, font).ok_or("formula fallback")?;
                let tree = resvg::usvg::Tree::from_data(&formula.svg, &Default::default())?;
                let name = format!("{}-{size}-{row}", font.id());
                std::fs::write(output.join(format!("{name}.svg")), &formula.svg)?;
                write!(
                    html,
                    "<td><span style='display:inline-block;position:relative;width:{}px;height:{}px;vertical-align:-{}px'><img src='{name}.svg' style='width:100%;height:100%'><span style='position:absolute;left:0;right:0;top:{}px;border-top:1px solid #d88'></span></span>",
                    formula.width * size,
                    (formula.ascent + formula.descent) * size,
                    formula.descent * size,
                    formula.ascent * size
                )?;
                for density in [1., 2.] {
                    let mut pixmap = Pixmap::new(
                        (formula.width * size * density).ceil() as u32,
                        ((formula.ascent + formula.descent) * size * density).ceil() as u32,
                    )
                    .ok_or("raster allocation")?;
                    resvg::render(
                        &tree,
                        Transform::from_scale(size * density / 1000., size * density / 1000.),
                        &mut pixmap.as_mut(),
                    );
                    pixmap.save_png(output.join(format!("{name}-{density}x.png")))?;
                    if density == 2. && (size == 14. || size == 36.) {
                        let y = sheet_y + (ascent - formula.ascent * size * 2.).ceil() as i32;
                        sheet.draw_pixmap(
                            40 + column as i32 * 780,
                            y,
                            pixmap.as_ref(),
                            &PixmapPaint::default(),
                            Transform::identity(),
                            None,
                        );
                    }
                    write!(html, " <a href='{name}-{density}x.png'>{density}×</a>")?;
                }
                write!(
                    html,
                    "<br><small>width {:.4}em · ascent {:.4}em · descent {:.4}em</small></td>",
                    formula.width, formula.ascent, formula.descent
                )?;
            }
            html.push_str("</tr>");
            if size == 14. || size == 36. {
                sheet_y += height.ceil() as i32 + 30;
            }
        }
        html.push_str("</table>");
    }
    sheet.save_png(output.join("comparison.png"))?;
    std::fs::write(output.join("index.html"), html)?;
    println!("{}", output.join("index.html").display());
    Ok(())
}
