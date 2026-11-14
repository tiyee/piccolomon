# 历史原型说明

[返回总览](README.md)。本目录 Rust 原型保留用于追溯，不是当前源码驱动方案的实现。

## 模型与范围

原型在打开时将 pulldown-cmark 事件构造成嵌套树，直接修改树的文字节点和列表项；保存时使用 `pulldown-cmark-to-cmark` 生成 Markdown，再全文解析确认事件和引用定义。它允许输出格式改变，与当前“源码事务为唯一修改入口、保存原始源码”的方案不同。

独立 `Cargo.toml` 包含自己的 `[workspace]`，不属于主程序 workspace；`target/` 在本目录忽略。依赖锁定见 [Cargo.toml](Cargo.toml) 和 [Cargo.lock](Cargo.lock)。本次文档整理未移动或修改原型代码。

| 文件 | 用途 |
| --- | --- |
| [src/lib.rs](src/lib.rs) | 事件树、局部文字替换、列表缩进、撤销、序列化验证与新文件写入 |
| [src/main.rs](src/main.rs) | 读取输入 Markdown，生成并验证输出，只创建新文件 |
| [src/nested.md](src/nested.md) | 嵌套引用/列表、任务列表、表格、公式、Mermaid 与 front matter 示例 |

原型开启的 Markdown 选项包含 metadata 扩展，与正式编辑器的方言/扩展设置并不完全相同。其事件一致性只适用于该原型的选项，不能据此声称正式语义完全兼容。

## 已有测试覆盖

`src/lib.rs` 中现有五个测试分别检查：

1. 示例序列化后再次打开，规范化输出稳定，并明确允许首次输出与输入不同。
2. 修改引用内的列表文字、缩进条目及逐次撤销，检查部分未改节点的 Arc 共享。
3. 未使用的引用定义可能被序列化器省略时，拒绝生成文件。
4. 新文件写入并重开；目标文件已存在时拒绝覆盖。
5. 单一文字节点内拒绝切开 grapheme 的编辑。

这些测试不覆盖累计长度树、offset/BlockId 映射、源码驱动的局部解析、块投影、高度估计、GPUI 虚拟布局、IME 或生产保存流程。第 5 项也不证明跨相邻文字节点的 grapheme 行为。

## 运行方式

在仓库根目录执行：

```sh
cargo test --locked --manifest-path docs/reviews/block_tree/Cargo.toml
cargo run --locked --manifest-path docs/reviews/block_tree/Cargo.toml -- docs/reviews/block_tree/src/nested.md /tmp/ryne-block-tree-output.md
```

第二条命令需要输出路径不存在；若已有同名文件，选择另一个新路径，不覆盖原文件。CLI 执行打开与重新生成，不自动执行测试里的文字编辑和列表缩进。

此前会话曾验证这五个测试及 CLI 往返；本次仅核对代码与说明，没有重新运行测试，不将此前结果计为当前方案的新验证。

2026-11-14 依赖维护：原型的 `pulldown-cmark` 同步升级并锁定为 0.13.6，重新执行上述 `cargo test --locked` 命令，五项测试全部通过；未重跑 CLI 或历史性能测量。

## 后续原型要求

源码驱动方案应使用新的独立实验入口，避免把历史树编辑模型逐步接入正式会话。至少分别验证位置索引、当前解析器的局部语义候选、现有投影的块级适配，以及真实 GPUI 的虚拟布局与锚点修正。纯数学高度模拟可以证明前缀求和，却不能证明字体折行、鼠标命中或 IME 正确性。
