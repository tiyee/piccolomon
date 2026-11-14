//! Export backend registration and storage ports; no platform or encoder types.
use crate::PdfExporter;
use std::{path::Path, sync::Arc};

/// Writes a complete artifact using same-directory atomic replacement.
pub trait ArtifactStore: Send + Sync {
    fn write(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()>;
}

#[derive(Clone)]
pub struct ExportDescriptor {
    pub id: &'static str,
    pub label: &'static str,
    pub fallback: &'static str,
    pub file_suffix: &'static str,
    pub exporter: Arc<dyn PdfExporter>,
}
impl ExportDescriptor {
    pub fn file_name(&self, stem: &str) -> String {
        format!("{stem}{}.pdf", self.file_suffix)
    }
}

/// Validated at startup, immutable while exports are running.
#[derive(Clone)]
pub struct ExportRegistry {
    formats: Vec<ExportDescriptor>,
}
impl ExportRegistry {
    pub fn new(formats: Vec<ExportDescriptor>) -> Result<Self, String> {
        let mut ids = std::collections::BTreeSet::new();
        for format in &formats {
            if format.id.is_empty() || !ids.insert(format.id) {
                return Err("empty or duplicate export backend ID".into());
            }
            if format.file_suffix.contains(['/', '\\', '\0']) {
                return Err("invalid export filename suffix".into());
            }
        }
        Ok(Self { formats })
    }
    pub fn formats(&self) -> &[ExportDescriptor] {
        &self.formats
    }
    pub fn get(&self, id: &str) -> Option<&ExportDescriptor> {
        self.formats.iter().find(|format| format.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake;
    impl PdfExporter for Fake {
        fn export(&self, _: &ryne_projection::Display) -> Result<Vec<u8>, String> {
            Ok(b"fake backend".to_vec())
        }
    }
    #[test]
    fn registration_dispatches_and_preserves_naming() {
        let format = ExportDescriptor {
            id: "custom",
            label: "export.custom",
            fallback: "Custom PDF",
            file_suffix: "-custom",
            exporter: Arc::new(Fake),
        };
        assert!(ExportRegistry::new(vec![format.clone(), format.clone()]).is_err());
        let registry = ExportRegistry::new(vec![format]).unwrap();
        assert!(registry.get("missing").is_none());
        let selected = registry.get("custom").unwrap();
        assert_eq!(selected.file_name("报告"), "报告-custom.pdf");
        assert_eq!(
            selected.exporter.export(&Default::default()).unwrap(),
            b"fake backend"
        );
    }
}
