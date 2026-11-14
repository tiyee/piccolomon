//! Differential checks use a real GPUI text system and return failures normally.
use super::*;

pub(super) fn sample() -> String {
    (0..100).map(|i| format!("paragraph {i} 中😀 é words\n\n")).collect::<String>()
        + "> - nested **bold**\n>   continuation\n>\n>   ```rust\n>   let x = 1;\n>   ```\n\n| left | right |\n| --- | ---: |\n| $x^2$ | text<br>more |\n"
}

pub(super) fn verify(
    editor: &Editor,
    geometry: &Geometry,
    window: &mut Window,
) -> Result<(), String> {
    let before = (
        editor.document.source.id(),
        editor.document.selection(),
        editor.document.history_len(false),
    );
    editor.viewport.inline_layouts.borrow_mut().bypass = true;
    let reference =
        text_layout::TextLayout::build(editor, geometry.text_layout.key.clone(), window);
    editor.viewport.inline_layouts.borrow_mut().bypass = false;
    let reference = reference?;
    let cached = &geometry.text_layout;
    if cached.lines.len() != reference.lines.len()
        || cached.rows.len() != reference.rows.len()
        || cached.height != reference.height
        || cached.cells != reference.cells
    {
        return Err("block layout cache differs from full layout geometry".into());
    }
    for (a, b) in cached.lines.iter().zip(reference.lines.iter()) {
        if a.start != b.start
            || a.top != b.top
            || a.line_height != b.line_height
            || a.inset != b.inset
            || a.cell != b.cell
            || a.align != b.align
            || a.width != b.width
            || a.code != b.code
            || a.diagram != b.diagram
            || a.shaped.wrap_boundaries != b.shaped.wrap_boundaries
            || format!("{:?}", a.shaped.unwrapped_layout)
                != format!("{:?}", b.shaped.unwrapped_layout)
        {
            return Err(format!(
                "block layout cache glyph/offset difference at {}",
                a.start
            ));
        }
        match (&a.mixed, &b.mixed) {
            (None, None) => {}
            (Some(a), Some(b))
                if a.width == b.width
                    && a.baseline == b.baseline
                    && a.font_size == b.font_size
                    && a.pieces.len() == b.pieces.len() =>
            {
                for (a, b) in a.pieces.iter().zip(&b.pieces) {
                    if a.range != b.range || a.x != b.x || a.width != b.width {
                        return Err("cached formula placement differs".into());
                    }
                }
            }
            _ => return Err("cached formula metrics differ".into()),
        }
    }
    for (a, b) in cached.rows.iter().zip(reference.rows.iter()) {
        if a.range != b.range
            || a.top != b.top
            || a.start_x != b.start_x
            || a.align_offset != b.align_offset
        {
            return Err("cached visual row differs".into());
        }
    }
    if before
        != (
            editor.document.source.id(),
            editor.document.selection(),
            editor.document.history_len(false),
        )
    {
        return Err("layout differential mutated document".into());
    }
    Ok(())
}
