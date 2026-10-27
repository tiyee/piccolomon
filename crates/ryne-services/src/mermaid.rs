//! Bounded, disposable Mermaid views; the document owns every editable byte.
use merman::svg::{RenderResourcePolicy, ResourceLimitId, SvgPipeline};
use merman::{
    Engine, MermaidConfig, OperationControl, ParseOptions, RenderOutput, RenderTarget, Renderer,
    SvgEnvironment, SvgRequest,
};
pub use ryne_projection::Diagram;
#[cfg(test)]
use ryne_syntax::code_block::CodeBlock;
#[cfg(test)]
use std::collections::HashMap;
use std::{
    ops::Range,
    sync::{Arc, LazyLock},
};

#[cfg(test)]
pub const MAX_BLOCKS: usize = 32;
const MAX_SOURCE: usize = 32 * 1024;
const MAX_PIXELS: u64 = 2_000_000;
#[cfg(test)]
const MAX_CACHE_BYTES: usize = 32 * 1024 * 1024;
pub type Outcome = Result<Arc<Diagram>, String>;
pub type View = (Range<usize>, Outcome);

#[cfg(test)]
#[derive(Default)]
pub struct Cache {
    entries: HashMap<String, Outcome>,
}
#[cfg(test)]
impl Cache {
    pub fn get(&self, source: &str) -> Option<Outcome> {
        self.entries.get(source).cloned()
    }
    pub fn retain(&mut self, sources: &[String]) {
        self.entries.retain(|key, _| sources.contains(key));
    }
    pub fn insert(&mut self, source: String, mut result: Outcome) {
        let used: usize = self
            .entries
            .values()
            .filter_map(|v| v.as_ref().ok())
            .map(|d| d.bytes)
            .sum();
        if result
            .as_ref()
            .is_ok_and(|d| used + d.bytes > MAX_CACHE_BYTES)
        {
            result = Err("图表缓存已达上限，保留源码".into());
        }
        if self.entries.len() < MAX_BLOCKS || self.entries.contains_key(&source) {
            self.entries.insert(source, result);
        }
    }
}

/// The Markdown AST and its exact code rows decide the input, including containers.
#[cfg(test)]
pub fn source(text: &str, block: &CodeBlock) -> Option<Result<String, String>> {
    if block.language.split_whitespace().next() != Some("mermaid") || block.closing.is_none() {
        return None;
    }
    let len = block
        .rows
        .iter()
        .map(|row| row.content.len() + 1)
        .sum::<usize>();
    if len > MAX_SOURCE {
        return Some(Err("图表超过 32 KiB，保留源码".into()));
    }
    Some(Ok(block
        .rows
        .iter()
        .map(|row| &text[row.content.clone()])
        .collect::<Vec<_>>()
        .join("\n")))
}

pub use ryne_extensions::CancelControl as Control;
pub fn control() -> Control {
    Control::default()
}

pub fn render(source: &str, control: Control) -> Outcome {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        render_inner(source, control)
    }))
    .unwrap_or_else(|_| Err("图表渲染失败，保留源码".into()))
}

fn render_inner(source: &str, control: Control) -> Outcome {
    if source.len() > MAX_SOURCE {
        return Err("图表超过 32 KiB，保留源码".into());
    }
    control.checkpoint().map_err(str::to_owned)?;
    // Bound recursive input before parsing, and semantic work before layout.
    let mut nesting = 0usize;
    for ch in source.chars() {
        match ch {
            '[' | '{' | '(' => {
                nesting += 1;
                if nesting > 32 {
                    return Err("图表嵌套超限，保留源码".into());
                }
            }
            ']' | '}' | ')' => nesting = nesting.saturating_sub(1),
            _ => {}
        }
    }
    if source.lines().count() > 256 {
        return Err("图表超过 256 行，保留源码".into());
    }
    let mut resources = RenderResourcePolicy::interactive();
    for (id, limit) in [
        (ResourceLimitId::MaxSourceBytes, MAX_SOURCE),
        (ResourceLimitId::MaxModelItems, 2048),
        (ResourceLimitId::MaxModelNestingDepth, 32),
        (ResourceLimitId::MaxModelTextBytes, MAX_SOURCE * 4),
        (ResourceLimitId::MaxSvgBytes, 2 * 1024 * 1024),
    ] {
        resources
            .apply_limit(id, limit)
            .map_err(|e| e.to_string())?;
    }
    // Select the former presentation choices without compiling ELK or a second math engine.
    let renderer = Renderer::new()
        .with_engine(Engine::new().with_site_config(MermaidConfig::from_value(
            serde_json::json!({ "layout": "dagre", "theme": "default", "look": "classic" }),
        )))
        .with_parse_options(ParseOptions::strict())
        .with_resource_policy(*resources.input_policy());
    let operation =
        OperationControl::new().with_deadline(control.remaining().map_err(str::to_owned)?);
    let cancelled_operation = operation.clone();
    let _cancellation = control.on_cancel(move || cancelled_operation.cancel());
    let parsed = renderer
        .prepare_semantic(source, operation)
        .map_err(|e| e.to_string())?
        .ok_or("未识别图表，保留源码")?;
    let model = parsed.compatibility_json().map_err(|e| e.to_string())?;
    let mut pending = vec![(model.as_value(), 0usize)];
    let mut items = 0usize;
    let mut text_bytes = 0usize;
    while let Some((value, depth)) = pending.pop() {
        items += 1;
        if items > 2048 || depth > 32 {
            return Err("图表模型复杂度超限，保留源码".into());
        }
        if let Some(array) = value.as_array() {
            pending.extend(array.iter().map(|v| (v, depth + 1)));
        }
        if let Some(object) = value.as_object() {
            pending.extend(object.values().map(|v| (v, depth + 1)));
        }
        if let Some(text) = value.as_str() {
            text_bytes += text.len();
        }
        if text_bytes > MAX_SOURCE * 4 {
            return Err("图表标签超限，保留源码".into());
        }
    }
    control.checkpoint().map_err(str::to_owned)?;
    let request = SvgRequest {
        environment: SvgEnvironment::deterministic().with_resource_policy(resources),
        pipeline: Some(SvgPipeline::resvg_safe()),
        ..Default::default()
    };
    let output = parsed
        .render(RenderTarget::Svg(request))
        .map_err(|e| e.to_string())?;
    let RenderOutput::Svg(Some(svg)) = output else {
        return Err("未生成图表，保留源码".into());
    };
    let (svg, _) = svg.into_parts();
    if svg.len() > 2 * 1024 * 1024 {
        return Err("图表 SVG 超限，保留源码".into());
    }
    control.checkpoint().map_err(|e| e.to_string())?;
    static FONTS: LazyLock<Arc<resvg::usvg::fontdb::Database>> = LazyLock::new(|| {
        let mut fonts = resvg::usvg::fontdb::Database::new();
        fonts.load_system_fonts();
        Arc::new(fonts)
    });
    let mut options = resvg::usvg::Options {
        fontdb: FONTS.clone(),
        ..Default::default()
    };
    // Diagrams never load resources from the document's directory or network.
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    let tree = resvg::usvg::Tree::from_str(&svg, &options).map_err(|e| e.to_string())?;
    let width = tree.size().width();
    let height = tree.size().height();
    if ![width, height]
        .iter()
        .all(|v| v.is_finite() && *v > 0. && *v <= 4096.)
    {
        return Err("图表尺寸超限，保留源码".into());
    }
    // 2x raster for Retina; never allocate unbounded image surfaces.
    let w = (width * 2.).ceil() as u32;
    let h = (height * 2.).ceil() as u32;
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err("图表像素面积超限，保留源码".into());
    }
    let mut pixels = resvg::tiny_skia::Pixmap::new(w, h).ok_or("图表图像分配失败")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(2., 2.),
        &mut pixels.as_mut(),
    );
    control.checkpoint().map_err(|e| e.to_string())?;
    let bytes = pixels
        .pixels()
        .iter()
        .flat_map(|p| {
            let p = p.demultiply();
            [p.blue(), p.green(), p.red(), p.alpha()]
        })
        .collect::<Vec<_>>();
    Ok(Arc::new(Diagram {
        pixels: bytes.into(),
        pixel_width: w,
        pixel_height: h,
        width,
        height,
        bytes: (w as usize) * (h as usize) * 4,
    }))
}

#[derive(Default)]
pub struct MermanRenderer;
impl ryne_extensions::DiagramRenderer for MermanRenderer {
    fn render(&self, source: &str, control: Control) -> Outcome {
        render(source, control)
    }
    fn generation(&self) -> u64 {
        902
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use editable_markdown_parser::Source;
    use ryne_syntax::ParseSnapshot;
    #[test]
    fn extracts_only_closed_mermaid_fences_and_preserves_container_bytes() {
        for eol in ["\n", "\r\n", "\r"] {
            let raw = [
                "- item",
                "",
                "  > ~~~mermaid",
                "  > flowchart LR",
                "  > A[中文] --> B",
                "  > ~~~",
                "",
            ]
            .join(eol);
            let parse = ParseSnapshot::new(Source::new(&raw));
            let blocks = ryne_syntax::code_block::blocks(&parse);
            assert_eq!(
                source(&raw, &blocks[0]),
                Some(Ok("flowchart LR\nA[中文] --> B".into()))
            );
            assert_eq!(parse.source().text(), raw);
        }
        for raw in [
            "```rust\nflowchart LR\nA-->B\n```\n",
            "```mermaid\nflowchart LR\nA-->B\n",
            "    flowchart LR\n    A-->B\n",
        ] {
            let parse = ParseSnapshot::new(Source::new(raw));
            assert!(ryne_syntax::code_block::blocks(&parse)
                .iter()
                .all(|b| source(raw, b).is_none()));
        }
    }
    #[test]
    fn renders_color_unicode_and_errors_without_panicking() {
        let diagram = render(
            "flowchart LR\nA[中文] --> B\nstyle A fill:#ff0000",
            control(),
        )
        .expect("valid diagram");
        assert!(diagram.width > 0. && diagram.height > 0.);
        assert!(diagram
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[2] > 240 && p[0] < 20 && p[1] < 20));
        assert!(render("flowchart LR\nA[Unclosed --> B", control()).is_err());
        assert!(render("unknown\nA-->B", control()).is_err());
        assert!(render(&"x".repeat(MAX_SOURCE + 1), control()).is_err());
        let cancelled = control();
        cancelled.cancel();
        assert!(render("flowchart LR\nA-->B", cancelled).is_err());
        assert!(render(
            "flowchart LR\nA-->B",
            Control::with_timeout(std::time::Duration::ZERO)
        )
        .is_err());
    }
    #[test]
    fn basic_diagram_families_render_through_the_pinned_backend() {
        for source in [
            "sequenceDiagram\nAlice->>Bob: Hello\nBob-->>Alice: OK",
            "classDiagram\nAnimal <|-- Duck\nAnimal : +int age\nDuck : +swim()",
            "stateDiagram-v2\n[*] --> Editing\nEditing --> Saved: save\nSaved --> [*]",
            "erDiagram\nUSER ||--o{ NOTE : owns\nUSER {\n int id PK\n string name\n}",
            "gantt\ndateFormat YYYY-MM-DD\nsection Work\nPlan :a, 2026-10-01, 2d\nBuild :after a, 3d",
            "mindmap\nroot((Editor))\n Markdown\n  Source\n  Preview\n Save",
            "flowchart TB\nsubgraph Outer[Outer]\nsubgraph Inner[Inner]\nA[One] --> B[Two]\nend\nB --> C[Three]\nend",
            "flowchart LR\nA[\"first<br/>second\"] --> B[Done]",
        ] {
            assert!(render(source, control()).is_ok(), "{source}");
        }
        assert!(render(
            &format!("flowchart LR\n{}", "subgraph X\n".repeat(300)),
            control()
        )
        .is_err());
        assert!(render(
            &format!("flowchart LR\nA{}x{}", "[".repeat(40), "]".repeat(40)),
            control()
        )
        .is_err());
    }

    #[test]
    fn cache_is_content_addressed_and_bounded() {
        let mut cache = Cache::default();
        for i in 0..MAX_BLOCKS + 10 {
            cache.insert(i.to_string(), Err("invalid".into()));
        }
        assert_eq!(cache.entries.len(), MAX_BLOCKS);
        cache.retain(&["2".into()]);
        assert!(cache.get("2").is_some());
        assert!(cache.get("1").is_none());
    }
}
