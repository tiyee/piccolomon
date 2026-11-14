//! Headless stage characterization; samples are not UI latency measurements.
use ryne_parser::{Edit, Source};
use ryne_syntax::{directory::BlockDirectory, ParseSnapshot};
use std::{hint::black_box, time::Instant};

fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    let sizes: Vec<usize> = args.get(1).map_or(Ok(vec![1024, 5120, 10240]), |s| {
        s.parse().map(|n| vec![n]).map_err(|_| "invalid KiB")
    })?;
    let samples: usize = args
        .get(2)
        .map_or(Ok(3), |s| s.parse().map_err(|_| "invalid samples"))?;
    if samples == 0 {
        return Err("samples must be positive".into());
    }
    for kib in sizes {
        let paragraph = "paragraph 中文😀 é unchanged content for block identity.\n\n";
        let raw = paragraph.repeat((kib * 1024).div_ceil(paragraph.len()));
        let source = Source::new(&raw);
        let parse = ParseSnapshot::with_math(source.clone());
        let directory = BlockDirectory::new(&parse);
        let mut timings = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        for _ in 0..samples {
            let start = Instant::now();
            let applied = source
                .apply(
                    source.id(),
                    vec![Edit {
                        range: 2..2,
                        replacement: "x".into(),
                    }],
                )
                .map_err(|e| format!("{e:?}"))?;
            timings[0].push(start.elapsed().as_secs_f64() * 1000.);
            let start = Instant::now();
            let updated = ParseSnapshot::with_math(applied.source);
            timings[1].push(start.elapsed().as_secs_f64() * 1000.);
            let start = Instant::now();
            black_box(BlockDirectory::new(&updated));
            timings[2].push(start.elapsed().as_secs_f64() * 1000.);
            let start = Instant::now();
            black_box(
                directory
                    .updated(&updated, &applied.positions)
                    .map_err(|e| format!("{e:?}"))?,
            );
            timings[3].push(start.elapsed().as_secs_f64() * 1000.);
        }
        println!(
            "{{\"bytes\":{},\"blocks\":{},\"pieces\":{},\"samples\":{},\"debug\":{}}}",
            raw.len(),
            directory
                .entries(source.id())
                .map_err(|e| format!("{e:?}"))?
                .len(),
            directory
                .pieces(source.id())
                .map_err(|e| format!("{e:?}"))?
                .len(),
            samples,
            cfg!(debug_assertions)
        );
        for (name, mut times) in ["source_apply", "parse", "directory_new", "directory_update"]
            .into_iter()
            .zip(timings)
        {
            times.sort_by(f64::total_cmp);
            println!(
                "{{\"stage\":\"{name}\",\"min_ms\":{:.3},\"median_ms\":{:.3},\"max_ms\":{:.3}}}",
                times[0],
                times[samples / 2],
                times[samples - 1]
            );
        }
    }
    Ok(())
}
