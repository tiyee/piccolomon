# 编辑核心契约与验证

[English](Editing_Core.md)

状态：2026-10-12 当前仓库，API 变更尚未发布。本文记录已实现的最小编辑闭环。v4 规范及较早的可行性/对比报告描述目标或特定日期的实验。本文替代其中较早的 Plain Text 默认策略：**Hybrid 默认复制 Markdown**，Plain Text 为明确命令。

## 身份与迁移

将数字版本的 `source.apply(revision, edits)` 替换为 `source.apply(source.id(), edits)`。`Source` 字段通过 `text()`、`revision()` 和 `id()` 只读访问；通过 `applied.source.commit(applied.inverse)` 应用逆事务。`Transaction`、`Composition`、`Session`、`Interaction`、解析快照和投影命中都携带源码身份。`PositionMap` 绑定两端快照；旧映射不能再次变换同一会话，也不能定位另一分支或文档。

Source 克隆共享文档身份和 revision 分配器。成功修改得到唯一 revision，从相同旧快照分支编辑也不重复。NoChange 和校验失败不分配源码 revision。ID 仅在进程内有效，不提供持久文件身份，也不实施宿主的单写者策略。宿主必须用当前 Source 校验传入操作，不能用归档快照代替。

全部范围使用 UTF-8 字节半开区间。原子事务拒绝过期/跨文档身份、反向/越界范围、Unicode 标量内部端点、CRLF 内部端点，以及重叠/有歧义补丁；允许相邻非空替换。逆补丁先配对新旧范围，合并相邻补丁，再吸收导致新端点非法的未修改 CR/LF 字节，并合并重叠后生成逆事务。

`map_bytes(base, offset, affinity)` 校验身份、范围和 UTF-8，刻意允许 CRLF 内部字节位置。`map_cursor(old, new, offset, affinity)` 进一步校验旧可编辑端点，并按 affinity 修正新 CRLF 内部位置。`Session::transform()` 使用相同策略。投影光标校验拒绝 UTF-8、CRLF 和最终字素内部位置，包括跨语义节点的字素。布局几何和 bidi affinity 由宿主负责。

当前仓库将 `unicode-segmentation` 精确锁定到 1.13.3（Unicode 17.0.0 数据；该依赖最低要求 Rust 1.85）。投影光标校验、源码到投影的 affinity 映射和前后删除使用此版本的字素边界。回归覆盖跨语义 run 的 Unicode 17 组合标记、组合重音、emoji ZWJ 序列、旗帜、中文和精确撤销。较早的可行性结果保留原有 1.12.0 / Unicode 16.0.0 基线，本次升级未重新生成这些历史结果。

## 解析与来源模型

`ParseSnapshot` 持有一个不可变 Source、`Dialect`、带父子索引的语义节点、`LosslessSyntax`、`FieldOrigin` 和规范化引用索引。接口不暴露后端事件类型。节点索引只在本快照有效，使用时必须携带其身份。已解析和未解析引用均记录使用者；定义修改后重新构建会更新解析结果，尚未实现增量失效。

语法 token 恰好覆盖全部原文字节一次，记录类型、owner 和反向归属索引。行内标记、代码围栏、标题、任务及容器前缀具有明确角色。多行行内内容使用排除容器前缀的来源片段列表。在 `> **a\r\n> b**` 中，两处引用前缀均归 BlockQuote，Strong 拥有开闭分隔符。语法 envelope 独立于语义内容范围，包含归属的缩进。行内展开窗口排除容器前缀，跨越前缀的编辑需先展开容器。无法分类的语法作为 `Source`/`Trivia` 保留全部字节。

来源采用保守策略，转换字段不插值。目标/标题来源可能覆盖整个链接或引用定义；复杂表格分隔符、容器缩进和少见标记形式尚未形成完整语法。这些字段不能任意直接进行语义编辑。`LosslessSyntax::serialize(source)` 拒绝其他快照；旧的 `serialize(source_text, partition(...))` 仍是未校验身份的连续范围工具，无效输入可能 panic。

## 投影与编辑规划

`project(parse, interaction)` 将语义叶子与活动源码窗口组合。Direct 精确映射字节；Transform/MultiSource 保留原子来源；Hidden 记录显示锚点和明确展开窗口；Generated 具有锚点；Object 具有语义节点和源码范围。当前显示对象使用 U+FFFC；块间分隔符是合成显示内容，不是原文字节。

`Projection::hit()` 返回源码身份、投影 revision、来源片段、解析后的源码端点、显示位置、命中类型和展开目标。`source_to_projection()` 处理隐藏标记和最终字素边界。即使源码未变，重建投影也使旧命中失效。`resolve()` 只是便捷端点查询，编辑请求应保留完整命中。

Interaction 将展开窗口和选区锚定在源码。进入命中、投影原文、提交编辑、变换交互状态、重建解析，再次投影。`**b** → **** → **x**` 在 Strong 变为 Rule 时仍可编辑。离开折叠窗口；拖动冻结展开集合；composition 窗口保持活动。Preedit 是隔离数据，宿主协议集成和显示 overlay 仍由宿主实现。

Planner 接受带版本意图，返回：

| 结果 | 调用方行为 |
| --- | --- |
| `Commit` | 基于当前 Source 提交绑定事务 |
| `RevealThenRetry` | 展开返回的源码窗口，重建投影并获取新命中 |
| `PendingConfirmation` | 向用户展示具体事务和源码选区，等待明确确认 |
| `Unsupported` | 保留输入，提供返回的源码窗口 |
| `NoChange` | 保持源码和历史 |

已实现：源码窗口替换、预览文字替换、前后字素删除、实体/转义部分编辑、明确格式移除、任务切换及表格竖线转义。预览态删除格式内全部内容时也删除分隔符。字面标点使用数字实体，表格竖线使用转义。预览替换提交前重解析候选字节，将显示结果与用户请求的显示替换比较；不一致则要求展开。源码窗口保留用户输入的原文。

多个原子转换、代码编码、对象、合成分隔符及复杂块的预览编辑采用展开/不支持回退。跨越隐藏内部语法的选区可能返回待确认事务。完整 v4 命令集（拆分/合并/缩进、结构化表格命令、通用格式化及任意跨块编辑）仍待实现。

剪贴板辅助接口从源码选区提供明确的 Markdown 或 PlainText 数据，默认 Markdown。PlainText 当前截取显示数据，对象 alt 和任意语义 Markdown 导出仍需完善。GPUI/布局、真实系统 IME 回调和候选窗、剪贴板 I/O、文件 I/O，以及撤销历史安装由宿主适配器承担。

## HTML 验证

当前仓库将 `pulldown-cmark` 精确锁定到 0.13.6，该版本[修复了嵌套图片 wikilink 的平方级解析耗时](https://github.com/pulldown-cmark/pulldown-cmark/releases/tag/v0.13.6)。默认选项仍仅显式启用表格、删除线和任务列表；数学扩展仍需主动开启，wikilink 保持禁用。回归门禁覆盖全部 677 例源码往返、原生 HTML 的 629/677 基线及完全相同的 48 个逐例差异，以及自有 renderer 的 677 例。保留 0.13.5 升级时添加的引用定义后空白行回归。

`tests/backend_upgrade.rs` 锁定扩展开关，并检查嵌套 wikilink 形式的输入在 HTML、投影及 Unicode 可逆编辑中保持字面内容，覆盖 LF/CRLF/CR 和数学开关两种配置。仅后端测试显式启用 wikilink，检查嵌套深度 2 和 4096 时最内层图片优先的行为；该用例已在 0.13.5 上复现旧行为，不使用依赖机器性能的耗时阈值。这不会在编辑器中启用 wikilink，也不构成端到端性能上限保证。历史可行性测量保留在 0.13.4。

自有 renderer 分类处理原有 48 个原生差异：双引号转义、表格布局/对齐属性、任务 HTML、块换行、扩展自动链接、旧行内注释语法及 tagfilter。`backend_html()` 保持原诊断后端；`tests/fixtures/backend-differences.json` 按 example 和章节锁定每个已知差异的实际输出。其余示例必须继续精确匹配，不能通过总数相等掩盖差异互换。

自有 renderer 对 677 个固定示例逐字节比较，不做规范化。章节使用明确配置：基础自动链接（`6.8`）通过 CommonMark029 关闭扩展；GFM 扩展使用 Gfm029；原始 HTML 仅在禁止 HTML 章节启用过滤。生产 `html()` 使用 Gfm029 和全局 tagfilter。这一区分遵循[固定 GFM 规范](https://github.github.com/gfm/)。tagfilter 保留源码字节，不能代替 HTML sanitization。通过此测试集不能证明所有输入都兼容。

执行[开发指南](README.zh-CN.md)中的标准检查。回归覆盖新 CRLF 的逆事务、4,000 组确定性多补丁、分支/跨文档拒绝、过期投影命中、最终字素、双向映射、ownership、引用依赖、会话转变及 Planner 命令。历史基准未重跑，不代表本次实现性能。

2026-11-14 升级验证（macOS、Rust 1.99.1）：`cargo test --locked` 为 447 通过、2 项原有忽略（外网图片下载和大侧栏测量）；三项 GFM 样例门禁均保持通过。架构检查、`cargo fmt --check` 及带 `--locked --all-targets -- -D warnings` 的 Clippy 通过。应用完整 396 阶段 `--smoke-test` 通过，使用真实窗口几何调用编辑回调，覆盖嵌套块、源码模式、撤销/重做、公式、Mermaid 和 IME 保护；不等同人工输入法候选窗口或 Windows 实机验收。独立区块树原型同步锁定 0.13.6，五项测试通过。
