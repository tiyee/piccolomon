use super::*;
use crate::templates::MarkdownTemplate;
impl Planner<'_> {
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
