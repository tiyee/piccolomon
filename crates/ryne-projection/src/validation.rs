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
                && !span.style.image_failure
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
                    || row.line.start < table.source.range.start
                    || row.line.end > table.source.range.end
                    || row.cells.iter().any(|c| {
                        !sr(&c.content)
                            || c.content.start < row.line.start
                            || c.content.end > row.line.end
                    })
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
            if !sr(&view.source.range)
                || !sr(&view.source.scope)
                || !point(view.source.anchor)
                || view.source.scope.start > view.source.range.start
                || view.source.range.end > view.source.scope.end
                || !view.source.range.contains(&view.source.anchor)
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
        for view in &self.image_failures {
            if !sr(&view.source.range)
                || !sr(&view.source.scope)
                || !point(view.source.anchor)
                || !view.source.range.contains(&view.source.anchor)
                || view.source.scope.start > view.source.range.start
                || view.source.range.end > view.source.scope.end
                || view.source.indent > 32768
                || !dr(&view.display)
            {
                return Err("invalid image failure source mapping".into());
            }
        }
        self.validate_layout()
    }

    /// Consumer invariants, also checked once when building a new GUI layout.
    /// This checks metadata, never scans the source or reparses the document.
    pub fn validate_layout(&self) -> Result<(), String> {
        let mut end = 0;
        for view in &self.image_failures {
            let range = &view.display;
            if range.is_empty()
                || range.start < end
                || !self
                    .text
                    .get(range.clone())
                    .is_some_and(|s| !s.contains(['\r', '\n']) && s.graphemes(true).count() <= 257)
                || (range.start > 0 && self.text.as_bytes().get(range.start - 1) != Some(&b'\n'))
                || self
                    .diagrams
                    .iter()
                    .any(|v| v.display.start < range.end && range.start < v.display.end)
                || self
                    .formulas
                    .iter()
                    .any(|v| v.display.start < range.end && range.start < v.display.end)
                || self
                    .tables
                    .iter()
                    .any(|v| v.range.start < range.end && range.start < v.range.end)
            {
                return Err("invalid image failure preview".into());
            }
            end = range.end;
        }
        let metric = |n: f32| n.is_finite() && (0.0..=32768.0).contains(&n);
        if self.code_blocks.iter().any(|v| {
            v.source.indent > 32768 || v.rows.iter().any(|r| self.text.get(r.clone()).is_none())
        }) || self
            .lists
            .iter()
            .any(|v| v.source.item.inset > 32768 || !self.text.is_char_boundary(v.start))
            || self.quotes.iter().any(|v| {
                v.depth == 0
                    || v.depth > 32768
                    || v.inset > 32768
                    || self.text.get(v.display.clone()).is_none()
            })
        {
            return Err("invalid layout container geometry".into());
        }
        let mut end = 0;
        for table in &self.tables {
            if table.source.inset > 32768
                || table.range.is_empty()
                || table.range.start < end
                || self.text.get(table.range.clone()).is_none()
                || table.rows.is_empty()
                || table.source.align.is_empty()
                || table.rows.len() != table.source.rows.len()
            {
                return Err("invalid ordered table range".into());
            }
            end = table.range.end;
            let mut cell_end = table.range.start;
            let mut source_end = table.source.range.start;
            for (row, source) in table.rows.iter().zip(&table.source.rows) {
                if row.len() != table.source.align.len() || row.len() != source.cells.len() {
                    return Err("invalid layout table dimensions".into());
                }
                for (cell, raw) in row.iter().zip(&source.cells) {
                    if self.text.get(cell.clone()).is_none()
                        || cell.start < cell_end
                        || cell.end > table.range.end
                        || raw.content.start < source_end
                        || raw.content.start < source.line.start
                        || raw.content.end > source.line.end
                    {
                        return Err("invalid ordered table cell".into());
                    }
                    cell_end = cell.end;
                    source_end = raw.content.end;
                }
            }
        }
        end = 0;
        for view in &self.formulas {
            let f = &view.formula;
            if view.display.start < end
                || self.text.get(view.display.clone()) != Some(OBJECT)
                || !metric(f.width)
                || !metric(f.ascent)
                || !metric(f.descent)
                || f.width == 0.
                || f.ascent + f.descent == 0.
            {
                return Err("invalid ordered formula object".into());
            }
            end = view.display.end;
        }
        let mut table = 0;
        let mut formula = 0;
        end = 0;
        for view in &self.diagrams {
            let d = &view.diagram;
            let bytes = (d.pixel_width as usize)
                .checked_mul(d.pixel_height as usize)
                .and_then(|n| n.checked_mul(4));
            if view.display.start < end
                || self.text.get(view.display.clone()) != Some(OBJECT)
                || view.source.indent > 32768
                || !metric(d.width)
                || !metric(d.height)
                || d.width == 0.
                || d.height == 0.
                || d.pixel_width == 0
                || d.pixel_height == 0
                || bytes != Some(d.pixels.len())
                || d.bytes != d.pixels.len()
            {
                return Err("invalid ordered diagram object".into());
            }
            while self
                .tables
                .get(table)
                .is_some_and(|t| t.range.end <= view.display.start)
            {
                table += 1;
            }
            while self
                .formulas
                .get(formula)
                .is_some_and(|f| f.display.end <= view.display.start)
            {
                formula += 1;
            }
            if self
                .tables
                .get(table)
                .is_some_and(|t| t.range.start < view.display.end)
                || self
                    .formulas
                    .get(formula)
                    .is_some_and(|f| f.display.start < view.display.end)
            {
                return Err("overlapping layout objects".into());
            }
            end = view.display.end;
        }
        let mut formula = 0;
        let mut diagram = 0;
        let mut failure = 0;
        for span in &self.spans {
            if span.style.image_failure {
                if span.style.math
                    || span.style.diagram
                    || !self
                        .image_failures
                        .get(failure)
                        .is_some_and(|v| v.display == span.display && v.source.range == span.source)
                {
                    return Err("image failure mapping mismatch".into());
                }
                failure += 1;
            } else if span.style.math {
                if span.style.diagram
                    || !self
                        .formulas
                        .get(formula)
                        .is_some_and(|f| f.display == span.display && f.source == span.source)
                {
                    return Err("formula object mapping mismatch".into());
                }
                formula += 1;
            } else if span.style.diagram {
                if !self
                    .diagrams
                    .get(diagram)
                    .is_some_and(|d| d.display == span.display && d.source.range == span.source)
                {
                    return Err("diagram object mapping mismatch".into());
                }
                diagram += 1;
            }
        }
        if formula != self.formulas.len()
            || diagram != self.diagrams.len()
            || failure != self.image_failures.len()
        {
            return Err("unmapped layout object".into());
        }
        Ok(())
    }

    /// A jump must consume text. EOF's ordinary empty line is handled separately.
    pub fn checked_layout_advance(&self, start: usize, end: usize) -> Result<usize, String> {
        if start >= end || self.text.get(start..end).is_none() {
            Err("layout object did not advance on valid boundaries".into())
        } else {
            Ok(end)
        }
    }

    /// Check a complete inline range before either shaping or slicing fragments.
    pub fn validate_inline_layout(&self, range: &Range<usize>) -> Result<(), String> {
        if self.text.get(range.clone()).is_none() {
            return Err("invalid inline layout range".into());
        }
        // Do not binary-search until the ordered-object contract has been checked
        // by validate_layout at the layout build boundary.
        let first = self
            .formulas
            .partition_point(|f| f.display.end <= range.start);
        let mut end = range.start;
        for f in self.formulas[first..]
            .iter()
            .take_while(|f| f.display.start < range.end)
        {
            if f.display.start < end
                || f.display.end > range.end
                || self.text.get(f.display.clone()) != Some(OBJECT)
            {
                return Err("invalid inline formula order or containment".into());
            }
            end = f.display.end;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagram_objects_require_order_unique_complete_ranges_and_matching_spans() {
        let source = format!("{OBJECT}\n{OBJECT}");
        let resource = Arc::new(Diagram {
            pixels: vec![0, 0, 0, 255].into(),
            pixel_width: 1,
            pixel_height: 1,
            width: 1.,
            height: 1.,
            bytes: 4,
        });
        let mut display = Display::plain_source(&source);
        display.spans = vec![
            Span {
                source: 0..3,
                display: 0..3,
                style: Style {
                    diagram: true,
                    ..Default::default()
                },
            },
            Span {
                source: 3..4,
                display: 3..4,
                style: Style::default(),
            },
            Span {
                source: 4..7,
                display: 4..7,
                style: Style {
                    diagram: true,
                    ..Default::default()
                },
            },
        ];
        display.diagrams = [0..3, 4..7]
            .into_iter()
            .map(|range| DiagramView {
                source: PreviewSource {
                    range: range.clone(),
                    scope: range.clone(),
                    anchor: range.start,
                    indent: 0,
                },
                display: range,
                diagram: resource.clone(),
            })
            .collect();
        display.validate(&source).unwrap();
        for mode in 0..6 {
            let mut bad = display.clone();
            match mode {
                0 => bad.diagrams.swap(0, 1),
                1 => bad.diagrams[1] = bad.diagrams[0].clone(),
                2 => bad.diagrams[0].display = 0..0,
                3 => bad.diagrams[0].display = 0..2,
                4 => bad.spans[0].style.diagram = false,
                _ => bad.diagrams.clear(),
            }
            assert!(bad.validate(&source).is_err(), "diagram mode {mode}");
            assert!(bad.validate_layout().is_err());
        }
    }
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
