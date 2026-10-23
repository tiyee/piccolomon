use super::*;
impl Display {
    /// A lossless, editable fallback with physical newline bytes retained in the mapping.
    pub fn plain_source(source: &str) -> Self {
        let mut display = Self {
            source_len: source.len(),
            ..Self::default()
        };
        let mut chars = source.char_indices().peekable();
        while let Some((start, ch)) = chars.next() {
            let mut end = start + ch.len_utf8();
            if ch == '\r'
                && chars.peek().is_some_and(|(_, c)| *c == '\n')
                && let Some((at, _)) = chars.next()
            {
                end = at + 1;
            }
            let from = display.text.len();
            display.text.push(if ch == '\r' { '\n' } else { ch });
            if let Some(last) = display.spans.last_mut().filter(|last| {
                ch != '\r' && last.source.end == start && last.source.len() == last.display.len()
            }) {
                last.source.end = end;
                last.display.end = display.text.len();
            } else {
                display.spans.push(Span {
                    source: start..end,
                    display: from..display.text.len(),
                    style: Style {
                        source: true,
                        ..Style::default()
                    },
                });
            }
        }
        display
    }
    /// Validate the ranges and resources consumed by layout, hit testing and PDF export.
    pub fn validate(&self, source: &str) -> Result<(), String> {
        let sr = |r: &Range<usize>| source.get(r.clone()).is_some();
        let dr = |r: &Range<usize>| self.text.get(r.clone()).is_some();
        let point = |p| source.is_char_boundary(p);
        let mut previous = (0, 0);
        if self.source_len != source.len() {
            return Err("projection source length mismatch".into());
        }
        for span in &self.spans {
            if !sr(&span.source)
                || !dr(&span.display)
                || span.display.start != previous.1
                || span.source.start < previous.0
                || span.display.is_empty()
            {
                return Err("invalid projection source mapping".into());
            }
            if span.source.len() == span.display.len()
                && !span.style.math
                && !span.style.diagram
                && !source[span.source.clone()]
                    .char_indices()
                    .map(|(p, _)| p)
                    .eq(self.text[span.display.clone()]
                        .char_indices()
                        .map(|(p, _)| p))
            {
                return Err("projection byte mapping splits UTF-8".into());
            }
            previous = (span.source.end, span.display.end);
        }
        if previous.1 != self.text.len() {
            return Err("unmapped projection text".into());
        }
        let code = |b: &CodeBlock| {
            sr(&b.range)
                && sr(&b.scope)
                && point(b.anchor)
                && b.closing.as_ref().is_none_or(sr)
                && b.rows.iter().all(|row| sr(&row.line) && sr(&row.content))
        };
        if self
            .code_blocks
            .iter()
            .any(|b| !code(&b.source) || b.rows.iter().any(|r| !dr(r)))
            || self.flow.iter().any(|b| !dr(&b.range))
        {
            return Err("invalid projection block range".into());
        }
        for row in &self.lists {
            let r = &row.source;
            let i = &r.item;
            if !self.text.is_char_boundary(row.start)
                || !sr(&r.line)
                || !sr(&r.content)
                || r.hidden.iter().any(|r| !sr(r))
                || !sr(&i.range)
                || !sr(&i.scope)
                || !sr(&i.marker)
                || !point(i.body)
                || i.task.as_ref().is_some_and(|t| !sr(&t.marker))
            {
                return Err("invalid projection list range".into());
            }
        }
        for quote in &self.quotes {
            if !sr(&quote.source)
                || !dr(&quote.display)
                || quote.depth == 0
                || quote.inset > 32768
                || quote
                    .empty_tail
                    .is_some_and(|tail| !point(tail) || tail != quote.source.end)
            {
                return Err("invalid projection quote range".into());
            }
        }
        for table in &self.tables {
            if !dr(&table.range)
                || !sr(&table.source.range)
                || !sr(&table.source.scope)
                || table.source.align.is_empty()
                || table.rows.len() != table.source.rows.len()
            {
                return Err("invalid projection table shape".into());
            }
            for (display, row) in table.rows.iter().zip(&table.source.rows) {
                if !sr(&row.line)
                    || display.len() != row.cells.len()
                    || display.len() != table.source.align.len()
                    || display.iter().any(|r| !dr(r))
                    || row.cells.iter().any(|c| !sr(&c.content))
                {
                    return Err("invalid projection cell range".into());
                }
            }
        }
        let metric = |n: f32| n.is_finite() && (0.0..=32768.0).contains(&n);
        for view in &self.formulas {
            let f = &view.formula;
            if !sr(&view.source)
                || !dr(&view.display)
                || !metric(f.width)
                || !metric(f.ascent)
                || !metric(f.descent)
                || f.width == 0.
                || f.ascent + f.descent == 0.
            {
                return Err("invalid projection formula".into());
            }
        }
        for view in &self.diagrams {
            let d = &view.diagram;
            let bytes = (d.pixel_width as usize)
                .checked_mul(d.pixel_height as usize)
                .and_then(|n| n.checked_mul(4));
            if !code(&view.source)
                || !dr(&view.display)
                || !metric(d.width)
                || !metric(d.height)
                || d.pixel_width == 0
                || d.pixel_height == 0
                || d.width == 0.
                || d.height == 0.
                || bytes != Some(d.pixels.len())
                || d.bytes != d.pixels.len()
            {
                return Err("invalid projection diagram".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fallback_maps_unicode_and_crlf_without_losing_source_bytes() {
        let source = "中😀\r\ne\u{301}\rlast\n";
        let display = Display::plain_source(source);
        display.validate(source).unwrap();
        assert_eq!(display.text, "中😀\ne\u{301}\nlast\n");
        let newline = display.text.find('\n').unwrap();
        assert_eq!(display.source_at(newline, Affinity::Downstream), 7);
        assert_eq!(display.source_at(newline + 1, Affinity::Downstream), 9);
        assert_eq!(display.deletion_range(7, false), Some(3..7));
        Display::plain_source("").validate("").unwrap();
    }
    #[test]
    fn same_byte_length_cannot_disguise_incompatible_character_boundaries() {
        let mut display = Display::plain_source("😀");
        display.text = "abcd".into();
        assert!(display.validate("😀").is_err());
        let mut display = Display::plain_source("text");
        display.spans[0].display.end = usize::MAX;
        assert!(display.validate("text").is_err());
    }
}
