use merman::{OperationControl, RenderOutput, RenderRequest, Renderer, SvgRequest};
use std::{error::Error, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let out = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .unwrap_or_else(|| "output".into()),
    );
    fs::create_dir_all(&out)?;
    let renderer = Renderer::new();
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let fixtures = [
        (
            "flow",
            "flowchart LR\n A[Start] --> B{Ready?}\n B -->|Yes| C[Done]\n B -->|No| A",
        ),
        (
            "chinese",
            "flowchart LR\n A[打开文件] --> B{需要保存？}\n B -->|是| C[写入磁盘]",
        ),
        (
            "nested",
            "flowchart TB\n subgraph Outer[Outer]\n subgraph Inner[Inner]\n A[One] --> B[Two]\n end\n B --> C[Three]\n end",
        ),
        (
            "styles",
            "flowchart LR\n A[Red] --> B[Blue]\n style A fill:#ff0000,color:#ffffff\n style B fill:#0000ff,color:#ffffff",
        ),
        (
            "sequence",
            "sequenceDiagram\n participant Alice\n participant Bob\n Alice->>Bob: Hello\n activate Bob\n alt success\n Bob-->>Alice: OK\n else failure\n Bob-->>Alice: Error\n end\n deactivate Bob",
        ),
        (
            "class",
            "classDiagram\n Animal <|-- Duck\n Animal : +int age\n Duck : +swim()",
        ),
        (
            "state",
            "stateDiagram-v2\n [*] --> Editing\n Editing --> Saved: save\n Saved --> [*]",
        ),
        (
            "er",
            "erDiagram\n USER ||--o{ NOTE : owns\n USER {\n int id PK\n string name\n }",
        ),
        (
            "gantt",
            "gantt\n dateFormat YYYY-MM-DD\n section Work\n Plan :a, 2026-10-01, 2d\n Build :after a, 3d",
        ),
        (
            "mindmap",
            "mindmap\n root((Editor))\n  Markdown\n   Source\n   Preview\n  Save",
        ),
        (
            "html",
            "flowchart LR\n A[\"first<br/>second\"] --> B[\"**bold**\"]",
        ),
        ("invalid", "flowchart LR\n A[Unclosed --> B"),
        ("unknown", "notADiagram\n A-->B"),
    ];
    println!("backend\tfixture\tresult\tdetail");
    for (name, source) in fixtures {
        for backend in ["mmdr", "rusty", "merman"] {
            let rendered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || -> Result<String, String> {
                    match backend {
                        "mmdr" => mermaid_rs_renderer::render(source).map_err(|e| e.to_string()),
                        "rusty" => rusty_mermaid::to_svg(source, &rusty_mermaid::Theme::default())
                            .map_err(|e| e.to_string()),
                        _ => {
                            let request = SvgRequest {
                                pipeline: Some(merman::svg::SvgPipeline::resvg_safe()),
                                ..Default::default()
                            };
                            match renderer
                                .render(RenderRequest::svg(
                                    source,
                                    OperationControl::new(),
                                    request,
                                ))
                                .map_err(|e| e.to_string())?
                            {
                                RenderOutput::Svg(Some(svg)) => Ok(svg.svg().to_owned()),
                                _ => Err("no SVG".into()),
                            }
                        }
                    }
                },
            ));
            let svg = match rendered {
                Ok(Ok(svg)) => svg,
                Ok(Err(error)) => {
                    println!(
                        "{backend}\t{name}\terror\t{}",
                        error.replace(['\n', '\r'], " ")
                    );
                    continue;
                }
                Err(_) => {
                    println!("{backend}\t{name}\tpanic\t");
                    continue;
                }
            };
            fs::write(out.join(format!("{backend}-{name}.svg")), &svg)?;
            let tree = match resvg::usvg::Tree::from_str(&svg, &options) {
                Ok(tree) => tree,
                Err(error) => {
                    println!("{backend}\t{name}\tbad-svg\t{error}");
                    continue;
                }
            };
            let size = tree.size();
            let scale = (1200. / size.width()).min(2.).min(1200. / size.height());
            let Some(mut pixels) = resvg::tiny_skia::Pixmap::new(
                (size.width() * scale).ceil() as u32,
                (size.height() * scale).ceil() as u32,
            ) else {
                println!("{backend}\t{name}\tbad-size\t{size:?}");
                continue;
            };
            resvg::render(
                &tree,
                resvg::tiny_skia::Transform::from_scale(scale, scale),
                &mut pixels.as_mut(),
            );
            let painted = pixels.pixels().iter().filter(|p| p.alpha() > 0).count();
            pixels.save_png(out.join(format!("{backend}-{name}.png")))?;
            println!(
                "{backend}\t{name}\tok\t{:.1}x{:.1}; painted={painted}",
                size.width(),
                size.height()
            );
        }
    }
    Ok(())
}
