use crate::{options, serialize, Source};
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag};
use std::{collections::BTreeMap, ops::Range};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    Gfm029,
    CommonMark029,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    None,
    Left,
    Center,
    Right,
}
/// Backend-independent semantic kinds. IDs are indices local to ParseSnapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Document,
    Paragraph,
    Heading(u8),
    BlockQuote,
    CodeBlock(String),
    HtmlBlock,
    List(Option<u64>),
    Item,
    Table(Vec<Alignment>),
    TableHead,
    TableRow,
    TableCell,
    Emphasis,
    Strong,
    Strikethrough,
    Link {
        destination: String,
        title: String,
        reference: String,
    },
    Image {
        destination: String,
        title: String,
        reference: String,
    },
    Text(String),
    Code(String),
    /// Literal LaTeX between single dollar delimiters (opt-in math extension).
    InlineMath(String),
    /// Literal LaTeX between double dollar delimiters (opt-in math extension).
    DisplayMath(String),
    Html(String),
    SoftBreak,
    HardBreak,
    Rule,
    Task(bool),
    SourceBlock,
}
#[derive(Clone, Debug)]
pub struct SemanticNode {
    pub kind: NodeKind,
    pub envelope: Range<usize>,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Text,
    Transform,
    OpenMarker,
    CloseMarker,
    ContainerPrefix,
    TaskMarker,
    Trivia,
    Source,
}
#[derive(Clone, Debug)]
pub struct SyntaxToken {
    pub range: Range<usize>,
    pub kind: TokenKind,
    pub owner: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxKind {
    Document,
    Paragraph,
    Heading,
    BlockQuote,
    CodeBlock,
    HtmlBlock,
    List,
    Item,
    Table,
    TableHead,
    TableRow,
    TableCell,
    Emphasis,
    Strong,
    Strikethrough,
    Link,
    Image,
    Text,
    Code,
    InlineMath,
    DisplayMath,
    Html,
    SoftBreak,
    HardBreak,
    Rule,
    Task,
    SourceBlock,
}
impl From<&NodeKind> for SyntaxKind {
    fn from(kind: &NodeKind) -> Self {
        match kind {
            NodeKind::Document => Self::Document,
            NodeKind::Paragraph => Self::Paragraph,
            NodeKind::Heading(_) => Self::Heading,
            NodeKind::BlockQuote => Self::BlockQuote,
            NodeKind::CodeBlock(_) => Self::CodeBlock,
            NodeKind::HtmlBlock => Self::HtmlBlock,
            NodeKind::List(_) => Self::List,
            NodeKind::Item => Self::Item,
            NodeKind::Table(_) => Self::Table,
            NodeKind::TableHead => Self::TableHead,
            NodeKind::TableRow => Self::TableRow,
            NodeKind::TableCell => Self::TableCell,
            NodeKind::Emphasis => Self::Emphasis,
            NodeKind::Strong => Self::Strong,
            NodeKind::Strikethrough => Self::Strikethrough,
            NodeKind::Link { .. } => Self::Link,
            NodeKind::Image { .. } => Self::Image,
            NodeKind::Text(_) => Self::Text,
            NodeKind::Code(_) => Self::Code,
            NodeKind::InlineMath(_) => Self::InlineMath,
            NodeKind::DisplayMath(_) => Self::DisplayMath,
            NodeKind::Html(_) => Self::Html,
            NodeKind::SoftBreak => Self::SoftBreak,
            NodeKind::HardBreak => Self::HardBreak,
            NodeKind::Rule => Self::Rule,
            NodeKind::Task(_) => Self::Task,
            NodeKind::SourceBlock => Self::SourceBlock,
        }
    }
}
#[derive(Clone, Debug)]
pub struct SyntaxNode {
    pub semantic: usize,
    pub kind: SyntaxKind,
    pub envelope: Range<usize>,
    pub children: Vec<usize>,
    pub owned_tokens: Vec<usize>,
    /// Ordered content fragments; container prefixes are excluded.
    pub content: Vec<Range<usize>>,
}
#[derive(Clone, Debug)]
pub struct LosslessSyntax {
    base: crate::SnapshotId,
    pub tokens: Vec<SyntaxToken>,
    pub nodes: Vec<SyntaxNode>,
}
impl LosslessSyntax {
    pub fn serialize(&self, source: &Source) -> Result<String, crate::Error> {
        source.check(self.base)?;
        let mut end = 0;
        for token in &self.tokens {
            if token.range.start != end
                || token.range.is_empty()
                || token.range.end > source.text().len()
                || !source.text().is_char_boundary(token.range.start)
                || !source.text().is_char_boundary(token.range.end)
            {
                return Err(crate::Error::InvalidRange);
            }
            end = token.range.end;
        }
        if end != source.text().len() {
            return Err(crate::Error::InvalidRange);
        }
        Ok(serialize(
            source.text(),
            &self
                .tokens
                .iter()
                .map(|t| t.range.clone())
                .collect::<Vec<_>>(),
        ))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Destination,
    Title,
}
#[derive(Clone, Debug)]
pub struct FieldOrigin {
    pub node: usize,
    pub field: FieldKind,
    pub sources: Vec<Range<usize>>,
    /// False means provenance is atomic/conservative, never interpolated.
    pub direct: bool,
}
#[derive(Clone, Debug)]
pub struct Reference {
    pub label: String,
    pub definition: Option<Range<usize>>,
    pub destination: Option<String>,
    pub users: Vec<usize>,
}
/// One immutable parse result owns its source, semantic tree, lossless syntax,
/// field provenance and reference dependency index. No backend events escape.
#[derive(Clone, Debug)]
pub struct ParseSnapshot {
    pub(crate) source: Source,
    pub dialect: Dialect,
    /// Whether the non-GFM dollar-math extension is enabled.
    pub math_enabled: bool,
    pub nodes: Vec<SemanticNode>,
    pub syntax: LosslessSyntax,
    pub fields: Vec<FieldOrigin>,
    pub references: BTreeMap<String, Reference>,
}
fn tag_kind(tag: Tag<'_>) -> NodeKind {
    match tag {
        Tag::Paragraph => NodeKind::Paragraph,
        Tag::Heading { level, .. } => NodeKind::Heading(level as u8),
        Tag::BlockQuote(_) => NodeKind::BlockQuote,
        Tag::CodeBlock(k) => NodeKind::CodeBlock(match k {
            CodeBlockKind::Fenced(s) => s.into_string(),
            CodeBlockKind::Indented => String::new(),
        }),
        Tag::HtmlBlock => NodeKind::HtmlBlock,
        Tag::List(n) => NodeKind::List(n),
        Tag::Item => NodeKind::Item,
        Tag::Table(a) => NodeKind::Table(
            a.into_iter()
                .map(|a| match a {
                    pulldown_cmark::Alignment::None => Alignment::None,
                    pulldown_cmark::Alignment::Left => Alignment::Left,
                    pulldown_cmark::Alignment::Center => Alignment::Center,
                    pulldown_cmark::Alignment::Right => Alignment::Right,
                })
                .collect(),
        ),
        Tag::TableHead => NodeKind::TableHead,
        Tag::TableRow => NodeKind::TableRow,
        Tag::TableCell => NodeKind::TableCell,
        Tag::Emphasis => NodeKind::Emphasis,
        Tag::Strong => NodeKind::Strong,
        Tag::Strikethrough => NodeKind::Strikethrough,
        Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        } => NodeKind::Link {
            destination: if link_type == pulldown_cmark::LinkType::Email {
                format!("mailto:{dest_url}")
            } else {
                dest_url.into_string()
            },
            title: title.into_string(),
            reference: id.into_string(),
        },
        Tag::Image {
            dest_url,
            title,
            id,
            ..
        } => NodeKind::Image {
            destination: dest_url.into_string(),
            title: title.into_string(),
            reference: id.into_string(),
        },
        _ => NodeKind::SourceBlock,
    }
}
pub(crate) fn normalize_label(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}
impl ParseSnapshot {
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn new(source: Source) -> Self {
        Self::with_dialect(source, Dialect::Gfm029)
    }
    pub fn with_dialect(source: Source, dialect: Dialect) -> Self {
        Self::with_dialect_and_math(source, dialect, false)
    }
    /// Parse GFM with literal inline `$...$` and display `$$...$$` math nodes.
    /// Envelopes include delimiters; syntax content ranges exclude them. All
    /// positions are half-open UTF-8 byte ranges in the unchanged source.
    ///
    /// ```
    /// use ryne_parser::{NodeKind, ParseSnapshot, Source};
    /// let parse = ParseSnapshot::with_math(Source::new("value $x_1$"));
    /// assert!(parse.nodes.iter().any(|n| n.kind == NodeKind::InlineMath("x_1".into())));
    /// assert_eq!(parse.syntax.serialize(parse.source()).unwrap(), "value $x_1$");
    /// ```
    pub fn with_math(source: Source) -> Self {
        Self::with_dialect_and_math(source, Dialect::Gfm029, true)
    }
    /// Parse a dialect with an explicit math-extension setting. The default
    /// constructors keep math disabled and retain the fixed GFM baseline.
    pub fn with_dialect_and_math(source: Source, dialect: Dialect, math_enabled: bool) -> Self {
        let mut unresolved = Vec::new();
        let mut callback = |link: pulldown_cmark::BrokenLink<'_>| {
            unresolved.push((normalize_label(&link.reference), link.span));
            None
        };
        let parser = Parser::new_with_broken_link_callback(
            source.text(),
            (if dialect == Dialect::Gfm029 {
                options()
            } else {
                pulldown_cmark::Options::empty()
            }) | if math_enabled {
                pulldown_cmark::Options::ENABLE_MATH
            } else {
                pulldown_cmark::Options::empty()
            },
            Some(&mut callback),
        );
        let mut references = BTreeMap::new();
        for (label, definition) in parser.reference_definitions().iter() {
            let label = normalize_label(label);
            references.insert(
                label.clone(),
                Reference {
                    label,
                    definition: Some(definition.span.clone()),
                    destination: Some(definition.dest.to_string()),
                    users: Vec::new(),
                },
            );
        }
        let mut nodes = vec![SemanticNode {
            kind: NodeKind::Document,
            envelope: 0..source.text().len(),
            parent: None,
            children: Vec::new(),
        }];
        let mut stack = vec![0];
        for (event, range) in parser.into_offset_iter() {
            if matches!(event, Event::End(_)) {
                stack.pop();
                continue;
            }
            let container = matches!(event, Event::Start(_));
            let kind = match event {
                Event::Start(tag) => tag_kind(tag),
                Event::Text(s) => NodeKind::Text(s.into_string()),
                Event::Code(s) => NodeKind::Code(s.into_string()),
                Event::InlineMath(s) => NodeKind::InlineMath(s.into_string()),
                Event::DisplayMath(s) => NodeKind::DisplayMath(s.into_string()),
                Event::Html(s) | Event::InlineHtml(s) => NodeKind::Html(s.into_string()),
                Event::SoftBreak => NodeKind::SoftBreak,
                Event::HardBreak => NodeKind::HardBreak,
                Event::Rule => NodeKind::Rule,
                Event::TaskListMarker(b) => NodeKind::Task(b),
                _ => NodeKind::SourceBlock,
            };
            let id = nodes.len();
            let parent = *stack.last().unwrap();
            if let NodeKind::Link { reference, .. } | NodeKind::Image { reference, .. } = &kind {
                if !reference.is_empty() {
                    let label = normalize_label(reference);
                    references
                        .entry(label.clone())
                        .or_insert(Reference {
                            label,
                            definition: None,
                            destination: None,
                            users: Vec::new(),
                        })
                        .users
                        .push(id);
                }
            }
            nodes.push(SemanticNode {
                kind,
                envelope: range,
                parent: Some(parent),
                children: Vec::new(),
            });
            nodes[parent].children.push(id);
            if container {
                stack.push(id);
            }
        }
        for (label, span) in unresolved {
            let user = nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.envelope.start <= span.start && span.end <= n.envelope.end)
                .min_by_key(|(_, n)| n.envelope.len())
                .map_or(0, |(id, _)| id);
            references
                .entry(label.clone())
                .or_insert(Reference {
                    label,
                    definition: None,
                    destination: None,
                    users: Vec::new(),
                })
                .users
                .push(user);
        }
        if dialect == Dialect::Gfm029 {
            crate::render::extend_autolinks(&mut nodes, source.text());
        }
        let syntax = build_syntax(&source, &nodes);
        let mut fields = Vec::new();
        for (id, node) in nodes.iter().enumerate() {
            match &node.kind {
                NodeKind::Text(value)
                | NodeKind::Code(value)
                | NodeKind::Html(value)
                | NodeKind::InlineMath(value)
                | NodeKind::DisplayMath(value) => fields.push(FieldOrigin {
                    node: id,
                    field: FieldKind::Text,
                    sources: syntax.nodes[id].content.clone(),
                    direct: source.text().get(node.envelope.clone()) == Some(value),
                }),
                NodeKind::Link { .. } | NodeKind::Image { .. } => {
                    for field in [FieldKind::Destination, FieldKind::Title] {
                        // A reference destination depends on its definition, not its user.
                        let reference = match &node.kind {
                            NodeKind::Link { reference, .. }
                            | NodeKind::Image { reference, .. } => reference,
                            _ => unreachable!(),
                        };
                        let span = references
                            .get(&normalize_label(reference))
                            .and_then(|r| r.definition.clone())
                            .unwrap_or_else(|| node.envelope.clone());
                        fields.push(FieldOrigin {
                            node: id,
                            field,
                            sources: vec![span],
                            direct: false,
                        });
                    }
                }
                _ => (),
            }
        }
        Self {
            source,
            dialect,
            math_enabled,
            nodes,
            syntax,
            fields,
            references,
        }
    }
}
fn build_syntax(source: &Source, nodes: &[SemanticNode]) -> LosslessSyntax {
    let text = source.text();
    // A byte ledger is converted into contiguous tokens after ownership claims.
    // Later/deeper claims override envelopes; container claims override inline gaps.
    let mut ledger = vec![(0, TokenKind::Trivia); text.len()];
    let mut claim = |range: Range<usize>, id, kind| {
        for slot in &mut ledger[range] {
            *slot = (id, kind);
        }
    };
    for (id, node) in nodes.iter().enumerate().skip(1) {
        let r = node.envelope.clone();
        if r.is_empty() {
            continue;
        }
        match &node.kind {
            NodeKind::Table(_) | NodeKind::TableHead | NodeKind::TableRow | NodeKind::TableCell => {
                claim(r, id, TokenKind::Source)
            }
            NodeKind::Paragraph
            | NodeKind::BlockQuote
            | NodeKind::List(_)
            | NodeKind::Item
            | NodeKind::HtmlBlock => claim(r, id, TokenKind::Trivia),
            NodeKind::CodeBlock(_) => {
                claim(r.clone(), id, TokenKind::Source);
                let body = &text[r.clone()];
                let first_end = body.find(['\r', '\n']).unwrap_or(body.len());
                let first = &body[..first_end];
                let trimmed = first.trim_start_matches([' ', '\t']);
                if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                    claim(r.start..r.start + first_end, id, TokenKind::OpenMarker);
                    let trimmed_end =
                        r.end - body.len() + body.trim_end_matches(['\r', '\n']).len();
                    let last_start = text[r.start..trimmed_end]
                        .rfind(['\r', '\n'])
                        .map_or(r.start, |p| r.start + p + 1);
                    let last = text[last_start..trimmed_end].trim_start_matches([' ', '\t']);
                    if last_start > r.start
                        && last.starts_with(trimmed.chars().next().unwrap())
                        && last
                            .chars()
                            .all(|c| c == trimmed.chars().next().unwrap() || c.is_whitespace())
                    {
                        claim(last_start..trimmed_end, id, TokenKind::CloseMarker);
                    }
                }
            }
            NodeKind::Heading(_) => {
                claim(r.clone(), id, TokenKind::Source);
                let raw = &text[r.clone()];
                if raw.starts_with('#') {
                    let width = raw
                        .bytes()
                        .take_while(|b| matches!(b, b'#' | b' ' | b'\t'))
                        .count();
                    claim(r.start..r.start + width, id, TokenKind::OpenMarker);
                }
                let trimmed_end = r.start + raw.trim_end_matches(['\r', '\n']).len();
                let last_start = text[r.start..trimmed_end]
                    .rfind(['\r', '\n'])
                    .map_or(r.start, |p| r.start + p + 1);
                if last_start > r.start
                    && text[last_start..trimmed_end]
                        .trim()
                        .bytes()
                        .all(|b| matches!(b, b'-' | b'='))
                {
                    claim(last_start..trimmed_end, id, TokenKind::CloseMarker);
                }
            }
            NodeKind::Rule => claim(r, id, TokenKind::OpenMarker),
            NodeKind::Text(value) => claim(
                r.clone(),
                id,
                if &text[r] == value {
                    TokenKind::Text
                } else {
                    TokenKind::Transform
                },
            ),
            NodeKind::SoftBreak | NodeKind::HardBreak => claim(r, id, TokenKind::Transform),
            NodeKind::Html(_) | NodeKind::SourceBlock => claim(r, id, TokenKind::Source),
            NodeKind::Task(_) => claim(r, id, TokenKind::TaskMarker),
            NodeKind::Emphasis
            | NodeKind::Strong
            | NodeKind::Strikethrough
            | NodeKind::Link { .. }
            | NodeKind::Image { .. }
            | NodeKind::Code(_)
            | NodeKind::InlineMath(_)
            | NodeKind::DisplayMath(_) => {
                claim(r.clone(), id, TokenKind::Source);
                if matches!(node.kind, NodeKind::Link { .. } | NodeKind::Image { .. }) {
                    if let (Some(&first), Some(&last)) =
                        (node.children.first(), node.children.last())
                    {
                        let start = nodes[first].envelope.start;
                        let end = nodes[last].envelope.end;
                        claim(r.start..start, id, TokenKind::OpenMarker);
                        claim(end..r.end, id, TokenKind::CloseMarker);
                    }
                }
                let n = match node.kind {
                    NodeKind::Emphasis => 1,
                    NodeKind::Strong => 2,
                    NodeKind::Strikethrough => {
                        text[r.clone()].bytes().take_while(|b| *b == b'~').count()
                    }
                    NodeKind::Code(_) => text[r.clone()].bytes().take_while(|b| *b == b'`').count(),
                    NodeKind::InlineMath(_) => 1,
                    NodeKind::DisplayMath(_) => 2,
                    NodeKind::Image { .. } => 2,
                    _ => 1,
                };
                if n > 0
                    && r.len() >= 2 * n
                    && !matches!(node.kind, NodeKind::Link { .. } | NodeKind::Image { .. })
                {
                    claim(r.start..r.start + n, id, TokenKind::OpenMarker);
                    claim(r.end - n..r.end, id, TokenKind::CloseMarker);
                    if matches!(node.kind, NodeKind::Code(_)) {
                        claim(r.start + n..r.end - n, id, TokenKind::Transform);
                    } else if matches!(
                        node.kind,
                        NodeKind::InlineMath(_) | NodeKind::DisplayMath(_)
                    ) {
                        claim(r.start + n..r.end - n, id, TokenKind::Text);
                    }
                }
            }
            _ => (),
        }
    }
    // Scan active containers from outermost to innermost on every source line.
    // A container prefix is never owned by an inline envelope spanning that line.
    let containers = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n.kind, NodeKind::BlockQuote | NodeKind::Item))
        .collect::<Vec<_>>();
    let mut container_cursor = 0;
    let mut active = Vec::new();
    let mut line = 0;
    while line < text.len() {
        let content_end = text[line..]
            .find(['\r', '\n'])
            .map_or(text.len(), |p| line + p);
        let mut end = content_end;
        if text.as_bytes().get(end) == Some(&b'\r') {
            end += 1;
        }
        if text.as_bytes().get(end) == Some(&b'\n') {
            end += 1;
        }
        active.retain(|&index: &usize| containers[index].1.envelope.end > line);
        while container_cursor < containers.len()
            && containers[container_cursor].1.envelope.start < end
        {
            active.push(container_cursor);
            container_cursor += 1;
        }
        let mut p = line;
        for &index in &active {
            let (id, node) = containers[index];
            if matches!(node.kind, NodeKind::BlockQuote) {
                if let Some(r) = quote_prefix(text, p, content_end) {
                    p = r.end;
                    claim(r, id, TokenKind::ContainerPrefix);
                }
            } else if matches!(node.kind, NodeKind::Item) {
                let start = p;
                while p < content_end && matches!(text.as_bytes()[p], b' ' | b'\t') {
                    p += 1;
                }
                if line <= node.envelope.start && node.envelope.start < end {
                    p = p.max(node.envelope.start);
                    while p < content_end && text.as_bytes()[p].is_ascii_digit() {
                        p += 1;
                    }
                    if p < content_end
                        && matches!(text.as_bytes()[p], b'-' | b'+' | b'*' | b'.' | b')')
                    {
                        p += 1;
                    }
                    while p < content_end && matches!(text.as_bytes()[p], b' ' | b'\t') {
                        p += 1;
                    }
                }
                if start < p {
                    claim(start..p, id, TokenKind::ContainerPrefix);
                }
            }
        }
        line = end;
    }
    let mut tokens: Vec<SyntaxToken> = Vec::new();
    for (start, (owner, kind)) in ledger.into_iter().enumerate() {
        if let Some(last) = tokens
            .last_mut()
            .filter(|t| t.owner == owner && t.kind == kind)
        {
            last.range.end += 1;
        } else {
            tokens.push(SyntaxToken {
                range: start..start + 1,
                kind,
                owner,
            });
        }
    }
    let mut syntax_nodes = (0..nodes.len())
        .map(|semantic| SyntaxNode {
            semantic,
            kind: (&nodes[semantic].kind).into(),
            envelope: nodes[semantic].envelope.clone(),
            children: nodes[semantic].children.clone(),
            owned_tokens: Vec::new(),
            content: Vec::new(),
        })
        .collect::<Vec<_>>();
    for (index, token) in tokens.iter().enumerate() {
        let owner = &mut syntax_nodes[token.owner];
        owner.owned_tokens.push(index);
        owner.envelope.start = owner.envelope.start.min(token.range.start);
        owner.envelope.end = owner.envelope.end.max(token.range.end);
    }
    for id in (1..nodes.len()).rev() {
        if let Some(parent) = nodes[id].parent {
            syntax_nodes[parent].envelope.start = syntax_nodes[parent]
                .envelope
                .start
                .min(syntax_nodes[id].envelope.start);
            syntax_nodes[parent].envelope.end = syntax_nodes[parent]
                .envelope
                .end
                .max(syntax_nodes[id].envelope.end);
        }
    }
    for (id, node) in nodes.iter().enumerate() {
        let start = tokens.partition_point(|t| t.range.end <= node.envelope.start);
        for token in tokens[start..]
            .iter()
            .take_while(|t| t.range.start < node.envelope.end)
        {
            if token.range.start < node.envelope.start
                || token.range.end > node.envelope.end
                || token.kind == TokenKind::ContainerPrefix
                || (token.owner == id
                    && matches!(token.kind, TokenKind::OpenMarker | TokenKind::CloseMarker))
            {
                continue;
            }
            let content = &mut syntax_nodes[id].content;
            if let Some(last) = content.last_mut().filter(|r| r.end == token.range.start) {
                last.end = token.range.end;
            } else {
                content.push(token.range.clone());
            }
        }
    }
    LosslessSyntax {
        base: source.id(),
        tokens,
        nodes: syntax_nodes,
    }
}
fn quote_prefix(text: &str, mut p: usize, end: usize) -> Option<Range<usize>> {
    let start = p;
    while p < end && matches!(text.as_bytes()[p], b' ' | b'\t') {
        p += 1;
    }
    if p >= end || text.as_bytes()[p] != b'>' {
        return None;
    }
    p += 1;
    if p < end && matches!(text.as_bytes()[p], b' ' | b'\t') {
        p += 1;
    }
    Some(start..p)
}
pub(crate) fn ancestors(nodes: &[SemanticNode], id: usize) -> Vec<usize> {
    let mut result = Vec::new();
    let mut parent = nodes[id].parent;
    while let Some(id) = parent {
        result.push(id);
        parent = nodes[id].parent;
    }
    result
}
