//! Work counts and invariants, never a GUI latency benchmark.
use ryne_extensions::*;
use ryne_session::{EditorSession, SessionServices};
use std::sync::{Arc, atomic::Ordering};

struct NoMath;
impl FormulaRenderer for NoMath {
    fn render(&self, _: &str) -> Option<Arc<ryne_projection::Formula>> {
        None
    }
}
fn document(text: &str) -> EditorSession {
    EditorSession::new(
        text,
        SessionServices {
            features: Arc::new(
                ryne_features::MarkdownFeatures::new(Arc::new(NoMath), vec![]).unwrap(),
            ),
            files: Arc::new(ryne_services::files::AtomicFileStore),
        },
    )
}
fn main() {
    for count in [100usize, 200, 400, 800] {
        let raw =
            "| a | b | c |\n| --- | --- | --- |\n".to_owned() + &"| $x$ | y | z |\n".repeat(count);
        let doc = document(&raw);
        assert_eq!(ryne_syntax::math::cached(&doc.parse).len(), count);
        ryne_features::presentation::AUDIT_VISITS.store(0, Ordering::Relaxed);
        let batch = doc.project_batch(&doc, None, &[]);
        assert!(batch.diagnostics.is_empty());
        let visits = ryne_features::presentation::AUDIT_VISITS.load(Ordering::Relaxed);
        assert_eq!(visits, count * 2 * (count + 1));
        println!("formula_table rows={count} formula_count={count} retain_visits={visits}");
    }
    for (count, consume) in [(100usize, false), (200, false), (400, false), (400, true)] {
        let mut doc = document("");
        for _ in 0..count {
            doc.insert_text(doc.selection(), "*").unwrap();
            doc.insert_text(doc.selection(), "a").unwrap();
            if consume {
                doc.insert_text(doc.selection(), "*").unwrap();
            } else {
                let end = doc.source.text().len();
                doc.select(end, false, false); // leave the auto closer using navigation
            }
            doc.insert_text(doc.selection(), " ").unwrap();
        }
        assert_eq!(doc.closers.len(), if consume { 0 } else { count });
        assert_eq!(doc.source.text(), "*a* ".repeat(count));
        let bytes = doc.history_memory().reserved_bytes;
        println!(
            "completion_history pairs={count} consume={consume} source_bytes={} undo_entries={} retained_bytes={bytes}",
            doc.source.text().len(),
            doc.history_len(false)
        );
        let before = doc.source.text().to_owned();
        assert!(doc.try_undo(false).unwrap());
        assert!(doc.try_undo(true).unwrap());
        assert_eq!(doc.source.text(), before);
    }
    // A bare-CR fence is the known parser normalization trigger. Check its following heading.
    for newline in ["\n", "\r\n", "\r"] {
        for heading in ["# title", "###### title", "title\n====="] {
            let raw = format!("```\ncode\n```\n\n{heading}\n\ntail").replace('\n', newline);
            let mut doc = document(&raw);
            let source = doc.source.id();
            let before = doc.display();
            assert!(before.spans.iter().any(|span| span.style.heading > 0));
            doc.select(raw.find("title").unwrap(), false, false);
            doc.reveal_clicked_heading();
            assert_eq!(doc.heading_reveal.is_some(), newline != "\r");
            assert_eq!(doc.source.id(), source);
            assert!(!doc.dirty());
            assert_eq!(doc.history_len(false), 0);
            println!(
                "heading_after_fence newline={newline:?} heading={heading:?} reveal={} source_unchanged=true",
                doc.heading_reveal.is_some()
            );
        }
    }
}
