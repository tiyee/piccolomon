# editable_markdown_parser 0.0.3 需求符合性检查

检查日期：2026-10-12。

**结论：可作为源码保真的编辑内核基础，但尚不满足本项目 H1 Hybrid 编辑需求，也不能凭固定规范示例通过就认为任意 GFM 输入正确。建议保留现有 Source/Transaction/PositionMap 基础，先修语义、显隐和剪贴板，再补块级协议与结构命令。**

## 1. 检查对象和证据

- 直接下载并解压 [crates.io 的 0.0.3 发布归档](https://static.crates.io/crates/editable_markdown_parser/editable_markdown_parser-0.0.3.crate)，没有用搜索索引中落后的 0.0.2 代替。
- 发布归档 SHA-256：`600c64571851f316a350b4dd90cb9de78eb30efe4d861e18af553efe4f1c292c`。
- `.cargo_vcs_info.json` 记录提交 `0911a38ff06fe4db4095cb3bab959dfd8e0da1da`；发布包 `src/` 与本地 `/Users/jiangtao/code/github.com/editable_md_parser/crates/editable_markdown_parser/src/` 逐文件一致。
- 验收以本项目 [第十一份 Hybrid 规格](11_GPUI_Typora_Hybrid_View_Spec.md)、[第八份编辑行为规格](08_GPUI_Editing_Behavior_Interaction_Conformance_Spec.md)为产品依据；包仓库的 v4 规格用于核查其自有承诺。GUI 绘制、系统 IME、剪贴板 I/O 属于宿主，不能全部算作 parser 包的缺陷。
- 在发布归档中使用 Rust/Cargo 1.99.0 执行 `cargo test --locked --offline`：21 个单元测试、24 个 editing 集成测试、3 个 GFM 集成测试、1 个 doctest，**49/49 通过**。
- 固定 677 个 GFM 示例的 owned renderer 比较通过；原生后端的 48 项已知差异记录也通过。owned renderer 测试按规范章节选择方言与 HTML policy，不等价于一个 production profile 对任意输入的全面正确性证明。
- 新增 [11 个需求探针](reviews/editable_markdown_parser_0.0.3/requirements.rs)：**1 通过、10 失败**。这是一组针对疑似缺口构造的测试，不是随机样本，不能将通过率解释为总体质量比例。
- 未修改包的生产代码，未发布版本。未运行 GUI、真实输入法、布局、大文档性能或跨平台测试。先尝试的本机旧 Rust 1.81 无法编译测试中的 `is_multiple_of`；切换现成的 1.99 工具链后完成上述验证。

## 2. 需要修复的具体问题

### R1 · P1：扩展自动链接在后端 Text 节点边界被截断

位置：包 `src/render.rs:276`，`extend_autolinks()`。

输入 `https://example.org/a*b*c`，实际输出为：

```html
<p><a href="https://example.org/a">https://example.org/a</a><em>b</em>c</p>
```

预期是一个包含整个 `a*b*c` 路径的链接。输入 `https://example.org/?a=1&amp;b=2` 也在实体前截断链接。

原因：当前在后端已经完成行内解析后，逐个 Text 节点补自动链接；强调和实体已经将原始 URL 切开。只扫描 `source[envelope] == value` 的节点无法恢复正确的优先级和完整范围。

修改方向：把 GFM 自动链接识别纳入同一权威行内解析过程，或在适配层按连续原始源码与容器上下文识别，再一致地重建被覆盖的语义节点和 provenance。必须避开代码、已有链接和图片等禁止上下文；不能仅拼接显示字符串或只改 HTML 输出器。

验收：包含内部 `*`、`_`、实体、转义、括号和末尾标点的组合输入，检查 Link 的完整 destination、envelope、子节点与投影。上述两个用例已与本地 cmark-gfm 二进制的 autolink 扩展交叉比较；其结果支持完整 URL。实体在该辅助实现的自动链接内保留原始拼写，HTML 因而包含 `&amp;amp;`，探针没有先规范化输出。

规范依据：[GFM §6.9](https://github.github.com/gfm/#autolinks-extension-)。辅助实现是差分证据，不是取代规范的运行时权威。

### R2 · P1：引用索引合并不同标签，字段来源可能与语义不一致

位置：包 `src/syntax.rs:268`，`normalize_label()`，及 `:297` 的定义索引构建。

输入中同时定义 `[a b]: /one` 与 `[a b]: /two`（第二个标签中间是 U+00A0 NBSP），实际 `references` 只有一个 `A B`，两个 user 被合并。后端解析出的链接目标仍可能各自正确，但本包的引用依赖和 Destination/Title 来源会指向错误定义。由于随后使用 `insert` 覆盖，保留下来的定义还受后端定义表迭代顺序影响。

原因：`split_whitespace()` 接受全部 Unicode 空白；GFM 0.29 标签空白仅为 U+0020、U+0009—U+000D。`to_uppercase()` 也不等于 Unicode case folding。

修改方向：单独实现并测试 GFM 0.29 的标签规范化；保留 NBSP 等非规范空白；使用正确 case folding。按源码顺序保存定义候选和有效首定义，统一 resolved/unresolved users 与字段来源。不要只修索引 key 而忽略其与后端实际引用解析的一致性。

验收：NBSP 与普通空格区分、Unicode 大小写折叠（包括 `ẞ`）、重复定义首项优先、删除首定义后接替，以及未解析引用在新增定义后升级。检查每个 Link 的目标和 FieldOrigin 指向同一有效定义。

规范依据：[GFM §6.5 标签匹配规则](https://github.github.com/gfm/#matches)。包 v4 规格 §19 已明确禁止通用 Unicode `split_whitespace()`，因此这里属于实现违背既有契约。

### R3 · P1：自动 reveal 没有区分 Inline 祖先与 Block 源码入口

位置：包 `src/projection.rs:432` 的 reveal 选择，`:91` 的 `Interaction::enter()`。

复现结果：

| 操作 | 实际展开 | 需求 |
| --- | --- | --- |
| 进入 `**a *b* c**` 的 b | `a *b* c` | 相关 Emphasis/Strong 祖先一起展开 |
| 进入 `[**x**](https://example.org)` 的 x | `**x**` | 展开完整链接及其内部格式 |
| 进入 `# title` 正文 | `# title\n` | 标题仍结构化，`#` 保持隐藏 |

原因：按 hidden record 的最小 envelope 选择一个范围；最近内层格式截断祖先 reveal，普通块正文则可能选中整块范围。这个范围同时被用于普通进入和显式源码进入。

修改方向：由语义祖先链决定自动展开的 Inline 集合，在所属内容块处停止；为显式 BlockSource/DocumentSource 另设状态与入口。保留跨行内容 spans 和现有容器前缀排除，不通过简单选取最大的 envelope 修复，否则会展开整个 Block。

验收：CT-102、CT-104、CT-105、CT-108、CT-109；嵌套 Link/Strong、跨行引用、键盘移动、冻结拖选和 composition。模式变化不改源码及历史。

### R4 · P1：Markdown 复制不是需求中的格式片段复制

位置：包 `src/planner.rs:378`，`copy_selection()`。

复现：

- `**world**` 仅选择源码 `2..7` 的内容，默认 Markdown 复制得到 `world`，需求应得到保留粗体的 `**world**`。
- `[label](https://example.org)` 仅选 label，复制得到 `label`，目标丢失。
- 同一完整 Source Selection 复制 PlainText，折叠时为 `world`，展开后变为 `**world**`。

原因：Markdown 分支直接切源码；PlainText 分支直接切当前投影。前者没有补足语义上下文，后者受临时 reveal 状态影响。

修改方向：提供独立的 `Source`、`MarkdownFragment`、`PlainText` 复制策略。Source 精确切片；MarkdownFragment 依据语义选区补足必要格式、链接目标或引用定义；PlainText 从语义树导出，处理图片 alt 和任务可读标记。选区进入语法区或无法证明可安全序列化时，按产品规格回退精确源码。剪切仍只删除原 Source Selection，不删除复制时补出的外层语法。

验收：第八份 EB-040—043、CT-095—101；部分粗体、部分链接、引用、跨格式、跨块、展开前后相同结果，以及剪切后逆事务逐字节恢复。

## 3. 阶段能力缺口

### R5 · 稳定 Block 编辑与结构命令尚未交付

`EditIntent` 目前只有 Replace、双向删除、ToggleTask、RemoveFormat。没有 SplitBlock、JoinBlock、Indent/Outdent、SetHeading、通用格式命令及表格结构命令。预览态文本中包含 CR/LF 时返回 RevealThenRetry，不能满足 Hybrid 普通段落 Return、列表续项/退出、代码换行等完整行为。

当前 `Projection` 主要是一份线性文本和来源映射。表格 `| a | b | ... | c | d |` 被输出为 `ab\ncd`；解析树保留了 TableCell，但投影没有单元格显示分隔或二维结构协议。不能据此称为网格编辑已经完成。**本项目 H1 要求表格先原文回退，H2 再提供网格；H1 不必为通过验收提前实现全部表格 UI。**

修改方向：新增 BlockPresentation/FlowBlock 数据协议，携带类型、内容 spans、容器上下文、结构装饰和源码入口；由 GPUI 适配层完成绘制、几何和命中。Planner 先补 H1 的段落/标题拆分、列表续项与退出、缩进、引用操作和安全代码换行，再补 H2 单元格导航与增删行列。

须同时对齐当前产品规格：普通段落 Hybrid Return 使用段落边界，Shift+Return 使用两个尾随空格加换行；包的较早 v4 文档写的是反斜杠加换行。两者语义都可为硬换行，但验收中的实际字节规则应以本项目第十一份为准。

### R6 · GUI、历史与性能仍需独立验收

已有 Composition 数据隔离和 inverse transaction 是正确基础，但尚未证明宿主完整 Undo/Redo、系统 IME EditableTextSnapshot、候选框定位、跨块选区与辅助功能正确。宿主必须负责这些集成，包则需要提供足够的版本、选区和结构信息。

当前采用 Arc<str>、全文解析及全投影重建，没有增量 parser。旧 feasibility 结果明确属于较早实现，不能作为 0.0.3 的实时性能证据。本次没有测量性能，不据静态代码断言已经超预算。

建议先以全文正确结果建立基线，测量 Source.apply、parse、project、planner 候选重解析、hit 查询、布局各阶段，再决定行索引、区间查询、Rope/chunk 存储和增量失效的实现顺序。每个增量结果必须与全文结果比较语义、范围和映射。

### R7 · P2：发布文档仍将已发布 API 称为未发布

0.0.3 归档中的 README 第 16 行仍提示示例包含 unreleased API changes，Editing_Core 也写 API changes are not published；实际归档已包含这些 API。

修改方向：在下一版本中明确已发布版本与主分支状态，使用精确版本依赖编译 README 示例。同步英语和中文说明，避免用户误以为必须 path dependency 才能使用这些 API。

## 4. 推荐修复顺序与验收

1. **正确性修复**：R1 自动链接、R2 引用索引；将对应探针加入包正式回归，不修改既有 677 示例或通过数字来遮盖问题。
2. **Hybrid 行内闭环**：R3 分离 Inline 自动 reveal 与显式 BlockSource；R4 拆分三种复制语义；修复相关探针。
3. **H1 Block 闭环**：新增稳定块呈现协议与结构命令；表格先安全原文回退。实现并验证宿主 GPUI、历史、IME、剪贴板链路。
4. **H2/H3**：表格网格与二维编辑、大文档及增量优化；保持全文 oracle 和保真逆事务验收。

上述 R1—R4 的回归通过前，建议把 0.0.3 定位为“源码保真编辑原语与 Hybrid 原型核心”。不建议推翻已有事务和映射设计，也不建议将未完成块行为全部交给 UI 临时猜测。

## 5. 复现探针

在 0.0.3 解压副本中，将 [requirements.rs](reviews/editable_markdown_parser_0.0.3/requirements.rs) 复制为 `tests/review_requirements.rs`。发布归档的 Cargo.toml 使用 `autotests = false`，因此还需追加：

```toml
[[test]]
name = "review_requirements"
path = "tests/review_requirements.rs"
```

执行：

```sh
cargo test --locked --test review_requirements -- --test-threads=1 --nocapture
```

0.0.3 预期结果为 1 passed / 10 failed。10 个失败按根因归为：自动链接 2、引用索引 1、reveal 3、剪贴板 3、表格呈现能力 1。最后一项是产品能力缺口探针，不表示 Table 语义节点没有解析。短 URL 不 panic 的控制用例通过。
