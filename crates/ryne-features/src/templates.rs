use editable_markdown_parser::NodeKind;
use ryne_extensions::{InsertBlock, TemplateLabels};
use std::ops::Range;
pub const ITEMS: &[(&str, InsertBlock)] = &[
    ("¶  段落", InsertBlock::Paragraph),
    ("H1  一级标题", InsertBlock::Heading(1)),
    ("H2  二级标题", InsertBlock::Heading(2)),
    ("H3  三级标题", InsertBlock::Heading(3)),
    ("H4  四级标题", InsertBlock::Heading(4)),
    ("H5  五级标题", InsertBlock::Heading(5)),
    ("H6  六级标题", InsertBlock::Heading(6)),
    ("•  无序列表", InsertBlock::UnorderedList),
    ("1.  有序列表", InsertBlock::OrderedList),
    ("☐  任务列表", InsertBlock::TaskList),
    ("❯  引用", InsertBlock::Quote),
    ("</>  代码块", InsertBlock::Code),
    ("◇  Mermaid", InsertBlock::Mermaid),
    ("▦  表格", InsertBlock::Table),
    ("IMG  图片", InsertBlock::Image),
    ("ƒ  行内公式", InsertBlock::Formula),
    ("—  分隔线", InsertBlock::Rule),
];

pub trait MarkdownTemplate: Sized {
    fn template_with_labels(self, newline: &str, labels: &TemplateLabels)
    -> (String, Range<usize>);
    fn template(self, newline: &str) -> (String, Range<usize>) {
        self.template_with_labels(newline, &TemplateLabels::default())
    }
    fn matches_root(self, kind: &NodeKind) -> bool;
}
impl MarkdownTemplate for InsertBlock {
    fn template_with_labels(
        self,
        newline: &str,
        labels: &TemplateLabels,
    ) -> (String, Range<usize>) {
        let (raw, editable) = match self {
            Self::Paragraph => (labels.text.into(), labels.text.to_owned()),
            Self::Heading(level) => (
                format!("{} {}", "#".repeat(level.into()), labels.heading),
                labels.heading.to_owned(),
            ),
            Self::UnorderedList => (
                format!("- {}", labels.list_item),
                labels.list_item.to_owned(),
            ),
            Self::OrderedList => (
                format!("1. {}", labels.list_item),
                labels.list_item.to_owned(),
            ),
            Self::TaskList => (format!("- [ ] {}", labels.todo), labels.todo.to_owned()),
            Self::Quote => (format!("> {}", labels.quote), labels.quote.to_owned()),
            Self::Code => ("```\n\n```".into(), String::new()),
            Self::Mermaid => {
                let body = format!(
                    "flowchart TD\n    A[{}] --> B[{}]",
                    labels.start, labels.end
                );
                (format!("```mermaid\n{body}\n```"), body)
            }
            Self::Table => (
                format!(
                    "| {} | {} |\n| --- | --- |\n|  |  |",
                    labels.column_one, labels.column_two
                ),
                labels.column_one.to_owned(),
            ),
            Self::Image => (
                format!("![{}]({})", labels.image_description, labels.image_url),
                labels.image_url.to_owned(),
            ),
            Self::Formula => ("$x$".into(), "x".into()),
            Self::Rule => ("---\n\n".into(), String::new()),
        };
        let text = raw.replace('\n', newline);
        let selection = match self {
            Self::Code => 3 + newline.len()..3 + newline.len(),
            Self::Rule => text.len()..text.len(),
            _ => {
                let editable = editable.replace('\n', newline);
                let start = text.find(&editable).unwrap_or(0);
                start..start + editable.len()
            }
        };
        (text, selection)
    }

    fn matches_root(self, kind: &NodeKind) -> bool {
        match self {
            Self::Paragraph | Self::Image | Self::Formula => *kind == NodeKind::Paragraph,
            Self::Heading(level) => *kind == NodeKind::Heading(level),
            Self::UnorderedList | Self::OrderedList | Self::TaskList => {
                matches!(kind, NodeKind::List(_))
            }
            Self::Quote => *kind == NodeKind::BlockQuote,
            Self::Code | Self::Mermaid => matches!(kind, NodeKind::CodeBlock(_)),
            Self::Table => matches!(kind, NodeKind::Table(_)),
            Self::Rule => *kind == NodeKind::Rule,
        }
    }
}
