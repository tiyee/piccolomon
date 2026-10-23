use std::time::Instant;
pub const SAMPLES: usize = 96;
pub fn corpus() -> String {
    "# 标题\n\n中文 e\u{301} 😀 **bold** and `literal` [link](https://example.com).\n\n- first\n  - nested\n\n| 名称 | value |\n| --- | ---: |\n| 中 | **a** |\n\n```rust\nfn main() { println!(\"hello\"); }\n```\n\n$x^2$\n\n".repeat(8)
}
pub fn measure(label: &str, mut operation: impl FnMut()) {
    measure_samples(label, SAMPLES, 8, &mut operation);
}
pub fn measure_samples(label: &str, samples: usize, warmup: usize, mut operation: impl FnMut()) {
    for _ in 0..warmup {
        operation();
    }
    let mut times = (0..samples)
        .map(|_| {
            let start = Instant::now();
            operation();
            start.elapsed().as_secs_f64() * 1000.
        })
        .collect::<Vec<_>>();
    times.sort_by(f64::total_cmp);
    println!(
        "{{\"metric\":\"{label}\",\"samples\":{samples},\"p50_ms\":{:.4},\"p95_ms\":{:.4},\"p99_ms\":{:.4}}}",
        times[samples / 2],
        times[samples * 95 / 100],
        times[samples * 99 / 100]
    );
}
