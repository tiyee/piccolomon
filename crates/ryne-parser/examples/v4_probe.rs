use ryne_parser::{backend_html, partition, preview_inline, serialize, Edit, Source};
use serde::Deserialize;
use std::{
    hint::black_box,
    io::{self, Read},
    time::Instant,
};

#[derive(Deserialize)]
struct Fixture {
    example: usize,
    markdown: String,
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("fixtures") => {
            let mut input = String::new();
            io::stdin().read_to_string(&mut input).unwrap();
            let fixtures: Vec<Fixture> = serde_json::from_str(&input).unwrap();
            let output: Vec<_> = fixtures.iter().map(|f| {
                let tokens = partition(&f.markdown);
                assert_eq!(serialize(&f.markdown, &tokens), f.markdown);
                serde_json::json!({"example": f.example, "html": backend_html(&f.markdown), "coverage": true})
            }).collect();
            println!("{}", serde_json::to_string(&output).unwrap());
        }
        Some("bench") => {
            let mut results = Vec::new();
            for bytes in [10 * 1024, 100 * 1024, 1024 * 1024] {
                for shape in ["paragraphs", "long_inline"] {
                    let unit = if shape == "paragraphs" {
                        "中文 **bold** &amp; [link](/url)\n\n"
                    } else {
                        "中文 **bold** &amp; [link](/url) "
                    };
                    let source = Source::new(&unit.repeat(bytes / unit.len() + 1));
                    let mut times = Vec::new();
                    // Warm-up is excluded. 100 measured iterations, no GUI/layout.
                    for iteration in 0..110 {
                        let start = Instant::now();
                        let a = source
                            .apply(
                                source.id(),
                                vec![Edit {
                                    range: 0..0,
                                    replacement: "x".into(),
                                }],
                            )
                            .unwrap();
                        let tokens = partition(a.source.text());
                        let projection = preview_inline(a.source.text());
                        black_box((&tokens, &projection));
                        if iteration >= 10 {
                            times.push(start.elapsed().as_secs_f64() * 1000.0);
                        }
                    }
                    times.sort_by(f64::total_cmp);
                    results.push(serde_json::json!({"bytes": source.text().len(), "shape": shape,
                        "iterations": times.len(), "p50_ms": times[49], "p95_ms": times[94], "max_ms": times[99]}));
                }
            }
            println!("{}", serde_json::to_string_pretty(&results).unwrap());
        }
        _ => panic!("usage: cargo run --example v4_probe -- fixtures|bench"),
    }
}
