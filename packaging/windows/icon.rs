//! Build a multi-resolution Windows ICO from the shared SVG logo.
use std::error::Error;

const SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 128, 256];

pub fn from_svg(svg: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default())?;
    let mut images = Vec::with_capacity(SIZES.len());
    for size in SIZES {
        let scale = (size as f32 / tree.size().width()).min(size as f32 / tree.size().height());
        let transform = resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(
            (size as f32 - tree.size().width() * scale) / 2.,
            (size as f32 - tree.size().height() * scale) / 2.,
        );
        let mut pixels =
            resvg::tiny_skia::Pixmap::new(size, size).ok_or("Unable to allocate icon pixels")?;
        resvg::render(&tree, transform, &mut pixels.as_mut());
        images.push(pixels.encode_png()?);
    }

    // ICONDIR, followed by nine ICONDIRENTRY records and their PNG payloads.
    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&(SIZES.len() as u16).to_le_bytes());
    let mut offset = 6 + SIZES.len() * 16;
    for (size, png) in SIZES.into_iter().zip(&images) {
        // ICO encodes a 256-pixel dimension as zero.
        ico.extend_from_slice(&[size as u8, size as u8, 0, 0]);
        ico.extend_from_slice(&1_u16.to_le_bytes());
        ico.extend_from_slice(&32_u16.to_le_bytes());
        ico.extend_from_slice(&u32::try_from(png.len())?.to_le_bytes());
        ico.extend_from_slice(&u32::try_from(offset)?.to_le_bytes());
        offset += png.len();
    }
    for png in images {
        ico.extend_from_slice(&png);
    }
    Ok(ico)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_has_decodable_images_at_every_shell_size() -> Result<(), Box<dyn Error>> {
        let ico = from_svg(include_bytes!("../../assets/logo.svg"))?;
        assert_eq!(&ico[..6], &[0, 0, 1, 0, 9, 0]);
        let mut end = 6 + SIZES.len() * 16;
        for (index, size) in SIZES.into_iter().enumerate() {
            let entry = &ico[6 + index * 16..6 + (index + 1) * 16];
            assert_eq!(&entry[..8], &[size as u8, size as u8, 0, 0, 1, 0, 32, 0]);
            let length = u32::from_le_bytes(entry[8..12].try_into()?) as usize;
            let offset = u32::from_le_bytes(entry[12..16].try_into()?) as usize;
            assert_eq!(offset, end);
            end = offset + length;
            let pixels = resvg::tiny_skia::Pixmap::decode_png(&ico[offset..end])?;
            assert_eq!((pixels.width(), pixels.height()), (size, size));
            assert!(pixels.pixels().iter().any(|pixel| pixel.alpha() > 0));
        }
        assert_eq!(end, ico.len());
        Ok(())
    }

    #[test]
    fn invalid_svg_is_reported() {
        assert!(from_svg(b"not an SVG").is_err());
    }
}
