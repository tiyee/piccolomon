//! Application appearance, shared colors, typography and page geometry. Values use GPUI logical pixels.
use gpui::{font, Font, FontFallbacks, TextSystem};

pub const BODY: (f32, f32) = (18., 30.);
pub const CODE: (f32, f32) = (15., 24.);
pub const TABLE: (f32, f32) = (17., 28.);
pub const CARET_WIDTH: f32 = 2.;
pub const TOP: f32 = 36.;
pub const BOTTOM: f32 = 48.;
pub const IMAGE_FAILURE_INSET: f32 = 32.;

/// Color-only appearance changes never enter the document or projection state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub paper: u32,
    pub ink: u32,
    pub heading_ink: u32,
    pub muted: u32,
    pub marker: u32,
    pub image_error: u32,
    pub link: u32,
    pub code_bg: u32,
    pub quote_bg: u32,
    pub border: u32,
    pub hover: u32,
    pub selected: u32,
    pub popup: u32,
    pub table_header: u32,
    pub table_cell: u32,
    pub selection: u32,
}
impl Palette {
    pub const LIGHT: Self = Self {
        paper: 0xfafaf8,
        ink: 0x30343b,
        heading_ink: 0x20242b,
        muted: 0x727780,
        marker: 0x818791,
        image_error: 0xc62828,
        link: 0x306fa6,
        code_bg: 0xf0f1ee,
        quote_bg: 0xeff2f4,
        border: 0xe1e4de,
        hover: 0xecebe7,
        selected: 0xe5edf8,
        popup: 0xfdfcf9,
        table_header: 0xeeefed,
        table_cell: 0xffffff,
        selection: 0x9abfe966,
    };
    pub const DARK: Self = Self {
        paper: 0x202226,
        ink: 0xd9dde3,
        heading_ink: 0xf1f3f5,
        muted: 0x9da5b0,
        marker: 0x9099a6,
        image_error: 0xff8585,
        link: 0x82b7ef,
        code_bg: 0x282c32,
        quote_bg: 0x293039,
        border: 0x3d434c,
        hover: 0x363c45,
        selected: 0x334b68,
        popup: 0x292d33,
        table_header: 0x30363e,
        table_cell: 0x24272c,
        selection: 0x6ca7ed66,
    };
    pub fn syntax(self, class: ryne_projection::Class) -> u32 {
        use ryne_projection::Class::*;
        if self == Self::LIGHT {
            return class.color();
        }
        match class {
            Comment => self.muted,
            Keyword => 0xdba0e8,
            String => 0xa4cf89,
            Number | Constant => 0xe5ba80,
            Function => 0x85bfee,
            Type => 0xe9c98b,
            Property => 0xef9ca4,
            Variable => self.ink,
            Operator | Punctuation => 0xb8c1cc,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    System,
    Dark,
    Light,
}
impl Mode {
    pub const ALL: [Self; 3] = [Self::System, Self::Dark, Self::Light];
    pub fn id(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
    pub fn label(self) -> super::Text {
        match self {
            Self::System => super::Text::ThemeSystem,
            Self::Dark => super::Text::Dark,
            Self::Light => super::Text::Light,
        }
    }
    pub fn palette(self, system: gpui::WindowAppearance) -> Palette {
        let dark = match self {
            Self::System => matches!(
                system,
                gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark
            ),
            Self::Dark => true,
            Self::Light => false,
        };
        if dark {
            Palette::DARK
        } else {
            Palette::LIGHT
        }
    }
}

/// One app-level preference shared by document chrome, popups and filter inputs.
/// GPUI globals are scoped to an App, including each isolated test application.
pub struct Settings {
    pub mode: Mode,
    pub system: gpui::WindowAppearance,
    storage: Option<std::path::PathBuf>,
}
impl gpui::Global for Settings {}
impl Settings {
    pub fn load(storage: Option<std::path::PathBuf>, system: gpui::WindowAppearance) -> Self {
        let mode = storage
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|value| Mode::ALL.into_iter().find(|mode| mode.id() == value.trim()))
            .unwrap_or_default();
        Self {
            mode,
            system,
            storage,
        }
    }
    pub fn select(&mut self, mode: Mode) -> std::io::Result<()> {
        self.mode = mode;
        let path = self.storage.as_ref().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "No configuration directory")
        })?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let result =
            std::fs::write(&temporary, mode.id()).and_then(|()| std::fs::rename(&temporary, path));
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result
    }
}
pub fn palette(cx: &gpui::App) -> Palette {
    cx.try_global::<Settings>().map_or_else(
        || Mode::System.palette(cx.window_appearance()),
        |settings| settings.mode.palette(settings.system),
    )
}
pub fn selected(cx: &gpui::App) -> Mode {
    cx.try_global::<Settings>()
        .map_or(Mode::System, |settings| settings.mode)
}

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

#[derive(Clone, PartialEq)]
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
    fn theme_preference_defaults_persists_and_resolves_all_system_appearances() {
        use gpui::WindowAppearance::*;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config/theme");
        let mut settings = Settings::load(Some(path.clone()), Dark);
        assert_eq!(settings.mode, Mode::System);
        for mode in Mode::ALL {
            settings.select(mode).unwrap();
            assert_eq!(Settings::load(Some(path.clone()), Light).mode, mode);
            for appearance in [Light, VibrantLight, Dark, VibrantDark] {
                let expected = match mode {
                    Mode::Dark => Palette::DARK,
                    Mode::Light => Palette::LIGHT,
                    Mode::System if matches!(appearance, Dark | VibrantDark) => Palette::DARK,
                    Mode::System => Palette::LIGHT,
                };
                assert_eq!(mode.palette(appearance), expected);
            }
        }
        std::fs::write(&path, "invalid").unwrap();
        assert_eq!(Settings::load(Some(path), Dark).mode, Mode::System);
        let mut unavailable = Settings::load(None, Light);
        assert!(unavailable.select(Mode::Dark).is_err());
        assert_eq!(unavailable.mode, Mode::Dark);
    }

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
