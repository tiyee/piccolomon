//! Real-window alert geometry and native header hit regression.
use super::*;
pub(super) const SAMPLE: &str = "before\n\n> [!NOTE]\n> 这是备注内容，用于强调用户即使快速浏览也应该知道的信息。\n\n> [!TIP]\n> 这里是提示内容，提供有用的建议或技巧。\n\n> [!IMPORTANT]\n> 这里是重要内容，说明完成目标所需的关键信息。\n\n> [!WARNING]\n> 这里是警告内容，提醒用户立即注意，以避免出现问题。\n\n> [!CAUTION]\n> 这里是谨慎内容，说明某些操作可能带来的风险或负面后果。\n\n> ordinary quote\n\nafter";
pub(super) fn verify(
    editor: &Editor,
    geometry: &Geometry,
    window: &mut Window,
) -> Result<(), String> {
    for colors in [theme::Palette::LIGHT, theme::Palette::DARK] {
        let key = text_layout::LayoutKey::new(
            colors,
            editor,
            geometry.page,
            window,
            &editor.viewport.control_index.cells,
        );
        let layout = text_layout::TextLayout::build(editor, key, window)?;
        let panels: Vec<_> = layout
            .quote_panels
            .iter()
            .filter(|p| p.alert.is_some())
            .collect();
        if panels.len() != 5 {
            return Err("five alert panels were not built".into());
        }
        for (panel, kind) in panels.into_iter().zip(ryne_syntax::alert::Kind::ALL) {
            let alert = panel.alert.as_ref().ok_or("missing alert")?;
            if alert.source.kind != kind || !alert.folded || panel.title.is_none() {
                return Err("missing folded alert title".into());
            }
            let quote = layout
                .display
                .quotes
                .iter()
                .find(|q| q.source == alert.source.source)
                .ok_or("missing source quote")?;
            let body = layout
                .lines
                .iter()
                .find(|l| l.start > quote.display.start && l.start < quote.display.end)
                .ok_or("missing body line")?;
            if panel.header_bounds.bottom() > body.top + px(0.1)
                || panel.bounds.bottom() < body.top + body.line_height
                || panel.title_origin.y < panel.bounds.top()
            {
                return Err("alert header/body geometry overlaps or clips".into());
            }
            if ryne_projection::alert::background(
                kind,
                colors.paper,
                colors == theme::Palette::DARK,
            ) == colors.quote_bg
            {
                return Err("alert retained ordinary quote background".into());
            }
            let block = block::outermost_at(&editor.document.parse, alert.source.marker.start)
                .ok_or("missing alert block")?;
            if ryne_extensions::BlockMetadata::from(block.kind).icon != Some("alert") {
                return Err("wrong alert block icon".into());
            }
        }
        if layout
            .quote_panels
            .iter()
            .filter(|p| p.alert.is_none())
            .count()
            != 1
        {
            return Err("ordinary quote regression".into());
        }
    }
    if editor.document.source.text() != SAMPLE
        || editor.document.dirty()
        || editor.document.history_len(false) != 0
    {
        return Err("alert view changed source/history".into());
    }
    Ok(())
}
