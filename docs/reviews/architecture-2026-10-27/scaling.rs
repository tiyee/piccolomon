use ryne_session::{EditorSession, SessionServices};
use std::{hint::black_box, sync::Arc, time::Instant};
fn main() {
    let kinds = if std::env::var_os("RYNE_SECOND_MIXED_SCALING").is_some() { vec!["interleaved"] } else { vec!["plain","inline-code"] };
    for kind in kinds {
        for n in [256, 512, 1024, 2048] {
            let unit = if kind == "plain" {
                "| h | v |\n| --- | --- |\n| x | y |\n\n"
            } else if kind == "inline-code" {
                "| h | v |\n| --- | --- |\n| `x` | `y` |\n\n"
            } else {
                "paragraph `ordinary` and **bold**\n\n| h | v |\n| --- | --- |\n| `x` | `y` |\n\n"
            };
            let source = unit.repeat(n);
            let features = ryne_features::MarkdownFeatures::new(
                Arc::new(ryne_services::math::FormularyRenderer::default()),
                vec![],
            )
            .unwrap();
            let d = EditorSession::new(
                &source,
                SessionServices {
                    features: Arc::new(features),
                    files: Arc::new(ryne_services::files::AtomicFileStore),
                },
            );
            black_box(d.display());
            let mut times = vec![];
            for _ in 0..3 {
                let t = Instant::now();
                black_box(d.display());
                times.push(t.elapsed().as_secs_f64() * 1000.);
            }
            times.sort_by(f64::total_cmp);
            println!(
                "table_kind={kind} tables={n} bytes={} samples=3 warmup=1 debug=true p50_ms={:.3}",
                source.len(),
                times[1]
            );
        }
    }
}
