# GPUI + GFM Live Markdown 编辑器：段落级断行与 CJK 混排架构设计 v1.0

> 状态：建议方案 / 可进入技术原型阶段  
> 范围：Rust、GPUI、单文件 GFM、Live Markdown（源码为唯一事实来源）  
> 核心结论：**标准断行作为稳定基线，段落级全局平衡作为可控增强；默认左对齐，不默认 TeX 式两端对齐。**

## 1. 执行摘要

不建议把经典 TeX Knuth–Plass 算法直接替换为编辑器唯一断行器。推荐实现独立 `ParagraphLayoutEngine`，在统一的 Unicode 断行候选和字形测量基础上，提供三种策略：

1. **Standard**：基于 UAX #14 + 中文标点禁则的确定性贪心断行。默认编辑基线和降级路径。
2. **Balanced**：借鉴 Knuth–Plass 的全段动态规划，对自然语言段落最小化行尾参差、非法断点及孤行惩罚；**默认左对齐，不人为拉伸中文字符间距**。静态阅读和稳定编辑时优先采用。
3. **Justified（实验性）**：使用 Box/Glue/Penalty、伸缩率和 demerits，结合专门的 CJK spacing 策略。第一阶段不实现。

**首发产品建议：** 默认启用 Standard；将 Balanced 作为用户可开启的“平衡断行”功能。完成光标/IME/缓存压力测试后，再评估将 Balanced 作为普通自然语言段落默认模式。不可承诺全局最优算法总能更美观，需通过真实字体、语言和编辑体验验证。

## 2. 目标与非目标

### 2.1 目标

- 中英数字标点混排时，不在非法位置断行；兼容 Emoji、组合字符和多字体回退。
- 对一整段而非每行单独选择软换行，使行尾参差在可接受范围内更均衡。
- 严格保留 `.md` 文件原文：视觉软换行不写入 SourceText；显式硬换行仍遵循 GFM。
- 保持 SourceAnchor、双向 affinity、selection、IME、undo、滚动和块级装饰定位正确。
- 支持按段缓存、宽度变化重排、可见区域优先、极端输入回退。
- 不依赖 HTML 或浏览器排版引擎。

### 2.2 非目标

- 不复制 TeX 全套段落构造、断词词典、微排版与 PDF 输出体系。
- 不把视觉行作为 AST 或持久文档节点。
- 不默认两端对齐，不为“填满一行”而任意拉伸汉字间距。
- 不跨不同 GFM 块全局优化；一个自然语言段落是优化边界。
- 不在 v1 自动插入软连字符、改写标点、修改源码空格或换行。

## 3. 关键设计原则

**原则 A：源码和排版严格分离。** `SourceText (Rope)` 是唯一事实来源。Lossless Syntax、GFM Semantic AST、Projection、Layout、Interaction 各司其职。布局产生的软换行仅为视觉决策。

**原则 B：语义合法性先于美观。** UAX #14 断行机会只是基础，需结合语言禁则、不可拆分对象、GFM 内联语义、grapheme 和 shaping cluster。任何优化不得选取不合法断点。

**原则 C：测量一致性先于算法最优。** 候选行宽必须尽可能与最终 GPUI 绘制的 glyph advances 一致；shaping、kerning、连字、字体回退和 BiDi 均可能影响结果。

**原则 D：交互稳定优先。** 编辑时允许暂时使用 Standard 或维持旧断点，不允许光标丢失、IME 候选框错位或鼠标命中错误。

**原则 E：有限优化、明确回退。** 任何时间预算、候选数、段落长度或复杂排版超限，都可降级为 Standard，并记录原因。

## 4. 系统边界与数据流

```text
Rope SourceText (revision)
  └─ Lossless GFM Parser / Semantic AST
       └─ ProjectionBuilder (projection_revision)
            ├─ visible text + styled runs
            ├─ source ↔ projection mapping
            └─ atomic inline objects / explicit breaks
                 └─ ParagraphLayoutEngine
                      ├─ Grapheme / BiDi / language segmentation
                      ├─ UAX #14 + CJK rules + GFM constraints
                      ├─ Shaping / metrics cache
                      ├─ StandardLineBreaker
                      └─ BalancedLineBreaker
                           └─ VisualLineLayout (layout_revision)
                                ├─ paint / hit test
                                ├─ caret / selection / IME rect
                                └─ block adornment geometry
```

注意：以上为依赖关系，不要求所有 shaping 一次完成。候选片段测量与最终行 shaping 可迭代或校验。**不得直接假定单个字符 advance 的简单相加等于整行真实宽度。**

## 5. 断行规则与内容分类

### 5.1 断点生成

- 先按 Unicode grapheme cluster 确保用户可见字符不会被拆开。
- 基于 UAX #14 生成软断点机会；考虑语言属性与中文排版禁则（可参考 W3C CLReq）。
- 英文普通单词内部默认不拆分；连续超长 token 走明确的 emergency-break 策略，并避免拆开 grapheme/shaping cluster。
- CJK 标点的行首/行尾禁则、成对标点、引号、数字单位、不可断空格需单独处理。
- BiDi 的逻辑文本断点与视觉排序必须分离；断行在逻辑序列上确定，随后构造视觉行。
- 行内代码、链接标签、图片占位、数学/嵌入对象按策略指定允许断点和原子范围。链接文字可按普通文本断行，但点击命中必须映射到同一链接目标。
- GFM 软换行与显式硬换行（行尾两个空格或反斜杠语义）必须区分；硬换行是强制边界，不参与跨边界全局优化。
- Markdown 标记的隐藏/显示可能改变 projection 宽度；以当前 `projection_revision` 为布局输入，不能拿隐藏状态下的宽度复用到 reveal 状态。

### 5.2 块级策略矩阵

| GFM 内容 | 默认断行 | Balanced 适用性 | 说明 |
|---|---|---|---|
| 普通段落 | Standard | 是 | 主要目标 |
| 标题 H1–H6 | Standard | 可选 | 短标题常无需优化；避免突兀跳行 |
| 引用段落 | Standard | 是 | 仅在引用内的独立段落优化，宽度扣除缩进 |
| 列表项中的段落 | Standard | 是 | 宽度扣除列表标记、缩进；不跨子块 |
| 表格单元格 | Standard | 暂不 | 先确定列宽，再逐单元格布局；防止反馈循环 |
| 围栏/缩进代码块 | 保留源换行 | 否 | 横向滚动或显式折行单独配置 |
| 行内代码 | 按原子/受限规则 | 受限 | 不在危险 cluster 内断开 |
| 独立图片段落 | 对象布局 | 否 | 按对象尺寸与缩放策略 |
| 分隔线 | 非文本布局 | 否 | 无需断行 |
| 文档末尾虚拟插入锚点 | 非文本布局 | 否 | Interaction overlay，不参与段落优化 |

## 6. 算法选择

### 6.1 Standard：确定性贪心

按合法断点逐行填充，选择不超过目标宽度的最远合法断点。若首个可断单元已超宽，按 emergency-break 策略处理。此算法必须先达到生产可用，作为 Balanced 的对照基线与回退实现。

### 6.2 Balanced：段落级动态规划

定义候选合法断点 `b_0...b_n`。状态 `dp[j]` 表示到断点 `j` 的最小累计代价，边 `i→j` 表示把该区间放入一行：

```text
dp[0] = 0
dp[j] = min over feasible i < j:
  dp[i] + line_cost(i,j) + break_penalty(j) + transition_cost(i,j)
```

首版建议使用：

```text
line_cost(i,j) =
    w_ragged * (max(0, target_width - measured_width(i,j)))^2
  + w_overfull * overflow_penalty(i,j)
  + w_last * last_line_adjustment(i,j)
```

其中最后一行的参差权重应明显降低；过宽一般视为不可行，只有 emergency-break 才允许有限溢出。`transition_cost` 可选用于连续极短行、相邻行长度突变等，但一旦依赖上一行长度，DP 状态必须扩展为 `(previous_break, current_break)` 或其他足够状态，**不能在只保存 `dp[j]` 时错误地使用依赖历史的代价**。

进一步建议：

- 使用规范化宽度 `(W-L)/W` 避免不同字号的代价不可比。
- 不把“所有行一样长”当唯一目标，避免出现明显过短的倒数第二行。
- 最后两行可增加孤行/寡行策略，但先通过语料调参。
- 不对 CJK 断点无差别施加高惩罚；惩罚应依据标点和语言上下文。
- 若所有可行边都被剪枝，必须回退 Standard。

### 6.3 为什么不直接使用经典 Knuth–Plass

TeX 经典模型针对可伸缩 glue 的两端对齐和断词，核心评价包含 stretch/shrink ratio、badness、fitness class、flagged penalties 与 demerits。你的目标主要是**左对齐编辑中的平衡断行**，因此更适合先采用受 Knuth–Plass 启发的全段 DP。若将来提供真正的 Justified 模式，再引入完整 Box/Glue/Penalty 与语言专属 spacing 策略。

## 7. Rust 模块与接口草案

以下为架构接口草案，**不是声称已匹配某个 GPUI 版本的可直接编译 API**。

```rust
pub enum LineBreakMode {
    Standard,
    Balanced,
    JustifiedExperimental,
}

pub struct ParagraphLayoutKey {
    pub source_revision: u64,
    pub projection_revision: u64,
    pub paragraph_id: ParagraphId,
    pub available_width_px: u32, // 量化宽度，防止浮点键抖动
    pub style_revision: u64,
    pub font_revision: u64,
    pub language_revision: u64,
    pub mode: LineBreakMode,
}

pub struct BreakOpportunity {
    pub projection_offset: usize, // 必须是合法 grapheme/cluster 边界
    pub penalty: i32,
    pub mandatory: bool,
    pub emergency_only: bool,
}

pub struct VisualLine {
    pub projection_range: std::ops::Range<usize>,
    pub baseline_y: f32,
    pub measured_width: f32,
    pub visual_runs: Vec<ShapedRunRef>,
    pub caret_stops: Vec<CaretStop>,
}

pub struct ParagraphLayout {
    pub key: ParagraphLayoutKey,
    pub lines: Vec<VisualLine>,
    pub content_height: f32,
    pub fallback_reason: Option<FallbackReason>,
}

pub trait ParagraphMeasurer {
    fn measure_candidate(
        &mut self,
        paragraph: &ProjectedParagraph,
        start: usize,
        end: usize,
        context: &MeasureContext,
    ) -> CandidateMetrics;

    fn shape_final_line(
        &mut self,
        paragraph: &ProjectedParagraph,
        range: std::ops::Range<usize>,
        context: &MeasureContext,
    ) -> ShapedLine;
}
```

生产实现需承载字体回退、字形簇、BiDi、字距、对象尺寸和绘制所需数据；不能仅以 `measure(text) -> f32` 作为最终接口。`available_width` 必须扣除段落缩进、列表/引用前缀、内边距与悬浮控件占用的实际布局宽度。Block 左上角 adornment 通常位于外侧 gutter，**不应无故缩窄段落行宽**。

## 8. 光标、选区与隐藏 Markdown 标记

### 8.1 锚点不依赖视觉行

```text
SourceAnchor + affinity
    → source↔projection mapping
    → projected grapheme / shaping-cluster boundary
    → VisualLine + caret stop
    → GPUI caret rect
```

每次布局变化后从 SourceAnchor 重建 caret geometry；不能将旧 `line_index + x` 作为编辑位置真值。对实体解码（`&amp;`→`&`）、隐藏 `**`、隐藏 `# `、原子图片等非双射区域，必须保留反向 affinity 与可达 caret stop 规则。

### 8.2 Reveal 模式

用户靠近标题前缀、粗体标记等语法边界时，Projection 可能显露标记，导致段落宽度改变。推荐：

- reveal 状态变更使 `projection_revision` 递增；
- 仅重排受影响段落；
- 编辑中优先稳定断行，避免单纯 hover 触发大范围重排；
- 允许将语法标记渲染为不占主排版宽度的局部 overlay，但此方案必须先验证光标命中和选区语义，不能随意混用。

### 8.3 鼠标命中与选择

`VisualLine` 需携带真实 shaping caret stops；鼠标 x/y 先命中行，再命中视觉字形簇，最后反查 SourceAnchor。拖动选择期间使用一致的 layout snapshot；若布局版本改变，应以源锚点重算并维持拖拽语义。

## 9. IME 与实时编辑稳定性

**不推荐“每次按键必然全段重新最优断行”。** 推荐状态机：

| 状态 | 策略 |
|---|---|
| 阅读/未编辑 | Balanced（若开启且符合条件） |
| 普通输入 | 仅重算受影响段落；可先 Standard 后在安全时机切换 Balanced |
| IME composition | 组合文本必须实时正确绘制；优先维持稳定断点或局部 Standard |
| IME commit | 以最终提交文本重新布局该段；恢复正确 source mapping |
| 鼠标拖动选区 | 尽可能固定本次交互的布局快照 |
| 滚动/窗口缩放 | 可见段落优先，按宽度 key 重算 |

不建议通过“无条件冻结所有行”解决 IME，因为组合文本增宽仍需更新几何，且候选窗 rect 必须准确。必要时允许局部行变化，但需抑制非必要的整段跳动。

智能补全（`**` 自动闭合、代码围栏 Enter 补全、`###Hello` 智能修正）应先作为 SourceTransaction 提交，再对新的 Projection 进行布局。**断行器绝不能修改 Markdown 源码。**

## 10. 增量计算与缓存

### 10.1 缓存层级

1. `ProjectionCache`：按 paragraph ID、source/projection revision 缓存可见内容与映射。
2. `BreakOpportunityCache`：按内容、语言/规则版本缓存合法断点。
3. `ShapingCache`：按文本片段、字体特征、fallback、方向与 shaping context 缓存 metrics；不得错误跨不同上下文复用。
4. `ParagraphLayoutCache`：按完整布局 key 缓存断点、视觉行、caret stops。

### 10.2 失效规则

- 当前段文字变化：失效当前段的 projection/break/shaping/layout 相关条目。
- 样式变化：至少失效测量与布局；字体特征变化还需失效 shaping。
- 宽度变化：断点与 layout 失效，但不依赖宽度的 shaping 可复用。
- 字体回退、系统字体、语言、缩放、显示比例变化：按对应 key 失效。
- GFM 引用定义变化：可能跨段改变语义；必须通过 parser dependency tracking 触发相关 Projection/Layout 失效。
- Reveal 状态变化：只失效相关投影段落及其布局。

### 10.3 异步计算

耗时任务可以后台执行，但必须携带 `source_revision + projection_revision + style_revision + width_key`。提交结果时检查版本；过期结果直接丢弃。不要在 GPUI 渲染回调中阻塞等待超长段落全局优化。

## 11. 复杂度、预算与回退

全段 DP 在候选断点数为 `N` 时，朴素最坏情况为 `O(N²)`；若测量每条边时重新 shaping 整个片段，实际成本可能更高。建议通过宽度上界剪枝、候选测量缓存、活动节点上限和可行区间限制控制成本。

**建议初始工程预算（仅作为可调起点，须实测）：**

- 只对可见及附近段落启用 Balanced；其余延迟计算。
- 候选断点超过约 1,000 个、段落超出约 8,000 grapheme 或计算超出约 4 ms 主线程预算时回退 Standard/后台优化。
- 后台布局也设置任务取消与计算预算；防止粘贴超长段落导致队列积压。
- 采用最近编辑段落优先、视口优先的任务调度；不让低优先级平衡断行阻塞键盘输入。
- 所有阈值通过性能基准调整，不作为算法正确性条件。

回退原因枚举建议包含：`TooManyBreakpoints`、`TimeBudgetExceeded`、`UnsupportedComplexRun`、`ShapingMismatch`、`NoFeasibleSolution`、`EmergencyBreakRequired`。

## 12. 视觉质量策略

### 12.1 默认左对齐

左对齐可避免中文字符被任意拉开。Balanced 的目标是减小行尾参差，而不是追求视觉上完全齐边。最后一行允许自然偏短。

### 12.2 中文标点与英文单词

- 中文标点禁则优先于平衡优化；不允许为了更整齐产生错误标点位置。
- 英文长词、URL、代码 span 允许局部参差，不应为平衡整体而拆坏内容。
- 混合字号、加粗、斜体、链接等都必须使用实际 shaping 结果测量。
- 保留源文件硬换行的语义；不跨硬换行合并优化。

### 12.3 段落宽度建议

不应仅靠算法修复过宽的阅读区域。建议将阅读正文宽度设置为可配置，优先保证舒适行长；布局宽度是比断行算法更显著的阅读体验因素之一。

## 13. 测试计划

### 13.1 正确性

- 中英混排、全角/半角标点、中文引号、括号、数字单位、NBSP。
- Emoji ZWJ、区域旗帜、组合附加符号、阿拉伯语/希伯来语 BiDi。
- 超长英文单词、URL、行内代码、不可拆对象。
- GFM 标题、引用、嵌套列表、任务列表、表格、图片、硬换行。
- 源码标记隐藏/显露、HTML entity 显示映射、链接范围。
- 字体回退、连字、不同字号与样式混排。

### 13.2 交互

- 光标左右/上下移动、Home/End、跨视觉行选区。
- CJK IME 组合、候选窗口定位、撤销重做、智能补全。
- 拖动选择、窗口缩放、段落中间插入删除、粘贴大段文字。
- Hover 触发 Block 左上角类型标识、末尾虚拟插入锚点菜单；不能影响段落宽度或改变源码。

### 13.3 性能

使用 Criterion 或等效基准对比 Standard 与 Balanced：

- 100/500/2,000/10,000 grapheme 的段落。
- 纯中文、纯英文、50:50 混排、复杂 Emoji/BiDi。
- 典型窗口宽度及连续 resize。
- 每字符输入延迟 p50/p95/p99、IME composition frame time、布局缓存命中率、每秒布局次数、内存占用。

### 13.4 视觉评估

准备固定字体、宽度和语料的截图基准；比较非法断点数（必须为 0）、行尾参差分布、极短非末行比例、最后两行不平衡、用户主观阅读偏好。不可仅凭一个“整齐度分数”决定是否上线。

## 14. 分阶段实施

### Phase 0：验证 GPUI 文本系统能力

审计实际使用的 GPUI 版本：字体 shaping API、glyph cluster/caret stops、BiDi、行内样式测量、布局与绘制一致性、IME rect。建立小型可运行实验，避免凭想象绑定不存在的 GPUI API。

**验收：** 同一字符串在测量和绘制时的宽度/光标位置一致；多字体、CJK、Emoji、BiDi 用例可复现。

### Phase 1：正确的 StandardLineBreaker

实现 Projection → 断点机会 → shaping → VisualLine，覆盖 UAX #14、CJK 禁则、硬换行、原子对象、SourceMapping 与编辑操作。

**验收：** 正确性测试通过，IME/光标稳定；任何内容都能可靠回退。

### Phase 2：BalancedLineBreaker 原型

在纯文本自然段落上实现 DP，加入 last-line discount、非法断点过滤、宽度缓存与复杂度限制；建立与 Standard 的自动视觉对照。

**验收：** 没有新增非法断点；常见语料的排版质量有可观察改善，且无明显异常短行。

### Phase 3：GPUI 实时编辑集成

引入 paragraph layout key、异步任务版本校验、输入/IME 策略、光标重定位、拖选快照及回退监控。

**验收：** 长段输入和中文 IME 无明显卡顿/跳跃；过期后台结果不覆盖当前布局。

### Phase 4：产品化与调参

提供“标准断行 / 平衡断行”设置；针对中文、英文、混排进行语料调参，决定是否将 Balanced 设为默认。两端对齐单列后续研究，不与首发绑定。

## 15. 风险登记

| 风险 | 影响 | 应对 |
|---|---|---|
| GPUI shaping 与自定义测量不一致 | 行溢出、光标错位 | 统一测量/绘制数据源，最终行校验 |
| 每次按键全段重排 | 输入抖动、延迟 | 输入态 Standard/稳定断点，安全时机优化 |
| Unicode/CJK 禁则不完整 | 错误断行 | UAX #14 + CLReq + 语料测试 |
| 复杂字体上下文改变字宽 | DP 选错断点 | 上下文敏感测量与最终 shaping 校验 |
| BiDi / 复杂脚本 | 命中与 caret 错位 | 逻辑断点、视觉重排、cluster-aware caret |
| 极长段落候选爆炸 | CPU/内存过高 | 预算、剪枝、取消、回退 |
| 隐藏 Markdown 标记触发重排 | 光标跳动 | 投影版本化、局部稳定策略 |
| 表格/列表缩进宽度计算错误 | 溢出/对齐差 | 统一由布局容器提供有效行宽 |

## 16. 最终决策（ADR）

**ADR-001：采用 Hybrid Paragraph Line Breaking。**

- **决定**：实现 `StandardLineBreaker` 与 `BalancedLineBreaker`，共享断点生成、shaping 与 VisualLine 输出；首发 Standard 默认、Balanced 可选。
- **原因**：在提高中英混排质量的同时，保留 Live Editor 对输入稳定性、正确性和性能的控制。
- **不采用**：首发全面替换为 TeX 原始 Knuth–Plass + 默认两端对齐。
- **可逆性**：策略可切换，缓存按 mode 分离，Markdown 源码不受影响。
- **升级条件**：Balanced 通过视觉、IME、p95/p99 输入延迟、内存和大文档测试后可升级为默认。

## 17. 参考规范（实现前应核对当前版本）

- Unicode Standard Annex #14 — Unicode Line Breaking Algorithm：https://unicode.org/reports/tr14/
- Unicode Standard Annex #29 — Unicode Text Segmentation：https://unicode.org/reports/tr29/
- W3C Requirements for Chinese Text Layout (CLReq)：https://www.w3.org/TR/clreq/
- GitHub Flavored Markdown Spec：https://github.github.com/gfm/
- Knuth, D. E. & Plass, M. F. (1981), *Breaking Paragraphs into Lines*, Software: Practice and Experience.

---

**一句话结论：** 把 Knuth–Plass 当作“可选的段落级优化器”，而不是整个文本编辑系统的基础；把 Unicode/CJK 正确性、真实字形测量、SourceMapping 与 IME 稳定性放在算法之前。
