//! Isolated release block queries. Arguments: sizes in KiB, samples, rounds, corpus.
use editable_markdown_parser::Source;
use ryne_syntax::{block, ParseSnapshot};
use std::{hint::black_box, time::Instant};

struct Allocator;
thread_local! { static ALLOCATIONS: std::cell::Cell<Option<(usize,usize)>> = const { std::cell::Cell::new(None) }; }
// Count only this thread's hot query, excluding the driver and cold indexing.
unsafe impl std::alloc::GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let value = unsafe { std::alloc::System.alloc(layout) };
        let _ = ALLOCATIONS.try_with(|cell| {
            if let Some((n, b)) = cell.get() {
                cell.set(Some((n + 1, b + layout.size())));
            }
        });
        value
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let sizes = args.first().map_or("64,256,1024,10240", String::as_str);
    let samples: usize = args.get(1).map_or(1000, |v| v.parse().unwrap());
    let rounds: usize = args.get(2).map_or(3, |v| v.parse().unwrap());
    let filter = args.get(3).map_or("all", String::as_str);
    for (kind, unit) in [
        ("plain", "plain paragraph words\n\n"),
        ("heading", "## heading words\n\n"),
        ("code", "```rust\nlet n = 1;\n```\n\n"),
        ("table", "| a | b |\n| --- | --- |\n| x | y |\n\n"),
        ("list", "- [ ] first\n  - nested\n- last\n\n"),
        ("quote", "> first\n> second\n\noutside\n\n"),
    ] {
        if filter != "all" && filter.strip_prefix("deep-").unwrap_or(filter) != kind {
            continue;
        }
        let deep_unit = match filter {
            "deep-quote" => Some(format!(
                "{}first\n{}second\n\noutside\n\n",
                "> ".repeat(16),
                "> ".repeat(16)
            )),
            "deep-list" => Some(
                (0..16)
                    .map(|d| format!("{}- level {d}\n", "  ".repeat(d)))
                    .collect::<String>()
                    + "\noutside\n\n",
            ),
            _ => None,
        };
        let unit = deep_unit.as_deref().unwrap_or(unit);
        for amount in sizes.split(',').map(|v| v.parse::<usize>().unwrap()) {
            let raw = unit.repeat((amount * 1024).div_ceil(unit.len()));
            let started = Instant::now();
            let parse = ParseSnapshot::with_math(Source::new(&raw));
            let parse_ms = started.elapsed().as_secs_f64() * 1000.;
            #[cfg(feature = "query-metrics")]
            let before_cold = ryne_syntax::query_metrics::snapshot();
            let started = Instant::now();
            black_box(block::at(&parse, raw.len() - 3));
            let cold_ms = started.elapsed().as_secs_f64() * 1000.;
            println!("{{\"corpus\":\"{kind}\",\"bytes\":{},\"parse_ms\":{parse_ms:.4},\"cold_query_ms\":{cold_ms:.4}}}", raw.len());
            #[cfg(feature = "query-metrics")]
            {
                let after = ryne_syntax::query_metrics::snapshot();
                println!(
                    "{{\"new_block_records\":{},\"record_payload_bytes\":{}}}",
                    after.records - before_cold.records,
                    after.record_payload_bytes - before_cold.record_payload_bytes
                );
            }
            for (label, offset) in [
                ("start", 3),
                ("middle", raw.len() / 2),
                ("end", raw.len() - 3),
            ] {
                for _ in 0..100 {
                    black_box(block::at(&parse, offset));
                }
                for round in 0..rounds {
                    let mut times = Vec::with_capacity(samples);
                    let mut max_bytes = 0;
                    #[cfg(feature = "query-metrics")]
                    let before = ryne_syntax::query_metrics::snapshot();
                    for _ in 0..samples {
                        ALLOCATIONS.with(|c| c.set(Some((0, 0))));
                        let started = Instant::now();
                        black_box(block::at(&parse, black_box(offset)));
                        times.push(started.elapsed().as_secs_f64() * 1000.);
                        max_bytes = max_bytes.max(ALLOCATIONS.with(|c| c.replace(None).unwrap().1));
                    }
                    times.sort_by(f64::total_cmp);
                    println!("{{\"corpus\":\"{kind}\",\"bytes\":{},\"position\":\"{label}\",\"round\":{round},\"samples\":{samples},\"p50_ms\":{:.6},\"p95_ms\":{:.6},\"p99_ms\":{:.6}}}",raw.len(),times[samples/2],times[(samples*95/100).min(samples-1)],times[(samples*99/100).min(samples-1)]);
                    println!("{{\"corpus\":\"{kind}\",\"bytes\":{},\"position\":\"{label}\",\"round\":{round},\"max_hot_allocated_bytes\":{max_bytes}}}",raw.len());
                    #[cfg(feature = "query-metrics")]
                    {
                        let after = ryne_syntax::query_metrics::snapshot();
                        assert_eq!(before.directories, after.directories);
                        assert_eq!(before.semantic_indexes, after.semantic_indexes);
                        assert_eq!(before.physical_lines, after.physical_lines);
                        println!("{{\"queries\":{},\"comparisons\":{},\"candidates\":{},\"new_directories\":0,\"new_line_arrays\":0}}", after.queries-before.queries,after.comparisons-before.comparisons,after.candidates-before.candidates);
                    }
                    if cfg!(feature = "query-metrics") && amount >= 1024 {
                        assert!(
                            times[(samples * 95 / 100).min(samples - 1)]
                                <= if amount <= 1024 { 0.5 } else { 1. },
                            "hot query P95 exceeded acceptance limit"
                        );
                        assert!(
                            max_bytes <= 1024,
                            "hot query allocated a document-sized result"
                        );
                    }
                }
            }
        }
    }
}
