# GPUI 单文件 Live Markdown 编辑器：可行性评审

评审日期：2026-10-11。范围：`docs/01`—`docs/09` 的架构、算法、交互和验证方案，以及相关上游源码。本文仅提出评审结论和修订建议，不是已经批准的产品规格，不包含编辑器实现。

> 2026-10-12 修订：用户已将产品路线改为 [Typora 风格 Hybrid View](./11_GPUI_Typora_Hybrid_View_Spec.md)。正文保留初次评审的证据与问题；旧的“正文聚焦即展开块前缀”规则已被稳定结构与显式源码入口取代。以下平台与算法风险仍未经过编译/实机验证，不能因文档改写视为解决。

**结论：核心架构有条件可行；采用 Hybrid 后仍需补齐文本几何、IME 与结构编辑契约，并验证 GPUI 的原生编辑能力。**

单一源码、非破坏性投影、统一编辑焦点、版本化几何、源码事务与 Undo 的方向正确。主要风险集中在文本布局底层、输入协议和交互语义，而非 Markdown 的视觉绘制。建议先收敛规格，再验证平台缺口，随后实施最小完整编辑链路。

## 1. 证据与边界

- 本地没有 Cargo workspace、Rust 编辑器实现或可运行探针；第九份自身也明确说明未编译、未实机验证。
- 本次获取并检查了 Zed 官方仓库 commit `93d745668805029476e6d7fbb19652116fed51de` 的相关源码。此 SHA 是本次静态审查的证据版本，**不是经过构建测试的推荐依赖版本**。
- 检查了 GPUI 输入 trait、官方 input 示例、LineLayout、Apple 文本后端、macOS 输入桥和 AccessKit 接入代码。
- 未运行 Cargo 编译、GPUI 窗口、真实 IME、VoiceOver 或性能测试；没有操作 Bear 2 做行为对照。
- Bear 官方确认支持隐藏 Markdown，但不能由此推导其边界导航、删除、剪贴板或内部架构。[Bear 官方说明](https://bear.app/faq/how-to-use-markdown-in-bear/)

| 层次 | 评审判断 |
|---|---|
| 单文件 Document / Transaction / Undo | 设计方向可行，需补齐部分数学契约 |
| 光标感知 Inline Projection | 可行，是核心体验的合适基础 |
| GPUI 窗口、自定义 Element、输入回调 | 上游已有源码依据；本项目未编译验证 |
| 完整 Bidi、连字光标、多行几何 | 现有便捷 API 不足以直接满足规格，须补充后端能力 |
| Composition Overlay | 可行，但当前系统回调契约不完整 |
| VoiceOver 文本编辑 | 有 AccessKit 接入基础，完整体验仍须验证 |
| 百万字单段落的低延迟编辑 | 文档给出了目标，尚未给出足以证明目标的算法和实测 |

## 2. 优先修订的问题

### R1 · 高：Tree-sitter Markdown 不适合直接作为正确性权威

位置：第四份 §2.1—2.2，尤其第 79—92 行。

文档已经提醒解析器需验证，也保留替换路线；问题在于调查顺序仍容易让开发默认把 Tree-sitter 当作主引擎。其官方 README 明确说明存在较多解析不准确，目标主要是编辑器语法高亮，不建议用于正确性重要的场景。Live Markdown 会据解析结果隐藏字符、删除 delimiter 和执行格式改写，要求高于普通高亮。[上游说明](https://github.com/tree-sitter-grammars/tree-sitter-markdown#goals)

建议优先评估 `pulldown-cmark` 作为唯一语义权威，使用其 offset iterator，加上受语义约束的精确 marker 提取器。该 API 提供事件对应源码范围，**不等于已经提供每一种 delimiter 的独立精确范围，也不等于支持增量解析**。[Parser API](https://docs.rs/pulldown-cmark/0.13.4/pulldown_cmark/struct.Parser.html#method.into_offset_iter)

首轮允许在明确大小预算内对不可变快照全量解析，后台结果按版本发布；布局依然只更新受影响部分。输入期间若没有新语义快照，应采用确定的原文呈现策略。等语义正确和基准建立后，再优化增量调度。不能简单把若干段落截出来分别解析而忽略引用定义、容器上下文和围栏传播。

若继续采用 Tree-sitter，应明确受支持语法子集及每个差异的处理，证明它足以作为该子集的权威；不要让两个解析器在运行时分别决定语义和隐藏范围。

### R2 · 高：复杂文本布局的缺口已经能从源码定位

位置：第五份 §6、§7；第七份 §5；第九份 §11。

文档正确识别了 Bidi 和 ligature 风险，但现在可以把“待调查”细化成具体工作：

- 此版本 `LineLayout::x_for_index(index)` 返回一个 x，没有 affinity 参数；实现按 glyph index 搜索。
- `index_for_x` / `closest_index_for_x` 基于 glyph 位置，不提供完整的 grapheme、Bidi 双光标和连字内部停靠点模型。
- `ShapedRun` / `ShapedGlyph` 中没有文档所需的完整 bidi level、逻辑 cluster 范围及双 caret 信息。
- Apple 后端借助 CoreText 塑形，但返回给 GPUI 的对象保留 glyph 等信息，没有同时暴露 CTLine 的完整 caret 查询能力。

这并不表示 GPUI 无法绘制 RTL，而是**绘制正确不等于编辑导航正确**，不能只给现有两个查询函数包一层 trait 就宣称完成 CaretStop Graph。

证据：[LineLayout](https://github.com/zed-industries/zed/blob/93d745668805029476e6d7fbb19652116fed51de/crates/gpui/src/text_system/line_layout.rs#L17)、[Apple 文本后端](https://github.com/zed-industries/zed/blob/93d745668805029476e6d7fbb19652116fed51de/crates/gpui_apple/src/text_system.rs#L540)。

建议 macOS 首版评估在 GPUI 文本后端保留或导出 CoreText 的 caret/line 数据；Apple 的 `CTLineGetOffsetForStringIndex` 提供主、次偏移，另有命中与 caret 枚举接口。需要让绘制、换行和几何查询消费同一排版结果，避免另起一套测量器后与 GPUI 绘制不一致。[Apple API](https://developer.apple.com/documentation/coretext/ctlinegetoffsetforstringindex(_:_:_:))

纯 GPUI 界面可以使用其平台后端中的 CoreText；这仍然需要明确的适配或上游补丁工作。是否足够完成全部复杂文本行为，要由真实字体和输入测试决定。

### R3 · 高：IME Overlay 缺少完整的“平台可编辑文本视图”契约

位置：第三份 §8.2—8.6；第七份 §8；第九份 §10.2—10.4。

第七份已经补充 `EditablePosition::Composition`，这是必要修正。但其他接口仍使用 `SourceRangeAnchor` 表示当前 marked range，或只描述 UTF-16 ↔ committed SourceByte，无法表达 overlay 内部的位置。

例如 committed 源码为 `A😀B`，选中 😀 后预编辑为 `你好`。平台此时应针对临时文本视图 `A你好B` 查询 selection、marked range、substring 和 replacement range，不能有的回调读取 Rope，有的读取投影。再更新为长度不同的候选文本时，错用旧坐标会替换到后面的 `B`。

建议独立定义 `EditableTextSnapshot = committed source + temporary replacement`，统一提供：

1. 文档全局 UTF-16 范围与 Source/Composition 联合位置的转换。
2. `new_selected_range` 相对于新 marked text 的转换；与文档全局 replacement range 使用不同类型。
3. `range == None` 时的规则：当前 marked range 或当前 selection。
4. 覆盖 overlay 边界、跨段落 replacement、空 marked text、部分提交的行为。
5. 当前 composition 内容与选择变更的版本；统一进入 presentation/layout 的失效契约。

还需补齐 `unmark_text`：上游接口表示移除 composing 状态，官方示例仅清除 marked 标记而保留文本。采用 overlay 后，不能直接清空 overlay，否则可能丢掉屏幕上的文本。应区分“将现有预编辑内容确认为普通文本”“由 insertText 提交”“取消恢复”三条路径，具体事件序列实机记录。[输入协议](https://github.com/zed-industries/zed/blob/93d745668805029476e6d7fbb19652116fed51de/crates/gpui/src/platform.rs#L2240)、[示例](https://github.com/zed-industries/zed/blob/93d745668805029476e6d7fbb19652116fed51de/crates/gpui/examples/input.rs#L312)。

另外，当前 macOS 后端对 `NSAttributedString` 的 marked text 提取字符串再转交，不能据此假定候选分段属性和全部原生输入样式已经传到应用。若这属于产品要求，需要独立核验/扩展。[macOS marked text 桥](https://github.com/zed-industries/zed/blob/93d745668805029476e6d7fbb19652116fed51de/crates/gpui_macos/src/window.rs#L3453)

### R4 · 高：候选框坐标不能直接照设计接口返回 ScreenRect

位置：第五份 §15；第九份第 536—544 行 `PlatformTextInputBridge`。

第九份把内部 `first_rect_for_range` 设计为返回 `ScreenRect`，这可以作为内部 API，但适配到 GPUI 时必须明确转换责任。

本次检查的 macOS `first_rect_for_character_range` 会将 GPUI 回调返回的 bounds 加上窗口位置，并翻转 y 坐标；`character_index_for_point` 也会先把屏幕坐标转换成 GPUI 窗口坐标。即使公共 trait 注释提到 screen coordinates，**当前后端实现仍应纳入审计**。如果应用提前转换成屏幕矩形，后端再转换一次，候选框会随窗口移动而错位。[后端源码](https://github.com/zed-industries/zed/blob/93d745668805029476e6d7fbb19652116fed51de/crates/gpui_macos/src/window.rs#L3393)

建议平台适配边界显式使用 `GpuiWindowRect` / `GpuiWindowPoint`，由 macOS 后端完成屏幕转换；如果另有真正要求 ScreenRect 的系统接口，再独立转换。验收至少覆盖编辑区域 padding、文档滚动、窗口移位和副屏。

该后端的 `firstRect` 回调目前没有写入传入的 actualRange 指针。多行请求若只返回首行矩形，还需核验系统是否需要相应的实际范围，并在必要时补足桥接能力。

### R5 · 默认选择已确认：复制 Markdown；边界规则已补充，待实现验收

位置：第八份 EB-005、EB-040—043、CT-046—047、CT-095—101。

原方案存在两个问题，记录如下；后续修订已明确默认 Markdown，并补充序列化规则：

1. `ab**cd**ef` 从 `a` 选到 `f` 时，EB-005 要展开相交节点；此时“用户选中的可见文本”包含 `**`。直接复制 Projection 并不能实现去标记的纯文本复制，输出还可能依赖选区是如何形成的。
2. `**hello**` 只选中内容时，Source Selection 为 `[2,7)`。所谓 Copy as Markdown 若只是切片，得到的是 `hello`，不带粗体；`[label](url)` 只选 label 时也会丢失链接目标。

修订后明确区分三个概念：

- **Copy Source**：严格的源码切片，忠实保留用户选择。
- **Copy Plain Text**：根据语义树和选区序列化，结果不受临时 reveal 影响。
- **Copy Markdown Fragment（默认 Cmd+C）**：保留选中内容的格式，必要时补齐外层标记、处理部分链接和引用定义；它不等于源码切片。

用户已确认默认复制 Markdown；本次文档修订采用保留内容格式的片段序列化，`text/plain` 载荷同样写入 Markdown，平台支持时另提供等价 `text/markdown`。选区切入语法区域或无法可靠序列化时回退为精确源码切片；Copy Source 始终提供精确复制。剪切使用同一序列化器，只删除原 Source Selection，Undo 保证字节级恢复。第八份已增加部分粗体、链接、全选、标记切片和剪切验收用例。**默认格式是用户决策，具体边界规则是本文配套设计，尚未实现或通过测试。**

### R6 · 高：Selection reveal 还没有完整的拖选策略

位置：第二份 §4.3；第五份 §8.5；第八份 EB-005、EB-008。

“一次点击只 hit-test 一次”解决了单击，但拖选会不断产生新事件。若每次 selection 与新节点相交就展开标记，长 URL 或嵌套格式会在拖动期间重排。下一次几乎相同位置的 MouseMove 会命中不同字符；即使每次事件只 hit-test 一次，选区仍可能跳动。

建议将 Pointer Selection 作为单独状态机。推荐待验证的默认是：鼠标按下完成初始 reveal 后，在拖选期间冻结 reveal 集合；滚动仍更新 viewport transform，目标区域仍可按需布局；MouseUp 后一次性重新计算显隐，保持两个源码端点不变。宽度变化、编辑发生或 IME 启动时显式结束/更新此状态。

非折叠选区也可只展开端点相关节点，中间完整选中的节点保持紧凑。最终策略应通过长链接、窄窗口和自动滚动用例比较，不应仅凭 Anchor 稳定就认定视觉体验稳定。当前 Hybrid 规格已将冻结拖选作为本项目默认；本风险只针对 Inline reveal，外层 Block 不随选区展开。该规则仍待实测，不是 Typora/Bear 行为证明。

### R7 · 高：ChangeMap 的相邻编辑公共端点没有唯一规则

位置：第七份 §2.2、§2.4，第 146—177 行。

规范允许相邻但不重叠的替换，却同时把 `p == start` 和 `p == end` 视为可向替换两侧坍缩。

反例：源码 `ab`，同一事务中 `[0,1) → X`、`[1,2) → Y`，锚点 `p=1`。

- 按第一个 edit 的 `p==end` 解释：Before → 0，After → 1。
- 按第二个 edit 的 `p==start` 解释：Before → 1，After → 2。

两者都符合单个替换的规则，却不能构成唯一的批量映射；二分定位落在哪个 edit 将影响结果。

应正式规定公共边界与 edit 的关联，并写出映射真值表。一种简单候选是：非空替换起点映射到新起点、终点映射到新终点，仅内部点坍缩；零宽插入单独使用 Bias。若业务确实需要边界随替换整体移动，则保留关联语义，但必须规定相邻 edits 的决议顺序。不能未经语义约定就把相邻替换合并，因为那会抹去原本可保留的中间边界。

验收需覆盖相邻替换、相邻删除后逆向插入、同点多插入、替换端点插入和 SelectionPlan。

### R8 · 中：Block 语义树与布局序列尚未明确分离

位置：第四份 §4、§6；第五份 §11；第七份 §6。

语义树的 List、ListItem、Paragraph 范围相互包含；HeightIndex 则需要一个有确定次序且不重复计算高度的布局序列。文档没有明确哪些节点进入该序列，以及容器前缀、空行、段落分隔符和嵌套装饰由谁拥有。

建议增加薄的 `FlowBlock` / 布局块生成阶段：按阅读顺序输出叶子内容及必要的可编辑空行，携带祖先容器的缩进、引用层级和 marker 装饰。HeightIndex 只索引这份布局序列；语义容器不重复占高。跨行内容用 spans 表达，分隔符须有唯一归属。

这也能避免在超长列表中以整个 List 容器作为一个 Block 重排，以及同时绘制 ListItem 与 Paragraph 的重复内容。2026-10-12 的 Hybrid 修订已在设计中采纳 FlowBlocks；表格另以 cell graph 表达二维几何。源码覆盖与高度唯一归属仍须实现验证。

### R9 · 中：引用依赖需要包含“尚未解析为链接”的候选

位置：第四份 §9.3，第 598—605 行。

`usages: HashMap<NormalizedLabel, Vec<NodeId>>` 只说明记录引用节点，未说明未解析引用如何登记。

反例：文件头存在 `[guide][new]`，暂时没有定义，所以当前语义上是普通文本。用户在文末新增 `[new]: https://example.com`。如果索引只有成功解析的 Link 节点，文件头不会被重新解析；删除重复定义中的优先定义也可能改变有效目标。

建议记录未解析引用候选及其所属块，或在定义集合变化时保守地重新解析全部可能受影响的 inline 块。定义表应明确标准化、重复定义优先级以及有效定义切换。全量语义解析作为首版基线可自然覆盖该问题，未来局部优化必须保留差分测试。

### R10 · 中：长段落“只布局可见行”不能作为无条件复杂度保证

位置：第五份 §11.5；第七份 §11.4；第八份 EB-084、CT-092。

文档已意识到单 Block 虚拟化问题，但“按 VisualLine/chunk 虚拟化”仍是方向，不是完整算法。对单个超长段落，远处的第 N 个视觉行在哪里，可能需要先计算前面的换行；段首修改或宽度变化也可能使后续行全失效。Bidi 解析还存在段落级依赖。

应拆分并分别测量：解析、投影构造、段落方向分析、shaping、换行索引、绘制。仅减少 paint 或不创建离屏 Element，不能证明避免了全文 shaping。

建议设定明确的首版文件/单段预算，代码块优先按源码行处理；普通长段落采用换行检查点和复用，允许最坏情况扩大计算，提供取消与分批调度。长期的大文档目标可保留，但不要将“任意大单段永不全量计算”设成无法保证的 MUST。

## 3. 其他需收敛的规格

**Markdown 语义显示不止隐藏 delimiter。** 第四份 §7.4 提到 CodeSpan 的空白规范化，但首版 Projection 只定义 Visible/Hidden。CommonMark 的行内代码会将换行转为空格，并在特定条件下去掉首尾一个空格。应明确首版保留源码空白的编辑视图差异，或增加带 source map 的文本替换段；不必把普通文本替换和图片 Widget 绑定成同一功能包。实体解码已在第四份 §12.4 正确延期，应保留这一明确边界。[CommonMark 0.31.2](https://spec.commonmark.org/0.31.2/#code-spans)

**Pending Mark 只有单字符例子，还不是可实施状态机。** 第八份 EB-032 只覆盖 Cmd+B 后输入 `x`，需要补充连续输入、先输入空格、换行、IME 开始/提交、再次按 Cmd+B、移动光标和 Undo 的规则。尤其不能假设在任意输入两侧加 `**` 就一定形成合法 Strong。第一阶段可先完成非空选区格式化，再以独立验收接入 pending mark。

**AX 与 IME 不应被当成同一接口。** 此版本 GPUI 已通过 AccessKit 构建无障碍树，并支持自定义 Element 提供 synthetic children，单一 Editor Element 不必因此拆成多个输入框。但还需要文本节点、选择动作、稳定 ID、布局范围与离屏导航的具体协议；`Role::Document` 本身不能证明可编辑 VoiceOver 已完成。可先以 Source+Composition 建立稳定输入坐标，再独立验证语义无障碍树的映射。[GPUI AccessKit 接入](https://github.com/zed-industries/zed/blob/93d745668805029476e6d7fbb19652116fed51de/crates/gpui/src/window/a11y.rs#L1)

**统一行高与真实字体度量需要区分。** 第一份要求统一基础字号/行高；这可以作为主题偏好，但行盒必须能容纳 CJK、emoji 等 fallback 的 ascent/descent。建议“共同的最小行高 + 必要时按真实度量扩大”，避免裁切；标题字号策略是产品偏好，不是技术不变量。

**不应预先把所有优化做成框架。** 单文件、单选区时，长期交互 Anchor 数量很少，eager 变换可能比 Lazy AnchorStore/Epoch 更易验证。AST 继续用快照范围，不给每个 token 注册 Anchor。Projection 的 Vec segments 值得保留；HeightIndex 先使用成熟结构或简单参考实现，再依据负载升级聚合树。逻辑分层可先放在少数 crate 的 modules 中，无须一次创建 11 个生产 crate 与全部探针工程。

**单文件产品范围需要与后期增强分开。** 多光标、完整富文本粘贴、表格/图片、极端大文档等可作为独立后续能力；这不影响首版保留 Unicode、中文输入、跨段落选择和可靠保存。当前 CT-036 把多光标纳入核心门禁，而最初范围没有明确要求，建议标记适用版本。

## 4. 文档之间需要修正的冲突

| 位置 | 问题 | 建议 |
|---|---|---|
| 第七份与第八份 ADR-021—024 | 相同编号代表不同决策 | 建立唯一 ADR 索引，再让各文档引用 |
| 第二份 §5.3 与第七份 §4.4 | 前者合并重叠隐藏区间，后者要求不同 owner 重叠先报错 | 明确以后者的 ownership 校验为准，不能直接 union 掩盖解析错误 |
| 第三份 CompositionSession 与第七份 EditablePosition | 前者 current_marked_range 仍为 Source anchors | 统一为源替换范围 + overlay-local selection + editable text snapshot |
| 第八份 P-003 与第七份 composition 联合位置 | 前者要求每个显示 caret 回到 Source grapheme | 排除/显式涵盖 Composition 位置，不能要求它映射到不存在的 committed 字符 |
| 第二份 §6.5 与第五份 §13.1 | IME 更新是否触发 Syntax 不一致 | committed parser 不随预编辑更新；临时派生解析另行命名 |
| 第二份导航表与第八份 EB-015 | Cmd+Left/Right 的视觉行/逻辑行表述不一致 | 以实际 macOS 对照和正式 keymap 契约锁定 |
| 第八份 §18 与第九份 §24 | 前者晚接入 shaping/AX，前面的门禁却依赖视觉导航；后者先做危险探针 | 以平台探针先行、功能垂直集成为实施顺序 |
| 第一份统一字号、第四份 LF 规范化等 | 产品选择与正确性不变量混排 | 产品默认通过 ADR 决定；源码无损、合法坐标等继续作为不变量 |
| 第二份顶部配套文档链接 | 指向不存在的旧文件名 | 修正为当前编号文件名 |

第九份已经诚实说明证据等级，因此问题不是它虚报了验证结果，而是仍需实际补齐证据。建议后续在其核验矩阵内更新事实，不再继续增加重复的“下一份架构方案”。

## 5. 推荐的收敛方案与验证顺序

保留这一主链路：

```text
Document / Transactions / History
    → authoritative Markdown syntax
    → precise markers + HybridPresentationState
    → InlineVisibilityPolicy / BlockPresentationPolicy
    → FlowBlocks / table cell graph / source maps
    → Projection
    → native text layout + caret geometry
    → GPUI custom Editor Element
```

Composition 通过独立 EditableTextSnapshot 接入；所有持久选择、文件保存与历史仍以 committed Source 为基础。绘制、点击、键盘视觉导航和输入法几何共享同一布局结果。

| 顺序 | 工作 | 通过条件 |
|---|---|---|
| 1 | 收敛规格 | 复制/删除/拖选/IME/换行都有明确输入与预期，ADR 无冲突 |
| 2 | 固定依赖并验证平台 | 窗口、复杂文本 caret、中日韩输入、坐标转换、基础 VoiceOver 有实机证据 |
| 3 | 最小垂直闭环 | 两个段落 + Strong，能输入、点击、跨段落选择、Undo、保存与重开 |
| 4 | Hybrid H1 | Inline、结构化标题/列表/任务/引用/代码/Setext、局部与全文源码通过 CT-102—114 |
| 5 | Hybrid H2 | 表格网格、结构操作和二维映射通过 CT-115—117 |
| 6 | 性能与产品增强 | 用基准决定增量 parser、AnchorStore、HeightIndex、长段检查点等复杂化顺序 |

建议至少补充以下验收样例；它们是后续测试清单，本次没有实现或执行：

1. 连字、组合音标、家庭 emoji、Arabic+English 的点击/左右键/换行几何。
2. 窗口移动到另一屏幕后 IME 候选框仍对齐。
3. `A😀B` 中替换预编辑、改变 marked 长度、仅 unmark、取消和一次 Undo。
4. 从普通文本拖选经过超长链接，指针几乎不动时选区不会因 reveal 连续漂移。
5. `ab**cd**ef` 的同一语义选区分别复制 Source、Plain Text、Markdown Fragment。
6. `**hello**` 只选择内容后复制/剪切/粘回，预期格式与源字节明确。
7. 相邻替换的公共端点 Anchor 真值表及 forward/inverse 往返。
8. 文末新增引用定义后，文首未解析引用正确变化；删除优先定义后重新决议。
9. 嵌套列表、引用、空行、跨行 CodeSpan 的 FlowBlock 覆盖与唯一分隔符归属。
10. 宽度变化和单段段首编辑分别测量解析、投影、塑形、换行与绘制成本。

**本轮建议是保留架构、修订契约，并把平台文本能力验证作为进入正式开发的前置条件。** 初次评审仅新增本文；后续按用户要求同步修订了默认 Markdown 复制规则，并在 2026-10-12 改为 Typora 风格 Hybrid View；第十一份明确了本项目的块状态、源码入口、触发规则与阶段范围。未实现编辑器，未更改代码或用户既有工作区改动。
