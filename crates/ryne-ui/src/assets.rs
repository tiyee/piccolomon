use gpui::{AssetSource, SharedString};
use std::borrow::Cow;
pub struct Assets {
    pub formulas: std::sync::Arc<dyn ryne_extensions::FormulaRenderer>,
}
impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        if let Some(name) = path.strip_prefix("icon:") {
            let bytes = match name {
                "checkbox_checked" => {
                    &include_bytes!("../../../assets/icons/checkbox_checked.svg")[..]
                }
                "checkbox_unchecked" => {
                    &include_bytes!("../../../assets/icons/checkbox_unchecked.svg")[..]
                }
                "code" => &include_bytes!("../../../assets/icons/code.svg")[..],
                "format" => &include_bytes!("../../../assets/icons/format.svg")[..],
                "eye" => &include_bytes!("../../../assets/icons/eye.svg")[..],
                "divider" => &include_bytes!("../../../assets/icons/divider.svg")[..],
                "image" => &include_bytes!("../../../assets/icons/image.svg")[..],
                "mermaid" => &include_bytes!("../../../assets/icons/mermaid.svg")[..],
                "table" => &include_bytes!("../../../assets/icons/table.svg")[..],
                "more_vertical" => &include_bytes!("../../../assets/icons/more_vertical.svg")[..],
                "delete" => &include_bytes!("../../../assets/icons/delete.svg")[..],
                "table_add" => &include_bytes!("../../../assets/icons/table_add.svg")[..],
                "add_square" => &include_bytes!("../../../assets/icons/add_square.svg")[..],
                "arrow_turn_right" => {
                    &include_bytes!("../../../assets/icons/arrow_turn_right.svg")[..]
                }
                "arrow_move" => &include_bytes!("../../../assets/icons/arrow_move.svg")[..],
                "checkmark_square" => {
                    &include_bytes!("../../../assets/icons/checkmark_square.svg")[..]
                }
                "task_list_ltr" => &include_bytes!("../../../assets/icons/task_list_ltr.svg")[..],
                "text_bullet_list_ltr" => {
                    &include_bytes!("../../../assets/icons/text_bullet_list_ltr.svg")[..]
                }
                "text_number_list_ltr" => {
                    &include_bytes!("../../../assets/icons/text_number_list_ltr.svg")[..]
                }
                "text_t" => &include_bytes!("../../../assets/icons/text_t.svg")[..],
                "text_quote" => &include_bytes!("../../../assets/icons/text_quote.svg")[..],
                "text_header_1" => &include_bytes!("../../../assets/icons/text_header_1.svg")[..],
                "text_header_2" => &include_bytes!("../../../assets/icons/text_header_2.svg")[..],
                "text_header_3" => &include_bytes!("../../../assets/icons/text_header_3.svg")[..],
                "text_header_4" => &include_bytes!("../../../assets/icons/text_header_4.svg")[..],
                "text_header_5" => &include_bytes!("../../../assets/icons/text_header_5.svg")[..],
                "text_header_6" => &include_bytes!("../../../assets/icons/text_header_6.svg")[..],
                _ => return Ok(None),
            };
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        Ok(path
            .starts_with("math:")
            .then(|| self.formulas.render_asset(path))
            .flatten()
            .map(|formula| Cow::Owned(formula.svg.clone())))
    }
    fn list(&self, _: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

pub(crate) fn diagram_images(
    display: &ryne_projection::Display,
    previous: &Option<super::Geometry>,
) -> Vec<std::sync::Arc<gpui::RenderImage>> {
    display
        .diagrams
        .iter()
        .map(|view| {
            if let Some(old) = previous
                && let Some(index) = old
                    .display
                    .diagrams
                    .iter()
                    .position(|v| std::sync::Arc::ptr_eq(&v.diagram, &view.diagram))
            {
                return old.diagram_images[index].clone();
            }
            let diagram = &view.diagram;
            // Only services validated dimensions and pixel count reach this bridge.
            let buffer = image::RgbaImage::from_raw(
                diagram.pixel_width,
                diagram.pixel_height,
                diagram.pixels.to_vec(),
            )
            .unwrap_or_default();
            std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(buffer)]))
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use editable_markdown_parser::{ParseSnapshot, Source};
    struct NoFormulas;
    impl ryne_extensions::FormulaRenderer for NoFormulas {
        fn render(&self, _: &str) -> Option<std::sync::Arc<ryne_projection::Formula>> {
            None
        }
    }
    include!("icon_tests.rs");
}
