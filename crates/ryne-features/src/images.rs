use ryne_extensions::*;
use ryne_projection::PreviewSource;
use std::sync::Arc;

#[derive(Clone)]
pub struct ImagePreview(pub Arc<dyn DiagramRenderer>);
impl BuiltinFeature for ImagePreview {
    fn manifest(&self) -> FeatureManifest {
        FeatureManifest::builtin("image")
    }
    fn register(&self, registrar: &mut Registrar) -> Result<(), String> {
        registrar.preview(
            Contribution {
                id: "image.preview",
                slot: Slot::Renderer("image"),
                exclusive: true,
                priority: 0,
            },
            Arc::new(self.clone()),
        )
    }
}
impl PreviewProvider for ImagePreview {
    fn source_cache_generation(&self) -> Option<u64> {
        Some(0)
    }
    fn document_inputs(&self, state: &EditingState) -> Option<Vec<PreviewInput>> {
        let mut address_bytes = 0;
        Some(
            ryne_syntax::image::blocks(&state.parse)
                .iter()
                .take(32)
                .map(|image| PreviewInput {
                    fold_errors: true,
                    feature: "image.preview",
                    block: source(image),
                    input: image::validate_image_url(&image.destination).and_then(|()| {
                        // Embedded addresses also occupy preview keys and candidate caches.
                        address_bytes += image.destination.len();
                        if address_bytes > 32 * 1024 * 1024 {
                            Err("Image addresses exceed 32 MiB per document".into())
                        } else {
                            Ok(image.destination.clone())
                        }
                    }),
                    renderer: self.0.clone(),
                })
                .collect(),
        )
    }
}
pub(crate) fn source(image: &ryne_syntax::image::ImageBlock) -> PreviewSource {
    PreviewSource {
        range: image.range.clone(),
        scope: image.scope.clone(),
        anchor: image.anchor,
        indent: image.indent,
    }
}

/// Shorten only the displayed address. Never split a user-visible character.
pub(crate) fn failure_label(address: &str) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    let mut graphemes = address.graphemes(true);
    let mut label: String = graphemes.by_ref().take(256).collect();
    if graphemes.next().is_some() {
        label.push('…');
    }
    let label: String = label
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if label.is_empty() {
        "…".into()
    } else {
        label
    }
}
