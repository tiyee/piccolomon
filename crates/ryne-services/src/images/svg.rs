use super::*;
use resvg::{tiny_skia, usvg};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const MAX_SVG_BYTES: usize = 2 * 1024 * 1024;

pub(super) fn decode(bytes: &[u8], control: &CancelControl) -> DiagramOutcome {
    if bytes.len() > MAX_SVG_BYTES {
        return Err("SVG exceeds 2 MiB".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "Unsupported image format")?;
    let xml = usvg::roxmltree::Document::parse_with_options(
        text,
        usvg::roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 20_000,
        },
    )
    .map_err(|e| format!("Invalid SVG: {e}"))?;
    if xml.root_element().tag_name().name() != "svg" {
        return Err("Image is not an SVG".into());
    }
    for node in xml.descendants() {
        if node.ancestors().take(66).count() > 64 {
            return Err("SVG nesting exceeds 64 levels".into());
        }
        // usvg resolves internal fragments. External use documents are not loaded.
        if node.has_tag_name("use")
            && node
                .attributes()
                .any(|a| a.name() == "href" && !a.value().starts_with('#'))
        {
            return Err("SVG requires an external resource".into());
        }
    }
    let failed_resource = AtomicBool::new(false);
    let embedded_bytes = AtomicUsize::new(0);
    let mut options = usvg::Options::default();
    if xml.descendants().any(|n| n.has_tag_name("text")) {
        options.fontdb = crate::print_layout::FONTS.clone();
    }
    options.image_href_resolver = usvg::ImageHrefResolver {
        resolve_string: Box::new(|_, _| {
            failed_resource.store(true, Ordering::Relaxed);
            None
        }),
        resolve_data: Box::new(|_, bytes, _| {
            // Normalize embedded raster images through the same bounded decoder.
            // Nested SVG/SVGZ images deliberately do not recurse through usvg.
            let image = decode_raster(&bytes, control).ok().and_then(|image| {
                if embedded_bytes.fetch_add(image.bytes, Ordering::Relaxed) + image.bytes
                    > MAX_DOWNLOAD
                {
                    return None;
                }
                let mut rgba = image.pixels.to_vec();
                for p in rgba.as_chunks_mut::<4>().0 {
                    p.swap(0, 2);
                }
                let image =
                    image::RgbaImage::from_raw(image.pixel_width, image.pixel_height, rgba)?;
                let mut png = Cursor::new(Vec::new());
                image.write_to(&mut png, ImageFormat::Png).ok()?;
                Some(usvg::ImageKind::PNG(Arc::new(png.into_inner())))
            });
            if image.is_none() {
                failed_resource.store(true, Ordering::Relaxed);
            }
            image
        }),
    };
    control.checkpoint()?;
    let tree = usvg::Tree::from_xmltree(&xml, &options).map_err(|e| e.to_string())?;
    if failed_resource.load(Ordering::Relaxed) {
        return Err("SVG contains an unavailable or unsupported image resource".into());
    }
    let (width, height) = (tree.size().width(), tree.size().height());
    if width > 4096. || height > 4096. || width * height > MAX_DECODE_PIXELS as f32 {
        return Err("SVG dimensions exceed image limits".into());
    }
    // Two pixels per display unit when possible, with the same cache area limit.
    let scale = (MAX_PIXELS as f32 / (width * height)).sqrt().min(2.);
    let w = (width * scale).floor().max(1.) as u32;
    let h = (height * scale).floor().max(1.) as u32;
    validate_tree(tree.root(), &mut 0, &mut 0, 0, scale)?;
    control.checkpoint()?;
    let mut pixels = tiny_skia::Pixmap::new(w, h).ok_or("SVG image allocation failed")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(w as f32 / width, h as f32 / height),
        &mut pixels.as_mut(),
    );
    control.checkpoint()?;
    let pixels: Vec<_> = pixels
        .pixels()
        .iter()
        .flat_map(|p| {
            let p = p.demultiply();
            [p.blue(), p.green(), p.red(), p.alpha()]
        })
        .collect();
    ryne_extensions::validate_diagram(Arc::new(Diagram {
        bytes: pixels.len(),
        pixels: pixels.into(),
        pixel_width: w,
        pixel_height: h,
        width,
        height,
    }))
}

fn validate_tree(
    group: &usvg::Group,
    nodes: &mut usize,
    segments: &mut usize,
    depth: usize,
    scale: f32,
) -> Result<(), String> {
    if depth > 64 {
        return Err("Expanded SVG nesting exceeds 64 levels".into());
    }
    let bounds = group.abs_layer_bounding_box();
    if group.should_isolate()
        && bounds.width() * bounds.height() * scale * scale > MAX_DECODE_PIXELS as f32
    {
        return Err("SVG layer exceeds 16 million pixels".into());
    }
    for node in group.children() {
        *nodes += 1;
        if *nodes > 20_000 {
            return Err("Expanded SVG exceeds 20000 nodes".into());
        }
        match node {
            usvg::Node::Group(g) => validate_tree(g, nodes, segments, depth + 1, scale)?,
            usvg::Node::Path(p) => {
                *segments += p.data().segments().count();
                if *segments > 200_000 {
                    return Err("SVG paths exceed 200000 segments".into());
                }
            }
            _ => (),
        }
        let mut result = Ok(());
        node.subroots(|g| {
            if result.is_ok() {
                result = validate_tree(g, nodes, segments, depth + 1, scale);
            }
        });
        result?;
    }
    Ok(())
}
