use super::*;
use crate::regression_tests::new_document;
use std::sync::atomic::{AtomicUsize, Ordering};

struct CountFormula {
    calls: Arc<AtomicUsize>,
    renderer: ryne_services::math::FormularyRenderer,
}
impl FormulaRenderer for CountFormula {
    fn render(&self, text: &str) -> Option<Arc<ryne_projection::Formula>> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.renderer.render(text)
    }
}
fn counted(raw: &str) -> (EditorSession, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut services = crate::regression_tests::services();
    services.features = Arc::new(
        ryne_features::MarkdownFeatures::new(
            Arc::new(CountFormula {
                calls: calls.clone(),
                renderer: Default::default(),
            }),
            vec![],
        )
        .unwrap(),
    );
    (EditorSession::new(raw, services), calls)
}
#[test]
fn deletion_uses_one_lazy_mapping_and_dispatch_does_not_retain_it() {
    for newline in ["\n", "\r\n", "\r"] {
        for body in [
            "plain 中😀 tail",
            "- item 中😀 tail",
            "| a | b |\n| --- | --- |\n| 中😀 tail | value |",
        ] {
            let raw = format!("$x^2$\n\n{body}\n").replace('\n', newline);
            let (mut doc, calls) = counted(&raw);
            let at = raw.find("tail").unwrap() + 2;
            doc.select(at, false, false);
            calls.store(0, Ordering::Relaxed);
            doc.delete(false).unwrap();
            assert_eq!(calls.load(Ordering::Relaxed), 1, "{body} {newline:?}");
            assert_eq!(doc.source.text(), raw.replacen("tail", "til", 1));
            assert_eq!(doc.history_len(false), 1);
            doc.undo(false);
            assert_eq!(doc.source.text(), raw);
            assert!(!doc.dirty());
        }
        let raw = "$x^2$\n\n| a | b |\n| --- | --- |\n| x | y |\n".replace('\n', newline);
        let (mut doc, calls) = counted(&raw);
        let at = raw.find("| x").unwrap() + 2;
        doc.select(at, false, false);
        for i in 1..=2 {
            doc.delete(false).unwrap();
            assert_eq!(calls.load(Ordering::Relaxed), i);
            assert_eq!(doc.source.text(), raw);
            assert_eq!(doc.history_len(false), 0);
        }
    }
}

#[test]
fn indexed_deletion_keeps_cross_block_selection_code_literals_and_history() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw = "- **中😀**\n\n> quoted\n\n```rust\n**literal** 👩‍👩‍👦\n```\n\nend e\u{301}"
            .replace('\n', newline);
        let mut doc = new_document(&raw);
        doc.select(raw.len(), false, false);
        doc.delete(false).unwrap();
        assert_eq!(doc.source.text(), raw.trim_end_matches("e\u{301}"));
        doc.undo(false);
        let at = raw.find("👩").unwrap() + "👩‍👩‍👦".len();
        doc.select(at, false, false);
        doc.delete(false).unwrap();
        assert_eq!(doc.source.text(), raw.replace("👩‍👩‍👦", ""));
        doc.undo(false);
        let a = raw.find("中").unwrap();
        let b = raw.find("literal").unwrap() + 3;
        doc.select(a, false, false);
        doc.select(b, true, true);
        doc.delete(false).unwrap();
        assert_eq!(doc.source.text(), format!("{}{}", &raw[..a], &raw[b..]));
        doc.undo(false);
        assert_eq!(doc.source.text(), raw);
        assert!(!doc.dirty());
        doc.set_source_mode(true);
        doc.select(raw.len(), false, false);
        doc.delete(false).unwrap();
        assert_eq!(doc.source.text(), raw.trim_end_matches("e\u{301}"));
    }
}
