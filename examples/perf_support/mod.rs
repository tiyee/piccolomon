use std::time::Instant;
pub const SAMPLES: usize = 96;
pub fn corpus() -> String {
    "# 标题\n\n中文 e\u{301} 😀 **bold** and `literal` [link](https://example.com).\n\n- first\n  - nested\n\n| 名称 | value |\n| --- | ---: |\n| 中 | **a** |\n\n```rust\nfn main() { println!(\"hello\"); }\n```\n\n$x^2$\n\n".repeat(8)
}
pub fn measure(label: &str, mut operation: impl FnMut()) {
    for _ in 0..8 {
        operation();
    }
    let mut times = (0..SAMPLES)
        .map(|_| {
            let start = Instant::now();
            operation();
            start.elapsed().as_secs_f64() * 1000.
        })
        .collect::<Vec<_>>();
    times.sort_by(f64::total_cmp);
    println!(
        "{{\"metric\":\"{label}\",\"samples\":{SAMPLES},\"p50_ms\":{:.4},\"p95_ms\":{:.4},\"p99_ms\":{:.4}}}",
        times[SAMPLES / 2],
        times[SAMPLES * 95 / 100],
        times[SAMPLES * 99 / 100]
    );
}
