//! Application preference, deliberately separate from Markdown and its history.
use ryne_extensions::MathFont;
use std::{io, path::PathBuf};

pub(super) struct MathFontPreference {
    pub font: MathFont,
    storage: Option<PathBuf>,
}
impl MathFontPreference {
    pub fn load(storage: Option<PathBuf>) -> Self {
        let font = storage
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|value| MathFont::parse(value.trim()))
            .unwrap_or_default();
        Self { font, storage }
    }
    pub fn select(&mut self, font: MathFont) -> io::Result<()> {
        self.font = font;
        let Some(path) = &self.storage else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "No configuration directory",
            ));
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let result =
            std::fs::write(&temporary, font.id()).and_then(|()| std::fs::rename(&temporary, path));
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_invalid_and_persisted_preferences() {
        let path = std::env::temp_dir().join(format!("ryne-math-font-test-{}", std::process::id()));
        let mut preference = MathFontPreference::load(Some(path.clone()));
        assert_eq!(preference.font, MathFont::LeteSans);
        for font in MathFont::ALL {
            preference.select(font).unwrap();
            assert_eq!(MathFontPreference::load(Some(path.clone())).font, font);
        }
        std::fs::write(&path, "unknown font").unwrap();
        assert_eq!(
            MathFontPreference::load(Some(path.clone())).font,
            MathFont::LeteSans
        );
        std::fs::remove_file(&path).unwrap();
        let mut unavailable = MathFontPreference::load(None);
        assert!(unavailable.select(MathFont::StixTwo).is_err());
        assert_eq!(unavailable.font, MathFont::StixTwo);
    }
}
