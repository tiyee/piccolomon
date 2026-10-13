//! Disposable inline math views. Markdown bytes remain owned by Document.

use formulary::{ast::Node as MathNode, Item, MathFont};
pub use ryne_projection::Formula;
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
};

const FONT: &[u8] = include_bytes!("../../../assets/fonts/stix-two-math/STIXTwoMath-Regular.otf");
// Layout at the body size, then enlarge the SVG coordinate system before
// emission: Formulary rounds transforms to three decimals. Metrics stay in em.
const LAYOUT_EM: f32 = 18.;
const SVG_EM: f32 = 1000.;
const PAD: f32 = 0.06 * SVG_EM;
const SVG_MARGIN: f32 = 2.;
const MAX_BYTES: usize = 4096;

type Cache = HashMap<String, Option<Arc<Formula>>>;
static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn render(latex: &str) -> Option<Arc<Formula>> {
    if latex.len() > MAX_BYTES || latex.contains(['\r', '\n']) {
        return None;
    }
    if let Some(value) = CACHE.lock().ok()?.get(latex).cloned() {
        return value;
    }
    // Bound recursive input before it can reach the GUI event loop.
    let mut nesting = 0usize;
    for ch in latex.chars() {
        match ch {
            '{' | '[' | '(' => {
                nesting += 1;
                if nesting > 64 {
                    return None;
                }
            }
            '}' | ']' | ')' => nesting = nesting.saturating_sub(1),
            _ => {}
        }
    }
    let value = std::panic::catch_unwind(|| {
        let converter = math_core::LatexToMathML::new(math_core::MathCoreConfig::default()).ok()?;
        let mathml = converter
            .convert_with_local_counter(latex, math_core::MathDisplay::Inline)
            .ok()?;
        let root = formulary::parse(&mathml).ok()?;
        let face = ttf_parser::Face::parse(FONT, 0).ok()?;
        // Formulary omits unmapped glyphs from its display list. Inspect the
        // parsed, mathvariant-transformed tokens before any content can vanish.
        // Unsupported structures or variants keep the complete LaTeX source.
        if !root.warnings.is_empty()
            || root.has_arabic_text()
            || !root.children.iter().all(|node| supported_node(node, &face))
        {
            return None;
        }
        let font = MathFont::new(FONT, 0).ok()?;
        let mut laid = formulary::layout(
            &root,
            &font,
            &formulary::LayoutOptions {
                font_size: LAYOUT_EM,
            },
        );
        let scale = SVG_EM / LAYOUT_EM;
        let width = (laid.width * scale + 2. * PAD) / SVG_EM;
        let ascent = (laid.ascent * scale + PAD) / SVG_EM;
        let descent = (laid.descent * scale + PAD) / SVG_EM;
        if ![width, ascent, descent]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.)
            || width <= 0.
            || ascent + descent <= 0.
            || width > 256.
            || ascent + descent > 128.
            || laid.items.is_empty()
            || laid.items.len() > 16_384
        {
            return None;
        }
        // to_svg supplies a 2-unit margin. Expand it to our 0.06 em inset;
        // glyph y positions are relative to the baseline, x positions are not.
        let inset = PAD - SVG_MARGIN;
        laid.width = laid.width * scale + 2. * inset;
        laid.ascent = laid.ascent * scale + inset;
        laid.descent = laid.descent * scale + inset;
        for item in &mut laid.items {
            match item {
                Item::Glyph {
                    x,
                    y,
                    size,
                    advance,
                    ..
                } => {
                    *x = *x * scale + inset;
                    *y *= scale;
                    *size *= scale;
                    *advance *= scale;
                }
                Item::Rule { x, y, w, h, .. } | Item::Background { x, y, w, h, .. } => {
                    *x = *x * scale + inset;
                    *y *= scale;
                    *w *= scale;
                    *h *= scale;
                }
                _ => return None,
            }
        }
        let svg = formulary::svg::to_svg(&laid, &font);
        if svg.len() > 2 * 1024 * 1024 {
            return None;
        }
        Some(Arc::new(Formula {
            path: format!("math:{latex}"),
            width,
            ascent,
            descent,
            svg: svg.into_bytes(),
        }))
    })
    .ok()
    .flatten();
    let mut cache = CACHE.lock().ok()?;
    if cache.len() >= 256 {
        cache.clear();
    }
    cache.insert(latex.into(), value.clone());
    value
}

fn supported_text(text: &str, face: &ttf_parser::Face<'_>) -> bool {
    let mut base = None;
    for ch in text.chars() {
        if matches!(ch as u32, 0xFE00..=0xFE0F | 0xE0100..=0xE01EF) {
            if !base.is_some_and(|base| {
                face.glyph_variation_index(base, ch)
                    .is_some_and(|id| id.0 != 0)
            }) {
                return false;
            }
            continue;
        }
        // These MathML operators deliberately have no visible glyph.
        if matches!(ch as u32, 0x2061..=0x2064) {
            base = None;
            continue;
        }
        if !ch.is_whitespace() && face.glyph_index(ch).is_none_or(|id| id.0 == 0) {
            return false;
        }
        base = Some(ch);
    }
    true
}

fn supported_node(node: &MathNode, face: &ttf_parser::Face<'_>) -> bool {
    let supported = |node: &MathNode| supported_node(node, face);
    let optional = |node: &Option<Box<MathNode>>| node.as_deref().is_none_or(supported);
    match node {
        MathNode::Identifier(text)
        | MathNode::Number(text)
        | MathNode::Text(text)
        | MathNode::Operator { text, .. } => supported_text(text, face),
        MathNode::Row(children)
        | MathNode::Sqrt(children)
        | MathNode::Phantom(children)
        | MathNode::Styled { children, .. }
        | MathNode::Padded { children, .. } => children.iter().all(supported),
        MathNode::Frac { num, den, .. } => supported(num) && supported(den),
        MathNode::Root { base, index } => supported(base) && supported(index),
        MathNode::Scripts { base, sub, sup }
        | MathNode::UnderOver {
            base,
            under: sub,
            over: sup,
            ..
        } => supported(base) && optional(sub) && optional(sup),
        MathNode::MultiScripts { base, post, pre } => {
            supported(base)
                && post.iter().chain(pre).all(|(sub, sup)| {
                    sub.as_ref().is_none_or(supported) && sup.as_ref().is_none_or(supported)
                })
        }
        MathNode::Table { rows, .. } => rows.iter().flatten().all(|cell| supported(&cell.content)),
        MathNode::Space { .. } => true,
    }
}

#[derive(Default)]
pub struct FormularyRenderer;
impl ryne_extensions::FormulaRenderer for FormularyRenderer {
    fn render(&self, latex: &str) -> Option<Arc<Formula>> {
        render(latex)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_paths_and_inline_metrics() {
        let formula = render(r"\frac{a_1}{\sqrt{b}} + x^2").expect("valid formula");
        let svg = std::str::from_utf8(&formula.svg).expect("UTF-8 SVG");
        assert!(svg.contains("<path"));
        assert!(!svg.contains("<text"));
        assert!(formula.width > 0. && formula.ascent > 0. && formula.descent > 0.);
        assert!(Arc::ptr_eq(
            &formula,
            &render(r"\frac{a_1}{\sqrt{b}} + x^2").expect("cached")
        ));
        assert!(render(r"\unsupportedcommand{x}").is_none());
        assert!(render(&"{".repeat(100)).is_none());
    }

    #[test]
    fn formulary_svg_metrics_and_rasters_match_at_text_and_zoom_sizes() {
        for latex in [
            r"x_i^2 + a_{n+1}",
            r"\frac{-b \pm \sqrt{b^2-4ac}}{2a}",
            r"\sqrt[3]{\frac{a}{b}}",
            r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
            r"\left(\frac{a}{b}\right)^2",
        ] {
            let formula = render(latex).expect("valid formula");
            let tree =
                resvg::usvg::Tree::from_data(&formula.svg, &Default::default()).expect("SVG");
            assert!((tree.size().width() / SVG_EM - formula.width).abs() < 0.00001);
            assert!(
                (tree.size().height() / SVG_EM - formula.ascent - formula.descent).abs() < 0.00001
            );
            // The outer translation is the baseline, including our padding.
            let svg = std::str::from_utf8(&formula.svg).expect("SVG UTF-8");
            let baseline = svg
                .split("<g transform=\"translate(2 ")
                .nth(1)
                .and_then(|part| part.split(')').next())
                .and_then(|value| value.parse::<f32>().ok())
                .expect("SVG baseline");
            assert!((baseline / SVG_EM - formula.ascent).abs() < 0.00001);
            // Script scale must not round to zero as it could at a 1-unit em.
            assert!(!svg.contains("scale(0 "));
            for size in [18., 36.] {
                for density in [1., 2.] {
                    let scale = size * density / SVG_EM;
                    let mut pixels = resvg::tiny_skia::Pixmap::new(
                        (formula.width * size * density).ceil() as u32,
                        ((formula.ascent + formula.descent) * size * density).ceil() as u32,
                    )
                    .expect("pixels");
                    resvg::render(
                        &tree,
                        resvg::tiny_skia::Transform::from_scale(scale, scale),
                        &mut pixels.as_mut(),
                    );
                    assert!(pixels.pixels().iter().any(|pixel| pixel.alpha() > 0));
                }
            }
        }
    }

    #[test]
    fn missing_glyphs_unsupported_variants_and_invalid_input_keep_source() {
        for latex in [
            r"\frac{\text{距离}}{t}",
            r"x_{\text{中文}}",
            r"\begin{matrix} a & \text{中文} \end{matrix}",
            r"\mathcal{F}",
            r"\unsupportedcommand{x}",
            r"\frac{1}{",
            "a\nb",
            "a\rb",
        ] {
            assert!(render(latex).is_none(), "{latex}");
        }
        assert!(render(&"x".repeat(MAX_BYTES + 1)).is_none());
        let face = ttf_parser::Face::parse(FONT, 0).expect("font");
        assert!(supported_text("f\u{2061}x", &face));
        assert!(!supported_text("\u{fe00}", &face));
        assert!(render(r"\mathbb{R} + \mathbf{A}").is_some());
    }
}
