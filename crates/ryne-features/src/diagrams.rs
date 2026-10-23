//! A same-kind fenced preview needs only a feature descriptor and a renderer port.
use ryne_extensions::*;
use ryne_syntax::code_block::CodeBlock;
use std::sync::Arc;
#[derive(Clone)]
pub struct FencedPreview {
    pub id: &'static str,
    pub language: &'static str,
    pub renderer: Arc<dyn DiagramRenderer>,
}
impl BuiltinFeature for FencedPreview {
    fn manifest(&self) -> FeatureManifest {
        let mut m = FeatureManifest::builtin(self.id);
        m.requires.push("code");
        m
    }
    fn register(&self, r: &mut Registrar) -> Result<(), String> {
        r.preview(
            Contribution {
                id: self.id,
                slot: Slot::Renderer(self.language),
                exclusive: true,
                priority: 0,
            },
            Arc::new(self.clone()),
        )
    }
}
impl PreviewProvider for FencedPreview {
    fn input(&self, state: &EditingState, block: &CodeBlock) -> Option<PreviewInput> {
        if block.language.split_whitespace().next() != Some(self.language)
            || block.closing.is_none()
        {
            return None;
        }
        let len = block
            .rows
            .iter()
            .map(|r| r.content.len() + 1)
            .sum::<usize>();
        let input = if len > 32 * 1024 {
            Err("图表超过 32 KiB，保留源码".into())
        } else {
            Ok(block
                .rows
                .iter()
                .map(|r| &state.source.text()[r.content.clone()])
                .collect::<Vec<_>>()
                .join("\n"))
        };
        Some(PreviewInput {
            feature: self.id,
            block: block.clone(),
            input,
            renderer: self.renderer.clone(),
        })
    }
}
