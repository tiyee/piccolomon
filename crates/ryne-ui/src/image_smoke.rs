//! Deterministic resource fixture; real image loading and decoding have service-level tests.
use super::*;
use std::sync::Arc;
struct SmokeImages;
pub fn smoke_image_renderer() -> Option<Arc<dyn ryne_extensions::DiagramRenderer>> {
    if std::env::var_os("RYNE_SMOKE_IMAGES_URL").is_some() {
        return None;
    }
    (std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--smoke-test")))
        .then(|| Arc::new(SmokeImages) as Arc<dyn ryne_extensions::DiagramRenderer>)
}
impl ryne_extensions::DiagramRenderer for SmokeImages {
    fn generation(&self) -> u64 {
        0
    }
    fn render(
        &self,
        source: &str,
        _: ryne_extensions::CancelControl,
    ) -> ryne_projection::DiagramOutcome {
        if source != "https://images.test/example.png" {
            return Err("fixture image unavailable".into());
        }
        Ok(Arc::new(ryne_projection::Diagram {
            pixels: [180, 120, 40, 255].repeat(120 * 70).into(),
            pixel_width: 120,
            pixel_height: 70,
            width: 1200.,
            height: 700.,
            bytes: 120 * 70 * 4,
        }))
    }
}
fn sample() -> String {
    let url = std::env::var("RYNE_SMOKE_IMAGES_URL")
        .unwrap_or_else(|_| "https://images.test/example.png".into());
    let prefix = std::env::var("RYNE_SMOKE_IMAGE_PREFIX").unwrap_or_default();
    format!("before😀\r\n\r\n{prefix}![图片说明]({url})\r\n\r\nafter\r\n")
}
pub(super) fn step(
    stage: u16,
    editor: &mut Editor,
    geometry: &Geometry,
    window: &mut Window,
    cx: &mut Context<Editor>,
) -> Result<(), String> {
    match stage {
        373 => {
            editor.document = editor.document.fork(&sample());
            editor.viewport.scroll = px(0.);
            editor.move_to(0, false, cx);
        }
        374 => {
            if geometry.display.diagrams.len() != 1 || geometry.diagrams_painted != 1 {
                return Err("image preview was not painted".into());
            }
            let view = &geometry.display.diagrams[0];
            let line = geometry
                .lines
                .iter()
                .find(|l| l.diagram == Some(0))
                .ok_or("missing image line")?;
            if f32::from(line.inset + line.width) > geometry.page.width + 0.01
                || (f32::from(line.width) / f32::from(line.line_height - px(24.))
                    - view.diagram.width / view.diagram.height)
                    .abs()
                    > 0.01
            {
                return Err(
                    "image did not fit the content column with its original aspect ratio".into(),
                );
            }
            verify_widths(editor, window)?;
            let position = geometry.position(view.display.start)
                + point(line.width / 2., line.line_height / 2.);
            editor.mouse_down(
                &MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    click_count: 1,
                    ..Default::default()
                },
                window,
                cx,
            );
            editor.mouse_up(
                &MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    ..Default::default()
                },
                window,
                cx,
            );
            if editor.document.head != view.source.anchor {
                return Err("image click did not enter its source".into());
            }
        }
        375 => {
            if !geometry.display.diagrams.is_empty()
                || !geometry.display.text.contains("![图片说明]")
            {
                return Err("image source did not expand".into());
            }
            editor.move_to(0, false, cx);
        }
        376 => {
            if geometry.display.diagrams.len() != 1 {
                return Err("image did not refold".into());
            }
            editor.toggle_source(&ToggleSource, window, cx);
        }
        377 => {
            if !geometry.display.diagrams.is_empty()
                || editor.document.source.text() != sample()
                || editor.document.dirty()
                || editor.document.history_len(false) != 0
            {
                return Err("image preview changed source/history or ignored source mode".into());
            }
            println!("Image GUI checks passed: actual paint, proportional fit, click-to-source, refold and full source mode; CRLF source and history unchanged");
        }
        378 => {
            // Oversized display labels still retain the complete editable address.
            editor.document = editor.document.fork(&failure_sample());
            editor.viewport.scroll = px(0.);
            editor.move_to(0, false, cx);
        }
        379 | 381 => {
            let view = geometry
                .display
                .image_failures
                .first()
                .ok_or("missing failed image preview")?;
            let expected = format!(
                "{}…",
                failure_address().chars().take(256).collect::<String>()
            );
            if geometry.display.image_failures.len() != 1
                || geometry.failed_images_painted != 1
                || geometry.display.text[view.display.clone()] != expected
                || geometry.display.text.contains("![failed]")
                || !geometry
                    .display
                    .spans
                    .iter()
                    .any(|s| s.display == view.display && s.style.image_failure && s.style.strike)
                || editor.document.source.text() != failure_sample()
                || editor.document.dirty()
                || editor.document.history_len(false) != 0
            {
                return Err("failed image must show one icon and a red address limited to 256 characters plus ellipsis".into());
            }
            let row = geometry
                .row_for(view.display.start, Affinity::Downstream)
                .ok_or("missing failure row")?;
            let line = &geometry.lines[row.line];
            if line.inset < px(theme::IMAGE_FAILURE_INSET) {
                return Err("failed image icon has no space before its address".into());
            }
            let icon_left =
                geometry.position(view.display.start).x - px(theme::IMAGE_FAILURE_INSET);
            for (range, marker) in &geometry.list_markers {
                if range.start < view.source.range.end
                    && view.source.range.start < range.end
                    && marker.right() > icon_left - px(4.)
                {
                    return Err("failed image overlaps its list marker".into());
                }
            }
            let position = if stage == 379 {
                // The last wrapped address row must open the original source too.
                let last = geometry
                    .lines
                    .iter()
                    .rfind(|l| view.display.contains(&l.start))
                    .ok_or("missing failure address line")?;
                geometry.position(last.start) + point(px(10.), last.line_height / 2.)
            } else {
                geometry.position(view.display.start)
                    + point(
                        -px(theme::IMAGE_FAILURE_INSET) + px(12.),
                        row.line_height / 2.,
                    )
            };
            click(editor, position, window, cx);
            if editor.document.head != view.source.anchor {
                return Err("clicking failed image address/icon did not enter its source".into());
            }
        }
        380 | 382 => {
            if !geometry.display.image_failures.is_empty()
                || geometry.failed_images_painted != 0
                || !geometry.display.text.contains(&failure_address())
            {
                return Err("failed image did not reveal its complete Markdown source".into());
            }
            editor.move_to(0, false, cx);
            if stage == 382 {
                editor.toggle_source(&ToggleSource, window, cx);
            }
        }
        383 => {
            if !geometry.display.image_failures.is_empty()
                || geometry.failed_images_painted != 0
                || editor.document.source.text() != failure_sample()
                || editor.document.history_len(false) != 0
            {
                return Err(
                    "failed image preview changed source/history or ignored full source mode"
                        .into(),
                );
            }
            editor.toggle_source(&ToggleSource, window, cx);
        }
        384 => {
            if geometry.display.image_failures.len() != 1 || geometry.failed_images_painted != 1 {
                return Err("failed image preview did not return after leaving source mode".into());
            }
            println!("Failed-image icon GUI checks passed: inline SVG and red struck-through truncated address, wrapped address/icon click-to-source, complete source, refold and source-mode suppression without history changes");
        }
        _ => (),
    }
    Ok(())
}

fn failure_address() -> String {
    format!("data:invalid{}", "x".repeat(300))
}
fn failure_sample() -> String {
    let prefix = std::env::var("RYNE_SMOKE_FAILED_IMAGE_PREFIX").unwrap_or_default();
    format!(
        "before😀\r\n\r\n{prefix}![failed]({})\r\n\r\nafter",
        failure_address()
    )
}
fn click(
    editor: &mut Editor,
    position: Point<Pixels>,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    editor.mouse_down(
        &MouseDownEvent {
            button: MouseButton::Left,
            position,
            click_count: 1,
            ..Default::default()
        },
        window,
        cx,
    );
    editor.mouse_up(
        &MouseUpEvent {
            button: MouseButton::Left,
            position,
            ..Default::default()
        },
        window,
        cx,
    );
}

fn verify_widths(editor: &Editor, window: &mut Window) -> Result<(), String> {
    let source = editor.document.source.id();
    let history = editor.document.history_len(false);
    for width in [1., 16., 39., 40., 120., 320., 760.] {
        let page = theme::Page {
            left: 0.,
            width,
            top: theme::TOP,
        };
        let key = text_layout::LayoutKey::new(editor, page, window, &Default::default());
        let layout = text_layout::TextLayout::build(editor, key, window)?;
        let line = layout
            .lines
            .iter()
            .find(|l| l.diagram == Some(0))
            .ok_or("missing fitted image")?;
        let image = &layout.display.diagrams[0].diagram;
        let image_height = line.line_height - px(24.);
        let right = (width
            - if layout.display.quote_at(line.start).is_some() {
                12.
            } else {
                0.
            })
        .max(1.);
        if line.inset < px(0.)
            || line.width <= px(0.)
            || image_height <= px(0.)
            || line.inset + line.width > px(right + 0.001)
            || line.width > px(image.width)
            || (f32::from(image_height) - image.height * f32::from(line.width) / image.width).abs()
                > 0.001
        {
            return Err(format!("image exceeds content width or changes aspect ratio: column={width}, inset={:?}, image={:?}", line.inset, line.width));
        }
    }
    if editor.document.source.id() != source
        || editor.document.history_len(false) != history
        || editor.document.dirty()
    {
        return Err("fitting image widths modified source/history".into());
    }
    println!("Image width checks passed: 1/16/39/40/120/320/760 px columns, container inset, proportional downscaling and original small-image size");
    Ok(())
}
