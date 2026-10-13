use super::*;
impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.editable_text();
        let start = utf16_to_utf8(&text, range.start).ok()?;
        let end = utf16_to_utf8(&text, range.end).ok()?;
        actual.replace(range);
        text.get(start..end).map(String::from)
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let text = self.editable_text();
        let selection = self.selected();
        Some(UTF16Selection {
            range: text[..selection.start].encode_utf16().count()
                ..text[..selection.end].encode_utf16().count(),
            reversed: self.document.composition.is_none()
                && self.document.head < self.document.anchor,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.document.composition.as_ref().map(|c| {
            let start = self.document.source.text()[..c.range.start]
                .encode_utf16()
                .count();
            start..start + c.text.encode_utf16().count()
        })
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.commit_composition(cx);
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editable = self.editable_text();
        if let Some(composition) = self.document.composition.take() {
            // Preserve platform replacement ranges in the temporary text view.
            let marked = composition.range.start..composition.range.start + composition.text.len();
            let requested = range
                .and_then(|r| {
                    Some(
                        utf16_to_utf8(&editable, r.start).ok()?
                            ..utf16_to_utf8(&editable, r.end).ok()?,
                    )
                })
                .unwrap_or(marked.clone());
            if requested == marked {
                self.insert_from(
                    composition.range,
                    text,
                    ryne_extensions::InputOrigin::Ime,
                    cx,
                );
            } else {
                let mut committed = editable;
                if committed.get(requested.clone()).is_none() {
                    self.refresh(cx);
                    return;
                }
                committed.replace_range(requested.clone(), text);
                let end = requested.start + text.len();
                self.insert_from(
                    0..self.document.source.text().len(),
                    &committed,
                    ryne_extensions::InputOrigin::Ime,
                    cx,
                );
                self.document.select(end, false, false);
                self.refresh(cx);
            }
        } else {
            let requested = if let Some(range) = range {
                let (Ok(start), Ok(end)) = (
                    utf16_to_utf8(&editable, range.start),
                    utf16_to_utf8(&editable, range.end),
                ) else {
                    return;
                };
                start..end
            } else {
                self.document.selection()
            };
            self.insert(requested, text, cx);
        }
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editable = self.editable_text();
        let requested = range.and_then(|r| {
            Some(utf16_to_utf8(&editable, r.start).ok()?..utf16_to_utf8(&editable, r.end).ok()?)
        });
        let (range, value, marked_start) = if let Some(old) = self.document.composition.take() {
            let marked = old.range.start..old.range.start + old.text.len();
            let requested = requested.unwrap_or(marked.clone());
            if requested.start < marked.start || requested.end > marked.end {
                self.document.composition = Some(old);
                self.status = Text::CompositionRange.into();
                cx.notify();
                return;
            }
            let start = requested.start - marked.start;
            let mut value = old.text;
            value.replace_range(start..requested.end - marked.start, text);
            (old.range, value, start)
        } else {
            (
                requested.unwrap_or_else(|| self.document.selection()),
                text.to_owned(),
                0,
            )
        };
        if self.document.source.text().get(range.clone()).is_none() {
            return;
        }
        let local = selection
            .and_then(|r| {
                Some(utf16_to_utf8(text, r.start).ok()?..utf16_to_utf8(text, r.end).ok()?)
            })
            .unwrap_or(text.len()..text.len());
        let start = range.start + marked_start;
        self.document.composition = Some(Composition {
            range,
            text: value,
            selection: start + local.start..start + local.end,
        });
        self.preferred_x = None;
        self.caret_affinity = Affinity::Downstream;
        self.ensure_caret = true;
        self.refresh(cx);
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let offset = utf16_to_utf8(&self.editable_text(), range.start).ok()?;
        let geometry = self.geometry.as_ref()?;
        if !geometry.is_current(self) {
            return None;
        }
        let affinity = if range.is_empty() && offset == self.caret() {
            self.caret_affinity
        } else {
            Affinity::Downstream
        };
        let offset = geometry.display.position_at(offset);
        Some(geometry.caret_bounds(offset, affinity))
    }
    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let geometry = self.geometry.as_ref()?;
        if !geometry.is_current(self) {
            return None;
        }
        let (hit, affinity) = geometry.hit_caret(point);
        let source = geometry
            .display
            .source_byte_at(ryne_core::DisplayByte(hit), affinity)
            .0;
        Some(self.editable_text().get(..source)?.encode_utf16().count())
    }
}
