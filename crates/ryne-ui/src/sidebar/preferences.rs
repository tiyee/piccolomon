use std::{fs, io, path::PathBuf};
#[derive(Debug)]
pub(super) struct Preferences {
    pub visible: bool,
    pub width: f32,
    pub directory: Option<bool>,
    pub list: bool,
    pub path: Option<PathBuf>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            visible: false,
            width: 260.,
            directory: None,
            list: false,
            path: None,
        }
    }
}
impl Preferences {
    pub fn load(path: Option<PathBuf>) -> Self {
        let mut prefs = Self {
            path,
            ..Self::default()
        };
        if let Some(data) = prefs
            .path
            .as_ref()
            .and_then(|p| fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        {
            prefs.visible = data["visible"].as_bool().unwrap_or(false);
            prefs.width = data["width"]
                .as_f64()
                .filter(|w| (200.0..=360.0).contains(w))
                .unwrap_or(260.) as f32;
            prefs.directory = data["directory"].as_bool();
            prefs.list = data["list"].as_bool().unwrap_or(false);
        }
        prefs
    }
    pub fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?
        }
        let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
        let data = serde_json::json!({"visible":self.visible,"width":self.width,"directory":self.directory,"list":self.list});
        let written = fs::write(&tmp, data.to_string()).and_then(|_| fs::rename(&tmp, path));
        if written.is_err() {
            let _ = fs::remove_file(tmp);
        }
        written
    }
}
