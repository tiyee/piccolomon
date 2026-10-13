//! Rasterize the app's SVG logo into the standard macOS iconset sizes.
use std::{error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let source = PathBuf::from(args.next().ok_or("Usage: app-icon <logo.svg> <iconset>")?);
    let output = PathBuf::from(args.next().ok_or("Usage: app-icon <logo.svg> <iconset>")?);
    if args.next().is_some() {
        return Err("Usage: app-icon <logo.svg> <iconset>".into());
    }

    let source = source.canonicalize()?;
    let options = resvg::usvg::Options {
        resources_dir: source.parent().map(ToOwned::to_owned),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_data(&std::fs::read(source)?, &options)?;
    std::fs::create_dir_all(&output)?;
    for logical_size in [16, 32, 128, 256, 512] {
        for density in [1, 2] {
            let size = logical_size * density;
            let scale = (size as f32 / tree.size().width()).min(size as f32 / tree.size().height());
            let transform = resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(
                (size as f32 - tree.size().width() * scale) / 2.,
                (size as f32 - tree.size().height() * scale) / 2.,
            );
            let mut pixels = resvg::tiny_skia::Pixmap::new(size, size)
                .ok_or("Unable to allocate app icon pixels")?;
            resvg::render(&tree, transform, &mut pixels.as_mut());
            let suffix = if density == 2 { "@2x" } else { "" };
            pixels
                .save_png(output.join(format!("icon_{logical_size}x{logical_size}{suffix}.png")))?;
        }
    }
    Ok(())
}
