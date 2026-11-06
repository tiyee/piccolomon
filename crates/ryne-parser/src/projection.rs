use crate::{
    cursor_boundary, validate, Affinity, Error, NodeKind, ParseSnapshot, PositionMap, Session,
    SnapshotId, Source,
};
use std::{
    ops::Range,
    sync::atomic::{AtomicU64, Ordering},
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    Direct(Range<usize>),
    Transform(Range<usize>),
    MultiSource(Vec<Range<usize>>),
    Generated {
        anchor: usize,
    },
    Object {
        node: usize,
        sources: Vec<Range<usize>>,
    },
}
impl Origin {
    pub fn sources(&self) -> Vec<Range<usize>> {
        match self {
            Self::Direct(r) | Self::Transform(r) => vec![r.clone()],
            Self::MultiSource(r) | Self::Object { sources: r, .. } => r.clone(),
            Self::Generated { anchor } => std::iter::once(*anchor..*anchor).collect(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct Run {
    pub projected: Range<usize>,
    pub origin: Origin,
}
#[derive(Clone, Debug)]
pub struct HiddenSyntax {
    pub source: Range<usize>,
    pub projected: usize,
    pub reveal: Range<usize>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitKind {
    Direct,
    Transform,
    Hidden,
    Generated,
    Object,
}
/// Versioned hit retains provenance and a source window to reveal.
#[derive(Clone, Debug)]
pub struct ProjectionHit {
    pub base: SnapshotId,
    pub projection: u64,
    pub projected: usize,
    pub source: usize,
    pub sources: Vec<Range<usize>>,
    pub reveal: Range<usize>,
    pub kind: HitKind,
}
pub struct Projection {
    pub(crate) base: SnapshotId,
    revision: u64,
    pub text: String,
    pub runs: Vec<Run>,
    pub hidden: Vec<HiddenSyntax>,
    source: Source,
}
static NEXT_PROJECTION: AtomicU64 = AtomicU64::new(1);
/// Source-anchored interaction state. Dragging freezes reveal windows.
#[derive(Clone, Debug)]
pub struct Interaction {
    pub base: SnapshotId,
    pub sessions: Vec<Session>,
    pub selection: Session,
    pub composition: Option<Session>,
    frozen: Option<Vec<Session>>,
}
impl Interaction {
    pub fn new(source: &Source) -> Self {
        Self {
            base: source.id(),
            sessions: Vec::new(),
            selection: Session::new(source, 0..0).unwrap(),
            composition: None,
            frozen: None,
        }
    }
    pub fn enter(&mut self, projection: &Projection, hit: &ProjectionHit) -> Result<(), Error> {
        projection.check_hit(hit)?;
        if self.base != projection.base {
            return Err(Error::StaleSnapshot);
        }
        self.sessions
            .push(Session::new(&projection.source, hit.reveal.clone())?);
        self.selection = Session::new(&projection.source, hit.source..hit.source)?;
        Ok(())
    }
    pub fn leave(&mut self) {
        self.sessions.clear();
    }
    pub fn begin_drag(&mut self) {
        self.frozen = Some(self.sessions.clone());
    }
    pub fn end_drag(&mut self) {
        self.frozen = None;
    }
    pub fn transform(&mut self, map: &PositionMap) -> Result<(), Error> {
        if self.base != map.from {
            return Err(Error::StaleSnapshot);
        }
        let mut next = self.clone();
        for session in &mut next.sessions {
            session.transform(map)?;
        }
        for session in next.frozen.iter_mut().flatten() {
            session.transform(map)?;
        }
        if let Some(session) = &mut next.composition {
            session.transform(map)?;
        }
        next.selection.transform(map)?;
        next.base = map.to;
        *self = next;
        Ok(())
    }
}
/// Build a preview with raw source in active windows. Unsupported block details
/// remain reachable through explicit hidden records and source reveal targets.
pub fn project(parse: &ParseSnapshot, interaction: &Interaction) -> Result<Projection, Error> {
    parse.source.check(interaction.base)?;
    interaction.selection.visible(&parse.source)?;
    let sessions = interaction.frozen.as_ref().unwrap_or(&interaction.sessions);
    let mut windows = sessions
        .iter()
        .chain(interaction.composition.iter())
        .map(|s| {
            parse.source.check(s.base)?;
            validate(parse.source.text(), &s.range)?;
            Ok(s.range.clone())
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let mut leaves = Vec::new();
    collect_leaves(parse, 0, &mut leaves);
    // Atomic transforms cannot be partially replaced with raw source.
    for window in &mut windows {
        for (_, origin) in &leaves {
            if !matches!(origin, Origin::Direct(_)) {
                for r in origin.sources() {
                    if window.start < r.end && r.start < window.end {
                        window.start = window.start.min(r.start);
                        window.end = window.end.max(r.end);
                    }
                }
            }
        }
    }
    windows.sort_by_key(|r| r.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for window in windows {
        if let Some(last) = merged.last_mut().filter(|r| r.end >= window.start) {
            last.end = last.end.max(window.end);
        } else {
            merged.push(window);
        }
    }
    let mut pieces: Vec<(String, Origin)> = Vec::new();
    for (value, origin) in leaves {
        let spans = origin.sources();
        let r = spans.first().unwrap().start..spans.last().unwrap().end;
        if matches!(origin, Origin::Direct(_)) {
            let mut p = r.start;
            for window in &merged {
                if window.end <= p || window.start >= r.end {
                    continue;
                }
                if p < window.start {
                    pieces.push((
                        parse.source.text()[p..window.start].into(),
                        Origin::Direct(p..window.start),
                    ));
                }
                p = p.max(window.end).min(r.end);
            }
            if p < r.end {
                pieces.push((
                    parse.source.text()[p..r.end].into(),
                    Origin::Direct(p..r.end),
                ));
            }
        } else if !merged.iter().any(|w| {
            if r.is_empty() {
                w.start <= r.start && r.start <= w.end
            } else {
                w.start < r.end && r.start < w.end
            }
        }) {
            pieces.push((value, origin));
        }
    }
    for window in merged {
        let mut cursor = window.start;
        let first = parse
            .syntax
            .tokens
            .partition_point(|t| t.range.end <= window.start);
        for token in parse.syntax.tokens[first..]
            .iter()
            .take_while(|t| t.range.start < window.end)
        {
            let container = &parse.syntax.nodes[token.owner].envelope;
            if token.kind != crate::TokenKind::ContainerPrefix
                || (window.start <= container.start && container.end <= window.end)
            {
                continue;
            }
            let prefix_start = token.range.start.max(window.start);
            if cursor < prefix_start {
                pieces.push((
                    parse.source.text()[cursor..prefix_start].into(),
                    Origin::Direct(cursor..prefix_start),
                ));
            }
            cursor = cursor.max(token.range.end.min(window.end));
        }
        if cursor < window.end || window.is_empty() {
            pieces.push((
                parse.source.text()[cursor..window.end].into(),
                Origin::Direct(cursor..window.end),
            ));
        }
    }
    pieces.sort_by_key(|(_, origin)| origin.sources()[0].start);
    let mut projection = Projection {
        base: parse.source.id(),
        revision: NEXT_PROJECTION.fetch_add(1, Ordering::Relaxed),
        text: String::new(),
        runs: Vec::new(),
        hidden: Vec::new(),
        source: parse.source.clone(),
    };
    for (value, origin) in pieces {
        let start = projection.text.len();
        projection.text.push_str(&value);
        projection.runs.push(Run {
            projected: start..projection.text.len(),
            origin,
        });
    }
    // Every omitted source interval has an explicit mapping and reveal target.
    let mut covered = projection
        .runs
        .iter()
        .flat_map(|r| r.origin.sources())
        .filter(|r| !r.is_empty())
        .collect::<Vec<_>>();
    covered.sort_by_key(|r| r.start);
    let mut end = 0;
    for range in covered.into_iter().chain(std::iter::once(
        parse.source.text().len()..parse.source.text().len(),
    )) {
        if end < range.start {
            let first = parse.syntax.tokens.partition_point(|t| t.range.end <= end);
            for token in parse.syntax.tokens[first..]
                .iter()
                .take_while(|t| t.range.start < range.start)
            {
                let source = token.range.start.max(end)..token.range.end.min(range.start);
                let target = parse.syntax.nodes[token.owner].envelope.clone();
                let next_run = projection
                    .runs
                    .partition_point(|r| r.origin.sources()[0].start < source.end);
                let projected = projection
                    .runs
                    .get(next_run)
                    .map_or(projection.text.len(), |r| r.projected.start);
                projection.hidden.push(HiddenSyntax {
                    source,
                    projected,
                    reveal: target,
                });
            }
        }
        end = end.max(range.end);
    }
    Ok(projection)
}
fn collect_leaves(parse: &ParseSnapshot, id: usize, out: &mut Vec<(String, Origin)>) {
    let node = &parse.nodes[id];
    let r = node.envelope.clone();
    let value = match &node.kind {
        NodeKind::Text(s) | NodeKind::Code(s) | NodeKind::Html(s) => Some(s.clone()),
        NodeKind::SoftBreak | NodeKind::HardBreak => Some("\n".into()),
        NodeKind::Image { .. }
        | NodeKind::Rule
        | NodeKind::Task(_)
        | NodeKind::InlineMath(_)
        | NodeKind::DisplayMath(_) => {
            out.push((
                "\u{fffc}".into(),
                Origin::Object {
                    node: id,
                    sources: vec![r],
                },
            ));
            return;
        }
        NodeKind::SourceBlock => Some(parse.source.text()[r.clone()].into()),
        _ => None,
    };
    if let Some(value) = value {
        let origin = if parse.source.text()[r.clone()] == value {
            Origin::Direct(r)
        } else {
            let content = &parse.syntax.nodes[id].content;
            if content.len() > 1 {
                Origin::MultiSource(content.clone())
            } else {
                Origin::Transform(r)
            }
        };
        out.push((value, origin));
    } else {
        let mut previous_block = false;
        for &child in &node.children {
            let block = matches!(
                parse.nodes[child].kind,
                NodeKind::Paragraph
                    | NodeKind::Heading(_)
                    | NodeKind::BlockQuote
                    | NodeKind::List(_)
                    | NodeKind::Item
                    | NodeKind::Table(_)
                    | NodeKind::TableRow
                    | NodeKind::TableHead
                    | NodeKind::CodeBlock(_)
                    | NodeKind::HtmlBlock
                    | NodeKind::Rule
            );
            if previous_block && block {
                out.push((
                    "\n".into(),
                    Origin::Generated {
                        anchor: parse.nodes[child].envelope.start,
                    },
                ));
            }
            collect_leaves(parse, child, out);
            previous_block = block;
        }
    }
}
/// Convenience preview; use project() and versioned hits for editing.
pub fn preview_inline(source: &str) -> Projection {
    let parse = ParseSnapshot::new(Source::new(source));
    project(&parse, &Interaction::new(&parse.source)).unwrap()
}
impl Projection {
    pub fn base(&self) -> SnapshotId {
        self.base
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn check_hit(&self, hit: &ProjectionHit) -> Result<(), Error> {
        self.source.check(hit.base)?;
        if hit.projection != self.revision {
            return Err(Error::StaleProjection);
        }
        self.validate_cursor(hit.projected)?;
        validate(self.source.text(), &(hit.source..hit.source))
    }
    /// Validate graphemes on the complete displayed string, across run boundaries.
    pub fn validate_cursor(&self, offset: usize) -> Result<(), Error> {
        validate(&self.text, &(offset..offset))?;
        if offset == self.text.len() || self.text.grapheme_indices(true).any(|(p, _)| p == offset) {
            Ok(())
        } else {
            Err(Error::SplitGrapheme)
        }
    }
    pub fn hit(&self, offset: usize, affinity: Affinity) -> Result<ProjectionHit, Error> {
        self.validate_cursor(offset)?;
        let run = match affinity {
            Affinity::Upstream => self
                .runs
                .iter()
                .rev()
                .find(|r| r.projected.start < offset && offset <= r.projected.end)
                .or_else(|| self.runs.first()),
            Affinity::Downstream => self
                .runs
                .iter()
                .find(|r| r.projected.start <= offset && offset < r.projected.end)
                .or_else(|| self.runs.last()),
        };
        let Some(run) = run else {
            return Ok(ProjectionHit {
                base: self.base,
                projection: self.revision,
                projected: offset,
                source: 0,
                sources: std::iter::once(0..self.source.text().len()).collect(),
                reveal: 0..self.source.text().len(),
                kind: HitKind::Hidden,
            });
        };
        let sources = run.origin.sources();
        let range = sources.first().unwrap().start..sources.last().unwrap().end;
        let (source, kind) = match &run.origin {
            Origin::Direct(r) => (
                r.start + offset.saturating_sub(run.projected.start).min(r.len()),
                HitKind::Direct,
            ),
            Origin::Transform(_) | Origin::MultiSource(_) => (
                if affinity == Affinity::Upstream {
                    range.start
                } else {
                    range.end
                },
                HitKind::Transform,
            ),
            Origin::Generated { anchor } => (*anchor, HitKind::Generated),
            Origin::Object { .. } => (
                if affinity == Affinity::Upstream {
                    range.start
                } else {
                    range.end
                },
                HitKind::Object,
            ),
        };
        let source = cursor_boundary(self.source.text(), source, affinity)?;
        let reveal = self
            .hidden
            .iter()
            .filter(|h| h.reveal.start <= range.start && range.end <= h.reveal.end)
            .min_by_key(|h| h.reveal.len())
            .map_or(range, |h| h.reveal.clone());
        Ok(ProjectionHit {
            base: self.base,
            projection: self.revision,
            projected: offset,
            source,
            sources,
            reveal,
            kind,
        })
    }
    /// Convenience endpoint mapping; editing callers should retain hit provenance.
    pub fn resolve(&self, offset: usize, affinity: Affinity) -> Option<usize> {
        self.hit(offset, affinity).ok().map(|h| h.source)
    }
    fn snap_cursor(&self, offset: usize, affinity: Affinity) -> usize {
        self.text
            .grapheme_indices(true)
            .map(|(p, _)| p)
            .chain(std::iter::once(self.text.len()))
            .filter(|&p| {
                if affinity == Affinity::Upstream {
                    p <= offset
                } else {
                    p >= offset
                }
            })
            .reduce(|a, b| {
                if affinity == Affinity::Upstream {
                    a.max(b)
                } else {
                    a.min(b)
                }
            })
            .unwrap_or(offset)
    }
    pub fn source_to_projection(
        &self,
        source: &Source,
        offset: usize,
        affinity: Affinity,
    ) -> Result<ProjectionHit, Error> {
        source.check(self.base)?;
        validate(source.text(), &(offset..offset))?;
        if let Some(hidden) = self.hidden.iter().find(|h| {
            h.source.start <= offset
                && (offset < h.source.end
                    || (offset == self.source.text().len() && h.source.end == offset))
        }) {
            return Ok(ProjectionHit {
                base: self.base,
                projection: self.revision,
                projected: self.snap_cursor(hidden.projected, affinity),
                source: offset,
                sources: vec![hidden.source.clone()],
                reveal: hidden.reveal.clone(),
                kind: HitKind::Hidden,
            });
        }
        let runs = self
            .runs
            .iter()
            .filter(|r| {
                r.origin
                    .sources()
                    .iter()
                    .any(|s| s.start <= offset && offset <= s.end)
            })
            .collect::<Vec<_>>();
        let run = if affinity == Affinity::Upstream {
            runs.first()
        } else {
            runs.last()
        };
        if let Some(run) = run {
            let projected = if let Origin::Direct(range) = &run.origin {
                run.projected.start + offset.saturating_sub(range.start).min(range.len())
            } else if affinity == Affinity::Upstream {
                run.projected.start
            } else {
                run.projected.end
            };
            // Source byte positions may lie inside a final grapheme; locate its edge.
            let projected = self.snap_cursor(projected, affinity);
            return self.hit(projected, affinity);
        }
        self.hit(
            if affinity == Affinity::Upstream {
                0
            } else {
                self.text.len()
            },
            affinity,
        )
    }
}
