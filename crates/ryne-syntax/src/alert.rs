//! GitHub-style alert metadata over parser-owned quote containers.
//! The original quote tree and every source byte remain authoritative.
use crate::ParseSnapshot;
use ryne_parser::{NodeKind, TokenKind};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}
impl Kind {
    pub const ALL: [Self; 5] = [
        Self::Note,
        Self::Tip,
        Self::Important,
        Self::Warning,
        Self::Caution,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Tip => "Tip",
            Self::Important => "Important",
            Self::Warning => "Warning",
            Self::Caution => "Caution",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    pub owner: usize,
    pub kind: Kind,
    pub source: Range<usize>,
    /// First physical line excluding its original newline.
    pub header: Range<usize>,
    pub marker: Range<usize>,
}
pub fn cached(parse: &ParseSnapshot) -> &[Alert] {
    let parse = parse.semantic();
    parse.alerts.get_or_init(|| {
        let text = parse.source().text();
        parse
            .index()
            .quote_nodes
            .iter()
            .filter_map(|&id| {
                let node = &parse.nodes[id];
                if node
                    .parent
                    .is_none_or(|p| parse.nodes[p].kind != NodeKind::Document)
                {
                    return None;
                }
                let line = parse.index().line(node.envelope.start)?;
                let end = line.start + text[line.clone()].trim_end_matches(['\r', '\n']).len();
                // A type-only draft retains visible source and an editable caret.
                if line.end >= node.envelope.end {
                    return None;
                }
                let prefix = parse
                    .index()
                    .prefixes(parse, line.clone())
                    .find(|t| t.owner == id && t.kind == TokenKind::ContainerPrefix)?;
                let value = text
                    .get(prefix.range.end..end)?
                    .trim_end_matches([' ', '\t']);
                let name = value.strip_prefix("[!")?.strip_suffix(']')?;
                let kind = Kind::ALL
                    .into_iter()
                    .find(|k| name.eq_ignore_ascii_case(k.label()))?;
                Some(Alert {
                    owner: id,
                    kind,
                    source: node.envelope.clone(),
                    header: node.envelope.start..end,
                    marker: prefix.range.end..prefix.range.end + value.len(),
                })
            })
            .collect()
    })
}
pub fn for_owner(parse: &ParseSnapshot, owner: usize) -> Option<&Alert> {
    let alerts = cached(parse);
    alerts
        .binary_search_by_key(&owner, |a| a.owner)
        .ok()
        .map(|i| &alerts[i])
}

#[cfg(test)]
mod tests {
    use super::*;
    use ryne_parser::Source;
    #[test]
    fn alerts_are_top_level_source_preserving_quote_metadata() {
        for nl in ["\n", "\r\n", "\r"] {
            for kind in Kind::ALL {
                let raw = format!("> [!{}]{nl}> 中文😀{nl}", kind.label().to_uppercase());
                let parse = ParseSnapshot::new(Source::new(&raw));
                let a = &cached(&parse)[0];
                assert_eq!(a.kind, kind);
                assert_eq!(
                    &raw[a.marker.clone()],
                    format!("[!{}]", kind.label().to_uppercase())
                );
                assert_eq!(parse.source().text(), raw);
            }
        }
        for raw in [
            "> [!NOTE]",
            "> [!SEVERE]\n> body",
            "> [!NOTE] title\n> body",
            "> \\[!NOTE]\n> body",
            ">  [!NOTE]\n> body",
            "> > [!NOTE]\n> > body",
            "- > [!NOTE]\n  > body",
            "```\n> [!NOTE]\n> body\n```",
        ] {
            assert!(
                cached(&ParseSnapshot::new(Source::new(raw))).is_empty(),
                "{raw}"
            );
        }
    }
}
