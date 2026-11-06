# writ 对比与吸收计划

[English](Writ_Comparison_and_Adoption.md)

本文对比记录修改前的历史基线，当前实现和剩余边界见[编辑核心契约](Editing_Core.zh-CN.md)。

核对日期：2026-10-11。状态：待实施任务；本文不新增运行时能力或依赖。

本文作为 [v4 技术方案](https://github.com/iDigimon/markdown-engine/blob/main/docs/Editable_GFM_Engine_v4_Complete_Technical_Plan.md) 的实施补充，主要对应第 5、8、10、11、16–18 节。[v4 行为规范](https://github.com/iDigimon/markdown-engine/blob/main/docs/Editable_GFM_Engine_v4_Typora_Hybrid_Specification.md) 仍为行为依据。吸收 writ 的显式显示映射和实际交互机制，同时保留本项目的事务、源码保真及方言契约。

## 1. 核对基线与当前实现

外部参考为 writ 0.19.3，固定提交 [`0f32b7f2a99ffe11065a8834b64c8057061dbb60`](https://github.com/wilfreddenton/writ/tree/0f32b7f2a99ffe11065a8834b64c8057061dbb60)。下文外部源码链接均固定到该提交。

本地对比依据为 [库实现](../src/lib.rs) 与 [固定 GFM 集成测试](../tests/gfm_fixtures.rs)。

| 领域 | writ 实现 | 本包当前实现 |
| --- | --- | --- |
| 存储 | Rope buffer，解析器读取文本分块 | 不可变 `Arc<str>` 快照；编辑时复制全文 |
| 解析 | tree-sitter 块树与独立行内树，重解析时传入旧树 | pulldown-cmark 全文事件解析 |
| 标记 | 带类型和源码范围的块标记，用于显示及续行 | `partition()` 按事件边界分割原文；所有原子仍归根所有 |
| 显示映射 | 双向的逐行 `SegmentMap`，区分隐藏与折叠替换区域 | `Direct` / `Transform` 段，以及投影到源码的 `resolve()` |
| 显露交互 | 按光标位置决定是否显示源码 | `Session` 可变换源码范围，但未接入 `preview_inline()` |
| 布局与输入 | 渲染/高度缓存、视口布局、滚动锚点及平台 preedit 显示 | 尚无布局引擎或平台 IME 集成；已实现 composition 数据隔离 |

现有原语不代表已经实现完整语义树、语法 ownership、通用编辑 Planner 或完整块投影。后端原生 HTML 精确匹配固定示例的 629/677；这是诊断基线，不是完整 GFM 兼容成绩。

## 2. W01 — 显式投影与隐藏源码映射

参考：writ 的 [`segment_map.rs`](https://github.com/wilfreddenton/writ/blob/0f32b7f2a99ffe11065a8834b64c8057061dbb60/src/segment_map.rs) 显式区分原样显示段、隐藏源码范围，以及被显示 token 替换的范围。

扩展投影模型，加入样式、隐藏源码记录、合成锚点和对象身份。隐藏记录放入独立的 collapsed-source 索引，使隐藏分隔符内部的源码位置能同时定位显示边界和显露目标。在投影到源码的解析之外，增加源码到投影的定位。

保留以下区分：

- `PositionMap` 负责事务后的旧源码坐标到新源码坐标变换，与同一快照内的源码/显示映射独立。
- Direct 段允许按字节偏移精确换算；Transform 段按完整来源及 `Upstream` / `Downstream` affinity 解析，不按长度比例插值。
- 隐藏标记、合成分隔符和对象返回明确结果或锚点，不伪造直接源码位置。
- 投影及布局命中携带所属 source/projection/layout 版本上下文；规划编辑前拒绝过期命中。

验收：覆盖双向定位、两种 affinity、空内容/全部隐藏内容、文档端点、嵌套分隔符及转换值。对于 `&fjlig; → fj`，按现有端点策略，投影 byte 1 在 upstream 时映射到源码 0，在 downstream 时映射到源码 7。Direct 段往返必须保持合法 UTF-8 边界；隐藏与转换范围采用显式策略，不要求全局双射。

## 3. W02 — 类型化标记与容器来源

参考：writ 的 [`marker.rs`](https://github.com/wilfreddenton/writ/blob/0f32b7f2a99ffe11065a8834b64c8057061dbb60/src/marker.rs) 为列表、引用、标题、任务框及围栏等标记记录类型和字节范围。

引入带源码范围、ownership 和角色的类型化语法记录。通过投影层和 Planner，由这些记录派生显示装饰和续行请求。语法 ownership 独立于渲染策略；仅有显示标记列表不足以解释全部原文字节。

验收：使用嵌套引用/列表/任务及非连续行内输入。源码 `> **a\n> b**` 中（`\n` 表示换行），第二行引用前缀归 BlockQuote，不归 Strong。保留 Tab、空格、分隔符拼写和混合行尾。除逐字节序列化外，还要验证源码全覆盖和 ownership 关系。

## 4. W03 — 将显露会话接入投影

参考：writ 的 [`render.rs`](https://github.com/wilfreddenton/writ/blob/0f32b7f2a99ffe11065a8834b64c8057061dbb60/src/render.rs) 根据光标是否位于行或行内区域中构造显示文本和样式。

由解析快照和显式交互状态共同构造投影。活动源码窗口输出原文片段；预览区域输出语义文本、样式或对象。重解析改变语义节点类型时，会话仍以源码坐标为锚点。光标移动可以改变投影，无需重解析未修改的源码。

验收：演示点击 → 显露 → 原文编辑 → 重解析 → 离开 → 预览。回放 `**b** → **** → **x**`：删除 `b` 后，即使语义成为分隔线，也保持四个星号的源码会话可编辑；两次转换都保留选区和可见源码锚点。拖选期间冻结显露集合，composition 会话保持到平台协议完成。

此任务依赖 W01，以及足以覆盖所支持语法子集的 W02 来源记录。来源不足时使用 v4 的 SourceBlock 回退，不能猜测隐藏范围。

## 5. W04 — 投影缓存、视口布局与滚动锚点

参考：writ 的 [`doc_layout.rs`](https://github.com/wilfreddenton/writ/blob/0f32b7f2a99ffe11065a8834b64c8057061dbb60/src/doc_layout.rs) 缓存逐行渲染和实测高度，构建视口内容，并在重建后恢复滚动锚点。

本项目以 Markdown 块及逻辑范围作为缓存单元；只有来源和布局允许时才进一步细分。投影失效判断覆盖 source/parse 身份、显露状态、composition 和装饰变化；布局还依赖宽度、字体、缩放及已解析对象尺寸。只有输入仍然有效的缓存项才能复用。

重建前记录源码锚点及其相对视口位置。编辑后先变换锚点，再在新投影中定位并补偿高度变化。后续可用实测高度索引定位可见块和 overscan 区域；光标及组合输入上下文始终保留布局。

验收：比较有缓存与无缓存的构造结果；覆盖只移动光标、宽度/字体变化、表格显露、异步图片尺寸变化及视口上方编辑。分别测量投影、文字 shaping/布局和渲染。以上为拟议适配；writ 的逐行模型不能证明本项目跨行块语义的正确性。

## 6. W05 — 平台 composition overlay

参考：writ 的 [`shell.rs`](https://github.com/wilfreddenton/writ/blob/0f32b7f2a99ffe11065a8834b64c8057061dbb60/src/shell.rs) 将 preedit 作为仅用于显示的状态，并按组合光标更新操作系统候选窗位置。

将现有 `Composition` 隔离机制接入 v4 输入上下文快照及宿主适配器。在同一带版本输入上下文中维护 preedit、marked selection 和 overlay 到源码的映射。commit 通过一次带 revision 校验的源码事务完成；update/cancel 不改变源码字节、源码 revision 或历史。明确 commit/cancel 完成事件后，再处理排队的撤销、模式切换或外部编辑。

验收：回放中文输入、emoji、替换选区、取消、过期提交、提交后撤销及候选窗位置。在平台桥接处验证 UTF-16 surrogate 边界，在最终组合投影中验证字素边界，包含 `*e*` 后跟 U+0301 的情况。真实操作系统回调序列与无界面的隔离测试分别验收。

## 7. W06 — 存储与增量解析评估

参考：writ 的 [`buffer.rs`](https://github.com/wilfreddenton/writ/blob/0f32b7f2a99ffe11065a8834b64c8057061dbb60/src/buffer.rs) 使用 Rope 存储；[`parser.rs`](https://github.com/wilfreddenton/writ/blob/0f32b7f2a99ffe11065a8834b64c8057061dbb60/src/parser.rs) 向 tree-sitter 提供 Rope 分块及已应用编辑信息的旧树。

保留既定存储抽象（`len_bytes`、片段读取、cursor 和原子补丁应用）。测量源码复制成本后，再评估持久 Rope 或 piece tree。保持不可变快照、UTF-8 字节范围、CRLF 边界、全量校验后原子提交、逆事务及 affinity 语义。

分别评估增量语法、投影复用和布局虚拟化。所核对的 writ 代码仍遍历行内作用域，节点/样式缓存也可能全文重建；仅复用旧树不能证明整条编辑链路始终局部。不得直接采用无条件的 `O(log n + k)` 延迟承诺。

tree-sitter 后端实验在通过固定方言和来源检查前保持隔离。当前 pulldown-cmark 后端及选项继续作为基线。增量结果与全文解析对比，包含引用定义修改及未解析使用者、围栏、表格和列表边界。无法证明可复用时扩大失效范围或全文回退；最坏传播可以到 EOF。

验收：与全文解析比较语义、源码范围、ownership 和映射；替换存储后运行同一组编辑/逆事务测试。基准覆盖 10/100 KiB 和 1 MiB 文档、长段落、嵌套容器和长分隔符，同时报告 p95、内存及实际测量范围。

## 8. 吸收约束

- 普通编辑保留源码拼写。所核对的 writ buffer 可在编辑时重编号有序列表。本项目的显示编号放入装饰；改写源码编号必须是显式 Planner 命令及可逆事务。
- 保留带 affinity 的转换处理。writ 的折叠 token 定位选择来源起点且没有 affinity 参数，不能替换本项目的转换契约。
- 无界面内核保持独立，不依赖 GPUI、Vello、winit、文件监听、GitHub 或网络 I/O。借鉴机制不等于新增 writ 运行时依赖。
- 保持 GFM 0.29 和显式 tables/strikethrough/tasklists 选项。不引入额外方言功能，也不为迁就其他后端而直接调整 629/677 诊断数量。

## 9. 实施顺序与阶段门槛

以下任务全部待实施。它们细化现有 M0–M4 计划，不改变发布门槛。

| 顺序 / 阶段 | 交付物 | 完成证据 |
| --- | --- | --- |
| 1 / M0–M1 | W02 来源实验与所支持子集的 W01 投影映射 | ownership、隐藏范围、转换、affinity 和过期命中回归 |
| 2 / M1 | W03 最小交互视图及 W05 平台 composition | 显露/编辑/预览回放、语义类型转换及真实 IME 结果 |
| 3 / M2 | 将 W01–W03 扩展到结构化块及命令 | 嵌套容器、表格、任务、代码、SourceBlock 及逆事务回归 |
| 4 / M1 测量、M4 优化 | W04 缓存/布局与 W06 存储/解析评估 | 无缓存/全文解析等价、滚动稳定、延迟及内存实测 |

先完成所支持范围内的投影与交互闭环，再改变存储或解析后端。完整 GFM、完整 ownership、通用 Planner、GUI/IME 集成及增量引擎仍按原有要求验收。

## 10. 证据与限制

对比期间，`cargo test --workspace --locked` 通过 21 项单元测试、2 项集成测试和 1 项 doctest。这些测试覆盖现有原语，不覆盖上述待实施任务。

所核对的 writ `segment_map.rs` 使用 Rust 2021 模式单独编译，其 5 项单元测试通过。另一个合成探针将 `&fjlig; → fj` 作为折叠 token 输入，观察到显示 byte 1 映射到源码 0。该结果证明映射辅助类型的策略，不证明 writ 自身如何渲染实体。

对比期间未构建或操作完整 writ 应用；未验证真实 GUI、操作系统 IME 或端到端性能。本文不修改后端、依赖、fixtures 或已记录的可行性结果。
