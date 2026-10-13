//! Historical RaTeX / Formulary comparison, independent of the editor backend.
use formulary::{ast::Node, MathFont};
use std::collections::BTreeSet;

pub const FONT: &[u8] =
    include_bytes!("../../../../assets/fonts/stix-two-math/STIXTwoMath-Regular.otf");
pub const SIZES: [f32; 2] = [18., 36.];
pub const FIXTURES: &[(&str, &str)] = &[
    ("simple", r"E = mc^2"),
    ("scripts", r"x_i^2 + a_{n+1} + e^{i\pi} = 0"),
    ("fraction", r"\frac{-b \pm \sqrt{b^2-4ac}}{2a}"),
    ("nested-fraction", r"\frac{1}{1+\frac{1}{x}}"),
    ("roots", r"\sqrt{x^2+y^2} + \sqrt[3]{\frac{a}{b}}"),
    (
        "integral",
        r"\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}",
    ),
    ("sum", r"\sum_{k=1}^{n} k = \frac{n(n+1)}{2}"),
    ("matrix", r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}"),
    (
        "delimiters",
        r"\left(\frac{a}{b}\right)^2 + \left[\sqrt{1+x}\right]",
    ),
    ("accents", r"\hat{x} + \vec{v} + \overline{AB}"),
    ("alphabets", r"\mathbb{R} + \mathbf{A} + \mathscr{F}"),
    ("mathcal", r"\mathcal{F}"),
    ("text", r"\text{speed} = \frac{d}{t}"),
    ("chinese", r"\text{速度} = \frac{\text{距离}}{\text{时间}}"),
    ("color", r"\color{red}{x} + y"),
    ("unsupported", r"\unsupportedcommand{x}"),
    ("incomplete", r"\frac{1}{"),
];

#[derive(Clone, Copy, Debug)]
pub enum Backend {
    Ratex,
    Formulary,
}

impl Backend {
    pub fn id(self) -> &'static str {
        match self {
            Self::Ratex => "ratex",
            Self::Formulary => "formulary",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ratex => "RaTeX 0.1.14 · KaTeX",
            Self::Formulary => "Formulary 0.3.0 · STIX Two Math",
        }
    }
}

#[derive(Debug)]
pub struct Rendered {
    pub svg: String,
    /// Viewport metrics in CSS px, including the renderer's own padding.
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
    pub warnings: Vec<String>,
    pub mathml: Option<String>,
}

pub fn render(backend: Backend, latex: &str, size: f32) -> Result<Rendered, String> {
    validate_input(latex, size)?;
    // This is a CLI, but report a dependency panic as a failed sample as well.
    std::panic::catch_unwind(|| match backend {
        Backend::Ratex => render_ratex(latex, size),
        Backend::Formulary => {
            let converter = math_core::LatexToMathML::new(math_core::MathCoreConfig::default())
                .map_err(|e| format!("converter configuration: {e:?}"))?;
            let mathml = converter
                .convert_with_local_counter(latex, math_core::MathDisplay::Inline)
                .map_err(|e| format!("LaTeX → MathML: {e}"))?;
            render_mathml(&mathml, size)
        }
    })
    .map_err(|_| "renderer panicked; source retained".to_owned())?
}

fn validate_input(latex: &str, size: f32) -> Result<(), String> {
    if !size.is_finite() || !(1. ..=72.).contains(&size) {
        return Err("font size must be finite and between 1 and 72 px".into());
    }
    if latex.len() > 4096 || latex.contains(['\r', '\n']) {
        return Err("inline input exceeds 4096 bytes or contains a newline".into());
    }
    let mut depth = 0usize;
    for ch in latex.chars() {
        match ch {
            '{' | '[' | '(' => {
                depth += 1;
                if depth > 64 {
                    return Err("input nesting exceeds 64".into());
                }
            }
            '}' | ']' | ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

fn validate_metrics(width: f32, ascent: f32, descent: f32, size: f32) -> Result<(), String> {
    if ![width, ascent, descent]
        .iter()
        .all(|v| v.is_finite() && *v >= 0.)
        || width <= 0.
        || ascent + descent <= 0.
        || width / size > 256.
        || (ascent + descent) / size > 128.
    {
        return Err("empty, non-finite or oversized formula layout".into());
    }
    Ok(())
}

fn render_ratex(latex: &str, size: f32) -> Result<Rendered, String> {
    // Mirror the former RaTeX renderer, including its em-space SVG and 0.04 stroke width.
    // Do not render at a larger font_size: doing so would change the comparison.
    const PAD: f64 = 0.06;
    let nodes = ratex_parser::parse(latex).map_err(|e| format!("RaTeX: {e}"))?;
    let box_ = ratex_layout::layout(
        &nodes,
        &ratex_layout::LayoutOptions {
            style: ratex_types::math_style::MathStyle::Text,
            ..Default::default()
        },
    );
    let list = ratex_layout::to_display_list(&box_);
    let width = (list.width + 2. * PAD) as f32 * size;
    let ascent = (list.height + PAD) as f32 * size;
    let descent = (list.depth + PAD) as f32 * size;
    validate_metrics(width, ascent, descent, size)?;
    let svg = ratex_svg::render_to_svg(
        &list,
        &ratex_svg::SvgOptions {
            font_size: 1.,
            padding: PAD,
            stroke_width: 0.04,
            embed_glyphs: true,
            ..Default::default()
        },
    );
    Ok(Rendered {
        svg,
        width,
        ascent,
        descent,
        warnings: Vec::new(),
        mathml: None,
    })
}

fn render_mathml(mathml: &str, size: f32) -> Result<Rendered, String> {
    let root = formulary::parse(mathml).map_err(|e| format!("MathML: {e}"))?;
    let font = MathFont::new(FONT, 0).map_err(|e| format!("math font: {e}"))?;
    let face = ttf_parser::Face::parse(FONT, 0).map_err(|e| format!("font cmap: {e}"))?;
    let mut missing = BTreeSet::new();
    let mut unsupported_variants = BTreeSet::new();
    for child in &root.children {
        inspect_tokens(child, &mut |text| {
            let mut base = None;
            for ch in text.chars().filter(|ch| !ch.is_whitespace()) {
                // Selectors are consumed by shaping, not standalone glyphs.
                if matches!(ch as u32, 0xFE00..=0xFE0F | 0xE0100..=0xE01EF) {
                    if let Some(base) = base
                        && face.glyph_variation_index(base, ch).is_none()
                    {
                        unsupported_variants.insert((base, ch));
                    }
                    continue;
                }
                // MathML's invisible function/application/separator operators.
                if matches!(ch as u32, 0x2061..=0x2064) {
                    continue;
                }
                if face.glyph_index(ch).is_none_or(|id| id.0 == 0) {
                    missing.insert(ch);
                }
                base = Some(ch);
            }
        });
    }
    // Formulary 0.3 skips unmapped glyphs before creating the display list.
    // Inspect the parsed, mathvariant-transformed tokens, not just Item::Glyph.
    if !missing.is_empty() {
        let detail = missing
            .iter()
            .map(|ch| format!("{ch} (U+{:04X})", *ch as u32))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "STIX Two Math missing glyphs: {detail}; source retained"
        ));
    }
    if root.has_arabic_text() {
        return Err("Arabic contextual shaping is unsupported; source retained".into());
    }
    let laid = formulary::layout(&root, &font, &formulary::LayoutOptions { font_size: size });
    // formulary::svg::to_svg includes a fixed 2 px margin on every side.
    let width = laid.width + 4.;
    let ascent = laid.ascent + 2.;
    let descent = laid.descent + 2.;
    validate_metrics(width, ascent, descent, size)?;
    if laid.items.is_empty() {
        return Err("formula has no drawable items".into());
    }
    Ok(Rendered {
        svg: formulary::svg::to_svg(&laid, &font),
        width,
        ascent,
        descent,
        warnings: root
            .warnings
            .iter()
            .map(|warning| format!("{warning:?}"))
            .chain(unsupported_variants.iter().map(|(base, selector)| {
                format!(
                    "font has no variation mapping for {base} U+{:04X}; default glyph may be used",
                    *selector as u32
                )
            }))
            .collect(),
        mathml: Some(mathml.to_owned()),
    })
}

fn inspect_tokens(node: &Node, visit: &mut impl FnMut(&str)) {
    match node {
        Node::Identifier(text)
        | Node::Number(text)
        | Node::Text(text)
        | Node::Operator { text, .. } => visit(text),
        Node::Row(children)
        | Node::Sqrt(children)
        | Node::Phantom(children)
        | Node::Styled { children, .. }
        | Node::Padded { children, .. } => {
            for child in children {
                inspect_tokens(child, visit);
            }
        }
        Node::Frac { num, den, .. } => {
            inspect_tokens(num, visit);
            inspect_tokens(den, visit);
        }
        Node::Root { base, index } => {
            inspect_tokens(base, visit);
            inspect_tokens(index, visit);
        }
        Node::Scripts { base, sub, sup }
        | Node::UnderOver {
            base,
            under: sub,
            over: sup,
            ..
        } => {
            inspect_tokens(base, visit);
            for child in [sub, sup].into_iter().flatten() {
                inspect_tokens(child, visit);
            }
        }
        Node::MultiScripts { base, post, pre } => {
            inspect_tokens(base, visit);
            for (sub, sup) in post.iter().chain(pre) {
                for child in [sub, sup].into_iter().flatten() {
                    inspect_tokens(child, visit);
                }
            }
        }
        Node::Table { rows, .. } => {
            for cell in rows.iter().flatten() {
                inspect_tokens(&cell.content, visit);
            }
        }
        Node::Space { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_backends_rasterize_structures_at_body_and_zoom_sizes() {
        for backend in [Backend::Ratex, Backend::Formulary] {
            for size in SIZES {
                for id in ["scripts", "fraction", "roots", "matrix", "delimiters"] {
                    let (_, source) = FIXTURES
                        .iter()
                        .find(|(name, _)| *name == id)
                        .expect("fixture");
                    let rendered = render(backend, source, size).expect("valid formula");
                    assert!(
                        rendered.warnings.is_empty(),
                        "{id}: {:?}",
                        rendered.warnings
                    );
                    assert!(rendered.svg.contains("<path"));
                    assert!(!rendered.svg.contains("<text"));
                    let tree = resvg::usvg::Tree::from_str(&rendered.svg, &Default::default())
                        .expect("SVG");
                    let scale = rendered.width / tree.size().width();
                    assert!(
                        (tree.size().height() * scale - rendered.ascent - rendered.descent).abs()
                            < 0.02
                    );
                    let mut pixels = resvg::tiny_skia::Pixmap::new(
                        rendered.width.ceil() as u32,
                        (rendered.ascent + rendered.descent).ceil() as u32,
                    )
                    .expect("pixels");
                    resvg::render(
                        &tree,
                        resvg::tiny_skia::Transform::from_scale(scale, scale),
                        &mut pixels.as_mut(),
                    );
                    assert!(
                        pixels.pixels().iter().any(|pixel| pixel.alpha() > 0),
                        "{id}"
                    );
                }
            }
        }
    }

    #[test]
    fn unmapped_text_in_nested_scripts_tables_and_variants_is_reported() {
        for source in [
            r"x_{\text{中文}}",
            r"\begin{matrix} a & \text{中文} \end{matrix}",
        ] {
            let error = render(Backend::Formulary, source, 18.).expect_err("missing CJK");
            assert!(error.contains("missing glyphs"), "{error}");
        }
        let rendered =
            render(Backend::Formulary, r"\mathbb{R} + x^2", 18.).expect("variant glyphs");
        assert!(rendered.warnings.is_empty());
    }

    #[test]
    fn unsupported_and_incomplete_latex_never_pass_as_success() {
        for backend in [Backend::Ratex, Backend::Formulary] {
            for source in [r"\unsupportedcommand{x}", r"\frac{1}{"] {
                assert!(
                    render(backend, source, 18.).is_err(),
                    "{backend:?}: {source}"
                );
            }
        }
        for source in [r"\mathcal{F}", r"\color{red}{x}"] {
            assert!(render(Backend::Formulary, source, 18.).is_ok(), "{source}");
            assert!(render(Backend::Ratex, source, 18.).is_ok());
        }
    }

    #[test]
    fn recovered_mathml_is_distinguished_from_full_support() {
        let rendered =
            render_mathml("<math><unknown><mi>x</mi></unknown></math>", 18.).expect("recovery");
        assert!(!rendered.warnings.is_empty());
    }

    #[test]
    fn variation_selectors_and_invisible_operators_are_not_missing_glyphs() {
        let rendered = render(Backend::Formulary, r"\mathcal{F}", 18.).expect("base glyph");
        assert!(rendered
            .warnings
            .iter()
            .any(|warning| warning.contains("variation mapping")));
        assert!(rendered
            .mathml
            .as_ref()
            .is_some_and(|mathml| mathml.contains('\u{fe00}')));
        let rendered = render_mathml("<math><mi>f</mi><mo>\u{2061}</mo><mi>x</mi></math>", 18.)
            .expect("invisible operator");
        assert!(rendered.warnings.is_empty());
    }

    #[test]
    fn input_and_layout_limits_are_checked() {
        for backend in [Backend::Ratex, Backend::Formulary] {
            for source in [
                "{".repeat(65),
                "x".repeat(4097),
                "x\ny".into(),
                "x\ry".into(),
            ] {
                assert!(render(backend, &source, 18.).is_err());
            }
            for size in [f32::NAN, f32::INFINITY, 0., -1., 73.] {
                assert!(render(backend, "x", size).is_err());
            }
        }
        assert!(validate_metrics(f32::NAN, 1., 1., 18.).is_err());
        assert!(validate_metrics(5000., 1., 1., 18.).is_err());
    }
}
