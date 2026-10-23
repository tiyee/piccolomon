//! Host boundary for disposable presentation. Failures never alter the source snapshot.
use crate::*;
use std::panic::{catch_unwind, AssertUnwindSafe};

pub struct ProjectionBatch {
    pub display: Display,
    pub diagnostics: Vec<Diagnostic>,
}
struct CheckedHighlighter<'a> {
    inner: &'a mut dyn SyntaxHighlighter,
    failed: bool,
    diagnostics: Vec<Diagnostic>,
}
impl CheckedHighlighter<'_> {
    fn fail(&mut self) {
        if !self.failed {
            self.diagnostics.push(Diagnostic {
                feature: "highlight".into(),
                message: "Invalid or failed highlighter; retaining plain code".into(),
            });
            self.failed = true;
        }
    }
}
impl SyntaxHighlighter for CheckedHighlighter<'_> {
    fn begin_frame(&mut self) {
        if catch_unwind(AssertUnwindSafe(|| self.inner.begin_frame())).is_err() {
            self.fail();
        }
    }
    fn end_frame(&mut self) {
        if catch_unwind(AssertUnwindSafe(|| self.inner.end_frame())).is_err() {
            self.fail();
        }
    }
    fn spans(&mut self, source: &str, block: &CodeBlock) -> Vec<HighlightSpan> {
        if self.failed {
            return Vec::new();
        }
        match catch_unwind(AssertUnwindSafe(|| self.inner.spans(source, block))) {
            Ok(spans)
                if spans.iter().all(|s| {
                    source.get(s.range.clone()).is_some()
                        && block.range.start <= s.range.start
                        && s.range.end <= block.range.end
                }) =>
            {
                spans
            }
            _ => {
                self.fail();
                Vec::new()
            }
        }
    }
}
pub fn project_checked(
    engine: &dyn FeatureEngine,
    state: &EditingState,
    highlighter: Option<&mut dyn SyntaxHighlighter>,
    diagrams: &[DiagramViewInput],
) -> ProjectionBatch {
    let mut checked = highlighter.map(|inner| CheckedHighlighter {
        inner,
        failed: false,
        diagnostics: Vec::new(),
    });
    let result = catch_unwind(AssertUnwindSafe(|| {
        engine.display(
            state,
            checked.as_mut().map(|h| h as &mut dyn SyntaxHighlighter),
            diagrams,
        )
    }))
    .map_err(|_| "presentation provider panicked".to_string())
    .and_then(|display| {
        display.validate(state.source.text())?;
        Ok(display)
    });
    let highlight_failed = checked.as_ref().is_some_and(|h| h.failed);
    let mut diagnostics = checked.map_or_else(Vec::new, |h| h.diagnostics);
    let mut display = match result {
        Ok(display) => display,
        Err(message) => {
            diagnostics.push(Diagnostic {
                feature: "presentation".into(),
                message,
            });
            Display::plain_source(state.source.text())
        }
    };
    if highlight_failed {
        for span in &mut display.spans {
            span.style.syntax = None;
        }
    }
    ProjectionBatch {
        display,
        diagnostics,
    }
}
