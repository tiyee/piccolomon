# GPUI Live Markdown — 第五份：原生渲染、视口虚拟化、命中测试、滚动锚定与 Accessibility

> 2026-10-12 Hybrid 修订：交互路线与阶段范围以[第十一份](./11_GPUI_Typora_Hybrid_View_Spec.md)为准。Inline 按光标显隐；Block 保持结构化，显式进入局部/全文源码；模式切换不改源码或历史。下文领域 API 为设计草案，须纳入 Hybrid presentation 版本依赖后实施。

> **文档版本**：v0.1，架构与实现规格  
> **项目**：macOS 原生、单文件 Typora 风格 Hybrid View Markdown 编辑器  
> **技术方向**：Rust + GPUI；单一 Rope；自研 Editor Engine / Syntax / Projection / Layout  
> **关联文档**：第一份总体架构；第二份 Projection & Layout；第三份 Document / Transaction / IME；第四份 Markdown Syntax Engine  
> **说明**：本文所有 `struct`、`trait` 和代码段均为**架构草案**，不宣称是当前 GPUI 可直接编译的 API。应在锁定 GPUI Git commit 后完成接口验证与技术探针。  

---

## 0. 设计目标和验收定义

本篇专门定义**编辑器如何把一致的布局快照变成原生可交互像素**。

目标不仅是“画出文本”，而是保证以下操作都来自同一个几何事实来源：

- 文字、粗体/斜体/代码样式、Markdown 标记显隐。
- 单/多光标、选择高亮、软换行、Bidi。
- 鼠标点击、拖选、双击、三击、键盘上下移动。
- IME marked text、候选窗口矩形。
- 长文档视口虚拟化与稳定滚动。
- macOS 辅助功能中的文本、选区、范围和可见区域。
- 窗口缩放、字体切换、DPI/Retina、滚动惯性。
- 投影变化导致的局部重排，不允许光标跳跃或滚动位置突变。

**不可妥协原则**：

1. **一个 LayoutSnapshot 是一个几何真相**：Paint、HitTest、CaretRect、SelectionGeometry、IME、Accessibility 只能读取匹配版本的布局。
2. **Source Anchor 是交互真相**：任何屏幕像素位置最终转换为 Source Anchor + Affinity，不能把 PixelPoint 作为持久光标。
3. **Projection 在 Shaping 之前**：隐藏 `**` 必须从参与塑形的显示文本中消失，不是透明绘制。
4. **Block 虚拟化不等于 Block 独立编辑**：全篇仍只有一个 Document 和 SelectionSet。
5. **完整 Unicode**：grapheme、ligature、emoji ZWJ、RTL/Bidi 的 caret stops 由布局引擎与源映射共同决定。
6. **可访问性是核心接口**：不是完成视觉 UI 后再附加几个标签。
7. **缓存失效有可验证依赖图**：不以“全部重画就好了”作为常态方案。
8. **正确性优先于缓存命中**：失配版本应重新布局、推迟事件或进入安全呈现，绝不以旧几何编辑新源码。

---

## 1. 系统边界与依赖方向

```text
DocumentSnapshot (committed Rope)
          │
          ▼
SyntaxSnapshot + HybridPresentationState + Inline/Block policies + IME Overlay
          │
          ▼
ProjectionSnapshot (display text, mapping, style intents)
          │
          ▼
Layout Scheduler
  ├─ Block measurements
  ├─ Shaped text runs
  ├─ Visual lines / caret stops
  ├─ Hit-test index
  └─ Block height index
          │
          ▼
LayoutSnapshot (immutable, versioned)
          │
          ├────────► Native Paint
          ├────────► Pointer Hit Testing
          ├────────► Keyboard Navigation Geometry
          ├────────► IME Candidate Rect
          ├────────► Accessibility Text Geometry
          └────────► Scroll Anchor Reconciliation
```

`editor-gpui` 不修改 Rope，不重新解析 Markdown，不自行推断语法范围。

### 1.1 包与文件建议

```text
crates/editor-layout/src/
  key.rs
  block.rs
  inline.rs
  shaping.rs
  bidi.rs
  wrap.rs
  caret.rs
  hit_test.rs
  selection_geometry.rs
  height_index.rs
  viewport.rs
  scroll_anchor.rs
  cache.rs
  scheduler.rs

crates/editor-gpui/src/
  editor_view.rs
  editor_element.rs
  paint.rs
  layers.rs
  pointer.rs
  keyboard.rs
  ime_geometry.rs
  accessibility.rs
  platform_adapter.rs
  focus.rs
  diagnostics.rs

crates/editor-render-tests/
  golden/
  integration/
  benchmarks/
```

---

## 2. 坐标空间：从 Source 到屏幕

不要混用不同层级的“位置”。

| 类型 | 语义 | 单位 |
|---|---|---|
| `SourceByte` | Rope 原文位置 | UTF-8 byte |
| `DisplayByte` | Projection 显示文本位置 | UTF-8 byte |
| `GraphemeBoundary` | 合法用户导航边界 | 字素边界 |
| `VisualCaretStop` | 塑形后的视觉光标停靠点 | 带方向和亲和性 |
| `BlockPoint` | Block 内布局坐标 | 逻辑像素 |
| `DocumentPoint` | 全文排版坐标 | 逻辑像素 |
| `ViewportPoint` | 编辑器滚动视口坐标 | 逻辑像素 |
| `WindowPoint` | GPUI 窗口坐标 | 逻辑像素 |
| `ScreenRect` | 系统候选框/辅助功能所需坐标 | 平台定义 |

**显式转换链**：

```text
Source Anchor + Bias
   ⇅ Projection Map
Display Offset + Visual Affinity
   ⇅ Shaped Line Caret Map
BlockPoint
   ⇅ Block Origin + Height Index
DocumentPoint
   ⇅ Scroll Offset
ViewportPoint
   ⇅ Editor Frame / Window Transform
WindowPoint
   ⇅ Platform Screen Conversion
ScreenPoint
```

### 2.1 坐标强类型

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutPx(pub f32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockPoint { pub x: LayoutPx, pub y: LayoutPx }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DocumentPoint { pub x: LayoutPx, pub y: LayoutPx }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportPoint { pub x: LayoutPx, pub y: LayoutPx }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowPoint { pub x: LayoutPx, pub y: LayoutPx }
```

建议由 `CoordinateMapper` 集中维护编辑器内边距、滚动位置、缩放比例和窗口变换。**不要在鼠标事件、绘制和 IME 回调里分别手写减法**。

### 2.2 像素与缩放

- Layout 使用设备无关的逻辑坐标。
- 绘制时由平台统一换算设备像素。
- Caret 与细线需考虑像素对齐，但**不允许像素对齐反向修改源码定位**。
- Retina 缩放改变可影响栅格化缓存；字体/可用宽度改变会影响塑形或软换行缓存。
- 系统文字缩放与窗口缩放必须通过 `TypographyRevision` / `ViewportRevision` 驱动正确失效。

---

## 3. GPUI 自定义 Element 的职责

建议一个**文档级 Editor Element**，内部可有多个绘制层，但**不是每个 Block 一个独立 TextInput**。

### 3.1 职责划分

**Editor View / Controller**

- 保存焦点状态、SelectionSet、EditorEngine 句柄。
- 接收事件，分发命令。
- 协调滚动与失效。
- 请求新的 LayoutSnapshot。
- 管理 Accessibility 与 IME bridge。

**Editor Element**

- 接收已解析的不可变 `RenderSnapshot`。
- 参与 GPUI 的测量、布局、绘制。
- 注册指针交互命中区域。
- 绘制文字、背景、装饰、光标。
- 不直接修改 Document。

**Text Shaping Adapter**

- 把 Display text + StyleRun 交给 GPUI 的文字塑形能力。
- 取得 glyph run、line break、advance、caret positions 等**经版本核验确实可获得**的数据。
- 如果 GPUI 当前公开 API 不暴露足够的 caret/Bidi 几何，应设计受控适配层或 fork GPUI 对应内部能力，而不是自行按平均字宽猜测。

### 3.2 GPUI API 真实性边界

锁定版本前，不能断言以下具体签名存在：

- `Element::request_layout` / `prepaint` / `paint` 的确切参数。
- 文本塑形接口与行布局缓存的公开程度。
- 输入 handler 与焦点生命周期。
- accessibility 节点注册和 range bounds。
- 坐标从窗口到屏幕的转换 API。

**工程策略**：以独立 `gpui-compat` 模块隔离这些变化。外层使用稳定的项目内 `NativeTextShaper` / `NativeElementBridge` / `NativeAccessibilityBridge` trait。

---

## 4. RenderSnapshot：绘制层只读数据

```rust
pub struct RenderSnapshot {
    pub key: RenderKey,
    pub viewport: ViewportState,
    pub visible_blocks: Vec<Arc<BlockLayout>>,
    pub decorations: Vec<Decoration>,
    pub selection_geometry: SelectionGeometry,
    pub carets: Vec<CaretGeometry>,
    pub composition: Option<CompositionGeometry>,
}

pub struct RenderKey {
    pub document_revision: DocumentRevision,
    pub syntax_revision: u64,
    pub presentation_revision: u64,
    pub hybrid_presentation_revision: u64,
    pub composition_revision: u64,
    pub typography_revision: u64,
    pub viewport_width_revision: u64,
    pub layout_generation: u64,
}
```

同一帧的文本、选区和光标使用同一个 `RenderKey`。若布局变更但光标几何尚未计算，不能先画新文本再画旧光标。

### 4.1 三层快照

1. `ProjectionSnapshot`：可见字符串与 Source/Display 映射。
2. `LayoutSnapshot`：形状、视觉行、caret stop、Block 高度。
3. `RenderSnapshot`：可见 Block、选择几何、光标与绘制装饰。

三个快照的版本依赖必须可追溯；不要通过隐式全局变量拼装。

---

## 5. BlockLayout：单个 Block 的完整几何

```rust
pub struct BlockLayout {
    pub block_id: BlockId,
    pub source_range: SourceRange,
    pub projection_key: ProjectionKey,
    pub width: LayoutPx,
    pub height: LayoutPx,
    pub lines: Vec<VisualLine>,
    pub decorations: Vec<BlockDecoration>,
    pub hit_test_index: HitTestIndex,
}

pub struct VisualLine {
    pub display_range: Range<DisplayByte>,
    pub baseline: LayoutPx,
    pub top: LayoutPx,
    pub bottom: LayoutPx,
    pub fragments: Vec<ShapedFragment>,
    pub caret_stops: Vec<CaretStop>,
}
```

### 5.1 Fragment

一个视觉行可以包含多个样式片段、方向片段和 Widget：

```rust
pub enum LayoutFragment {
    Text(ShapedTextFragment),
    InlineWidget(WidgetFragment),
    Decoration(DecorationFragment),
}
```

Fragment 的**可选择性**与**占据的几何空间**必须分开描述。列表圆点可能占空间但不对应独立可编辑显示文本；Inline Widget 可以占空间且映射到一段源码。

### 5.2 Hybrid 结构布局

BlockPresentationPolicy 先输出 FlowBlocks、内容 spans、结构装饰或 BlockSource 文本，再进入统一 LayoutSnapshot。Structured 标题、列表、引用、代码块不随正文光标展开 marker，局部源码切换使相关单元布局失效。任务复选框命中触发源码事务；H2 表格使用 cell graph 和各 cell 的 source map，不能通过线性 elision 推导网格导航。单元格仅是布局单元，继续共享全文 Selection、IME、Undo 与 AX。详见[第十一份](./11_GPUI_Typora_Hybrid_View_Spec.md)。

### 5.3 样式与字体

- 同一 Block 内 `Strong` / `Emphasis` / `CodeSpan` 样式由语义范围派生。
- 按用户选定的设计约束，**各 Block 维持相同基础字号与行高**；标题可以使用颜色、字重、间距、装饰来表达层级，不能暗中放大字体导致全局行高变化。
- 粗体可能改变字形宽度，必须重新塑形。
- 行高不等于所有字体 glyph 的 ascent/descent 相同；行盒需要容纳字体 fallback 的最大必要度量，避免 emoji 被裁剪。
- 样式变化影响 shaping 时必须使相应 Block Layout 失效。

---

## 6. Text Shaping 与 Bidi

### 6.1 塑形管线

```text
Projected Display Text
  → Unicode grapheme segmentation
  → paragraph bidi resolution
  → style/font fallback runs
  → shaping (ligatures, kerning, clusters)
  → line breaking / soft wrap
  → visual fragment ordering
  → caret stop extraction
```

具体先后步骤要遵循所选 shaping API 的能力：有些系统将 Bidi、font fallback、line breaking 组合处理。**不可人为假定一个 UTF-8 字符对应一个 glyph**。

### 6.2 复杂文本案例

- `fi` 可能发生 ligature。
- `e` + combining acute 是一个 grapheme。
- 家庭 emoji 是多个 code point 的 ZWJ 序列。
- 阿拉伯文 shaping 随上下文改变 glyph。
- RTL 与 LTR 混排会让相同逻辑边界存在不同视觉亲和性。
- CJK 字体 fallback 影响 ascent、宽度和软换行。

### 6.3 Caret Stops

```rust
pub struct CaretStop {
    pub display_offset: DisplayByte,
    pub x: LayoutPx,
    pub affinity: VisualAffinity,
    pub bidi_level: u8,
    pub cluster_id: u32,
}
```

**不能**通过 `x = char_index * average_advance` 推算。需要从实际 shaped cluster / glyph geometry 构建。

若某字体 ligature 内存在多个合法 grapheme 边界，应确认 shaping API 是否支持 ligature 内 caret 位置；如不支持，应采用经验证的 fallback，不能把所有逻辑位置强行映射到同一个 x 而不处理导航歧义。

### 6.4 Bidi 的双重位置

视觉边界可能对应两种 Affinity。布局必须保存：

```text
(DisplayOffset, Affinity) -> (VisualLine, X)
```

而不是：

```text
DisplayOffset -> X
```

否则左右移动、跨行、选区高亮会在 RTL/LTR 交界处跳动。

---

## 7. 软换行：必须由塑形后的宽度驱动

### 7.1 Wrap Inputs

```rust
pub struct WrapConfig {
    pub available_width: LayoutPx,
    pub first_line_indent: LayoutPx,
    pub continuation_indent: LayoutPx,
    pub tab_width: u8,
    pub break_policy: BreakPolicy,
}
```

列表、引用、代码块需要不同的 continuation indent 与 break policy。

### 7.2 不允许先按字节断行

错误做法：

```text
每 80 个字符换一行
```

正确做法：

```text
实际 shaped advances
 + Unicode line-break opportunities
 + Block indentation
 + widget widths
 → VisualLines
```

### 7.3 投影变化引起重新换行

例：

```markdown
abc **long text** def
```

光标离开时 `**` 隐藏；进入时标记展开。即使基础字体与行高不变，标记的额外宽度也可能把内容推到下一行。

必须：

1. 保持 Source Anchor。
2. 生成新的 ProjectionSnapshot。
3. 对受影响 Block 重新塑形/换行。
4. 从新布局查 Source Anchor 对应的 caret stop。
5. 根据滚动锚点调整 viewport。
6. 一次性发布新的 RenderSnapshot。

**不能**为了视觉稳定而给隐藏标记预留不可见宽度；那会破坏真正的 WYSIWYG 排版。

---

## 8. Hit Testing：鼠标像素到 Source Anchor

### 8.1 流程

```text
Pointer(WindowPoint)
   → EditorFrame to ViewportPoint
   → + scroll offset to DocumentPoint
   → HeightIndex find Block
   → BlockLayout find VisualLine
   → nearest shaped caret stop
   → DisplayOffset + Affinity
   → Projection source mapping
   → Source Anchor + Bias
```

### 8.2 输出类型

```rust
pub struct HitTestResult {
    pub source_anchor: SourceAnchor,
    pub visual_affinity: VisualAffinity,
    pub block_id: BlockId,
    pub line_index: usize,
    pub snapshot_key: LayoutSnapshotKey,
}
```

命中测试结果必须携带布局版本。若事件分发前 Document 已变更，不能直接套用旧 Anchor offset。

### 8.3 隐藏语法造成的歧义

源码：

```markdown
ab**cd**ef
```

显示：

```text
abcdef
```

`b|c` 视觉边界可能映射到 `**` 左侧或右侧。Projection 需要定义：

- 点击内容起始侧优先进入语义内容。
- 需要编辑标记时可通过方向键或语法展开到达。
- 相同 DisplayOffset 的多个 SourceOffset 由 Affinity 和点击方向消歧。
- 每个命中点必须最终对应**合法 Source grapheme boundary**。

### 8.4 Inline 点击后显隐的时序

```text
hit_test(old LayoutSnapshot) → SourceAnchor
set_selection(SourceAnchor)
update Visibility
rebuild Projection/Layout
paint caret(new LayoutSnapshot, same SourceAnchor)
```

**不做第二次 hit_test**。这是防止“点击后光标瞬移”的关键规则。

### 8.5 多击与拖选

- 双击：使用 Unicode word boundary 和 Source 语义。
- 三击：按编辑器行为规范选择逻辑行或段落。
- 拖选：固定初始 Source Anchor，指针每次更新只改变活动端。
- 自动滚动：指针超出 viewport 时触发可控滚动，随后以新的 viewport transform 命中测试。
- 拖选期间 Projection 变化应保持选区两端 Source Anchor，不重置选择方向。
- 长按与触控板手势由平台适配层翻译成引擎事件。

---

## 9. Caret Geometry 与绘制

### 9.1 统一 CaretGeometry

```rust
pub struct CaretGeometry {
    pub source_anchor: SourceAnchor,
    pub rect: LayoutRect,
    pub affinity: VisualAffinity,
    pub visual_line: VisualLineId,
    pub snapshot_key: LayoutSnapshotKey,
}
```

Caret 的 x 来自 shaped caret stop，y 来自对应视觉行 line box。

### 9.2 Caret 样式

- 插入态细竖线。
- 焦点丢失时按平台规范隐藏或弱化。
- 闪烁由 UI 动画调度，不改变 DocumentRevision。
- 多光标时每个 caret 独立几何，闪烁时序可统一。
- IME composition 中的 caret 与 marked text selection 需遵循平台反馈。
- 光标不能在错误版本的文本上绘制。

### 9.3 选择高亮

选区可能跨越：

- 多个 Block。
- 一个 Block 的多条 VisualLine。
- Bidi 混排造成的多个不连续视觉片段。
- 隐藏 Markdown 语法。
- Inline Widget。

因此不能用“一个起点 rect 到一个终点 rect”画整个选择。

```rust
pub struct SelectionGeometry {
    pub fragments: Vec<SelectionRect>,
    pub snapshot_key: LayoutSnapshotKey,
}
```

每个视觉行按选中 source 范围映射出若干显示片段，再根据 shaped fragments 计算矩形。

### 9.4 Selection Paint 与 Source Selection 的区别

Selection 是源码语义范围；Paint 只画可见部分。默认复制 Markdown 由语义序列化器保留选中内容的格式：既处理选区内的隐藏 delimiter，也可在输出片段中补齐选区外的成对标记。复制结果不依赖 Paint 或临时显隐；Copy Source 才是严格源码切片，详见第八份 EB-040。

---

## 10. Paint Pipeline：层级与裁剪

推荐绘制顺序：

```text
0. editor background
1. block backgrounds / code block surfaces
2. selection highlights
3. structural decorations (quote bars, list bullets)
4. shaped text glyphs
5. link underline / diagnostic decorations
6. IME marked text underline / composition highlights
7. caret(s)
8. floating overlays (hover toolbar, popover, etc.)
```

实际层级可因特定交互调整，但应明确每层的 z-order。

### 10.1 视口裁剪

- Block 绘制按 viewport clip 裁剪。
- 超出可见区域的重型 glyph 绘制不执行。
- SelectionGeometry 只生成可见 Block 的矩形。
- Code block 背景、引用线可按 Block rect 绘制。
- 不允许“每帧遍历全部文档绘制”作为正式实现。

### 10.2 Decoration 与文本解耦

```rust
pub enum Decoration {
    QuoteBar(QuoteBarGeometry),
    ListMarker(ListMarkerGeometry),
    CodeBackground(CodeBackgroundGeometry),
    LinkUnderline(UnderlineGeometry),
    SearchMatch(HighlightGeometry),
    DiagnosticSquiggle(SquiggleGeometry),
}
```

Decoration 是派生绘制对象，不应成为源码第二份所有者。

### 10.3 绘制缓存

区分三种缓存：

1. **Shaping cache**：字形/cluster/line layout，昂贵。
2. **Geometry cache**：selection rect、caret rect、decorations。
3. **Raster/display-list cache**：具体 GPU 绘制指令或渲染资源。

不能仅因 caret blink 就重新塑形文本。Caret blink 只使最小覆盖区域重绘。

---

## 11. Viewport Virtualization：长文档不渲染全部 Block

### 11.1 基本模型

全文可能包含数万 Block，但任意时刻仅布局并绘制视口附近的 Block。

```rust
pub struct ViewportState {
    pub scroll_y: LayoutPx,
    pub width: LayoutPx,
    pub height: LayoutPx,
    pub overscan_before: LayoutPx,
    pub overscan_after: LayoutPx,
}
```

可见范围：

```text
[scroll_y - overscan_before,
 scroll_y + viewport_height + overscan_after]
```

### 11.2 为什么需要 HeightIndex

若只知道可见 Block 的高度，无法快速把 `scroll_y` 映射到全文位置。

建议使用 Fenwick Tree、Segment Tree 或带前缀和的平衡树维护 Block 高度：

```rust
pub trait BlockHeightIndex {
    fn total_height(&self) -> LayoutPx;
    fn block_at_y(&self, y: LayoutPx) -> BlockIndex;
    fn prefix_height(&self, block: BlockIndex) -> LayoutPx;
    fn update_height(&mut self, block: BlockIndex, new_height: LayoutPx);
}
```

目标复杂度：

- `block_at_y`: O(log N)
- `prefix_height`: O(log N)
- `update_height`: O(log N)

### 11.3 未布局 Block 的高度估计

虚拟化意味着很多 Block 尚未真实布局。HeightIndex 必须支持：

```rust
pub enum HeightKnowledge {
    Measured(LayoutPx),
    Estimated { value: LayoutPx, confidence: f32 },
}
```

估计可以来自：

- 上次同宽度下的实测高度。
- 行数与基本行高。
- Block 类型及文本长度。
- 已知代码块/标题/列表的布局特征。

**高度估计不是最终几何**：当 Block 进入视口完成真实测量后，应更新 HeightIndex，并通过 Scroll Anchor 消除跳动。

### 11.4 预布局策略

- 当前光标所在 Block 优先。
- 当前可见 Block 次优先。
- 滚动方向前方 overscan 再次。
- 搜索跳转目标 Block 可高优先级异步布局。
- 后台预布局不得阻塞输入事件。
- 过期任务结果必须验证 Projection/Width/Typography key。

### 11.5 单个超长 Block

一篇 Markdown 可能只有一个超长段落或一个超大代码围栏。

**Block 级虚拟化不足以覆盖这种情况。**

因此 Layout 层还需要**Block 内 VisualLine 虚拟化**：

- 可按文本 chunk 或 shaped line batch 建索引。
- 对长段落按局部换行检查点复用结果。
- 代码块按可见行区间布局与绘制。
- 命中测试需要能按需布局目标局部。
- 对极端长行可横向滚动或使用安全分段塑形，但必须保证 cluster 边界与 Bidi 正确。

不能把“单 Block 全量 shaping”视为百万字文档的通用解。

---

## 12. Scroll Anchoring：重排时保持视觉稳定

### 12.1 问题

当前视口顶部是第 100 个 Block。若上方某个 Block 因 Markdown 显隐、窗口宽度变化或延迟测量增高 20 px，简单维持 `scroll_y` 会让用户看到的内容下移/上移。

### 12.2 锚点模型

```rust
pub struct ScrollAnchor {
    pub source_anchor: SourceAnchor,
    pub affinity: VisualAffinity,
    pub desired_viewport_y: LayoutPx,
    pub policy: ScrollAnchorPolicy,
}

pub enum ScrollAnchorPolicy {
    KeepTopVisibleContent,
    KeepCaretVisible,
    KeepSelectionHeadVisible,
    ExplicitScrollPosition,
}
```

### 12.3 重排补偿

```text
Before relayout:
  anchor_source = first stable visible SourceAnchor
  old_viewport_y = caret_or_line_y(anchor_source) - scroll_y

After relayout:
  new_document_y = caret_or_line_y(anchor_source)
  new_scroll_y = new_document_y - old_viewport_y
```

然后 clamp 到有效滚动范围。

### 12.4 策略优先级

- 用户主动输入：优先让 caret 可见。
- 用户主动滚动：优先保留视口内容锚点，不要强制跳回 caret。
- 窗口 resize：保留视口顶部语义内容或当前阅读位置。
- 搜索跳转：以目标命中范围为中心。
- 高度估计修正：补偿上方高度变化。
- 显隐语法：优先保留编辑 caret 的 Source Anchor；必要时调整 scroll，不能通过旧像素重新定位 caret。

### 12.5 Scroll Anchor 失效

若锚点对应源码被删除：

1. 通过 Anchor Bias 折叠到最近合法源码位置。
2. 查询该位置的新 Block 与视觉行。
3. 若目标不再可见，选择最近可见内容。
4. 仍不能定位时回退到相对 scroll ratio，但必须明确这是最后兜底。

---

## 13. 增量失效与缓存键

### 13.1 失效维度

| 事件 | Syntax | Projection | Shaping | HeightIndex | Paint |
|---|---|---|---|---|---|
| 普通输入 | 受影响范围 | 受影响范围 | 受影响 Block | 高度变化时 | 是 |
| 光标进入 Strong | 否 | 活动 Inline | 相关 Block | 可能 | 是 |
| 仅 Caret Blink | 否 | 否 | 否 | 否 | Caret 区 |
| 选区移动 | 否 | 视显隐策略 | 视投影变化 | 可能 | 是 |
| 字体变化 | 否 | 样式可重用 | 全部布局失效 | 重新估计 | 是 |
| 视口宽度变化 | 否 | 通常可重用 | Wrap/Layout 失效 | 重新估计 | 是 |
| 仅垂直滚动 | 否 | 否 | 新进入视口部分 | 否 | 是 |
| 主题颜色变化 | 否 | 通常否 | 颜色不影响 shaping | 否 | 是 |
| IME 更新 | 否（committed） | Overlay 区 | 相关 Block | 可能 | 是 |

### 13.2 CacheKey

```rust
pub struct BlockLayoutCacheKey {
    pub block_id: BlockId,
    pub source_revision_or_fingerprint: u128,
    pub projection_revision: u64,
    pub block_presentation_revision: u64,
    pub composition_revision: u64,
    pub typography_revision: u64,
    pub available_width_bits: u32,
    pub wrap_policy_revision: u64,
}
```

**不要只用 DocumentRevision 作为全局缓存键**：那会让一次输入导致全篇 Block 缓存全部失效。应允许未受影响 Block 复用，同时通过 `SyntaxDelta` / `ProjectionDelta` 证明其依赖未变。

### 13.3 缓存一致性

- 缓存对象不可变。
- 键完整覆盖布局依赖。
- 旧结果只能在依赖等价时复用。
- 不允许靠“很可能没变”来忽略样式、字体或投影差异。
- 异步任务必须携带输入快照键。

---

## 14. 调度与主线程预算

### 14.1 两类工作

**需要同步完成的最小工作**：

- 当前输入事务与 Source Selection。
- 当前 caret 周围足以保证正确几何的布局。
- 当前帧的可见 RenderSnapshot。

**可异步/分批执行的工作**：

- 远离视口的 Block 解析、投影与布局。
- 高度估计精化。
- 远距离搜索跳转目标预布局。
- 不影响当前帧的缓存预热。

### 14.2 防止主线程卡顿

- 解析/布局工作有可取消任务令牌。
- 不在每次键入后同步全量排版整篇文档。
- 当前 Block 布局超预算时，可以保持旧帧并尽快生成一致的新帧；不能显示新文字配旧选区。
- 输入事件队列不能被重型渲染锁长时间阻塞。
- 重绘请求合并，同一帧多个状态更新只发布最终一致快照。

### 14.3 不要盲目多线程 GPUI 对象

GPUI 平台上下文、字体系统或 GPU 资源可能具有线程亲和性。可异步的应是**纯数据预处理**；涉及 GPUI 实际对象和平台绘制的部分，必须遵循锁定版本的线程约束。

---

## 15. macOS IME 候选框几何

### 15.1 单一几何来源

```text
Composition overlay caret
  → projection-aware display caret
  → shaped VisualLine caret stop
  → BlockPoint
  → DocumentPoint
  → ViewportPoint
  → WindowPoint
  → platform ScreenRect
```

`candidate_rect()` 不允许自己计算字宽。

### 15.2 输入法与滚动

- 组合更新导致换行时，候选框应跟随新 caret。
- 滚动发生时，候选框位置需更新。
- 字体 fallback/emoji 改变行盒时，使用真实布局。
- 当前 composition 必须保证 marked range 处于已布局区域。
- 窗口在不同 DPI 显示器之间移动时，屏幕坐标转换必须刷新。

### 15.3 Range 几何

系统可能请求某个 UTF-16 range 的第一个 rect 或选区矩形。必须：

1. 以 composition-aware 文本输入坐标系解释 range。
2. 转换到 Source/Overlay anchor。
3. 映射到同版本布局。
4. 返回系统要求坐标系的矩形。
5. 若跨多视觉行，按平台协议选择首个有效 rect 或提供支持的范围几何。

---

## 16. Accessibility：将语义与文本几何暴露给 macOS

### 16.1 为什么不能只暴露“一个文本框”

编辑器需要让 VoiceOver、辅助输入和系统文本服务知道：

- 当前编辑内容。
- 选区与插入点。
- 可见文本范围。
- 文本 range 到屏幕 bounds 的映射。
- 行/段落导航。
- 文档标题与未保存状态。
- 代码块、标题、链接等语义（在平台支持范围内）。

### 16.2 Accessibility 的文本坐标系

必须明确**对外暴露的文本模型**：

- **Source Model**：完整 Markdown，包含隐藏标记。
- **Presentation Model**：用户当前看见的文本，不含隐藏标记。

这不是可以临时混用的选择。macOS 文本输入协议与辅助功能查询可能要求一致的 selected range / attributed string。

**推荐架构**：

- 文本输入协议内部使用统一、明确的**composition-aware editable text coordinate space**。
- Accessibility 对外呈现可读语义，但编辑 range 操作通过稳定的 Source↔Presentation 映射转换。
- 不承诺系统能同时把一个 TextView 的字符 range 理解为两套不相同的字符串。
- 若平台能力不足以兼顾完整源码与视觉文本，应优先保证可编辑范围一致性，并通过额外 accessibility actions 暴露“读取 Markdown 源码”能力。

这个选择必须通过真实 VoiceOver、macOS 文本服务和 IME 组合测试验证。

### 16.3 AccessibilityAdapter

```rust
pub trait AccessibilityAdapter {
    fn document_text(&self, snapshot: &AccessibleSnapshot) -> AccessibleText;
    fn selected_ranges(&self, snapshot: &AccessibleSnapshot) -> Vec<AccessibleRange>;
    fn bounds_for_range(
        &self,
        snapshot: &AccessibleSnapshot,
        range: AccessibleRange,
    ) -> Vec<ScreenRect>;
    fn visible_range(&self, snapshot: &AccessibleSnapshot) -> AccessibleRange;
    fn perform_action(&mut self, action: AccessibleAction) -> Result<(), AccessibilityError>;
}
```

### 16.4 虚拟化与辅助功能

即使 Block 没有实际绘制，也应能：

- 查询文档逻辑文本与结构。
- 将辅助功能请求的目标范围滚动到视口。
- 按需布局目标范围并返回 bounds。
- 在无布局时明确报告不可见，而不是返回伪造坐标。
- 保持选区与 VoiceOver 焦点的 Source Anchor 稳定。

### 16.5 语义层级

建议至少暴露：

```text
Document
  ├─ Heading
  ├─ Paragraph
  ├─ List
  │    └─ ListItem
  ├─ Quote
  ├─ CodeBlock
  └─ Link
```

语义树来自 SyntaxSnapshot；几何来自 LayoutSnapshot。**不能让辅助功能自己再解析 Markdown**。

---

## 17. 绘制与交互状态机

### 17.1 指针状态

```rust
pub enum PointerInteraction {
    Idle,
    Pressed { anchor: SourceAnchor, click_count: u8 },
    DragSelecting { fixed_anchor: SourceAnchor },
    DragAutoScrolling { fixed_anchor: SourceAnchor },
}
```

- `MouseDown` 确定 Source Anchor。
- `MouseMove` 更新活动端。
- `MouseUp` 结束拖选。
- `ClickCount` 按平台双击/三击语义转换。
- 事件都携带接收时的布局版本，必要时重新命中测试**当前指针位置**，但不能用重新布局后的坐标重写已经确定的固定 Source Anchor。

### 17.2 Focus 状态

```rust
pub enum EditorFocus {
    Unfocused,
    Focused,
    Composing,
}
```

IME 的真实状态仍由第三份定义的 CompositionState 管理，Focus 是 View 层的摘要，不得成为第二个互相冲突的输入状态机。

### 17.3 光标与选择的动画

- Blink/hover 动画只修改 Paint state。
- 不能把动画 tick 当作文档编辑。
- 选择高亮可渐变，但其 Source 范围始终来自 SelectionSet。
- 系统 Reduce Motion 设置应影响非必要动画。

---

## 18. 性能目标与 Benchmark

### 18.1 工作负载

至少建立：

1. 10 KB 普通 Markdown。
2. 1 MB 长篇技术文档。
3. 10 MB 混合列表/引用/代码块。
4. 100k+ Block 的极端短段落文档。
5. 单个 1 MB 长段落。
6. 单个 1 MB 代码围栏。
7. 高密度 Unicode、emoji、RTL、CJK。
8. 光标频繁跨越隐藏语法标记。
9. 连续 IME composition 更新。
10. 快速滚动 + 后台解析 + 窗口 resize。

### 18.2 观测指标

- Keypress → visible consistent frame 延迟（P50/P95/P99）。
- Hit test 延迟。
- 受影响 Block 重排时间。
- 每帧 Paint 时间。
- 缓存命中率与缓存内存占用。
- 虚拟化可见 Block 数。
- HeightIndex 查询/更新成本。
- Scroll Anchor 补偿误差。
- IME 候选框相对 caret 的几何误差。
- Accessibility range bounds 请求延迟。

### 18.3 性能目标的制定方法

不要先写死未经设备验证的“所有文档 1 ms”指标。建议建立目标机型和测量基线：

- 正常编辑在目标刷新率下保持交互流畅。
- 输入延迟的 P99 优先于平均值。
- 快速滚动不能因离屏 Block 布局造成明显停顿。
- 百万字文档的首屏不要求先布局全文。
- 长 Block 的最坏情况必须有独立压力测试。
- 性能回归需在 CI 的稳定基准机上对比，而不是靠主观感受。

---

## 19. 正确性测试

### 19.1 几何属性测试

- `source_anchor → caret_rect → hit_test(center)` 在定义的 Affinity 下保持等价。
- 相同 LayoutSnapshot 下 HitTest 结果确定。
- 每个可视 caret stop 映射到合法 Source 边界。
- 同一源选区的 SelectionGeometry 不越出对应视觉行。
- 滚动/窗口变换前后命中 Source Anchor 不变。
- 隐藏语法展开后 Source Anchor 不变。
- 软换行位置变化后 Source Anchor 不变。
- Bidi 边界按视觉导航定义正确。

### 19.2 Golden Rendering

固定字体、宽度、DPI、主题和 GPUI commit，建立像素或结构化几何 golden：

- Strong/Emphasis/CodeSpan。
- 多层嵌套。
- CJK/emoji/RTL。
- 列表/引用/代码围栏。
- 选区跨多行、多 Block。
- 光标在隐藏 marker 左右。
- IME marked text 与下划线。
- 不同窗口宽度下的软换行。

对抗锯齿和系统字体差异，应优先使用**结构化布局几何断言**，像素 golden 作为补充，避免脆弱的跨系统截图对比。

### 19.3 虚拟化测试

- HeightIndex 前缀和与简单数组参考实现一致。
- 实测高度替换估计高度后，Scroll Anchor 稳定。
- 删除上方 Block 后 viewport 锚点正确迁移。
- 快速滚动不会绘制旧版本 Block。
- 单个超长 Block 不导致无界布局。
- 目标 Block 离屏时 Accessibility 可以按需布局。

### 19.4 IME/Accessibility 真实平台测试

Mock 只能验证内部逻辑，不能替代：

- macOS 中文拼音候选框。
- 日文转换输入。
- 韩文组合输入。
- VoiceOver 逐字/逐行导航。
- Accessibility selected text / bounds。
- 不同 DPI 显示器之间移动窗口。
- 系统缩放与 Reduce Motion。
- 外接键盘、触控板、窗口失焦。

---

## 20. 失效与降级策略

| 失败场景 | 安全策略 |
|---|---|
| Shaping API 无法提供精确 caret stops | 增强/适配底层 shaping 接口；不能用平均字宽替代 |
| Projection 映射失效 | 对相关范围退回完整源码显示 |
| LayoutSnapshot 版本过期 | 拒绝几何操作，重新布局 |
| 当前 Block 布局失败 | 保留源码，显示可编辑的安全原文视图 |
| 高度估计明显错误 | 更新 HeightIndex + Scroll Anchor 补偿 |
| GPU 绘制资源丢失 | 重建渲染资源，不重建 Document |
| Accessibility range 不可映射 | 返回明确错误或按需布局，不能伪造位置 |
| IME 候选框几何不可用 | 保持输入安全并请求重布局/重新同步 |
| 后台布局结果过期 | 丢弃，不覆盖最新快照 |
| 单个 Block 极端大 | 使用 Block 内分段布局与虚拟化 |

---

## 21. Rust 接口总览

以下是**内部契约示意**：

```rust
pub trait NativeTextShaper {
    fn shape(
        &self,
        input: &ProjectedBlock,
        style: &Typography,
        width: LayoutPx,
    ) -> Result<ShapedBlock, ShapeError>;
}

pub trait BlockLayouter {
    fn layout(
        &self,
        projected: &ProjectedBlock,
        style: &Typography,
        width: LayoutPx,
    ) -> Result<Arc<BlockLayout>, LayoutError>;
}

pub trait LayoutQuery {
    fn caret_rect(
        &self,
        anchor: SourceAnchor,
        affinity: VisualAffinity,
    ) -> Result<CaretGeometry, LayoutQueryError>;

    fn hit_test(
        &self,
        point: DocumentPoint,
    ) -> Result<HitTestResult, LayoutQueryError>;

    fn selection_geometry(
        &self,
        selection: &Selection,
        visible_range: Range<BlockIndex>,
    ) -> Result<SelectionGeometry, LayoutQueryError>;
}

pub trait ViewportLayouter {
    fn visible_blocks(
        &mut self,
        viewport: ViewportState,
        source: &ProjectionSnapshot,
    ) -> Result<VisibleLayout, LayoutError>;
}

pub trait ScrollAnchorResolver {
    fn reconcile(
        &self,
        old: &LayoutSnapshot,
        new: &LayoutSnapshot,
        anchor: &ScrollAnchor,
    ) -> Result<LayoutPx, ScrollError>;
}

pub trait EditorPainter {
    fn paint(
        &mut self,
        snapshot: &RenderSnapshot,
        viewport: ViewportState,
    ) -> Result<(), PaintError>;
}
```

**设计检查**：

- `NativeTextShaper` 不接收可变 Rope。
- `EditorPainter` 不返回新的 Source Selection。
- `LayoutQuery` 只读取快照。
- `ViewportLayouter` 可以更新缓存，但不能改写 Document。
- `ScrollAnchorResolver` 只计算滚动偏移，不重新命中测试 Source Anchor。

---

## 22. 实施工作包

### R0 — 锁定 GPUI 版本并验证平台能力

交付：

- 固定 GPUI Git commit。
- 确认自定义 Element 生命周期。
- 确认文字 shaping、Bidi/caret geometry 可用性。
- 确认输入法候选框坐标接口。
- 确认 macOS Accessibility 接口覆盖度。
- 形成 `gpui-compat` 最小可编译探针。

**门槛**：对无法公开取得的关键能力提出 fork/扩展方案，而不是绕过需求。

### R1 — LayoutSnapshot + Text Shaping

交付：

- BlockLayout / VisualLine / CaretStop。
- Bidi、grapheme、emoji、字体 fallback。
- 同字号行高与语义样式。
- 结构化几何 golden。

**门槛**：每个合法 Source Anchor 可得到确定的可视 caret 位置。

### R2 — Hit Testing + Selection Geometry

交付：

- Pixel→Source 双向定位。
- 拖选、双击、三击、跨 Block 高亮。
- 隐藏 marker 边界与重排测试。

**门槛**：点击显隐后光标不漂移。

### R3 — Viewport Virtualization

交付：

- HeightIndex。
- Block 缓存与 overscan。
- 超长 Block 内 VisualLine 虚拟化。
- 后台预布局与过期结果处理。

**门槛**：长文档无需全文 shaping 即可首屏显示和滚动。

### R4 — Scroll Anchoring + Cache Invalidation

交付：

- Source-backed ScrollAnchor。
- 估计高度补偿。
- Projection/Layout/Viewport 失效矩阵。
- 缓存一致性与内存预算。

**门槛**：光标展开语法和窗口 resize 不产生意外阅读位置跳跃。

### R5 — GPUI Native Paint

交付：

- Editor Element。
- Text、selection、caret、decorations、composition layers。
- Retina/主题/焦点适配。
- 渲染基准与回归测试。

**门槛**：所有图层来自同一 RenderSnapshot。

### R6 — macOS IME Geometry + Accessibility

交付：

- Composition candidate rect。
- VoiceOver 语义树、选区和 range bounds。
- 离屏目标按需布局。
- 真实 macOS 测试矩阵。

**门槛**：输入法和辅助功能都能通过统一布局模型定位文本。

### R7 — 压力与故障恢复

交付：

- 百万字文档与极端长 Block 测试。
- 快速滚动/连续输入/resize 交叉压力测试。
- GPU 资源恢复与布局错误安全降级。
- 性能指标和自动回归。

**门槛**：不发生源码丢失、光标错位、过期快照编辑或无界内存增长。

---

## 23. 必须在编码前核实的技术问题

1. 当前目标 GPUI 版本的文字塑形接口是否暴露 cluster/caret 位置？
2. Bidi 与 ligature 内 caret 是否由平台 shaping 提供？
3. 自定义 Element 能否按 Block 进行局部绘制/失效？
4. GPUI 是否允许后台执行纯文本塑形，哪些对象只能主线程使用？
5. macOS 输入法候选框要求窗口坐标还是屏幕坐标？
6. GPUI 提供何种 Accessibility 桥接？缺失时如何扩展？
7. 选择源文本还是呈现文本作为平台 editable range 的对外坐标空间？
8. 同字号行高约束下，emoji/font fallback 的行盒如何裁剪与扩展？
9. 长 Block 分段 shaping 如何处理跨 chunk 的 Bidi、ligature 和换行？
10. 文件宽度变化时 HeightIndex 的估计误差如何补偿？
11. 结构性列表标记如何参与 hit testing 与 Source 映射？
12. GPU/系统字体更新导致的缓存失效如何传播？

这些问题属于**必须做的源码核验和平台实验**，不是可忽略的实现细节。

---

## 24. 与前四份文档的接口闭环

```text
(第三份) Editor Engine
  ├─ Source Selection / Transactions / IME
  │
  ▼
(第四份) Syntax Engine
  ├─ precise AST / marker ranges
  │
  ▼
(第二份) Projection Engine
  ├─ visible text / Source↔Display mapping
  │
  ▼
(第五份) Layout + Native Rendering
  ├─ shaped lines / caret / selection / hit test
  ├─ virtualized viewport / scroll anchoring
  ├─ IME candidate geometry / Accessibility
  │
  ▼
(第一份) macOS GPUI App
```

**闭环关键**：输入由平台进入 Editor Engine；Document 产生新版本；Syntax/Projection/Layout 逐级派生不可变快照；GPUI 绘制并反馈交互几何。没有任何一层通过“猜测屏幕文字”反向修改 Markdown 源码。

---

## 25. 最终结论

Typora 风格 Hybrid View 编辑器真正困难的部分，不是 Markdown 解析或粗体绘制，而是：

- **隐藏语法后仍可精确编辑源码**；
- **文字塑形、光标、选区、IME 和辅助功能使用一致几何**；
- **重排、虚拟化和滚动补偿不破坏阅读位置**；
- **百万字文档中仍保持原生响应性**。

因此第五份架构明确采用：

**版本化 Projection → 原生 Shaping → Block/VisualLine Layout → 统一 Geometry Queries → 虚拟化 Paint + Scroll Anchoring + Accessibility。**

不使用多个 TextInput 拼接编辑器，不使用 DOM/WebView，不以透明 Markdown 标记假装隐藏，不允许过期布局决定新的源码编辑位置。

> **第六份建议**：《工程化落地与验证：GPUI API 源码核验、技术探针、模块依赖、CI、性能基准、测试体系和阶段性验收》。这会把前五份设计变成可执行的工程任务与架构约束。
