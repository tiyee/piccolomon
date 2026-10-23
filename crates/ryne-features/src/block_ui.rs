use ryne_extensions::*;
pub(super) struct MarkdownBlockUi;
impl BlockUiProvider for MarkdownBlockUi {
    fn adornment(&self, block: &ryne_syntax::block::Block, source_open: bool) -> AdornmentSpec {
        AdornmentSpec {
            metadata: BlockMetadata::from(block.kind),
            menu: vec![
                MenuItemSpec {
                    command: BlockCommand::Source,
                    name: if source_open {
                        "收起块源码"
                    } else {
                        "编辑块源码"
                    },
                    order: 0,
                },
                MenuItemSpec {
                    command: BlockCommand::Select,
                    name: "选择整块",
                    order: 1,
                },
                MenuItemSpec {
                    command: BlockCommand::Copy,
                    name: "复制块源码",
                    order: 2,
                },
                MenuItemSpec {
                    command: BlockCommand::AddBelow,
                    name: "在下方添加",
                    order: 3,
                },
                MenuItemSpec {
                    command: BlockCommand::AddAbove,
                    name: "在上方添加",
                    order: 4,
                },
            ],
        }
    }
}
