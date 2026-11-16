use super::*;
impl EntityInputHandler for Editor {
    fn paste(&mut self, item: ClipboardItem, _: &mut Window, cx: &mut Context<Self>) {
        self.paste_content(item.text(), cx);
    }

    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.document.editable_text_cow();
        let start = self.document.editable_utf16_to_byte(range.start).ok()?;
        let end = self.document.editable_utf16_to_byte(range.end).ok()?;
        actual.replace(range);
        text.get(start..end).map(String::from)
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let selection = self.selected();
        Some(UTF16Selection {
            range: self.document.editable_byte_to_utf16(selection.start).ok()?
                ..self.document.editable_byte_to_utf16(selection.end).ok()?,
            reversed: self.document.composition().is_none()
                && self.document.head < self.document.anchor,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.document.composition().and_then(|c| {
            let start = self.document.editable_byte_to_utf16(c.range.start).ok()?;
            Some(start..start + c.text.encode_utf16().count())
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
        let requested = match range {
            Some(r) => match (
                self.document.editable_utf16_to_byte(r.start),
                self.document.editable_utf16_to_byte(r.end),
            ) {
                (Ok(start), Ok(end)) => Some(start..end),
                _ => return,
            },
            None => None,
        };
        if self.document.composition().is_some() {
            if let Err(error) = self.document.commit_composition_text(requested, text) {
                self.status = self.locale.language.error(Text::InputFailed, error);
            }
            self.caret_state.preferred_x = None;
            self.caret_state.caret_affinity = Affinity::Downstream;
            self.viewport.ensure_caret = true;
            self.refresh(cx);
        } else {
            self.insert(
                requested.unwrap_or_else(|| self.document.selection()),
                text,
                cx,
            );
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
        let convert = |value: &str, r: Range<usize>| {
            Some(utf16_to_utf8(value, r.start).ok()?..utf16_to_utf8(value, r.end).ok()?)
        };
        let requested = match range {
            Some(r) => match self
                .document
                .editable_utf16_to_byte(r.start)
                .ok()
                .zip(self.document.editable_utf16_to_byte(r.end).ok())
                .map(|(a, b)| a..b)
            {
                Some(r) => Some(r),
                None => return,
            },
            None => None,
        };
        // Some IMEs report a caret inside a UTF-16 surrogate pair. Keep the
        // valid preedit and use its end, rather than dropping the whole update.
        let local = selection.and_then(|r| convert(text, r));
        if self
            .document
            .update_composition(requested, text, local)
            .is_err()
        {
            self.status = Text::CompositionRange.into();
            cx.notify();
            return;
        }
        self.caret_state.preferred_x = None;
        self.caret_state.caret_affinity = Affinity::Downstream;
        self.viewport.ensure_caret = true;
        self.refresh(cx);
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let offset = self.document.editable_utf16_to_byte(range.start).ok()?;
        let geometry = self.viewport.geometry.as_ref()?;
        if !geometry.is_current(self) {
            return None;
        }
        let affinity = if range.is_empty() && offset == self.caret() {
            self.caret_state.caret_affinity
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
        let geometry = self.viewport.geometry.as_ref()?;
        if !geometry.is_current(self) {
            return None;
        }
        let (hit, affinity) = geometry.hit_caret(point);
        let source = geometry
            .display
            .source_byte_at(ryne_core::DisplayByte(hit), affinity)
            .0;
        self.document.editable_byte_to_utf16(source).ok()
    }
}
