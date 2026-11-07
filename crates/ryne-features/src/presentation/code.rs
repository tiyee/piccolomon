use super::*;
pub(super) struct CodePresentation {
    pub structured: Vec<code_block::CodeBlock>,
    pub diagrams: Vec<(PreviewSource, std::sync::Arc<Diagram>)>,
    pub patch: BlockContribution,
}
pub(super) fn contribute(
    text: &str,
    code_blocks: &[code_block::CodeBlock],
    source_mode: bool,
    block_window: Option<&Range<usize>>,
    mut highlighter: Option<&mut dyn SyntaxHighlighter>,
    diagrams: &[DiagramViewInput],
) -> CodePresentation {
    let mut patch = BlockContribution::default();
    let mut structured = Vec::new();
    let mut folded_diagrams = Vec::new();
    if let Some(highlighter) = &mut highlighter {
        highlighter.begin_frame();
    }
    for block in code_blocks {
        patch.code(block.range.clone());
        if let Some(highlighter) = &mut highlighter {
            for span in highlighter.spans(text, block) {
                if text.get(span.range.clone()).is_some()
                    && block.range.start <= span.range.start
                    && span.range.end <= block.range.end
                {
                    patch.highlight(span.range, span.class);
                }
            }
        }
        if source_mode
            || block_window.is_some_and(|w| w.start < block.range.end && block.range.start < w.end)
        {
            continue;
        }
        patch.hide(block.range.clone(), true);
        if let Some((_, Ok(diagram))) = diagrams.iter().find(|(range, _)| *range == block.range) {
            folded_diagrams.push((block.into(), diagram.clone()));
            continue;
        }
        for row in &block.rows {
            patch.hide(row.content.start..row.line.end, false);
        }
        structured.push(block.clone());
    }
    if let Some(highlighter) = &mut highlighter {
        highlighter.end_frame();
    }
    CodePresentation {
        structured,
        diagrams: folded_diagrams,
        patch,
    }
}
