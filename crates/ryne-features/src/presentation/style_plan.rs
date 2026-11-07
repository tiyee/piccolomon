//! Ordered property assignments resolved by an endpoint sweep, without per-byte style arrays.
use super::*;
use std::collections::BTreeMap;
#[derive(Clone, Copy)]
pub(super) enum Attr {
    Hidden,
    Source,
    Heading,
    Bold,
    Italic,
    Strike,
    Code,
    CodeBlock,
    Syntax,
    Link,
    Marker,
    Math,
    Diagram,
}
#[derive(Default)]
pub(super) struct StylePlan {
    changes: Vec<(Range<usize>, Attr, u8)>,
}
pub(super) struct ResolvedStyles {
    runs: Vec<(Range<usize>, Style, bool)>,
}
impl ResolvedStyles {
    pub fn at(&self, at: usize) -> (Style, bool) {
        self.runs
            .get(self.runs.partition_point(|(r, _, _)| r.end <= at))
            .map_or((Style::default(), false), |(_, s, h)| (*s, *h))
    }
    pub fn cursor(&self) -> StyleCursor<'_> {
        StyleCursor {
            styles: self,
            index: 0,
        }
    }
}
pub(super) struct StyleCursor<'a> {
    styles: &'a ResolvedStyles,
    index: usize,
}
impl StyleCursor<'_> {
    pub fn at(&mut self, at: usize) -> (Style, bool) {
        while self
            .styles
            .runs
            .get(self.index)
            .is_some_and(|(r, _, _)| r.end <= at)
        {
            self.index += 1;
        }
        self.styles
            .runs
            .get(self.index)
            .map_or((Style::default(), false), |(_, s, h)| (*s, *h))
    }
}
impl StylePlan {
    pub fn set(&mut self, range: Range<usize>, attr: Attr, value: u8) {
        if !range.is_empty() {
            self.changes.push((range, attr, value));
        }
    }
    pub fn style(&mut self, range: Range<usize>, s: Style) {
        for (attr, value) in [
            (Attr::Source, s.source as u8),
            (Attr::Heading, s.heading),
            (Attr::Bold, s.bold as u8),
            (Attr::Italic, s.italic as u8),
            (Attr::Strike, s.strike as u8),
            (Attr::Code, s.code as u8),
            (Attr::CodeBlock, s.code_block as u8),
            (Attr::Syntax, s.syntax.map_or(0, |c| c as u8 + 1)),
            (Attr::Link, s.link as u8),
            (Attr::Marker, s.marker as u8),
            (Attr::Math, s.math as u8),
            (Attr::Diagram, s.diagram as u8),
        ] {
            self.set(range.clone(), attr, value);
        }
    }
    pub fn resolve(&self, len: usize) -> ResolvedStyles {
        let mut events = Vec::with_capacity(self.changes.len() * 2 + 2);
        for (i, (r, _, _)) in self.changes.iter().enumerate() {
            events.push((r.start, true, i));
            events.push((r.end, false, i));
        }
        events.sort_unstable();
        let mut active: Vec<BTreeMap<usize, u8>> = (0..13).map(|_| BTreeMap::new()).collect();
        let mut runs: Vec<(Range<usize>, Style, bool)> = Vec::new();
        let mut start = 0;
        let mut cursor = 0;
        while start < len {
            while let Some(&(at, enter, i)) = events.get(cursor).filter(|e| e.0 == start) {
                let (_, attr, value) = self.changes[i];
                let _ = at;
                if enter {
                    active[attr as usize].insert(i, value);
                } else {
                    active[attr as usize].remove(&i);
                }
                cursor += 1;
            }
            let end = events.get(cursor).map_or(len, |e| e.0.min(len));
            let get = |a: Attr| active[a as usize].last_key_value().map_or(0, |(_, v)| *v);
            let classes = [
                Class::Comment,
                Class::Keyword,
                Class::String,
                Class::Number,
                Class::Function,
                Class::Type,
                Class::Constant,
                Class::Property,
                Class::Variable,
                Class::Operator,
                Class::Punctuation,
            ];
            let style = Style {
                source: get(Attr::Source) != 0,
                heading: get(Attr::Heading),
                bold: get(Attr::Bold) != 0,
                italic: get(Attr::Italic) != 0,
                strike: get(Attr::Strike) != 0,
                code: get(Attr::Code) != 0,
                code_block: get(Attr::CodeBlock) != 0,
                syntax: get(Attr::Syntax)
                    .checked_sub(1)
                    .and_then(|v| classes.get(v as usize).copied()),
                link: get(Attr::Link) != 0,
                marker: get(Attr::Marker) != 0,
                math: get(Attr::Math) != 0,
                diagram: get(Attr::Diagram) != 0,
                image_failure: false,
            };
            let hidden = get(Attr::Hidden) != 0;
            if let Some((r, s, h)) = runs
                .last_mut()
                .filter(|(_, s, h)| *s == style && *h == hidden)
            {
                let _ = (s, h);
                r.end = end;
            } else {
                runs.push((start..end, style, hidden));
            }
            start = end;
        }
        ResolvedStyles { runs }
    }
}
