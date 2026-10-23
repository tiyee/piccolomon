use super::*;
use crate::templates::MarkdownTemplate;
impl Planner<'_> {
    pub fn insert_adjacent(
        &mut self,
        range: Range<usize>,
        before: bool,
        kind: InsertBlock,
        labels: &ryne_extensions::TemplateLabels,
    ) -> Result<(), String> {
        if self.state.source_mode {
            return Err("block insertion is unavailable in source mode".into());
        }
        let semantics = ryne_syntax::SemanticQuery::new(&self.state.parse);
        let block = semantics
            .block_at(range.start)
            .filter(|block| block.range == range)
            .or_else(|| {
                self.state.parse.nodes.iter().find_map(|node| {
                    semantics
                        .block_at(node.envelope.start)
                        .filter(|block| block.range == range)
                })
            })
            .ok_or("block disappeared")?;
        if before && !block::can_insert_above(&self.state.parse, &block) {
            return Err("the first block only supports adding below".into());
        }
        let (boundary, prefix) = semantics
            .insertion_context(&block)
            .ok_or("invalid block boundary")?;
        let at = if before { boundary.start } else { boundary.end };
        let text = self.state.source.text();
        let newline = self.newline_at(at);
        let (template, selection) = kind.template_with_labels(newline, labels);
        let contextual = template
            .split(newline)
            .map(|line| format!("{prefix}{line}"))
            .collect::<Vec<_>>()
            .join(newline);
        let leading = if at == 0 {
            String::new()
        } else if text[..at].ends_with(['\r', '\n']) {
            format!("{prefix}{newline}")
        } else {
            format!("{newline}{prefix}{newline}")
        };
        let trailing = if at < text.len() {
            format!("{newline}{prefix}{newline}")
        } else {
            String::new()
        };
        let replacement = format!("{leading}{contextual}{trailing}");
        let candidate = format!("{}{replacement}{}", &text[..at], &text[at..]);
        // The AST must recognize the new template at its intended boundary.
        // Unclosed fences/HTML or indentation must never swallow it as code.
        let start = at + leading.len() + prefix.len();
        let normalized = candidate.replace("\r\n", "\n").replace('\r', "\n");
        let normalized_start = candidate[..start].replace("\r\n", "\n").len();
        let parsed = ParseSnapshot::with_math(Source::new(&normalized));
        if !parsed.nodes.iter().any(|node| {
            node.envelope.start == normalized_start
                && node.parent.is_some_and(|parent| {
                    (kind.matches_root(&node.kind)
                        && !matches!(parsed.nodes[parent].kind, NodeKind::CodeBlock(_)))
                        || (node.kind == NodeKind::Item
                            && kind.matches_root(&parsed.nodes[parent].kind))
                })
        }) {
            return Err("new block is inside an unfinished structure".into());
        }
        let map_selection = |offset: usize| {
            start + offset + template[..offset].matches(newline).count() * prefix.len()
        };
        let selected = map_selection(selection.start)..map_selection(selection.end);
        self.apply_edits(
            vec![Edit {
                range: at..at,
                replacement,
            }],
            Some(at),
        )
        .map_err(|error| format!("adding block failed: {error:?}"))?;
        self.state.block_source = None;
        self.state.heading_reveal = None;
        self.select(selected.start, false, false);
        self.select(selected.end, true, false);
        Ok(())
    }

    pub fn append_block(
        &mut self,
        kind: InsertBlock,
        labels: &ryne_extensions::TemplateLabels,
    ) -> Result<(), String> {
        let end = self.state.source.text().len();
        let newline = self.newline_at(end);
        let (template, selection) = kind.template_with_labels(newline, labels);
        // Keep the original bytes intact, including whitespace on trailing lines.
        // Two added line endings separate the new top-level block from containers.
        let separator = if end == 0 {
            String::new()
        } else {
            newline.repeat(2)
        };
        let start = end + separator.len();
        let replacement = format!("{separator}{template}");
        let candidate = format!("{}{replacement}", self.state.source.text());
        // An unfinished fence or raw HTML can swallow appended Markdown. Do not
        // guess how to repair the user's source or insert an invisible template.
        let normalized = candidate.replace("\r\n", "\n").replace('\r', "\n");
        let normalized_start = candidate[..start].replace("\r\n", "\n").len();
        let parsed = ParseSnapshot::with_math(Source::new(&normalized));
        if !parsed.nodes.iter().any(|node| {
            node.envelope.start == normalized_start
                && node.parent.is_some_and(|parent| {
                    (parsed.nodes[parent].kind == NodeKind::Document && kind.matches_root(&node.kind))
                        // Markdown combines adjacent lists with the same marker.
                        // A new top-level item is still editable independently;
                        // it must never become a child of the last nested item.
                        || (node.kind == NodeKind::Item
                            && matches!(parsed.nodes[parent].kind, NodeKind::List(_))
                            && kind.matches_root(&parsed.nodes[parent].kind)
                            && parsed.nodes[parent].parent.is_some_and(|root| parsed.nodes[root].kind == NodeKind::Document))
                })
        }) {
            return Err("文档末尾有未闭合的结构，请先补全后再添加内容".into());
        }
        self.apply_edits(
            vec![Edit {
                range: end..end,
                replacement,
            }],
            Some(end),
        )
        .map_err(|error| format!("添加失败：{error:?}"))?;
        self.state.block_source = None;
        self.state.heading_reveal = None;
        self.select(start + selection.start, false, false);
        self.select(start + selection.end, true, false);
        Ok(())
    }
}
