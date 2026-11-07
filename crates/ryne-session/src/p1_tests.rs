use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Malformed {
    inner: Arc<dyn FeatureEngine>,
    fault: Arc<AtomicUsize>,
    frozen: Option<usize>,
}
impl Malformed {
    fn fault(&self) -> usize {
        self.frozen
            .unwrap_or_else(|| self.fault.load(Ordering::SeqCst))
    }
}
impl FeatureEngine for Malformed {
    fn snapshot(&self) -> Option<Arc<dyn FeatureEngine>> {
        Some(Arc::new(Self {
            inner: self.inner.snapshot()?,
            fault: self.fault.clone(),
            frozen: Some(self.fault()),
        }))
    }
    fn presentation_key(&self, s: &EditingState) -> Option<PresentationKey> {
        Some(PresentationKey::for_state(
            s,
            CacheDependencies {
                generation: self.fault() as u64,
                ..Default::default()
            },
        ))
    }
    fn plan(&self, i: EditIntent, s: &EditingState) -> Result<CommandPlan, String> {
        self.inner.plan(i, s)
    }
    fn block_window(&self, s: &EditingState) -> Option<Range<usize>> {
        self.inner.block_window(s)
    }
    fn protects_table_boundary(&self, s: &EditingState, f: bool) -> bool {
        self.inner.protects_table_boundary(s, f)
    }
    fn preview_inputs(&self, s: &EditingState) -> Vec<PreviewInput> {
        self.inner.preview_inputs(s)
    }
    fn block_ui(&self, b: &ryne_syntax::block::Block, o: bool) -> AdornmentSpec {
        self.inner.block_ui(b, o)
    }
    fn insert_templates(&self) -> &[(&'static str, InsertBlock)] {
        self.inner.insert_templates()
    }
    fn display(
        &self,
        s: &EditingState,
        h: Option<&mut dyn SyntaxHighlighter>,
        v: &[DiagramViewInput],
    ) -> Display {
        let mut d = self.inner.display(s, h, v);
        match self.fault() {
            1 => d.tables[0].range = 0..0,
            2 => d.tables.swap(0, 1),
            3 => d.tables[1].range.start = d.tables[0].range.start,
            4 => d.tables[0].rows[0][0].end = d.text.len(),
            5 => {
                d.tables[0].rows.clear();
                d.tables[0].source.rows.clear();
            }
            6 => {
                d.tables[0].rows[0].pop();
            }
            7 => d.formulas.swap(1, 2),
            8 => d.formulas[1] = d.formulas[0].clone(),
            9 => d.formulas[0].display.end = d.formulas[0].display.start,
            10 => d.formulas[0].display.end = d.formulas[0].display.start + 1,
            11 => d.formulas[0].source = 0..0,
            12 => d.tables[0].source.rows[0].cells[0].content = 0..s.source.text().len(),
            13 => {
                // A correctly mapped object, but a diagram cannot consume a
                // table cell independently of the enclosing table jump.
                let f = d.formulas.remove(0);
                let span = d
                    .spans
                    .iter_mut()
                    .find(|span| span.display == f.display)
                    .unwrap();
                span.style.math = false;
                span.style.diagram = true;
                d.diagrams.push(ryne_projection::DiagramView {
                    source: ryne_projection::PreviewSource {
                        range: f.source.clone(),
                        scope: f.source.clone(),
                        anchor: f.source.start,
                        indent: 0,
                    },
                    display: f.display,
                    diagram: Arc::new(ryne_projection::Diagram {
                        pixels: vec![0, 0, 0, 255].into(),
                        pixel_width: 1,
                        pixel_height: 1,
                        width: 1.,
                        height: 1.,
                        bytes: 4,
                    }),
                });
            }
            14 => {
                d.formulas.pop();
            }
            _ => {}
        }
        d
    }
}

fn malformed_document(raw: &str, mode: usize) -> (EditorSession, Arc<AtomicUsize>) {
    let mut services = regression_tests::services();
    let fault = Arc::new(AtomicUsize::new(mode));
    services.features = Arc::new(Malformed {
        inner: services.features,
        fault: fault.clone(),
        frozen: None,
    });
    (EditorSession::new(raw, services), fault)
}
const RAW: &str = "| $x$ | b |\n| --- | --- |\n| | $y$ |\n\n| c | d |\n| --- | --- |\n| e | f |\n\n$z$\n\n> quote\n> > nested\n\n```\n```\n\nplain 中😀e\u{301}\n";

#[test]
fn malformed_objects_fall_back_without_state_changes_and_recover_without_poisoning_cache() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw = RAW.replace('\n', newline);
        for mode in 1..=14 {
            let (mut doc, fault) = malformed_document(&raw, mode);
            doc.select(raw.len(), false, false);
            let source = doc.source.id();
            let selection = doc.selection();
            let invalid = doc.services.features.display(&doc, None, &[]);
            for pdf in [
                &ryne_services::pdf::PdfExporter as &dyn PdfExporter,
                &ryne_services::pdf_v2::KrillaPdfExporter,
            ] {
                assert!(pdf.export(&invalid).is_err(), "unvalidated PDF mode {mode}");
            }
            let first = doc.cached_project_batch(&doc, None, &[]);
            assert!(!first.diagnostics.is_empty(), "mode {mode}, {newline:?}");
            assert_eq!(*first.display, Display::plain_source(&raw));
            first.display.validate(&raw).unwrap();
            for _ in 0..100 {
                let next = doc.cached_project_batch(&doc, None, &[]);
                assert!(!Arc::ptr_eq(&first.display, &next.display));
            }
            assert!(doc.diagnostics.len() <= 64);
            assert_eq!(doc.source.id(), source);
            assert_eq!(doc.selection(), selection);
            assert!(!doc.dirty());
            assert_eq!(doc.history_len(false), 0);
            assert_eq!(doc.history_len(true), 0);
            assert!(doc.files.path.is_none());
            assert!(doc.composition().is_none());
            if mode == 1 {
                let display = doc.export_snapshot().render().unwrap();
                assert_eq!(display, Display::plain_source(&raw));
                for pdf in [
                    &ryne_services::pdf::PdfExporter as &dyn PdfExporter,
                    &ryne_services::pdf_v2::KrillaPdfExporter,
                ] {
                    assert!(pdf.export(&display).unwrap().starts_with(b"%PDF-"));
                }
            }
            fault.store(0, Ordering::SeqCst);
            let healthy = doc.cached_project_batch(&doc, None, &[]);
            assert!(healthy.diagnostics.is_empty());
            assert_eq!(healthy.display.tables.len(), 2);
            assert_eq!(healthy.display.formulas.len(), 3);
            assert!(Arc::ptr_eq(
                &healthy.display,
                &doc.cached_project_batch(&doc, None, &[]).display
            ));
            fault.store(1, Ordering::SeqCst);
            doc.update_composition(None, "预编辑😀", None).unwrap();
            let composition = doc.composition().cloned();
            let (state, views) = doc.projection_state(&[]);
            assert!(!doc
                .cached_project_batch(&state, None, &views)
                .diagnostics
                .is_empty());
            assert_eq!(doc.composition(), composition.as_ref());
            assert_eq!(doc.source.id(), source);
            doc.cancel_composition();
            fault.store(0, Ordering::SeqCst);
            doc.insert_text(raw.len()..raw.len(), "end").unwrap();
            doc.undo(false);
            assert_eq!(doc.source.text(), raw);
            assert!(!doc.dirty());
            let directory = std::env::temp_dir().join(format!(
                "ryne-p1-{}-{mode}-{}",
                std::process::id(),
                newline.len()
            ));
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join("test.md");
            doc.save(&path).unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), raw.as_bytes());
            std::fs::remove_dir_all(directory).unwrap();
        }
    }
}
