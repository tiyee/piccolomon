# GPUI Live Markdown：Projection、DisplayMap 与 Block Layout 深度实现设计

> 2026-10-12 Hybrid 修订：交互路线与阶段范围以[第十一份](./11_GPUI_Typora_Hybrid_View_Spec.md)为准。Inline 按光标显隐；Block 保持结构化，显式进入局部/全文源码；模式切换不改源码或历史。下文领域 API 为设计草案，须纳入 Hybrid presentation 版本依赖后实施。

> 版本：v1.0 · 技术设计草案  
> 配套文档：[总体架构](./01_GPUI_Live_Markdown_Architecture.md)、[Hybrid View 规格](./11_GPUI_Typora_Hybrid_View_Spec.md)  
> 技术路线：Rust + GPUI + 自研编辑内核  
> 目标：Typora 风格 Hybrid View；Inline 智能显隐、Block 稳定结构化、源码与视觉坐标一致、原生 macOS 输入体验。  
> **注意**：本文 Rust 代码是拟议的领域 API / 算法骨架，不宣称可直接与某个 GPUI 版本编译。所有 GPUI 具体接口必须在锁定 Git commit 后对照源码实现。

## 0. 本文的技术决策

1. **Document 是唯一文本真相**。Rope 保留原始 Markdown；Projection 仅提供派生视图，不可修改源码。
2. **Source Anchor 是唯一持久位置**。鼠标、键盘、选区、IME、Undo 使用源坐标；显示坐标不得长期保存。
3. **Inline Projection 是布局前置阶段**。先投影，再塑形、软换行、Bidi 解析与光标几何计算。
4. **Block 是布局和缓存单位，不是编辑器实例**。全局只有一个 Editor Controller、输入法会话和历史栈。
5. **所有绘制与命中测试绑定同一个版本化 LayoutSnapshot**。不允许新 Projection 与旧 Layout 混用。
6. **隐藏语法的光标位置是有歧义的**，必须用 Affinity + 导航策略显式消歧。
7. **Parser 负责语法事实；InlineVisibilityPolicy 负责行内显隐；BlockPresentationPolicy 负责结构与源码模式；Projection 负责坐标和可见内容；Layout 负责几何。**
8. **为 Widget 留出抽象，但第一版不让 Widget 进入可编辑文本主路径**；避免把语法隐藏与复杂原子对象语义混为一谈。

---

## 1. 从源码到屏幕的完整数据流

```text
                    ┌───────────────────────────────┐
                    │ DocumentSnapshot              │
                    │ Rope + revision + anchors     │
                    └──────────────┬────────────────┘
                                   │
                    ┌──────────────▼────────────────┐
                    │ MarkdownSyntaxSnapshot        │
                    │ Blocks / Inlines / Markers    │
                    └──────────────┬────────────────┘
                                   │
                    ┌──────────────▼────────────────┐
 Selection/IME ───► │ InlineVisibilityPolicy        │
 Hybrid state ────► │ BlockPresentationPolicy       │
                    │ FlowBlocks / spans / decor    │
                    └──────────────┬────────────────┘
                                   │
                    ┌──────────────▼────────────────┐
                    │ ProjectionSnapshot            │
                    │ Segments + text + style runs  │
                    └──────────────┬────────────────┘
                                   │
                    ┌──────────────▼────────────────┐
                    │ BlockLayoutSnapshot           │
                    │ shaping / bidi / soft wrap    │
                    │ glyph runs / caret stops      │
                    └──────────────┬────────────────┘
                                   │
                    ┌──────────────▼────────────────┐
                    │ ViewportLayout                │
                    │ block origins / scroll anchor │
                    └──────────────┬────────────────┘
                                   │
                    ┌──────────────▼────────────────┐
                    │ GPUI Element                  │
                    │ paint / hit test / IME rect   │
                    └───────────────────────────────┘
```

**核心约束**：`LayoutSnapshot` 持有或引用产生它的 `ProjectionSnapshot`，不能仅记录一个可能失效的裸指针。源文本改变时，相关快照应按版本废弃；不应允许一个事件中先读取旧几何、后以新投影解释旧偏移。

---

## 2. 强类型坐标与 Anchor

### 2.1 类型

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceByte(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectedByte(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct VisualLine(pub usize);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Px(pub f32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Affinity { Before, After }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorBias { Left, Right }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceAnchor {
    pub id: u64,
    pub bias: AnchorBias,
}

pub struct Selection {
    pub anchor: SourceAnchor,
    pub head: SourceAnchor,
    pub preferred_x: Option<Px>,
}
```

`SourceAnchor.id` 是概念性句柄，不代表可直接通过整数求位置。Anchor 解析器必须支持编辑事务后的重定位。可采用编辑日志转换、平衡索引或持久化缓冲区锚点；先写明语义，再根据实际性能选择实现。

### 2.2 四种不可混用的位置

| 位置 | 单位 | 用途 |
|---|---|---|
| `SourceByte` | UTF-8 字节 | Rope、AST、事务、文件 |
| `ProjectedByte` | 投影文本 UTF-8 字节 | StyleRun、塑形输入、映射 |
| `VisualLine` + caret stop | 排版行与视觉停靠点 | 上下左右导航、点击定位 |
| 像素点 / 矩形 | 逻辑像素 | 绘制、命中测试、IME 候选框 |

**补充**：GPUI 文本塑形内部可能使用 UTF-16、字符索引或 glyph cluster 等其他单位。平台适配器必须显式转换，不能把 glyph index 当作 byte offset。复杂 Bidi 场景下，一个逻辑文本边界可能对应多个视觉 caret stop。

### 2.3 Anchor 更新规则

插入 `I = [p, p) + text` 时：
- 位置小于 `p`：不变；大于 `p`：后移插入字节数。
- 位置等于 `p`：`Left` 留在插入文本前，`Right` 移到插入文本后。

替换 `[a,b)` 时：
- 外部锚点按 Delta 转换。
- 被替换范围内部锚点按明确的坍缩规则落到替换文本前或后。
- Undo 必须同时恢复文档内容和用户可观察的选区状态。

Anchor 不允许指向 UTF-8 字符中间；用户可见光标还必须位于合法 grapheme / caret stop 上。两者是不同层级的约束。

---

## 3. Markdown Syntax：如何获得精确的标记区间

### 3.1 AST 的职责

Parser 输出完整源码范围、内容范围及每个语法分隔符的范围，而不是只输出“加粗”布尔属性。

```rust
pub struct SyntaxNode {
    pub id: NodeId,
    pub kind: SyntaxKind,
    pub full: std::ops::Range<SourceByte>,
    pub content: std::ops::Range<SourceByte>,
    pub markers: Vec<SyntaxMarker>,
    pub parent: Option<NodeId>,
}

pub struct SyntaxMarker {
    pub source: std::ops::Range<SourceByte>,
    pub role: MarkerRole,
}

pub enum MarkerRole {
    OpenDelimiter,
    CloseDelimiter,
    LinkBracket,
    LinkDestination,
    BlockPrefix,
    CodeFence,
}
```

### 3.2 正确处理的语法

- `**strong**`、`*em*`、`***nested***`、`~~strike~~`。
- 转义：`\\*not italic\\*` 不得错误隐藏。
- 行内代码：`` `**literal**` `` 内的 `**` 不解析为加粗。
- 链接：`[label](url "title")`；目标与可见标签范围分别记录。
- 引用与列表：块级前缀、缩进、连续段落、懒续行。
- 未闭合分隔符：保持原文可见。
- UTF-8：范围从原始文本得到，不通过显示字符串搜索反推。

### 3.3 解析器选型原则

可以使用 `tree-sitter-markdown` 作为增量语法候选，但必须验证具体版本对 CommonMark、GFM 扩展、嵌套 delimiter、错误恢复及精确 marker range 的覆盖。必要时增加**精确标记范围适配器**，而不是假设 AST 天然提供全部范围。`pulldown-cmark` 可作为语义对照，但不能让两个解析器同时成为语法真相。

**验收门槛**：构建 CommonMark/GFM fixture 集合，确认解析器输出的 marker ranges 能驱动 Projection；若关键语法覆盖不合格，替换或扩展解析层，不降低显隐需求。

---

## 4. Hybrid Presentation：行内显隐与块模式

### 4.1 输入输出

```rust
pub struct VisibilityContext<'a> {
    pub selection: &'a SelectionSnapshot,
    pub composition: Option<&'a CompositionSnapshot>,
    pub interaction: InteractionMode,
}

pub struct VisibilityResult {
    pub hidden_markers: Vec<std::ops::Range<SourceByte>>,
    pub revealed_nodes: Vec<NodeId>,
    pub revision: u64,
}
```

`InlineVisibilityPolicy` 只决定可信 Inline marker 的显隐。下文旧接口名 `VisibilityPolicy` 指这部分策略；结构 marker 与源码模式由独立 `BlockPresentationPolicy` 决定，绝不能隐藏普通正文。

### 4.2 推荐规则表

| 情形 | 行为 |
|---|---|
| 折叠光标在普通正文 | 隐藏非活动 Inline 标记 |
| 光标进入 strong/em/code 内容 | 展开该节点对应分隔符 |
| 光标位于标记边界 | 根据 Anchor affinity 和上次导航方向决定激活归属 |
| 光标在嵌套节点内 | 展开当前节点与需要共同解释的祖先节点 |
| 选区与 Inline 交叉 | 展开相交 Inline 及必要 Inline 祖先；拖选期间冻结，结束后重算 |
| IME 正在组合 | 冻结组合影响区域的显隐决策，提交后统一更新 |
| 语法不完整、解析有歧义 | 保留源码可见 |
| 代码围栏内部 | 不执行 Markdown Inline 显隐 |
| Structured Block 正文获得/失去光标 | 外层结构 marker 保持隐藏，仅更新内部 Inline 显隐 |
| EditBlockSource / 全文 Source | 显示作用域内完整源码，纯模式切换不产生源码事务 |
| RawDraft 结构候选确认 | 按类型与输入事件转 Structured，实际换行/结构修改作为事务 |

Block 状态按第十一份管理，不能用 Inline 的选区相交规则决定块前缀显隐。布局前先生成 FlowBlocks，结构装饰与正文 spans 分开表达；表格使用单元格映射，不等于删除分隔符的线性字符串。

### 4.3 防止显隐震荡

**禁止**使用“光标像素坐标是否位于当前渲染的文字框内”来判断激活节点。正确依据是 Source Anchor + AST 的源码范围。否则显示 `**` 后文字重排，命中结果变化，可能反复显示/隐藏。

**点击时序**：

1. 用**旧 LayoutSnapshot** 计算点击对应的 Source Anchor。
2. 提交 Selection 更新。
3. 由 Source Anchor 重新计算 Visibility。
4. 生成新 Projection 和 Layout。
5. 从**同一 Source Anchor** 重新得到光标矩形；不再次用原点击坐标命中测试。

### 4.4 光标边界的精确定义

`**abc**` 中光标在 `a` 前或 `c` 后时，视觉边界可能同时对应隐藏标记与正文位置。Policy 应定义“进入”与“离开”的稳定规则，例如：

- 从左向右进入内容：选择内容侧 `After` affinity，展开节点。
- 从右向左进入内容：选择内容侧 `Before` affinity，展开节点。
- 从外部点击视觉边界：由点击点相对 glyph midpoint、Bidi caret stop 和导航历史决定。
- 如果仍存在二义性，保持上一次激活节点，直到 Source Anchor 明确跨出边界。

这是本项目 Hybrid View 的 Inline 边界规范；可参考 Typora 实机对照，Block 模式以第十一份为准，均不得靠像素重命中实现。

---

## 5. Projection Engine：数据结构与算法

### 5.1 最小可用模型

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentKind { Visible, HiddenSyntax }

#[derive(Clone, Debug)]
pub struct Segment {
    pub source: std::ops::Range<SourceByte>,
    pub display: std::ops::Range<ProjectedByte>,
    pub kind: SegmentKind,
}

pub struct ProjectionSnapshot {
    pub document_revision: u64,
    pub syntax_revision: u64,
    pub visibility_revision: u64,
    pub hybrid_presentation_revision: u64,
    pub source_block: std::ops::Range<SourceByte>,
    pub segments: Vec<Segment>,
    pub projected_text: std::sync::Arc<str>,
    pub style_runs: Vec<StyleRun>,
}
```

首版 `projected_text` 可为 `Arc<str>`；未来大块文本可以换成 chunked view，避免大范围拷贝。语法隐藏与 Widget 的坐标语义不同，**不要**为了“预留未来”让 Widget 进入首版 SegmentKind；后续可扩展为有独立尺寸、源码锚点与编辑策略的 ProjectionAtom。

### 5.2 示例与正确偏移

原文（ASCII，因此字节数等于字符数）：

```text
ab**cd**ef
```

| Segment | Source `[start,end)` | Display `[start,end)` | 类型 |
|---|---|---|---|
| `ab` | `[0,2)` | `[0,2)` | Visible |
| `**` | `[2,4)` | `[2,2)` | HiddenSyntax |
| `cd` | `[4,6)` | `[2,4)` | Visible |
| `**` | `[6,8)` | `[4,4)` | HiddenSyntax |
| `ef` | `[8,10)` | `[4,6)` | Visible |

最终显示：`abcdef`。光标进入 `cd` 后，两个 `**` 都可见，显示恢复 `ab**cd**ef`，但 Rope 从未变化。

### 5.3 构建算法

输入：Block 源码区间 `[B,E)`、按源码顺序排列的语法标记区间、VisibilityResult。

1. 校验所有 marker range 均在 Block 内、处于合法 UTF-8 边界。
2. 将 `hidden_markers` 裁剪到 Block 内并按 `(start,end)` 排序。
3. 合并**重叠**的隐藏范围；相邻范围是否合并由边界语义决定，默认保留独立 marker 身份用于调试。
4. 从 `cursor=B` 扫描：先输出 `[cursor, hidden.start)` 的 Visible Segment，再输出 HiddenSyntax Segment。
5. 扫描末尾输出 `[cursor,E)` Visible Segment。
6. Display 范围通过可见 UTF-8 字节累计得到；隐藏范围 Display 长度为 0。
7. 拼接 projected_text，生成与之对应的 StyleRun。
8. 对 Segment 进行单调性、覆盖性、长度一致性断言。

复杂度：排序 `O(H log H)`，构建 `O(S + B)`（`H` 为隐藏范围数，`S` 为 Segment 数，`B` 为拷贝的可见字节数）。对于局部重算可按 Block 缓存。

### 5.4 映射算法：不能只用一个 offset

```rust
pub struct SourceCaret {
    pub byte: SourceByte,
    pub affinity: Affinity,
}

pub struct ProjectedCaret {
    pub byte: ProjectedByte,
    pub affinity: Affinity,
}

pub trait ProjectionMap {
    fn source_to_display(&self, source: SourceCaret) -> ProjectedCaret;
    fn display_to_source(&self, display: ProjectedCaret) -> SourceCaret;
}
```

- `source_to_display`：二分查找 Source Segment；Visible 内部线性转换；Hidden 内部坍缩到 Display 边界。
- `display_to_source`：二分查找 Display Segment；如果显示位置位于多个 Segment 共享的零宽边界，利用 `Affinity` 决定返回隐藏区间前还是后。
- 多个连续隐藏区间共享一个 Display offset 时，`Affinity` 只能选择**端点**，无法恢复任意内部 SourceByte；这不是 bug，而是不可逆投影的数学性质。
- 因此公开接口应提供 `display_to_source_candidates` 或 `resolve_caret(display, affinity, navigation_context)`，返回合法编辑停靠点，而不是宣称一对一逆映射。

**重要不变量**：

1. Source Segments 连续覆盖整个 Block，不能重叠或留洞。
2. Display Segments 按顺序连续覆盖投影文本；Hidden Segment 为零长度。
3. Visible Segment 的 Source/Display 字节长度相等。
4. 所有 Range 端点均为合法 UTF-8 边界。
5. `projected_text.len() == final_display_end`。
6. 在 Visible Segment 内，映射往返保持恒等；Hidden 内仅保证边界规范化和确定性。
7. 每个 `StyleRun` 均使用 ProjectedByte 坐标且在投影文本范围内。

### 5.5 样式映射

`StyleRun` 不是把源码语法节点范围直接拿来用：它必须先与 Visible Segments 求交集，再映射到 Display 区间。

```rust
pub struct StyleRun {
    pub range: std::ops::Range<ProjectedByte>,
    pub style: TextStyleId,
}
```

嵌套样式按确定的合成顺序处理：base → block style → inline semantic style → interaction decorations → composition。重叠的 strong/em 需合成为同一 run 的属性集合，而不是互相覆盖。链接样式与编辑中可见的分隔符应分开着色。

### 5.6 对隐藏文本的删除和导航语义

隐藏不等于不存在：

- 普通 Left/Right 移动：按**可见 grapheme 停靠点**移动；跨过隐藏 marker 时 Source Anchor 跳过其字节范围。
- 进入语法节点时先展开，再按照新布局继续导航；要确保一次按键只移动一次。
- Backspace/Delete：必须由**源码编辑命令策略**决定删除哪个 grapheme 或 delimiter；不得简单把“投影上一个字符”转换成原文切片。
- 显式进入源码标记后，允许按源码字符删除 `*`；未展开时跨边界删除应有稳定、可预测的语义。
- Copy：默认复制保留选中内容格式的 Markdown 片段，不读取当前屏幕 glyph 或 reveal 后的显示字符串；精确源码切片使用独立 Copy Source。边界规则以第八份 EB-040 为准。

建议将导航和删除定义为 `EditIntent`，由 `EditorController` 根据当前 Projection 与 AST 解析，而不是由 Layout 直接修改 Rope。

---

## 6. Block Layout：从投影文本到字形与几何

### 6.1 核心数据结构

```rust
pub struct BlockLayoutKey {
    pub block_id: BlockId,
    pub document_revision: u64,
    pub projection_revision: u64,
    pub style_revision: u64,
    pub width_key: u32,
    pub font_metrics_revision: u64,
    pub scale_key: u32,
}

pub struct BlockLayoutSnapshot {
    pub key: BlockLayoutKey,
    pub projection: std::sync::Arc<ProjectionSnapshot>,
    pub lines: Vec<VisualLineLayout>,
    pub height: f32,
    pub width: f32,
}

pub struct VisualLineLayout {
    pub projected_range: std::ops::Range<ProjectedByte>,
    pub y: f32,
    pub height: f32,
    pub baseline: f32,
    pub caret_stops: Vec<CaretStop>,
}

pub struct CaretStop {
    pub projected: ProjectedCaret,
    pub x: f32,
    pub bidi_level: u8,
}
```

`width_key` 和 `scale_key` 必须是有定义的量化值，不能把任意 `f32` 直接当作稳定哈希键。BlockLayoutKey 中的 revision 可进一步改为 Block 内容哈希 / 局部 generation，以便文档其他部分编辑时复用未变 Block。

### 6.2 排版步骤

1. 取得 `ProjectionSnapshot.projected_text` 与 projected style runs。
2. 对文本做 grapheme segmentation、脚本识别与 Bidi 分析。
3. 由 GPUI 字形塑形能力（或其底层适配）产生 glyph runs、cluster mapping、font fallback 信息。
4. 根据实际字形宽度和合法换行机会做软换行；不允许按字符数粗略估算。
5. 构建每条 VisualLine 的视觉顺序、caret stops、baseline、selection geometry。
6. 输出 immutable `BlockLayoutSnapshot`。
7. 交给 Viewport Layout 组合 Block 的纵向位置，再由 GPUI 绘制。

**Bidi 细节**：左右箭头通常按视觉停靠点移动；Home/End、Option+Left/Right、Cmd+Left/Right 的 macOS 行为需要单独定义。不要把逻辑字节递增等同于屏幕向右移动。

### 6.3 Block 之间的硬换行

Block SourceRange 应明确定义是否包含行尾换行符。推荐：

- Block 文本范围与 Block 分隔符范围分离。
- 原文 `\n` / `\r\n` 均由 Document 持有，不能在 Projection 中丢失。
- 视觉段落间距是布局属性，不是插入的虚拟换行字符。
- 跨 Block 选区与删除跨过真实分隔符时，由 Document Transaction 处理。

这样不会出现“两个 Block 拼接时换行属于谁”的含混问题。

### 6.4 命中测试与光标矩形

```rust
pub trait BlockGeometry {
    fn hit_test(&self, x: f32, y: f32) -> HitTestResult;
    fn caret_rect(&self, caret: SourceCaret) -> CaretGeometry;
    fn selection_rects(&self, range: SourceRange) -> Vec<Rect>;
}

pub struct HitTestResult {
    pub source: SourceCaret,
    pub visual_line: VisualLine,
    pub visual_affinity: Affinity,
}
```

实现逻辑：

- 点击：`window point → viewport local → block local → visual line → glyph/caret stop → ProjectedCaret → SourceCaret`。
- 光标：`SourceCaret → ProjectedCaret → caret stop → block local rect → window rect`。
- 选区：SourceRange 与各 Block 相交，映射为**可能不连续**的 projected spans，再按 VisualLine 和 Bidi visual runs 输出矩形；不能假设一行选区一定是单个连续矩形。
- 光标附近的隐藏语法边界必须通过导航上下文消歧。

### 6.5 布局缓存和失效矩阵

| 事件 | Syntax | Visibility | Projection | Layout | Paint |
|---|---|---|---|---|---|
| 普通文本输入 | 受影响范围 | 受影响节点 | 受影响 Block | 受影响 Block + 后续纵向偏移 | 受影响视口 |
| 光标在同一节点内移动 | 不变 | 通常不变 | 不变 | 不变 | 光标/选区 |
| 光标跨语法节点 | 不变 | 相关节点变化 | 相关 Block | 相关 Block | 相关区域 |
| 窗口宽度改变 | 不变 | 不变 | 不变 | 可见 Block 重排 | 视口 |
| 字体变化 | 不变 | 不变 | Style 可能变化 | 相关 Block 重排 | 视口 |
| 滚动 | 不变 | 不变 | 不变 | 尽量复用 | 新视口 |
| IME 更新 | 受影响范围 | 冻结/更新 | 受影响 Block | 受影响 Block | 受影响区域 |

对于跨 Block 结构变化（例如删除空行合并段落、代码围栏闭合导致后续多段变为代码块），增量解析可能扩大受影响范围，不能只重排当前 Block。

### 6.6 虚拟化与滚动稳定性

- 为所有 Block 维护高度估计 / 已测量高度索引，支持 `O(log N)` 定位可见 Block（例如 Fenwick tree 或平衡树）。
- 实际绘制只处理视口及 overscan 范围；离屏布局按需计算。
- 布局更新前保存滚动 Source Anchor + 相对屏幕 Y；更新后根据新布局恢复视觉位置，避免上方段落高度变化导致滚动跳跃。
- 显隐只改变某个 Block 高度时，更新高度索引与后续 Block 的累计 Y，不必重新塑形所有后续 Block。

---

## 7. 输入事件与快照一致性

### 7.1 点击一个隐藏的加粗节点

```text
MouseDown(point)
  ├─ read layout_snapshot L0
  ├─ L0.hit_test(point) -> source_anchor A
  ├─ controller.set_selection(A)
  ├─ policy.evaluate(A, AST) -> visibility V1
  ├─ projection.build(V1) -> P1
  ├─ layout.build(P1) -> L1
  ├─ L1.caret_rect(A) -> rect
  └─ repaint
```

**不可执行** `L1.hit_test(point)` 再覆盖 `A`：因为 marker 展开会使视觉文本移动，造成光标跳跃。

### 7.2 输入一个字符

```text
Input(text)
  ├─ resolve source selection from anchors
  ├─ validate source transaction
  ├─ document.apply(replace_range, text) -> revision R+1 + delta
  ├─ transform anchors + selection
  ├─ incremental_parse(delta)
  ├─ policy.evaluate(new selection)
  ├─ rebuild affected projections
  ├─ invalidate affected layouts
  ├─ history.commit_or_coalesce(transaction)
  └─ request frame
```

### 7.3 光标移出节点

```text
MoveCaret(intent)
  ├─ resolve next source anchor using current layout and navigation rules
  ├─ update selection
  ├─ recompute visibility only if active syntax set changed
  ├─ reproject/reflow only changed blocks
  └─ preserve source anchor and viewport anchor
```

### 7.4 防止旧快照混用

所有异步或延迟计算结果都携带 `document_revision`、`syntax_revision`、`visibility_revision`、`hybrid_presentation_revision`、composition 版本和 layout key。提交时校验其依赖仍有效；无效结果丢弃或重新计算。GPUI 事件处理不等于天然线程安全，后台解析与 UI 更新必须通过明确的所有权/消息边界。

---

## 8. IME：macOS 输入法与 Projection 的严格边界

### 8.1 Composition Session

```rust
pub struct CompositionSession {
    pub source_range: SourceRange,
    pub original_text: String,
    pub preedit_text: String,
    pub selection_utf16: std::ops::Range<usize>,
    pub frozen_visibility_revision: u64,
}
```

- 输入法平台层的 UTF-16 范围需要映射到当前组合上下文的 UTF-8 源码范围。
- `marked text` 更新、提交、取消必须有独立状态机；不得把每个预编辑变化作为独立 Undo 步骤。
- 组合过程中冻结受影响 Inline 的显隐或采用稳定的 composition projection；组合结束后重新评估。
- 候选框坐标由**当前组合文本实际绘制的 caret geometry** 返回，不能用组合前的旧位置。
- 输入法可能要求查询上下文文本、选中范围、替换范围；这些必须以 Source Document 为准，配合受控的 UTF-16 映射。

### 8.2 IME 与普通编辑命令的区别

`InsertText`、`SetMarkedText`、`UnmarkText`、`ReplaceRange` 是不同语义事件。不要统一简化成“每次调用 insert(text)”。需要验证 GPUI 锁定版本的 InputHandler 契约及 macOS 平台行为。

### 8.3 必须手动测试

1. 拼音输入“中文测试”，连续选择候选词。
2. 在 `**加粗**` 中间开始组合输入、取消、再提交。
3. 候选框出现在窗口边缘、软换行行首和滚动后的位置。
4. 日文假名/汉字转换、韩文组合、Emoji picker。
5. 删除组合中的字符、撤销提交、跨 Block 选区替换。
6. 窗口缩放、字体 fallback、混合 RTL 文本。

---

## 9. 编辑命令与隐藏语法的行为规范

| 操作 | 建议规范 |
|---|---|
| 左右箭头 | 在当前视觉行按合法 caret stops 移动，投影边界按 Affinity 消歧 |
| 上下箭头 | 保留 `preferred_x`，按目标行 caret stops 选择最近停靠点 |
| Home/End | 遵循 macOS 对视觉行的定义，不直接跳 Block 源码端点 |
| Option+Left/Right | Unicode word boundary + Markdown 节点边界规则 |
| Backspace/Delete | 根据 SourceAnchor 和节点激活状态决定删可见 grapheme 还是语法 delimiter |
| Cmd+B / Cmd+I | Source Transaction 插入/移除 delimiters；选区与 Anchor 一起转换 |
| 复制 | 默认 Markdown 片段；Copy Source 为精确源码切片；Plain Text 为显式辅助命令，均与临时显隐无关 |
| 粘贴 | 普通文本与 Markdown 富格式导入策略明确，禁止无提示丢失原文 |
| 跨 Block 选择 | 单一全局 Source Selection，绘制时按 Block 分割 |
| Undo/Redo | 恢复源码与选区；Projection 变化不进入历史 |

**需要单独设计**“未激活节点边界按 Backspace 时，是否直接删除隐藏分隔符”的交互。不能仅用 `display_to_source` 自动推导删除范围，因为投影不可逆。依第八份编辑契约与第十一份 Hybrid 行为测试实现 `EditIntentResolver`；结构边界由结构命令或源码入口处理。

---

## 10. Parser、Projection 与 Layout 的模块 API

```rust
pub trait MarkdownParser {
    fn parse_full(&mut self, doc: &DocumentSnapshot) -> SyntaxSnapshot;
    fn apply_delta(
        &mut self,
        old: &SyntaxSnapshot,
        doc: &DocumentSnapshot,
        delta: &DocumentDelta,
    ) -> SyntaxSnapshot;
}

pub trait VisibilityPolicy {
    fn evaluate(
        &self,
        syntax: &SyntaxSnapshot,
        context: &VisibilityContext<'_>,
    ) -> VisibilityResult;
}

pub trait BlockPresentationPolicy {
    fn evaluate(
        &self,
        syntax: &SyntaxSnapshot,
        state: &HybridPresentationState,
    ) -> BlockPresentationSnapshot; // FlowBlocks、spans、装饰或源码区域
}

pub trait ProjectionBuilder {
    fn build_block(
        &self,
        doc: &DocumentSnapshot,
        block: &PresentedFlowBlock,
        presentation: &BlockPresentationSnapshot,
        visibility: &VisibilityResult,
        styles: &StyleResolver,
    ) -> ProjectionSnapshot;
}

pub trait TextLayoutBackend {
    fn layout_block(
        &self,
        projection: std::sync::Arc<ProjectionSnapshot>,
        width: f32,
        typography: &Typography,
    ) -> BlockLayoutSnapshot;
}
```

这里的 `DocumentSnapshot`、`SyntaxSnapshot` 等是领域类型。实现时建议使用不可变 `Arc` 快照 + 受控缓存，避免持有跨 GPUI frame 的短生命周期借用。

**不要**让 Parser 依赖 GPUI，也不要让 Layout 决定 Markdown 语法标记是否隐藏。

---

## 11. 测试：属性、不变量、黄金用例

### 11.1 Projection 属性测试

使用 `proptest` 生成：
- 随机合法 UTF-8 文本（含中日韩、emoji、组合附加符）。
- 随机合法、不重叠的 marker ranges（均落在字符边界）。
- 随机 Visibility 激活集合。

验证：

```text
segments cover source exactly
segments ordered and non-overlapping
visible source bytes concatenate to projected_text
all display ranges monotonic
hidden segment display length = 0
visible source/display lengths equal
visible positions roundtrip correctly
boundary normalization deterministic
style runs stay within projected text
no UTF-8 boundary violations
```

### 11.2 黄金用例

| 原文 | 非活动投影 | 激活后 |
|---|---|---|
| `ab**cd**ef` | `abcdef` | `ab**cd**ef` |
| `中**文**字` | `中文字` | `中**文**字` |
| `*斜体*` | `斜体` | `*斜体*` |
| `` `a**b` `` | `a**b`（code 样式） | `` `a**b` `` |
| `[Rust](https://rust-lang.org)` | `Rust`（link 样式） | `[Rust](https://rust-lang.org)` |
| `**未闭合` | `**未闭合` | `**未闭合` |
| `\\*literal\\*` | `\\*literal\\*`（具体反斜杠显示需另定义） | 由语法策略决定 |

**注意**：转义 Markdown 的反斜杠本身是否隐藏属于独立的 Projection 规则，不能因为处理强调标记而顺带误删；首版可先保持转义源码可见。

### 11.3 几何集成测试

- 同一 `SourceCaret` 经 `source→display→layout→caret_rect` 产生有效屏幕矩形。
- 点击该矩形附近，经 `hit_test` 返回语义上相同的合法 Source caret（考虑 affinity 和 Bidi）。
- 展开 marker 后光标仍指向相同 Source Anchor。
- 软换行宽度变化后，所有 caret stops 与 glyph cluster 边界一致。
- 跨 Block 与 RTL 选区绘制矩形覆盖正确的可见字形。

### 11.4 IME、编辑历史与文件回归

- 组合输入预编辑更新不产生多个 Undo 步骤。
- 语法显隐不修改 DocumentRevision、不写入 Undo。
- 任意编辑事务及 Undo/Redo 后，保存的 Markdown 与 Source Buffer 一致。
- 重开文件后语义树与投影可完整重建。

### 11.5 基准测试

分别测量：

1. `source_to_display` / `display_to_source` 在 10、100、1000 个 Segment 下的延迟。
2. 单 Block 投影构建时间、可见文本拷贝量。
3. 光标跨节点显隐时的局部布局时间。
4. 10 KB / 100 KB / 1 MB 文档输入、滚动、打开、保存。
5. 长单行（100 KB）与大量短 Block（10 万段）的退化行为。

先记录基线与测试设备，再确定具体性能门槛；避免凭空承诺“恒定 16 ms”。

---

## 12. 需要刻意解决的难点

### 12.1 Markdown 语法变化的级联失效

一个代码围栏闭合符可能改变后续多个 Block 的语法归属；删除空行可能合并列表或段落。因此 Parser 的 invalidation 必须返回**真实影响范围**，而不是永远只标记编辑点所在 Block。

### 12.2 Nested Markdown 的 Reveal Scope

`***abc***` 可能被解析为嵌套 strong/em。若只展开最内层 delimiter，用户可能无法理解完整源码。应在 Policy 中提供 `RevealScope::{Current, Ancestors, WholeInline}`，并由语法 fixture 决定默认行为。

### 12.3 Bidi 与隐藏标记

RTL 文本中逻辑源顺序与视觉 glyph 顺序不一致。Projection 应先输出逻辑顺序文本，之后由布局层执行 Bidi；不要在 Projection 层自行重排字节。

### 12.4 软换行与尾随空格

Markdown 硬换行可能由两个尾随空格或反斜杠构成。它们既是源码字符又具有排版语义，不能无条件作为普通隐藏 marker；应由 Syntax + Layout 明确表达硬换行约束。

### 12.5 段落内换行与 Block AST

一个 Markdown 段落可能跨多条源码行。Block 的语义范围不等于单个 Rope 行。Layout 需正确处理硬换行、软换行、懒续行及源文件行尾，避免“每行一个 Block”的错误架构。

### 12.6 大文档的稳定 ID

不要用当前数组下标作为 BlockId。增量解析后需要稳定身份或可恢复的内容锚点，以维持布局缓存、滚动锚点和局部状态。跨结构变化时允许重建，但必须有明确失效策略。

### 12.7 文本绘制与 Accessibility

原生可访问性树应反映真实可编辑内容和选区。屏幕阅读器朗读显示文本还是 Markdown 源码，需要定义模式；隐藏语法不应导致文本输入上下文和辅助功能范围互相矛盾。

---

## 13. 实施工作包与退出标准

### WP-1：坐标系统与纯 Rust Projection

**实现**：强类型坐标、Segment 构建、隐藏范围规范化、映射、属性测试。  
**退出标准**：Unicode、零宽边界、多隐藏段、嵌套样式相关映射全部通过。

### WP-2：精确 Markdown Syntax 与 Visibility

**实现**：解析适配、marker ranges、激活规则、嵌套语法、错误恢复。  
**退出标准**：黄金 Markdown fixture 的 Inline 活动/非活动投影、稳定结构与源码模式均符合第八/十一份验收。

### WP-3：GPUI 字形布局适配

**实现**：ProjectedText → glyph runs → soft wrap → caret stops；BlockLayoutSnapshot。  
**退出标准**：CJK、emoji、字体 fallback、长行、Bidi 的 hit test 与 caret geometry 正确。

### WP-4：Editor Controller 与交互

**实现**：Selection、键盘导航、点击、拖选、删除、编辑事务、Undo/Redo。  
**退出标准**：显隐前后 Source Anchor 不漂移；跨 Block 选区、软换行和撤销稳定。

### WP-5：IME 与原生系统行为

**实现**：GPUI 输入桥、组合状态机、UTF-16 映射、候选框、剪贴板、Accessibility。  
**退出标准**：中文/日文/韩文输入与取消/提交/撤销测试通过。

### WP-6：增量性能与虚拟化

**实现**：增量 Parser、局部 Projection、Block layout cache、高度索引、滚动锚点。  
**退出标准**：大文档编辑不全量重排，性能基准稳定且无正确性回归。

---

## 14. 代码评审检查表

- [ ] 所有位置类型都标注单位；没有把 UTF-8 byte 当作字符数。
- [ ] Rope 是唯一可编辑正文；没有第二份可写字符串状态。
- [ ] Syntax marker range 来自解析器精确范围，不是字符串猜测。
- [ ] Projection 只隐藏确认的语法标记，不吞普通内容。
- [ ] 映射处理零宽区间与 Affinity；不宣称任意隐藏位置可逆。
- [ ] StyleRun 全部是 ProjectedByte 范围。
- [ ] Layout 使用真实塑形后的 glyph clusters 与 Bidi caret stops。
- [ ] 点击命中只用旧快照一次；重排后从 Source Anchor 重新求光标矩形。
- [ ] IME 组合文本与 UTF-16 替换范围有明确状态机。
- [ ] Undo 只记录源码编辑，不记录语法显隐。
- [ ] Block 高度变化不触发不必要的全篇塑形。
- [ ] 所有后台结果提交前检查 revision / snapshot key。
- [ ] 每个重要不变量有单元测试、属性测试或集成测试覆盖。

---

## 15. 需要在编码前明确的开放设计问题

这些是**必须作出明确决策的产品/平台语义**，不是降低架构标准：

1. 未激活加粗节点边界按 Backspace，优先删除可见文字还是展开 delimiter？
2. 点击隐藏标记对应的零宽位置，光标落在标记前、标记后，还是由历史 affinity 决定？
3. **已确认**：默认复制 Markdown 片段，`text/plain` 载荷也放 Markdown 字符串；支持时同时提供等价 `text/markdown`。详见第八份 EB-040—043。
4. 链接展开范围：只显示 `[]()`，还是完整 URL 和 title？
5. 多层嵌套语法：展开当前节点、祖先链还是整组 Inline？
6. **已确认**：Structured 正文编辑保持标题、列表、引用前缀隐藏，语法修改走块源码入口；类型触发见第十一份。
7. macOS 键盘导航采用哪些系统约定，尤其是 Bidi、Option、Cmd 与 Home/End？
8. 屏幕阅读器默认读取可见文本还是 Markdown 原文，是否提供模式切换？

其余开放项写成独立行为测试，参考 Typora 交互校准；第八份与第十一份共同作为验收依据。

---

## 16. 最终结论

真正的 Live Markdown 编辑器不是“Markdown 解析器 + 文本框 + 样式”，而是一个拥有统一 Source Anchor、不可逆 Projection、字形级 Layout、输入法状态机和版本化快照的原生编辑系统。

**正确的长期架构是：**

```text
Document (single Rope)
  → Incremental Syntax
  → Cursor-aware Visibility
  → Projection / DisplayMap
  → Text Shaping + Bidi + Soft Wrap
  → Block Layout Snapshots
  → GPUI Paint / Hit Test / IME Geometry
```

只要这些边界从第一天建立正确，未来加入图片、任务复选框、数学公式、表格、嵌入 Widget 或更复杂的 Markdown 扩展，都可以通过扩展 ProjectionAtom、布局和编辑语义来实现，而无需推翻文档模型与坐标系统。
