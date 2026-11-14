//! Real-window block formula geometry checks; failures return through the smoke runner.
use super::*;
pub(super) const SAMPLE: &str = "before 中😀\n\n$$\n\\sum_{i=1}^{n} i\n$$\n\n> $$\n> \\frac{a}{b}\n> $$\n\n- $$\n  \\sqrt{x}\n  $$\n\nafter";
pub(super) fn verify(
    editor: &Editor,
    geometry: &Geometry,
    window: &mut Window,
) -> Result<(), String> {
    if geometry.display.formulas.len() != 3 || geometry.math_error.is_some() {
        return Err("block math projection/painting failed".into());
    }
    let f = &geometry.display.formulas[0];
    let row = geometry
        .row_for(f.display.start, Affinity::Downstream)
        .ok_or("no formula row")?;
    let line = &geometry.lines[row.line];
    let mixed = line.mixed.as_ref().ok_or("missing block formula layout")?;
    let right = line.inset + mixed.width;
    if (line.inset - (px(geometry.page.width) - right)).abs() > px(0.5)
        || row.line_height < px((f.formula.ascent + f.formula.descent) * mixed.font_size)
    {
        return Err("block formula is not centered or is clipped".into());
    }
    for f in &geometry.display.formulas {
        if !f.block {
            return Err("inline formula used for display math".into());
        }
        let a = geometry.position_with_affinity(f.display.start, Affinity::Downstream);
        let b = geometry.position_with_affinity(f.display.end, Affinity::Upstream);
        let at = point(
            (a.x + b.x) / 2.,
            a.y + geometry.line_height(f.display.start) / 2.,
        );
        if geometry.formula_at_point(at).map(|v| &v.source) != Some(&f.source) {
            return Err("block formula hit map mismatch".into());
        }
    }
    let target =
        block::outermost_at(&editor.document.parse, f.source.start).ok_or("no formula block")?;
    if ryne_extensions::BlockMetadata::from(target.kind).icon != Some("math_format_professional") {
        return Err("wrong block formula icon".into());
    }
    let paragraph =
        block::outermost_at(&editor.document.parse, 0).ok_or("no preceding paragraph")?;
    let paragraph_origin = geometry
        .block_origin(&paragraph)
        .ok_or("no paragraph control origin")?;
    let formula_origin = geometry
        .block_origin(&target)
        .ok_or("no formula control origin")?;
    if (formula_origin.x - paragraph_origin.x).abs() > px(0.5) {
        return Err("block formula control does not align with the content gutter".into());
    }
    for width in [1., 60., 240.] {
        let mut page = geometry.page;
        page.width = width;
        let key = text_layout::LayoutKey::new(
            geometry.text_layout.key.colors,
            editor,
            page,
            window,
            &editor.viewport.control_index.cells,
        );
        let layout = text_layout::TextLayout::build(editor, key, window)?;
        for formula in &geometry.display.formulas {
            let line = layout
                .lines
                .iter()
                .find(|line| line.start == formula.display.start)
                .ok_or("missing narrow formula line")?;
            let mixed = line
                .mixed
                .as_ref()
                .ok_or("missing narrow formula geometry")?;
            if line.inset < px(0.) || line.inset + mixed.width > px(width + 0.5) {
                return Err("narrow block formula exceeds its container".into());
            }
        }
    }
    if editor.document.dirty() || editor.document.history_len(false) != 0 {
        return Err("formula presentation changed source/history".into());
    }
    Ok(())
}
