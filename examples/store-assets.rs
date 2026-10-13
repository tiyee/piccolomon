//! Generate MSIX manifest logos from the app's existing SVG artwork.
use std::{error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let source = PathBuf::from(
        args.next()
            .ok_or("Usage: store-assets <logo.svg> <output>")?,
    );
    let output = PathBuf::from(
        args.next()
            .ok_or("Usage: store-assets <logo.svg> <output>")?,
    );
    if args.next().is_some() {
        return Err("Usage: store-assets <logo.svg> <output>".into());
    }
    let source = source.canonicalize()?;
    let options = resvg::usvg::Options {
        resources_dir: source.parent().map(ToOwned::to_owned),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_data(&std::fs::read(source)?, &options)?;
    std::fs::create_dir_all(&output)?;
    for (name, size) in [
        ("StoreLogo", 50),
        ("Square44x44Logo", 44),
        ("Square150x150Logo", 150),
    ] {
        let scale = (size as f32 / tree.size().width()).min(size as f32 / tree.size().height());
        let transform = resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(
            (size as f32 - tree.size().width() * scale) / 2.,
            (size as f32 - tree.size().height() * scale) / 2.,
        );
        let mut pixels = resvg::tiny_skia::Pixmap::new(size, size)
            .ok_or("Unable to allocate Store logo pixels")?;
        resvg::render(&tree, transform, &mut pixels.as_mut());
        pixels.save_png(output.join(format!("{name}.png")))?;
    }
    Ok(())
}
