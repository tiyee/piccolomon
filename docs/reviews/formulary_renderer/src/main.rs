use ryne_formulary_eval::{render, Backend, Rendered, FIXTURES, SIZES};
use std::{
    error::Error,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let mut output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("output");
    let mut custom = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" => output = args.next().ok_or("--output requires a directory")?.into(),
            "--latex" => custom = Some(args.next().ok_or("--latex requires a formula")?),
            "--help" | "-h" => {
                println!("ryne-formulary-eval [--output DIR] [--latex 'FORMULA']\nWithout --latex, renders the built-in comparison suite. Outputs index.html, SVG, PNG and results.tsv.");
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    fs::create_dir_all(&output)?;
    let fixtures = custom.as_ref().map_or_else(
        || FIXTURES.to_vec(),
        |source| vec![("custom", source.as_str())],
    );
    let mut html = HTML_START.to_owned();
    let mut results =
        String::from("fixture\tbackend\tfont_px\tstatus\twidth\tascent\tdescent\tdetail\n");
    let mut totals = [0usize; 3];
    for (id, source) in fixtures {
        write!(
            html,
            "<section><h2>{}</h2><pre>{}</pre>",
            escape(id),
            escape(source)
        )?;
        for size in SIZES {
            write!(
                html,
                "<h3>{size:.0} px · inline / text style</h3><div class=pair>"
            )?;
            for backend in [Backend::Ratex, Backend::Formulary] {
                write!(html, "<article><h4>{}</h4>", backend.label())?;
                match render(backend, source, size) {
                    Ok(rendered) => {
                        let stem = format!("{id}-{}-{size:.0}", backend.id());
                        save_images(&output, &stem, &rendered)?;
                        if let Some(mathml) = &rendered.mathml {
                            fs::write(output.join(format!("{stem}.mathml")), mathml)?;
                            write!(html, "<details><summary>转换后的 MathML</summary><pre>{}</pre></details>", escape(mathml))?;
                        }
                        let status = if rendered.warnings.is_empty() {
                            "ok"
                        } else {
                            "warning"
                        };
                        totals[usize::from(status == "warning")] += 1;
                        let detail = rendered.warnings.join("; ");
                        writeln!(
                            results,
                            "{id}\t{}\t{size:.0}\t{status}\t{:.3}\t{:.3}\t{:.3}\t{}",
                            backend.id(),
                            rendered.width,
                            rendered.ascent,
                            rendered.descent,
                            tsv(&detail)
                        )?;
                        write!(html, "<div class=reading style=\"font-size:{size}px\">正文 Abg <span class=formula style=\"width:{:.3}px;height:{:.3}px;vertical-align:-{:.3}px\">{}<i class=baseline style=\"bottom:{:.3}px\"></i></span> 接着阅读</div>", rendered.width, rendered.ascent + rendered.descent, rendered.descent, rendered.svg, rendered.descent)?;
                        write!(
                            html,
                            "<p class=metrics>width {:.2} · ascent {:.2} · descent {:.2} px</p>",
                            rendered.width, rendered.ascent, rendered.descent
                        )?;
                        for warning in &rendered.warnings {
                            write!(html, "<p class=warning>降级警告：{}</p>", escape(warning))?;
                        }
                        // Display whole physical pixels at each image's density,
                        // without shrinking the allocation's rounded edge pixels.
                        let height = rendered.ascent + rendered.descent;
                        write!(html, "<details><summary>resvg 栅格对比 · 1× / 2×</summary><div class=raster><span>1× <img alt=\"1x rendering\" width=\"{:.3}\" height=\"{:.3}\" src=\"{stem}@1x.png\"></span><span>2× <img alt=\"2x rendering\" width=\"{:.3}\" height=\"{:.3}\" src=\"{stem}@2x.png\"></span></div></details>", rendered.width.ceil(), height.ceil(), (rendered.width * 2.).ceil() / 2., (height * 2.).ceil() / 2.)?;
                    }
                    Err(error) => {
                        totals[2] += 1;
                        write!(
                            html,
                            "<p class=error>保留源码：{}</p><pre>{}</pre>",
                            escape(&error),
                            escape(source)
                        )?;
                        writeln!(
                            results,
                            "{id}\t{}\t{size:.0}\terror\t\t\t\t{}",
                            backend.id(),
                            tsv(&error)
                        )?;
                    }
                }
                html.push_str("</article>");
            }
            html.push_str("</div>");
        }
        html.push_str("</section>");
    }
    write!(html, "<footer>本次结果：{} 项生成，{} 项带警告，{} 项保留源码。生成成功仅表示完成渲染，视觉质量与语义仍需检查。</footer></main></body></html>", totals[0], totals[1], totals[2])?;
    fs::write(output.join("index.html"), html)?;
    fs::write(output.join("results.tsv"), &results)?;
    if custom.is_none() {
        save_overview(&output)?;
    }
    print!("{results}");
    println!(
        "Report: {}",
        output.canonicalize()?.join("index.html").display()
    );
    // Invalid fixtures are expected sample results; I/O and raster errors above fail the run.
    Ok(())
}

fn save_images(output: &Path, stem: &str, rendered: &Rendered) -> Result<(), Box<dyn Error>> {
    fs::write(output.join(format!("{stem}.svg")), &rendered.svg)?;
    let tree = resvg::usvg::Tree::from_str(&rendered.svg, &Default::default())?;
    for dpr in [1, 2] {
        let width = (rendered.width * dpr as f32).ceil() as u32;
        let height = ((rendered.ascent + rendered.descent) * dpr as f32).ceil() as u32;
        if width > 20_000 || height > 20_000 || u64::from(width) * u64::from(height) > 16_000_000 {
            return Err("raster exceeds pixel limits".into());
        }
        let mut pixels =
            resvg::tiny_skia::Pixmap::new(width, height).ok_or("pixel allocation failed")?;
        let scale = rendered.width * dpr as f32 / tree.size().width();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixels.as_mut(),
        );
        if !pixels.pixels().iter().any(|pixel| pixel.alpha() > 0) {
            return Err(format!("{stem}: empty raster").into());
        }
        pixels.save_png(output.join(format!("{stem}@{dpr}x.png")))?;
    }
    Ok(())
}

fn save_overview(output: &Path) -> Result<(), Box<dyn Error>> {
    let ids = [
        "scripts",
        "fraction",
        "roots",
        "integral",
        "matrix",
        "delimiters",
    ];
    let height = 90 + 120 * ids.len();
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="{height}"><rect width="100%" height="100%" fill="white"/><g font-family="Arial" fill="#282b30"><text x="24" y="30" font-size="20">Ryne: inline formulas at 36 px / rasterized at 2x</text><text x="190" y="66" font-size="16">RaTeX 0.1.14 / KaTeX</text><text x="590" y="66" font-size="16">Formulary 0.3.0 / STIX Two Math</text>"##
    );
    for (row, id) in ids.iter().enumerate() {
        let (_, source) = FIXTURES
            .iter()
            .find(|(name, _)| name == id)
            .ok_or("missing overview fixture")?;
        let top = 90 + row * 120;
        let baseline = (top + 80) as f32;
        write!(
            svg,
            r##"<path d="M24 {top}H976" stroke="#e2e8f0"/><text x="24" y="{}" font-size="14">{id}</text>"##,
            top + 55
        )?;
        for (column, backend) in [Backend::Ratex, Backend::Formulary].into_iter().enumerate() {
            let x = 190 + column * 400;
            let rendered = render(backend, source, 36.)?;
            let tree = resvg::usvg::Tree::from_str(&rendered.svg, &Default::default())?;
            let scale = rendered.width / tree.size().width();
            write!(
                svg,
                r##"<path d="M{x} {baseline}h370" stroke="#ec4899" stroke-dasharray="3 3" stroke-width="0.5"/><g transform="translate({x} {}) scale({scale})">{}</g>"##,
                baseline - rendered.ascent,
                rendered.svg
            )?;
        }
    }
    svg.push_str("</g></svg>");
    fs::write(output.join("comparison.svg"), &svg)?;
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(&svg, &options)?;
    let mut pixels = resvg::tiny_skia::Pixmap::new(2000, height as u32 * 2)
        .ok_or("overview allocation failed")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(2., 2.),
        &mut pixels.as_mut(),
    );
    pixels.save_png(output.join("comparison.png"))?;
    Ok(())
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn tsv(text: &str) -> String {
    text.replace(['\t', '\r', '\n'], " ")
}

const HTML_START: &str = r#"<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Ryne · RaTeX / Formulary 对比</title>
<style>
*{box-sizing:border-box}body{margin:0;background:#f3f5f7;color:#282b30;font:15px/1.6 -apple-system,BlinkMacSystemFont,sans-serif}
main{max-width:1400px;margin:auto;padding:32px}h1{font-size:28px;margin:0}h2{font-size:19px;margin:0 0 8px}h3{font-size:14px;color:#64748b}h4{font-size:14px;margin:0 0 12px}
header{margin-bottom:28px}section{background:white;border:1px solid #dce2e9;border-radius:12px;padding:24px;margin:20px 0}.pair{display:grid;grid-template-columns:1fr 1fr;gap:20px}article{min-width:0;border:1px solid #e5e9ef;border-radius:8px;padding:16px;overflow:auto}
pre{white-space:pre-wrap;overflow-wrap:anywhere;background:#f5f7fa;padding:10px;border-radius:6px;font:13px/1.6 Menlo,monospace}.reading{white-space:nowrap;line-height:1.667;font-family:"Helvetica Neue","PingFang SC",sans-serif;padding:12px 0}.formula{display:inline-block;position:relative}.formula>svg{display:block;width:100%;height:100%}.baseline{display:none;position:absolute;left:0;right:0;border-top:1px dashed #ec4899;pointer-events:none}#guides:checked~main .baseline{display:block}
.metrics{font:12px/1.6 Menlo,monospace;color:#64748b}.error,.warning{color:#9a3412;background:#fff7ed;padding:8px;border-radius:4px}summary{cursor:pointer;color:#52637c;font-size:13px}.raster{display:flex;gap:16px;flex-wrap:wrap;padding:12px 0}.raster span{display:flex;gap:8px;align-items:center;font-size:12px}.raster img{max-width:none}footer{color:#64748b}#guides{position:fixed;bottom:22px;right:155px;z-index:2}label[for=guides]{position:fixed;bottom:16px;right:24px;padding:2px 10px;background:#fff;z-index:2;border-radius:4px}@media(max-width:850px){main{padding:16px}.pair{grid-template-columns:1fr}}
</style></head><body><input id="guides" type="checkbox"><label for="guides">显示公式基线</label><main>
<header><h1>RaTeX / Formulary 公式对比</h1>
<p>Ryne 编辑器已切换为 Formulary；此页保留切换前的独立对比。相同 LaTeX，18 px 正文与 36 px 放大，均为行内样式。Formulary 使用 math-core 0.5.5 转换及 STIX Two Math 2.13 b171。</p>
<p>上方 SVG 用浏览器查看字形与基线，下方 1× / 2× PNG 由 resvg 0.45.1 生成。RaTeX 沿用原编辑器参数；保留两种渲染器各自 padding，并将 padding 计入基线。正式编辑器另有 SVG 精度修正与严格源码回退；此页不代表 GPUI 的最终绘制或编辑验收。</p></header>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_and_diagnostics_are_safe_for_html_and_tsv() {
        assert_eq!(escape("<x a=\"&\">'"), "&lt;x a=&quot;&amp;&quot;&gt;&#39;");
        assert_eq!(tsv("a\tb\r\nc"), "a b  c");
    }
}
