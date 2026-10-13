use super::*;
use ryne_extensions::CodeLanguageDetector;

impl Planner<'_> {
    /// Amend content input before publication; navigation and source views never write.
    pub fn complete_code_languages(
        &mut self,
        original: &EditingState,
        detector: &dyn CodeLanguageDetector,
    ) -> Result<(), Error> {
        if self.state.source_mode || self.state.block_source.is_some() {
            return Ok(());
        }
        let Some(prepared) = &self.prepared else {
            return Ok(());
        };
        let changes = &prepared.applied().inverse.edits;
        let text = self.state.source.text();
        let normalized = ryne_syntax::parser::normalized(&self.state.parse);
        let parse = normalized.as_ref().unwrap_or(&self.state.parse);
        let mut edits = Vec::new();
        let mut attempts = 0;
        for block in code_block::blocks(parse) {
            if !block.language.trim().is_empty()
                || block.closing.is_none()
                || !block.rows.iter().any(|row| {
                    changes.iter().any(|edit| {
                        edit.range.start <= row.content.end && row.content.start <= edit.range.end
                    })
                })
            {
                continue;
            }
            let Some(node) = parse.nodes.iter().find(|node| {
                matches!(node.kind, NodeKind::CodeBlock(_))
                    && block.range.contains(&node.envelope.start)
            }) else {
                continue;
            };
            let opening = &text[node.envelope.start..];
            let marker = opening.as_bytes().first().copied().unwrap_or_default();
            if !matches!(marker, b'`' | b'~') {
                continue;
            }
            let width = opening.bytes().take_while(|b| *b == marker).count();
            if width < 3 {
                continue;
            }
            let size: usize = block.rows.iter().map(|row| row.content.len() + 1).sum();
            // Bound extraction as well as the detector. At most eight blocks per input.
            if size > 32 * 1024 {
                continue;
            }
            if attempts >= 8 {
                break;
            }
            attempts += 1;
            let mut code = String::with_capacity(size);
            for row in &block.rows {
                code.push_str(&text[row.content.clone()]);
                code.push('\n');
            }
            if let Some(language) = detector.detect(&code) {
                let offset = node.envelope.start + width;
                edits.push(Edit {
                    range: offset..offset,
                    replacement: language.into(),
                });
            }
        }
        if edits.is_empty() {
            return Ok(());
        }
        // Use the regular mapping path for selection, revealed ranges and closers.
        self.prepared = None;
        self.apply_edits(edits, None)?;
        // Rebase the two candidate steps into one publication and one history entry.
        let before = original.source.text();
        let after = self.state.source.text();
        let mut start = before
            .bytes()
            .zip(after.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        while !before.is_char_boundary(start) || !after.is_char_boundary(start) {
            start -= 1;
        }
        let mut suffix = before[start..]
            .bytes()
            .rev()
            .zip(after[start..].bytes().rev())
            .take_while(|(a, b)| a == b)
            .count();
        while !before.is_char_boundary(before.len() - suffix)
            || !after.is_char_boundary(after.len() - suffix)
        {
            suffix -= 1;
        }
        let prepared = PreparedEdit::prepare(
            &original.source,
            vec![Edit {
                range: start..before.len() - suffix,
                replacement: after[start..after.len() - suffix].into(),
            }],
        )?;
        self.state.source = prepared.applied().source.clone();
        self.state.parse = std::sync::Arc::new(ParseSnapshot::with_math(self.state.source.clone()));
        for session in [
            &mut self.state.reveal,
            &mut self.state.heading_reveal,
            &mut self.state.block_source,
        ]
        .into_iter()
        .flatten()
        {
            *session = Session::new(&self.state.source, session.range.clone())?;
        }
        self.prepared = Some(prepared);
        Ok(())
    }
}
