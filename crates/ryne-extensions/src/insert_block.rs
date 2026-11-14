//! Templates for the virtual append block. Only choosing a template edits source.

#[derive(Clone, Copy, Debug)]
pub enum InsertBlock {
    Paragraph,
    Heading(u8),
    UnorderedList,
    OrderedList,
    TaskList,
    Quote,
    Code,
    Mermaid,
    Table,
    Image,
    Formula,
    BlockFormula,
    Rule,
}

/// Presentation-provided words; Markdown syntax and selection planning stay in features.
#[derive(Clone, Debug)]
pub struct TemplateLabels {
    pub text: &'static str,
    pub heading: &'static str,
    pub list_item: &'static str,
    pub todo: &'static str,
    pub quote: &'static str,
    pub start: &'static str,
    pub end: &'static str,
    pub column_one: &'static str,
    pub column_two: &'static str,
    pub image_description: &'static str,
    pub image_url: &'static str,
}
impl Default for TemplateLabels {
    fn default() -> Self {
        Self {
            text: "文本",
            heading: "标题",
            list_item: "列表项",
            todo: "待办事项",
            quote: "引用",
            start: "开始",
            end: "结束",
            column_one: "列一",
            column_two: "列二",
            image_description: "图片说明",
            image_url: "图片地址",
        }
    }
}
