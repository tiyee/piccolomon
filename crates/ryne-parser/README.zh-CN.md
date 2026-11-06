# ryne-parser

[English](README.md)

面向 Hybrid 编辑器的 Markdown 解析与编辑原语库，保留原始源码。源码与投影坐标均使用 UTF-8 字节偏移。

## 工作区依赖

在 Ryne 工作区成员的清单中声明依赖：

```toml
[dependencies]
ryne-parser.workspace = true
```

## 编辑与恢复源码

```rust
use ryne_parser::{Affinity, Edit, Source};

let source = Source::new("**world**");
let applied = source.apply(source.id(), vec![Edit {
    range: 2..7,
    replacement: "Rust".into(),
}]).unwrap();
assert_eq!(applied.source.text(), "**Rust**");
assert_eq!(applied.positions.map_bytes(source.id(), 7, Affinity::Downstream), Ok(6));
let restored = applied.source.commit(applied.inverse).unwrap();
assert_eq!(restored.source.text(), source.text());
```

`SnapshotId` 包含文档身份与 revision。`Source::apply()` 先基于同一不可变快照校验全部范围，再执行修改；拒绝跨文档或过期快照、非法 UTF-8/CRLF 端点、重叠及有歧义的插入。允许相邻非空替换；同位置插入和替换边界插入需预先合并。字节不变返回 `Error::NoChange`。

逆事务绑定修改后的快照。其范围会吸收新形成的 CRLF，必要时合并，精确恢复原始字节。一个文档内的 revision 唯一，从旧快照分支编辑也不会重复。身份只在当前进程内有效，不是持久化文件 ID。

`PositionMap::map_bytes()` 保留 affinity 与精确字节映射，可能返回新 CRLF 的内部位置。可编辑源码位置应使用 `map_cursor()`：校验两端快照，按 affinity 修正 CRLF 位置。最终显示的字素边界由 `Projection::validate_cursor()` 跨全部映射段校验。

## 解析、展开与规划编辑

```rust
use ryne_parser::{project, Affinity, EditIntent, EditPlan, EditPlanner,
    Interaction, ParseSnapshot, Source};

let parse = ParseSnapshot::new(Source::new("&fjlig;"));
let interaction = Interaction::new(parse.source());
let projection = project(&parse, &interaction).unwrap();
assert_eq!(projection.text, "fj");
let intent = EditIntent::Replace {
    start: projection.hit(0, Affinity::Downstream).unwrap(),
    end: projection.hit(1, Affinity::Upstream).unwrap(),
    text: String::new(),
};
let EditPlan::Commit(transaction) = EditPlanner::plan(
    &parse, &projection, &interaction, intent,
).unwrap() else { panic!("expected an entity edit") };
let applied = parse.source().commit(transaction).unwrap();
assert_eq!(applied.source.text(), "j");
```

`ParseSnapshot` 持有源码、方言、不依赖后端类型的语义树、类型化保真语法、字段来源，以及已解析/未解析引用索引。语法 token 恰好覆盖全部字节，每个 token 只有一个 owner；标记角色和容器前缀明确记录，多行 Strong 内容不包含引用前缀。`LosslessSyntax::serialize()` 校验快照身份。`partition()` 仍是原文范围工具，不是类型化语法树。

`project(parse, interaction)` 在活动窗口输出源码，其他区域输出语义预览。映射区分 Direct、原子 Transform、多段来源、合成分隔符、隐藏语法及对象。带版本的命中保留来源和展开目标，`source_to_projection()` 提供反向查询。`Interaction::enter()` 展开命中，`leave()` 折叠；应用编辑后先通过位置映射变换交互状态，再重新解析。拖动期间可以冻结展开集合；composition 窗口保持展开。

Planner 处理源码窗口输入、预览文字替换和字素删除、实体部分编辑、格式移除、任务切换及表格竖线转义，明确返回 `Commit`、`RevealThenRetry`、`PendingConfirmation`、`Unsupported` 或 `NoChange`。复杂块、对象、代码跨度编码，以及来源不足的跨区域编辑使用显式回退。这是最小编辑闭环，尚未实现完整 v4 命令集。

## HTML 与能力边界

`html()` 从自有 GFM 语义模型输出 HTML，并启用 tagfilter；`backend_html()` 保留 pulldown-cmark 原生输出供诊断。固定后端仍匹配 629/677，每个已知差异独立锁定。自有 renderer 按文档约定的章节配置精确匹配全部 677 个固定示例：基础自动链接使用 `Dialect::CommonMark029`，扩展使用 `Gfm029`，禁止的 HTML 使用 `HtmlPolicy::Export`；`HtmlPolicy::Specification` 保留原始 HTML。tagfilter 不是通用 HTML sanitizer。

通过此测试集不等于任意输入的完整 GFM 兼容。复杂块中的标记提取与字段来源仍保守，目标/标题来源可能覆盖整个定义或链接。尚无带样式的块布局、bidi 引擎、增量解析、存储优化或真实 GUI 集成。

`Composition` 将 preedit 与源码隔离，更新/取消不改字节或 revision，提交校验身份。`utf16_to_utf8()` 拒绝 surrogate 内部位置。GPUI、系统 IME 回调、候选窗定位、剪贴板 I/O 和布局命中校验由宿主适配器承担。核心剪贴板数据默认使用 Markdown（`CopyFormat::default()`），Plain Text 为明确命令。对象 alt 导出及任意跨块 Markdown 选区仍有限制。

运行 `cargo doc --locked -p ryne-parser --no-deps` 可生成本地 API 文档。

## 可选公式扩展

`ParseSnapshot::with_math(source)` 开启锁定后端的美元符号公式扩展。`with_dialect_and_math(source, dialect, enabled)` 可显式选择；`new()` 和 `with_dialect()` 默认关闭。重新解析时需保留 `math_enabled`。

`NodeKind::InlineMath(String)` 和 `DisplayMath(String)` 保存字面 LaTeX，内部不生成 Markdown 子节点。envelope 包含 `$` / `$$`；开闭标记 token 与正文范围保留 UTF-8 字节半开区间及容器上下文。代码、转义美元符号、链接目标和 HTML 沿用后端的上下文规则；未闭合标记保留普通原文。

预览输出带源码来源的公式对象，展开恢复可编辑原文；HTML 导出转义包含定界符的原始公式。排版、颜色与 LaTeX 有效性由宿主负责，库不依赖排版引擎或 GUI。

两种公式均沿用后端的行内元素规则，可以在同一个块内跨行，但空行和 Markdown 块边界可能打断识别。允许内部空行的完整 `$$` 公式块、`math` 围栏及 Mermaid 图表尚未实现。

## 许可证

库代码使用 MIT；随包提供的规范 fixtures 及相关摘录适用独立的 [CC-BY-SA-4.0 声明](tests/fixtures/NOTICE.md)。
