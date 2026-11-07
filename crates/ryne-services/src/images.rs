//! Bounded image loading on the existing background preview queue.
mod ftp;
mod source;
mod svg;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use ryne_extensions::{CancelControl, DiagramRenderer};
use ryne_projection::{Diagram, DiagramOutcome};
use std::{io::Cursor, sync::Arc};

use ryne_extensions::image::MAX_IMAGE_BYTES as MAX_DOWNLOAD;
const MAX_PIXELS: u64 = 2_000_000;
const MAX_DECODE_PIXELS: u64 = 16_000_000;
#[derive(Default)]
pub struct ImageRenderer;
impl DiagramRenderer for ImageRenderer {
    fn generation(&self) -> u64 {
        0
    }
    fn snapshot(&self) -> Option<Arc<dyn DiagramRenderer>> {
        Some(Arc::new(Self))
    }
    fn render(&self, source: &str, control: CancelControl) -> DiagramOutcome {
        control.checkpoint()?;
        ryne_extensions::image::validate_image_url(source)?;
        let bytes = source::load(source, &control)?;
        control.checkpoint()?;
        decode(&bytes, &control)
    }
}

fn decode(bytes: &[u8], control: &CancelControl) -> DiagramOutcome {
    if bytes.len() > MAX_DOWNLOAD {
        return Err("Image exceeds 8 MiB".into());
    }
    control.checkpoint()?;
    if image::guess_format(bytes).is_err() {
        return svg::decode(bytes, control);
    }
    decode_raster(bytes, control)
}

fn decode_raster(bytes: &[u8], control: &CancelControl) -> DiagramOutcome {
    if bytes.len() > MAX_DOWNLOAD {
        return Err("Image exceeds 8 MiB".into());
    }
    control.checkpoint()?;
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Gif | ImageFormat::WebP)
    ) {
        return Err("Unsupported image format (PNG, JPEG, GIF, WebP and SVG are supported)".into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_DECODE_PIXELS {
        return Err("Image exceeds 16 million pixels".into());
    }
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    control.checkpoint()?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    image.apply_orientation(orientation);
    let (natural_width, natural_height) = (image.width(), image.height());
    let pixels = u64::from(natural_width) * u64::from(natural_height);
    if pixels > MAX_PIXELS {
        let scale = (MAX_PIXELS as f64 / pixels as f64).sqrt();
        image = image.thumbnail(
            (natural_width as f64 * scale).floor().max(1.) as u32,
            (natural_height as f64 * scale).floor().max(1.) as u32,
        );
    }
    let image = image.to_rgba8();
    let (width, height) = image.dimensions();
    let mut pixels = image.into_raw();
    // The shared preview resource is straight-alpha BGRA, as expected by GPUI.
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    control.checkpoint()?;
    ryne_extensions::validate_diagram(Arc::new(Diagram {
        bytes: pixels.len(),
        pixels: pixels.into(),
        pixel_width: width,
        pixel_height: height,
        width: natural_width as f32,
        height: natural_height as f32,
    }))
}

#[cfg(test)]
mod tests;
