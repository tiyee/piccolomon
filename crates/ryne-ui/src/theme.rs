//! Shared typography and page geometry. Values use GPUI logical pixels.
use gpui::{font, Font, FontFallbacks, TextSystem};

pub const BODY: (f32, f32) = (18., 30.);
pub const CODE: (f32, f32) = (15., 24.);
pub const TABLE: (f32, f32) = (17., 28.);
pub const CARET_WIDTH: f32 = 2.;
pub const TOP: f32 = 36.;
pub const BOTTOM: f32 = 48.;
pub const PAPER: u32 = 0xfafaf8;
pub const INK: u32 = 0x30343b;
pub const HEADING_INK: u32 = 0x20242b;
pub const MUTED: u32 = 0x727780;
pub const MARKER: u32 = 0x818791;
pub const LINK: u32 = 0x306fa6;
pub const CODE_BG: u32 = 0xf0f1ee;
pub const BORDER: u32 = 0xe1e4de;

pub fn heading(level: u8) -> (f32, f32) {
    match level {
        1 => (32., 44.),
        2 => (26., 38.),
        3 => (22., 34.),
        4 => (20., 32.),
        5 => (18., 30.),
        6 => (17., 28.),
        _ => BODY,
    }
}

pub struct Fonts {
    pub body: Font,
    pub code: Font,
}

impl Fonts {
    pub fn new(system: &TextSystem) -> Self {
        let names = system.all_font_names();
        let available = |name: &str| names.iter().any(|n| n == name);
        let mut body = font(if available("Helvetica Neue") {
            "Helvetica Neue"
        } else {
            ".SystemUIFont"
        });
        let mut code = font(if available("Menlo") {
            "Menlo"
        } else {
            ".SystemUIFont"
        });
        if available("PingFang SC") {
            let fallback = FontFallbacks::from_fonts(vec!["PingFang SC".into()]);
            body.fallbacks = Some(fallback.clone());
            code.fallbacks = Some(fallback);
        }
        Self { body, code }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Page {
    pub left: f32,
    pub width: f32,
    pub top: f32,
}

impl Page {
    pub fn new(viewport: f32, source: bool) -> Self {
        let viewport = viewport.max(1.);
        let margin = if source {
            24_f32.min(viewport / 4.)
        } else if viewport >= 600. {
            64.
        } else {
            52_f32.min(viewport / 4.)
        };
        let width = (viewport - margin * 2.).max(1.);
        let width = if source { width } else { width.min(760.) };
        Self {
            left: (viewport - width) / 2.,
            width,
            top: TOP,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_column_is_centered_and_source_uses_viewport() {
        for viewport in [1., 100., 320., 640., 960., 1440.] {
            let page = Page::new(viewport, false);
            assert!(page.width > 0. && page.width <= viewport.min(760.));
            assert_eq!(page.left * 2. + page.width, viewport);
        }
        assert_eq!(Page::new(1440., false).width, 760.);
        assert_eq!(Page::new(1440., true).width, 1392.);
    }
}
