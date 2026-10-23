//! Presentation defaults are independent of syntax identity.
use crate::BlockMetadata;
use ryne_syntax::block::BlockKind;
impl From<BlockKind> for BlockMetadata {
    fn from(kind: BlockKind) -> Self {
        use BlockKind::*;
        let (icon, label, name) = match kind {
            Paragraph => ("text_t", "¶", "段落"),
            Heading(level) => (
                match level {
                    1 => "text_header_1",
                    2 => "text_header_2",
                    3 => "text_header_3",
                    4 => "text_header_4",
                    5 => "text_header_5",
                    _ => "text_header_6",
                },
                match level {
                    1 => "H1",
                    2 => "H2",
                    3 => "H3",
                    4 => "H4",
                    5 => "H5",
                    _ => "H6",
                },
                "标题",
            ),
            List { ordered, task } => (
                if task {
                    "task_list_ltr"
                } else if ordered {
                    "text_number_list_ltr"
                } else {
                    "text_bullet_list_ltr"
                },
                if ordered { "1." } else { "•" },
                if task {
                    "任务列表"
                } else if ordered {
                    "有序列表"
                } else {
                    "无序列表"
                },
            ),
            Quote => ("quote_bubble", "❯", "引用"),
            Code => ("code", "</>", "代码块"),
            Mermaid => ("mermaid", "◇", "Mermaid"),
            Table => ("table", "▦", "表格"),
            Raw => ("code", "<>", "原始块"),
            Rule => ("divider", "—", "分隔线"),
            Image => ("image", "IMG", "图片"),
        };
        Self {
            kind: Some(kind),
            icon: Some(icon),
            label,
            name,
        }
    }
}
