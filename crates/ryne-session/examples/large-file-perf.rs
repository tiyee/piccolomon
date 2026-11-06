//! Headless large-file diagnosis. Run with --release; this excludes GUI layout/paint.
//! Arguments: corpus (plain/formatted/code/list/table/quote/long-line), sizes (MiB or KiB with k), samples (3).
//! Optional fourth argument: lf/crlf/cr. Default sizes: 1,10. No warmup.
use ryne_core::PreparedEdit;
use ryne_parser::Edit;
use ryne_session::{EditorSession, SessionServices};
use ryne_syntax::ParseSnapshot;
use std::{hint::black_box, sync::Arc, time::Instant};

fn measure<T>(name: &str, samples: usize, mut operation: impl FnMut() -> T) {
    let mut times = Vec::with_capacity(samples);
    for _ in 0..samples {
        let start = Instant::now();
        let result = black_box(operation());
        times.push(start.elapsed().as_secs_f64() * 1000.);
        drop(result);
    }
    times.sort_by(f64::total_cmp);
    println!(
        "{{\"metric\":\"{name}\",\"samples\":{samples},\"min_ms\":{:.4},\"p50_ms\":{:.4},\"max_ms\":{:.4}}}",
        times[0],
        times[samples / 2],
        times[samples - 1]
    );
}

fn main() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let kind = args.first().map_or("plain", String::as_str);
    let sizes = args.get(1).map_or("1,10", String::as_str);
    let newline = args.get(3).map_or("lf", String::as_str);
    let samples = args
        .get(2)
        .map_or(Ok(3), |s| s.parse::<usize>())
        .map_err(|e| e.to_string())?;
    if samples == 0 || samples > 100 {
        return Err("samples must be between 1 and 100".into());
    }
    let unit = match kind {
        "plain" => "plain text 中文😀 and more words in a paragraph.\n\n",
        "formatted" => "plain text 中文😀 **bold** and `code` in a paragraph.\n\n",
        "long-line" => "plain text 中文😀 and more words. ",
        "code" => "```rust\nfn main() { println!(\"hello 中文\"); }\n```\n\n",
        "list" => "- first 中文😀\n  - nested **bold**\n\n",
        "table" => "| a | b | c |\n| --- | --- | --- |\n| 中文 | **bold** | value |\n\n",
        "quote" => "> first 中文😀\n> second **bold**\n\ntail\n\n",
        _ => return Err("unknown corpus".into()),
    };
    let unit = match newline {
        "lf" => unit.to_owned(),
        "crlf" => unit.replace('\n', "\r\n"),
        "cr" => unit.replace('\n', "\r"),
        _ => return Err("newline must be lf/crlf/cr".into()),
    };
    let services = SessionServices {
        features: Arc::new(ryne_features::MarkdownFeatures::new(
            Arc::new(ryne_services::math::FormularyRenderer::default()),
            vec![],
        )?),
        files: Arc::new(ryne_services::files::AtomicFileStore),
    };
    for size in sizes.split(',') {
        let (amount, multiplier) = size
            .strip_suffix('k')
            .map_or((size, 1024 * 1024), |amount| (amount, 1024));
        let target_bytes = amount
            .parse::<usize>()
            .map_err(|e| e.to_string())?
            .checked_mul(multiplier)
            .ok_or("size overflow")?;
        if target_bytes == 0 || target_bytes > 32 * 1024 * 1024 {
            return Err("size must be positive and at most 32 MiB".into());
        }
        let source = unit.repeat(target_bytes.div_ceil(unit.len()));
        println!(
            "{{\"corpus\":\"{kind}\",\"newline\":\"{newline}\",\"bytes\":{},\"profile\":\"{}\",\"warmup\":0,\"style_bytes_per_source_byte\":{}}}",
            source.len(),
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            std::mem::size_of::<ryne_projection::Style>()
        );
        measure("new_session", samples, || {
            EditorSession::new(&source, services.clone())
        });
        let mut doc = EditorSession::new(&source, services.clone());
        println!(
            "{{\"nodes\":{},\"tokens\":{}}}",
            doc.parse.nodes.len(),
            doc.parse.syntax.tokens.len()
        );
        measure("parse_only", samples, || {
            ParseSnapshot::with_math(doc.source.clone())
        });
        let end = source.len();
        measure("prepare_one_byte_edit_at_end", samples, || {
            PreparedEdit::prepare(
                &doc.source,
                vec![Edit {
                    range: end..end,
                    replacement: "x".into(),
                }],
            )
        });
        for (label, position) in [("start", 1), ("end", end - 2)] {
            measure(&format!("select_{label}"), samples, || {
                doc.select(position, false, false)
            });
            measure(&format!("normalize_{label}"), samples, || {
                doc.normalize_heading_caret()
            });
        }
        measure("code_block_query", samples, || {
            ryne_syntax::code_block::blocks(&doc.parse)
        });
        measure("table_query", samples, || {
            ryne_syntax::table::tables(&doc.parse)
        });
        measure("list_row_query", samples, || {
            ryne_syntax::list::rows(&doc.parse)
        });
        measure("full_projection", samples, || doc.display());
        measure("publish_indexed_projection", samples, || {
            ryne_projection::IndexedDisplay::new(doc.display())
        });
        let batch = doc.cached_project_batch(&doc, None, &[]);
        if !batch.diagnostics.is_empty() {
            return Err(format!("projection diagnostics: {:?}", batch.diagnostics));
        }
        let projected = batch.display;
        if !Arc::ptr_eq(
            &projected,
            &doc.cached_project_batch(&doc, None, &[]).display,
        ) {
            return Err("projection fixture did not hit the cache".into());
        }
        println!(
            "{{\"display_bytes\":{},\"spans\":{},\"flow_blocks\":{}}}",
            projected.text.len(),
            projected.spans.len(),
            projected.flow.len()
        );
        measure("cached_projection_hit", samples, || {
            doc.cached_project_batch(&doc, None, &[])
        });
        measure("display_snap_end", samples, || {
            projected.snap(projected.text.len() - 2)
        });
        measure("display_position_end", samples, || {
            projected.position_at(end - 2)
        });
        measure("dirty_clean", samples, || doc.dirty());
        measure("platform_editable_text_copy", samples, || {
            doc.editable_text()
        });
        let _ = black_box(doc.editable_byte_to_utf16(end));
        measure("platform_indexed_utf16_offset_end", samples, || {
            doc.editable_byte_to_utf16(end)
        });
        measure("platform_utf16_offset_end", samples, || {
            doc.source.text()[..end].encode_utf16().count()
        });
        measure("full_controls", samples, || doc.render_controls());
        black_box(doc.cached_render_control_batch(&doc)?);
        measure("cached_controls_hit", samples, || {
            doc.cached_render_control_batch(&doc)
        });
        // Successive edits append one byte each; no undo or projection is included.
        measure("insert_end", samples, || {
            let end = doc.source.text().len();
            doc.insert_text(end..end, "x")
        });
        if doc.source.text().len() != source.len() + samples {
            return Err("insertion benchmark did not publish every edit".into());
        }
        measure("undo", samples, || doc.undo(false));
        if doc.source.text() != source {
            return Err("undo did not restore the fixture".into());
        }
        let mut core = ryne_core::DocumentCore::new(&source);
        let last_letter = source
            .bytes()
            .rposition(|byte| byte.is_ascii_alphabetic())
            .ok_or("fixture needs an ASCII letter")?;
        let prepared = PreparedEdit::prepare(
            core.source(),
            vec![Edit {
                range: last_letter..last_letter + 1,
                replacement: if source.as_bytes()[last_letter] == b'x' {
                    "y"
                } else {
                    "x"
                }
                .into(),
            }],
        )
        .map_err(|e| format!("{e:?}"))?;
        core.publish(prepared, ()).map_err(|e| format!("{e:?}"))?;
        measure("dirty_same_length_end_edit", samples, || core.dirty());
        doc.select(end, false, false);
        doc.update_composition(None, "中", None)?;
        measure("ime_temporary_parse", samples, || doc.projection_state(&[]));
        doc.cancel_composition();
        doc.set_source_mode(true);
        measure("source_mode_select_end", samples, || {
            doc.select(end - 2, false, false)
        });
        measure("source_mode_projection", samples, || doc.display());
    }
    Ok(())
}
