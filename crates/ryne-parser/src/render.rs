use crate::{Alignment, NodeKind, ParseSnapshot, SemanticNode};
use std::ops::Range;

/// Fixed specification examples retain raw HTML except the tagfilter section.
/// Export always filters disallowed raw tags; this is not HTML sanitization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HtmlPolicy {
    Specification,
    Export,
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn href(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        match b {
            b'&' => out.push_str("&amp;"),
            b'\'' => out.push_str("&#x27;"),
            b'!' | b'#'..=b';' | b'=' | b'?'..=b'Z' | b'_' | b'a'..=b'z' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
fn tagfilter(value: &str) -> String {
    let mut out = String::new();
    let mut copied = 0;
    for (p, _) in value.match_indices('<') {
        let start = p + 1 + usize::from(value.as_bytes().get(p + 1) == Some(&b'/'));
        let tail = &value[start..];
        let len = tail.bytes().take_while(u8::is_ascii_alphabetic).count();
        if matches!(
            tail[..len].to_ascii_lowercase().as_str(),
            "title"
                | "textarea"
                | "style"
                | "xmp"
                | "iframe"
                | "noembed"
                | "noframes"
                | "script"
                | "plaintext"
        ) && tail
            .as_bytes()
            .get(len)
            .is_some_and(|b| b.is_ascii_whitespace() || matches!(b, b'>' | b'/'))
        {
            out.push_str(&value[copied..p]);
            out.push_str("&lt;");
            copied = p + 1;
        }
    }
    out.push_str(&value[copied..]);
    out
}
fn invalid_comment(value: &str) -> bool {
    value.strip_prefix("<!--").is_some_and(|s| {
        s.starts_with('>')
            || s.starts_with("->")
            || s.strip_suffix("-->")
                .is_some_and(|s| s.contains("--") || s.ends_with('-'))
    })
}
impl ParseSnapshot {
    pub fn html(&self, policy: HtmlPolicy) -> String {
        let mut out = String::new();
        self.render(0, policy, &mut out);
        out
    }
    fn children_html(&self, id: usize, policy: HtmlPolicy, out: &mut String) {
        for &child in &self.nodes[id].children {
            self.render(child, policy, out);
        }
    }
    fn render(&self, id: usize, policy: HtmlPolicy, out: &mut String) {
        let node = &self.nodes[id];
        let (open, close) = match &node.kind {
            NodeKind::Document => (String::new(), String::new()),
            NodeKind::Paragraph => ("<p>".into(), "</p>\n".into()),
            NodeKind::Heading(n) => (format!("<h{n}>"), format!("</h{n}>\n")),
            NodeKind::BlockQuote => {
                ensure_newline(out);
                ("<blockquote>\n".into(), "</blockquote>\n".into())
            }
            NodeKind::HtmlBlock => (String::new(), String::new()),
            NodeKind::Emphasis => ("<em>".into(), "</em>".into()),
            NodeKind::Strong => ("<strong>".into(), "</strong>".into()),
            NodeKind::Strikethrough => ("<del>".into(), "</del>".into()),
            NodeKind::List(start) => {
                ensure_newline(out);
                match start {
                    Some(1) => ("<ol>\n".into(), "</ol>\n".into()),
                    Some(n) => (format!("<ol start=\"{n}\">\n"), "</ol>\n".into()),
                    None => ("<ul>\n".into(), "</ul>\n".into()),
                }
            }
            NodeKind::Item => {
                let block = node.children.first().is_some_and(|&id| {
                    matches!(
                        self.nodes[id].kind,
                        NodeKind::Heading(_)
                            | NodeKind::Paragraph
                            | NodeKind::HtmlBlock
                            | NodeKind::BlockQuote
                            | NodeKind::CodeBlock(_)
                            | NodeKind::List(_)
                    )
                });
                (
                    if block { "<li>\n" } else { "<li>" }.into(),
                    "</li>\n".into(),
                )
            }
            NodeKind::CodeBlock(info) => {
                let language = info.split_whitespace().next().unwrap_or("");
                (
                    if language.is_empty() {
                        "<pre><code>".into()
                    } else {
                        format!("<pre><code class=\"language-{}\">", escape(language))
                    },
                    "</code></pre>\n".into(),
                )
            }
            NodeKind::Link {
                destination, title, ..
            } => (
                format!(
                    "<a href=\"{}\"{}>",
                    href(destination),
                    if title.is_empty() {
                        String::new()
                    } else {
                        format!(" title=\"{}\"", escape(title))
                    }
                ),
                "</a>".into(),
            ),
            NodeKind::Image {
                destination, title, ..
            } => {
                let mut alt = String::new();
                self.plain_children(id, &mut alt);
                out.push_str(&format!(
                    "<img src=\"{}\" alt=\"{}\"{} />",
                    href(destination),
                    escape(&alt),
                    if title.is_empty() {
                        String::new()
                    } else {
                        format!(" title=\"{}\"", escape(title))
                    }
                ));
                return;
            }
            NodeKind::Table(alignments) => {
                out.push_str("<table>\n");
                for (i, &row) in node.children.iter().enumerate() {
                    if i == 0 {
                        out.push_str("<thead>\n");
                    } else if i == 1 {
                        out.push_str("<tbody>\n");
                    }
                    out.push_str("<tr>\n");
                    for (col, &cell) in self.nodes[row].children.iter().enumerate() {
                        let tag = if i == 0 { "th" } else { "td" };
                        let align = match alignments.get(col).unwrap_or(&Alignment::None) {
                            Alignment::None => "",
                            Alignment::Left => "left",
                            Alignment::Right => "right",
                            Alignment::Center => "center",
                        };
                        out.push_str(&format!(
                            "<{tag}{}>",
                            if align.is_empty() {
                                String::new()
                            } else {
                                format!(" align=\"{align}\"")
                            }
                        ));
                        self.children_html(cell, policy, out);
                        out.push_str(&format!("</{tag}>\n"));
                    }
                    out.push_str("</tr>\n");
                    if i == 0 {
                        out.push_str("</thead>\n");
                    }
                }
                if node.children.len() > 1 {
                    out.push_str("</tbody>\n");
                }
                out.push_str("</table>\n");
                return;
            }
            NodeKind::TableHead | NodeKind::TableRow | NodeKind::TableCell => {
                (String::new(), String::new())
            }
            NodeKind::Text(value) => {
                out.push_str(&escape(value));
                return;
            }
            NodeKind::Code(value) => {
                out.push_str(&format!("<code>{}</code>", escape(value)));
                return;
            }
            NodeKind::InlineMath(_) | NodeKind::DisplayMath(_) => {
                // No typesetting dependency in the parser: export escaped raw
                // LaTeX including delimiters, without interpreting its contents.
                out.push_str(&escape(&self.source.text()[node.envelope.clone()]));
                return;
            }
            NodeKind::Html(value) => {
                let inline = !node
                    .parent
                    .is_some_and(|p| matches!(self.nodes[p].kind, NodeKind::HtmlBlock));
                if inline && invalid_comment(value) {
                    out.push_str(&escape(value));
                } else {
                    out.push_str(&if policy == HtmlPolicy::Export {
                        tagfilter(value)
                    } else {
                        value.clone()
                    });
                }
                return;
            }
            NodeKind::SoftBreak => {
                out.push('\n');
                return;
            }
            NodeKind::HardBreak => {
                out.push_str("<br />\n");
                return;
            }
            NodeKind::Rule => {
                ensure_newline(out);
                out.push_str("<hr />\n");
                return;
            }
            NodeKind::Task(checked) => {
                out.push_str(if *checked {
                    "<input checked=\"\" disabled=\"\" type=\"checkbox\"> "
                } else {
                    "<input disabled=\"\" type=\"checkbox\"> "
                });
                return;
            }
            NodeKind::SourceBlock => {
                out.push_str(&escape(&self.source.text()[node.envelope.clone()]));
                return;
            }
        };
        out.push_str(&open);
        self.children_html(id, policy, out);
        out.push_str(&close);
    }
    fn plain_children(&self, id: usize, out: &mut String) {
        for &child in &self.nodes[id].children {
            match &self.nodes[child].kind {
                NodeKind::Text(s) | NodeKind::Code(s) => out.push_str(s),
                NodeKind::SoftBreak | NodeKind::HardBreak => out.push(' '),
                NodeKind::Html(_) => (),
                _ => self.plain_children(child, out),
            }
        }
    }
}
fn ensure_newline(out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

/// Extended autolinks are semantic nodes, not renderer-only substitutions.
pub(crate) fn extend_autolinks(nodes: &mut Vec<SemanticNode>, source: &str) {
    let count = nodes.len();
    for id in 0..count {
        if crate::syntax::ancestors(nodes, id).iter().any(|&a| {
            matches!(
                nodes[a].kind,
                NodeKind::Link { .. } | NodeKind::Image { .. } | NodeKind::CodeBlock(_)
            )
        }) {
            continue;
        }
        let NodeKind::Text(value) = &nodes[id].kind else {
            continue;
        };
        if source.get(nodes[id].envelope.clone()) != Some(value) {
            continue;
        }
        let value = value.clone();
        let previous = source[..nodes[id].envelope.start].chars().next_back();
        let next = source[nodes[id].envelope.end..].chars().next();
        let spans = autolinks(&value, previous)
            .into_iter()
            .filter(|(r, _)| !(r.end == value.len() && next == Some('_')))
            .collect::<Vec<_>>();
        if spans.is_empty() {
            continue;
        }
        let mut children = Vec::new();
        let mut cursor = 0;
        let base = nodes[id].envelope.start;
        for (range, destination) in spans {
            if cursor < range.start {
                children.push(push_text(
                    nodes,
                    id,
                    base + cursor..base + range.start,
                    &value[cursor..range.start],
                ));
            }
            let link = nodes.len();
            nodes.push(SemanticNode {
                kind: NodeKind::Link {
                    destination,
                    title: String::new(),
                    reference: String::new(),
                },
                envelope: base + range.start..base + range.end,
                parent: Some(id),
                children: Vec::new(),
            });
            let text = push_text(
                nodes,
                link,
                base + range.start..base + range.end,
                &value[range.clone()],
            );
            nodes[link].children.push(text);
            children.push(link);
            cursor = range.end;
        }
        if cursor < value.len() {
            children.push(push_text(
                nodes,
                id,
                base + cursor..base + value.len(),
                &value[cursor..],
            ));
        }
        nodes[id].kind = NodeKind::Document;
        nodes[id].children = children;
    }
}
fn push_text(
    nodes: &mut Vec<SemanticNode>,
    parent: usize,
    range: Range<usize>,
    value: &str,
) -> usize {
    let id = nodes.len();
    nodes.push(SemanticNode {
        kind: NodeKind::Text(value.into()),
        envelope: range,
        parent: Some(parent),
        children: Vec::new(),
    });
    id
}
fn autolinks(value: &str, previous: Option<char>) -> Vec<(Range<usize>, String)> {
    let mut result = Vec::new();
    let mut p = 0;
    while p < value.len() {
        let tail = &value[p..];
        let before = if p == 0 {
            previous
        } else {
            value[..p].chars().next_back()
        };
        let boundary = match before {
            Some(c) => c.is_whitespace() || "*_~(".contains(c),
            None => true,
        };
        let scheme = ["https://", "http://", "www."]
            .iter()
            .find(|&&s| tail.starts_with(s));
        let found = if let Some(scheme) = scheme.filter(|_| boundary) {
            let mut end = tail
                .find(|c: char| c.is_whitespace() || c == '<')
                .unwrap_or(tail.len());
            while end > 0 && "?!.,:*_~".contains(tail.as_bytes()[end - 1] as char) {
                end -= 1;
            }
            while tail[..end].ends_with(')')
                && tail[..end].matches(')').count() > tail[..end].matches('(').count()
            {
                end -= 1;
            }
            if tail[..end].ends_with(';') {
                if let Some(amp) = tail[..end].rfind('&') {
                    if tail[amp + 1..end - 1]
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric())
                    {
                        end = amp;
                    }
                }
            }
            let domain_start = if **scheme == *"www." { 0 } else { scheme.len() };
            let domain_end = tail[domain_start..end]
                .find(|c: char| !c.is_ascii_alphanumeric() && !"_-.".contains(c))
                .map_or(end, |q| domain_start + q);
            let parts = tail[domain_start..domain_end]
                .split('.')
                .collect::<Vec<_>>();
            let valid = parts.len() >= 2
                && parts.iter().all(|s| !s.is_empty())
                && parts.iter().rev().take(2).all(|s| !s.contains('_'));
            valid.then(|| {
                (
                    end,
                    if tail.starts_with("www.") {
                        format!("http://{}", &tail[..end])
                    } else {
                        tail[..end].into()
                    },
                )
            })
        } else if match before {
            Some(c) => !c.is_ascii_alphanumeric() && !".+-_@\\<".contains(c),
            None => true,
        } {
            email_link(tail)
        } else {
            None
        };
        if let Some((len, destination)) = found {
            result.push((p..p + len, destination));
            p += len;
        } else {
            p += tail.chars().next().unwrap().len_utf8();
        }
    }
    result
}
fn email_link(tail: &str) -> Option<(usize, String)> {
    let (prefix, start) = if tail.starts_with("mailto:") {
        ("mailto:", 7)
    } else if tail.starts_with("xmpp:") {
        ("xmpp:", 5)
    } else {
        ("", 0)
    };
    let local = tail[start..]
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || b".+-_".contains(b))
        .count();
    if local == 0 || tail.as_bytes().get(start + local) != Some(&b'@') {
        return None;
    }
    let domain = start + local + 1;
    let mut end = domain
        + tail[domain..]
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || b".-_".contains(b))
            .count();
    while end > domain && tail.as_bytes()[end - 1] == b'.' {
        end -= 1;
    }
    if end == domain
        || !tail[domain..end].contains('.')
        || matches!(tail.as_bytes()[end - 1], b'-' | b'_')
    {
        return None;
    }
    if prefix == "xmpp:" && tail.as_bytes().get(end) == Some(&b'/') {
        end += 1;
        end += tail[end..]
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || b"@.-_".contains(b))
            .count();
    }
    Some((
        end,
        if prefix.is_empty() {
            format!("mailto:{}", &tail[..end])
        } else {
            tail[..end].into()
        },
    ))
}
