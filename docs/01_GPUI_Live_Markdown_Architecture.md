# 纯 GPUI 单文件 Live Markdown 编辑器：架构与实现设计

> 2026-10-12 Hybrid 修订：交互路线与阶段范围以[第十一份](./11_GPUI_Typora_Hybrid_View_Spec.md)为准。Inline 按光标显隐；Block 保持结构化，显式进入局部/全文源码；模式切换不改源码或历史。下文领域 API 为设计草案，须纳入 Hybrid presentation 版本依赖后实施。

> 版本：v1.0（设计草案）  
> 技术路线：Rust + GPUI + 自研编辑内核  
> 产品目标：Typora 风格 Hybrid View 的原生 macOS 单文件 Markdown 编辑器  
> 设计原则：优先正确性、交互品质、架构长期演进和性能上限，不以开发成本压缩核心能力。

## 1. 目标与边界

### 1.1 核心体验

- **单文件、单窗口、单一文档真相**：直接编辑和保存 `.md` 文件，不引入笔记数据库。
- **Hybrid View**：Inline 非活动时隐藏标记、光标进入时展开；Block 按类型即时渲染，结构标记在普通正文编辑时保持隐藏，修改原始语法通过块源码入口或全文源码模式完成。语义样式始终保留。
- **统一基础排版**：全局字体、字号、行高一致；加粗、斜体、删除线、链接、代码使用语义修饰。行内代码等宽字体可作为显式例外。
- **原生输入**：鼠标、键盘、跨段落选区、中文/日文 IME、剪贴板、撤销重做、辅助功能和标准 macOS 快捷键。
- **Markdown 源码是唯一持久化格式**：隐藏、展开、装饰和布局均为可重建的派生状态。

### 1.2 第一版语法范围

| 类别 | 第一版 | 行为 |
|---|---|---|
| Paragraph / Heading | H1 | 标题结构化，正文获得光标不展开前缀；Hybrid Return 拆分普通段落，Source Return 插入源码换行 |
| Strong / Emphasis / Strike | 是 | 光标感知 Inline 显隐 |
| Inline Code | 是 | 反引号显隐，内部 Markdown 不解析 |
| Link | 是 | 非活动显示标题；活动时展开 `[]()` 和目标 URL |
| Quote / Ordered / Unordered List | H1 | 引用线、bullet/编号与缩进保持结构化；块源码入口可修改前缀 |
| Fenced Code | H1 | 直接编辑代码正文，围栏保持隐藏；语言字段/围栏由明确入口修改 |
| Task List | H1 | Source-backed 复选框，切换状态走同一事务与 Undo |
| Setext Heading | H1 | 语义确认后结构化，underline 通过块源码入口编辑 |
| Table | H2 | 网格、单元格编辑、增删行列与源码切换；H1 暂显示源码 |
| Image / Math | 架构预留 | 保持源码可编辑，暂不实现复杂可视化 |

单文件不意味着单 Block，也不意味着单一 GPUI 文本 Element。全篇共享一个文档与输入状态，按 FlowBlock/单元格缓存布局。H1/H2 范围、类型触发和源码入口以[第十一份 Hybrid View 方案](./11_GPUI_Typora_Hybrid_View_Spec.md)为准。

## 2. 不可妥协的不变量

1. **Single Source of Truth**：文档正文仅由一个 Rope/Buffer 拥有；语法树、投影、布局不得维护可独立编辑的正文副本。
2. **Source-first Selection**：Selection、IME 组合范围、编辑事务、Undo 历史以源码锚点为基准；显示位置仅是派生值。
3. **Snapshot Consistency**：命中测试、选区绘制、光标几何与软换行必须使用匹配的 DocumentRevision、ProjectionRevision、LayoutRevision。
4. **Projection is Non-destructive**：语法隐藏/展开不修改 Rope，不进入 Undo 历史，不改变文件内容。
5. **Correctness before Elision**：不完整、歧义或不受支持的 Markdown 语法保持源码可见，不错误隐藏。
6. **No Independent Block Editors**：Block 只负责语义、布局和局部命中测试；不存在独立输入法、选区、撤销栈。
7. **Unicode Correctness**：字节偏移、grapheme 边界、UTF-16 平台范围、双向文本和字体 shaping 明确区分。
8. **Incremental Invalidation**：文本编辑、光标移动、窗口宽度改变分别触发不同的最小失效集合。

## 3. 依赖方向与工作区

```text
app (GPUI window, menus, file dialogs)
 └── editor-gpui (input bridge, Element, renderer, accessibility)
      ├── editor-engine (commands, selections, transactions, history, composition)
      ├── editor-layout (block layout, text shaping adapter, hit test, viewport)
      └── markdown-presentation (Hybrid policies, FlowBlocks, projection, style runs)
           └── markdown-syntax (block/inline parse, precise marker ranges)
                └── editor-document (Rope, anchors, revisions, edit deltas)
```

跨层事件使用明确的接口，不把 `gpui::Window`、`gpui::Context` 等类型传入核心 crate。布局层可以定义平台无关的 `TextShaper` trait，GPUI 适配层实现它。

```text
live-markdown/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── crates/
│   ├── editor-document/src/{lib,buffer,anchor,delta,snapshot}.rs
│   ├── markdown-syntax/src/{lib,parser,block,inline,marker,incremental}.rs
│   ├── markdown-presentation/src/{lib,visibility,projection,segment,style,map}.rs
│   ├── editor-engine/src/{lib,controller,command,selection,history,composition,movement}.rs
│   ├── editor-layout/src/{lib,block,shaping,wrap,hit_test,geometry,cache,viewport}.rs
│   ├── editor-gpui/src/{lib,view,element,input_bridge,paint,accessibility}.rs
│   └── app/src/{main,window,files,menus,settings}.rs
├── tests/{fixtures,model,unicode,editing,ime,visual}/
├── benches/
└── docs/{architecture,projection-spec,editing-spec,testing}.md
```

**依赖策略**：锁定 GPUI Git commit；不依赖 gpui-kit。Rope 候选（如 ropey）、解析器候选（如 tree-sitter-markdown / pulldown-cmark）应以实际 API、语法范围准确度、编辑增量性能和许可核实后锁定版本。不要在未经核查时假定某解析器直接提供全部精确 delimiter ranges。

## 4. Document Engine：Rope、Anchor、Delta

### 4.1 强类型坐标

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceByte(pub usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectedByte(pub usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bias { Before, After }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchor { pub byte: SourceByte, pub bias: Bias }
#[derive(Clone, Debug)]
pub struct Selection { pub anchor: Anchor, pub head: Anchor }
```

源码 byte offset 是核心存储单位；编辑器的“合法光标位置”由 grapheme 分割及 bidi 光标停靠规则决定。不要把 Unicode scalar index、UTF-16 offset、像素坐标隐式转为 byte offset。

### 4.2 文档版本与编辑事务

```rust
pub struct DocumentSnapshot {
    pub revision: u64,
    // 不可变 Rope snapshot，具体类型随选定实现而定
}
pub struct TextEdit { pub old_range: std::ops::Range<SourceByte>, pub insert: String }
pub struct Transaction {
    pub edits: Vec<TextEdit>,
    pub selection_before: Selection,
    pub selection_after: Selection,
    pub group: Option<u64>,
}
pub struct EditDelta {
    pub old_revision: u64,
    pub new_revision: u64,
    // 按旧文档坐标排序的变更区间与长度变化
}
```

**事务约束**：多处编辑的 ranges 统一采用事务开始时的旧文档坐标，要求非重叠；实际写入按逆序执行或通过批量编辑 API 完成。提交前校验 UTF-8 字符边界，完成后变换所有 Anchor。Undo/Redo 保存反向编辑与前后选区，连续打字按时间、命令类别和相邻性分组，但 IME 的多次更新不得形成多个用户可见撤销步骤。

### 4.3 Anchor 演进

初版可采用 `SourceByte + Bias + EditDelta`，每次事务批量变换当前 Anchor；若将来存在大量持久标记，升级为带 ID 的 Anchor Store / 索引树。对于被删除范围内的 Anchor，明确塌缩到删除边界的规则。不能仅靠裸 byte offset 追踪编辑后的选择位置。

## 5. Markdown Syntax Engine

### 5.1 AST 与精确标记范围

```rust
pub struct InlineNode {
    pub id: NodeId,
    pub kind: InlineKind,
    pub full_range: std::ops::Range<SourceByte>,
    pub content_range: std::ops::Range<SourceByte>,
    pub markers: Vec<MarkerRange>,
    pub parent: Option<NodeId>,
}
pub struct MarkerRange {
    pub range: std::ops::Range<SourceByte>,
    pub role: MarkerRole,
}
pub enum MarkerRole { Open, Close, LinkDestination, Separator }
```

`**hello**` 的完整范围是 `0..9`（ASCII bytes），开标记 `0..2`、内容 `2..7`、闭标记 `7..9`。`[title](url)` 必须分别标识 `[`、`](`、URL、`)`；链接标题与 URL 的可见策略可以独立调整。

### 5.2 解析策略

- 建立 **Block parser**：段落、标题、引用、列表、围栏代码、空行；保存源码区间与块层级。
- 建立 **Inline parser**：加粗、斜体、删除线、行内代码、链接；返回 delimiter 精确 byte ranges。
- 对 CommonMark 边界规则、转义、反引号长度、嵌套/交错 delimiter、链接目标转义与未闭合语法执行一致性测试。
- 借助增量解析器确定受影响块范围；对可能跨块传播的构造（围栏、列表、引用）向后重新解析直到状态收敛。不能假设编辑仅影响当前块。
- 将 parser 输出规范化为编辑器自己的稳定 AST；增量节点 ID 通过结构和编辑映射尽量保持稳定。
- 当解析不确定时退化为原文可见，禁止通过正则表达式猜测并隐藏字符。

**实现关卡**：先对目标 Markdown 子集制作包含精确 marker ranges 的测试语料；解析器选型必须通过这套语料，而不是仅凭“支持 Markdown”决定。

## 6. Hybrid Presentation：Inline 显隐与 Block 结构化

### 6.1 状态输入与规则

```rust
pub struct RevealContext<'a> {
    pub selections: &'a [Selection],
    pub composition: Option<&'a CompositionState>,
    pub pointer_dragging: bool,
}
pub enum MarkerVisibility { Visible, Hidden }
pub trait VisibilityPolicy {
    fn visible(&self, node: &InlineNode, marker: &MarkerRange,
               context: &RevealContext<'_>) -> MarkerVisibility;
}
```

- 光标位于节点内容的内部或指定边界：显示该节点的起止 delimiter。
- 光标位于节点外：隐藏合法 delimiter。
- 嵌套语法：展开包含光标的语法路径，必要时同时展开祖先，避免不可定位的隐藏区间。
- 非折叠选区：只展开相交 Inline 及必要 Inline 祖先；拖选期间冻结显隐，结束后重算。外层 Block 不随选择展开。
- 输入法组合：锁定受影响节点的 reveal 决策，提交/取消后统一重新计算。
- 不完整语法：始终显示用户正在输入的标记。
- Block marker：独立 BlockPresentationPolicy 按语义与 RawDraft/Structured/BlockSource 状态处理。已渲染块的前缀不因正文获得光标或跨块选区而展开。
- 已有合法块打开即结构化；输入候选按块类型确认。Enter 遵守段落拆分、列表延续、代码换行等各自规则，不统一解释为“隐藏块标记”。
- EditBlockSource 与全文 Source 显示同一 Rope 的原始语法；切换自身不修改文档或历史。BlockSource 内关闭 Inline 隐藏，组合期间协调模式切换。

**边界契约**：节点起止位置的归属通过 Source Anchor + Bias 决定；不能用投影后的像素位置反过来推导 reveal 状态，否则可能发生显隐振荡。

### 6.2 示例

```text
Source:      abc **hello** xyz
Inactive:    abc hello xyz
Active:      abc **hello** xyz
Source:      ***bold italic***
Inactive:    bold italic
Active:      ***bold italic***   (按祖先链展开)
```

以上是本项目 Hybrid View 的 Inline 行为；Block 按第十一份稳定呈现，不采用整段获得焦点即展开的统一策略，不推断 Typora 的专有内部实现。

## 7. Projection Engine：分段显示映射

### 7.1 数据结构

```rust
pub struct ProjectionSnapshot {
    pub document_revision: u64,
    pub visibility_revision: u64,
    pub hybrid_presentation_revision: u64,
    pub blocks: Vec<ProjectedBlock>,
}
pub struct ProjectedBlock {
    pub block_id: BlockId,
    pub source_range: std::ops::Range<SourceByte>,
    pub segments: Vec<Segment>,
    pub styles: Vec<StyleRun>,
}
pub struct Segment {
    pub source: std::ops::Range<SourceByte>,
    pub projected: std::ops::Range<ProjectedByte>,
    pub kind: SegmentKind,
}
pub enum SegmentKind { VisibleText, HiddenSyntax, Replacement(WidgetId) }
```

第一版的 Replacement 可以不渲染，但必须定义其源码锚定、光标停靠和复制语义。投影文本可以按需从 Segment 生成，不必始终保存第二份可修改字符串。

### 7.2 正确映射例子

```text
Source:    ab**cd**ef     (10 ASCII bytes)
Projected: abcdef         (6 ASCII bytes)

Source range  Projected range  Kind
0..2          0..2             VisibleText
2..4          2..2             HiddenSyntax
4..6          2..4             VisibleText
6..8          4..4             HiddenSyntax
8..10         4..6             VisibleText
```

多个源码位置会坍缩到同一个 projected offset。反向映射必须返回 **合法 Source Anchor + Bias**，而不是承诺一对一的 offset 逆函数。对于隐藏标记内部的位置，普通鼠标点击应映射到可编辑的相邻停靠点；键盘编辑和 reveal 规则再决定标记是否展开。

### 7.3 API 与复杂度

```rust
pub trait ProjectionMap {
    fn source_to_projected(&self, source: Anchor) -> ProjectedPosition;
    fn projected_to_source(&self, pos: ProjectedPosition) -> Anchor;
    fn project_selection(&self, selection: &Selection) -> Vec<ProjectedRange>;
    fn projected_runs(&self) -> impl Iterator<Item = ProjectedRun<'_>>;
}
```

- Segment 按 source 起点排序，区间非重叠，覆盖对应 Block 的源码范围。
- 保存前缀隐藏字节数；二分查找目标 `O(log S)`，`S` 为 Block 的 segment 数。
- StyleRun 以 projected 坐标输出，确保隐藏标记不影响加粗内容的样式范围。
- ProjectionSnapshot 只在相关语法节点、选区 reveal 状态或文档修订变化时更新。
- 对每个可见 grapheme 停靠点测试往返一致性；对所有输入位置测试单调性、边界合法性、无越界和可编辑停靠点可达性。

## 8. Layout Engine：Block 级布局与统一排版

### 8.1 布局流水线

```text
Document + AST + HybridPresentationState
           ↓
FlowBlocks / ProjectedBlock (content spans + styles + structural decorations)
           ↓
Font shaping (GPUI adapter)
           ↓
Soft wrap / bidi / line metrics
           ↓
BlockLayout (visual lines + glyph geometry)
           ↓
Viewport layout / hit test / selection rectangles
           ↓
GPUI paint
```

**绝不在原始 Markdown 上先做软换行再隐藏语法**。加粗字体可能改变字宽，即使字号不变，也必须重新 shaping。复杂脚本、连字、emoji、bidi 均由实际 shaping 与字形位置决定，不使用平均字符宽度估算。

### 8.2 BlockLayout 快照

```rust
pub struct BlockLayout {
    pub block_id: BlockId,
    pub document_revision: u64,
    pub projection_revision: u64,
    pub width: f32,
    pub height: f32,
    pub lines: Vec<VisualLine>,
    // 具体字形运行、可编辑 caret stops、hit-test 数据由实现定义
}
```

Block ID 稳定；Block 布局可缓存；全局选区与 Block 的 source range 求交后映射成局部 selection rectangles。鼠标事件先基于**点击前**的 LayoutSnapshot 命中测试得到 Source Anchor，再触发 reveal 和重排，不能对同一次点击重复 hit-test。

### 8.3 统一主题

```rust
pub struct Typography {
    pub font_family: String,
    pub font_size: f32,
    pub line_height: f32,
    pub paragraph_spacing: f32,
    pub content_width: f32,
}
```

标题、正文共享基础字号/行高；标题允许字重与颜色变化；引用使用边线/缩进；链接使用颜色与下划线；代码可使用等宽字体例外。所有视觉修饰由样式 Run 和 Block decoration 实现，不更改 Markdown 源码。

### 8.4 虚拟化与缓存

- 维护 Block 高度索引（Fenwick tree / prefix-sum tree）以支持 `scrollY → block` 的对数级定位。
- 仅对可视区及 overscan Block 保持完整字形布局；非可视 Block 保留高度估计或已缓存高度。
- 光标移动引起 reveal 时，只失效涉及的旧/新活动节点所在 Block；窗口宽度变化则失效宽度相关布局。
- Block 高度变化时，通过 scroll anchoring 保持视口视觉稳定。
- 解析器可后台计算，但所有快照必须带 revision；过期结果丢弃，输入与绘制路径不得消费错配快照。

## 9. Editor Engine：命令、光标、选区、Undo

### 9.1 命令模型

```rust
pub enum EditorCommand {
    InsertText(String), DeleteBackward, DeleteForward,
    MoveLeft { extend: bool }, MoveRight { extend: bool },
    MoveUp { extend: bool }, MoveDown { extend: bool },
    MoveWordLeft { extend: bool }, MoveWordRight { extend: bool },
    ToggleStrong, ToggleEmphasis, ToggleCode,
    Undo, Redo, SelectAll, Copy, Cut, Paste(String),
}
```

所有修改文档的编辑命令归约为 Source-based Transaction。格式快捷键直接修改 Markdown delimiter；当选区已有格式时，基于 AST 执行语义取消，不能盲目插入重复标记。**默认 `Cmd+C` 复制 Markdown**（用户已确认）：复制选中内容时保留其格式和链接目标，不依赖当前 marker 是否展开；必要时补齐片段的成对标记。另设 Copy Source 用于精确源码切片，Copy as Plain Text 可作为显式辅助命令。剪切与默认复制使用同一 Markdown 序列化规则，但只删除原 Source Selection。具体边界和剪贴板载体以第八份 EB-040—043 为准。

### 9.2 Selection 与导航

- 支持 anchor/head、前进/后退选择方向、首选列 `preferred_x`。
- 左右移动按可视 grapheme caret stops；上下移动基于布局行和 preferred_x；Option+方向键按词边界。
- 源码中的隐藏标记不可形成鼠标无法解释的死区；通过 Affinity 及 reveal 状态确保可编辑性。
- 支持 Shift 拖选、双击词选、三击行/段落选择、跨 Block 选区。
- Bidi 情况下区分逻辑移动与视觉移动策略，并建立测试。

### 9.3 Undo/Redo

Undo 存储 Source edits、反向 edits、selection before/after 和合并分组。光标移动、语法显隐、窗口调整不产生 Undo 项。IME 提交应形成一个用户可见事务；自动配对、格式快捷键等按语义组合为原子事务。

## 10. GPUI Input Bridge 与 macOS IME

**这里是纯 GPUI 路线的高风险核心。** 必须以锁定版本的 GPUI 输入接口为准实现 `InputHandler` 或等效桥接，不能假设其他版本的方法签名兼容。

### 10.1 输入状态机

```text
Idle
  ├── BeginComposition → Composing
  ├── InsertText → Commit Transaction
  └── SelectionChanged → Recompute Reveal
Composing
  ├── UpdateMarkedText → Update provisional composition
  ├── Commit → Commit one Undo group → Idle
  └── Cancel → Restore precomposition state → Idle
```

IME 组合区应有独立的临时编辑状态和文档基准修订。可以通过暂存层或可回滚事务显示 marked text，但必须确保取消恢复准确、Undo 分组正确。组合期间冻结受影响节点的 reveal 状态，候选框位置来自与当前组合视图匹配的布局快照。

### 10.2 UTF-16 桥接

macOS 文本服务常使用 UTF-16 范围；Document 内部使用 UTF-8 byte offset。建立 `Utf16Range ↔ SourceRange` 显式转换接口，保证代理对、组合字符与无效边界正确处理。不要按 `String::len()` 当作字符数或 UTF-16 长度。

### 10.3 必测场景

中文拼音连续组词、日文转换、韩文组合、emoji、替换选区、组合中鼠标点击、组合中滚动、组合中 Undo、组合期间修改加粗边界、候选框跨软换行位置。需要在真实 macOS 上测试，单元测试不能替代平台输入法测试。

## 11. GPUI View、绘制与辅助功能

### 11.1 View / Element 分工

```text
MarkdownEditorView (GPUI Entity)
 ├── EditorEngine
 ├── ViewportState
 ├── InputBridge / FocusHandle
 └── MarkdownEditorElement
      ├── VisibleBlockLayouts
      ├── SelectionPaint
      ├── TextPaint
      ├── CaretPaint
      ├── DecorationPaint
      └── IMECompositionPaint
```

可使用单一自定义 Element 管理连续编辑区域，但 BlockLayout 是内部缓存单元，不必每个 Inline 创建 GPUI Element。交互事件统一进入 Controller；布局与绘制使用不可变快照。

### 11.2 辅助功能

设计初期即保留 macOS accessibility 的文档文本、选择范围、光标位置、可见文本与滚动动作接口。语法隐藏造成“可见文本”与“原始 Markdown”不一致，需要定义屏幕阅读器语义，避免重复朗读 delimiter 或丢失格式含义。具体 GPUI 平台支持范围需在实现时核查。

## 12. 单文件 File Session

```rust
pub struct FileSession {
    pub path: Option<std::path::PathBuf>,
    pub saved_revision: u64,
    pub encoding: FileEncoding,
    pub line_ending: LineEnding,
    pub external_file_identity: Option<FileIdentity>,
}
```

- 新建、打开、保存、另存为、最近文件（可选）、未保存提示。
- UTF-8 与 UTF-8 BOM 检测；非 UTF-8 文件拒绝静默乱码，必要时提供明确编码转换。
- 保留原始行尾风格（LF/CRLF）与最终换行状态；保存不擅自格式化 Markdown。
- 同目录临时文件写入、flush/sync、按平台语义安全替换；保存失败保留原文件。
- 保存前检查外部修改，冲突时要求用户决定重新载入、覆盖或另存。
- 关闭窗口前处理未保存修改；崩溃恢复可采用独立自动恢复快照，但不能覆盖源文件。

## 13. 关键事件流程

### 13.1 鼠标点击加粗内容

```text
MouseDown(pixel)
 → hit_test(previous LayoutSnapshot)
 → projected position
 → ProjectionMap.projected_to_source() → Source Anchor
 → update Selection
 → VisibilityPolicy (reveal **)
 → new ProjectionSnapshot
 → reshape/reflow affected Block
 → source anchor → new caret geometry
 → paint
```

注意：不要在 reveal 之后对同一次点击再次 hit-test，避免因文本宽度变化而产生光标振荡。

### 13.2 输入一个字符

```text
TextInput / IME Commit
 → validate source selection
 → Transaction
 → Rope revision++ / transform anchors
 → incremental syntax update
 → visibility update
 → projection revision++
 → affected block layout
 → caret geometry / scroll anchoring
 → repaint
```

### 13.3 光标离开 Inline

```text
MoveCaret
 → update source selection
 → compare old/new active syntax path
 → hide markers on old path; reveal new path
 → invalidate affected projection/layout only
 → repaint
```

此操作不修改 Rope、不增加 Undo 记录。

## 14. 测试与验收体系

### 14.1 纯 Rust 单元与属性测试

- `**bold**`、`*italic*`、`~~strike~~`、`` `code` ``、`[title](url)`、`***nested***`。
- 不完整 Markdown、转义 delimiter、长反引号、相邻 Inline 节点。
- 中文、emoji ZWJ、组合附加符号、RTL、混合方向文本。
- 映射单调性、范围覆盖、非重叠、边界 Affinity、可见停靠点往返、编辑后的 Anchor 变换。
- 基于随机 Markdown 编辑序列对比“增量解析/投影”与“全量重建”结果。

### 14.2 GPUI 集成测试

- 点击隐藏语法两侧、进入/离开强调整体、跨行选择、跨 Block 选择。
- Reveal 引发软换行变化后光标仍位于正确源码位置。
- 窗口宽度改变、滚动、缩放、输入法候选框定位。
- Undo/Redo、格式快捷键、复制/粘贴、删除 delimiter。
- 打开→编辑→保存→重新打开字节级或规范化约定一致性。

### 14.3 性能基准（目标，不是已验证指标）

| 场景 | 目标 |
|---|---|
| 10 KB 文档单次编辑 | 交互路径 P95 ≤ 16 ms（目标机器上测量） |
| 100 KB 文档单次编辑 | 交互路径 P95 ≤ 25 ms |
| 光标进入 Inline | 仅相关 Block 投影/布局失效 |
| 1 MB 文档 | 支持平稳滚动和局部编辑，不全篇布局 |
| IME 连续输入 | 无丢字、候选框错位或撤销分裂 |

指标需要指定硬件、系统版本、release 构建、文档分布和采样方法。正确性是发布门槛，性能优化不能以破坏映射或输入语义为代价。

## 15. 实施顺序与决策关卡

> 采用“完整架构、分阶段验证”，不是以简化架构换速度。每阶段产出可测的完整子系统。

| 阶段 | 主要任务 | 退出标准 |
|---|---|---|
| A. GPUI 平台研究 | 锁定版本；验证 text shaping、InputHandler、selection geometry、macOS IME 接口 | 完整 API 调用链与真实 macOS 输入原型 |
| B. Document Engine | Rope、Anchor、Transaction、Undo、UTF-8/UTF-16 转换 | 编辑模型/Unicode 属性测试通过 |
| C. Syntax Engine | Block/Inline AST、精确 marker ranges、增量失效 | 语法语料与全量对照测试通过 |
| D. Projection Engine | Reveal Policy、Segment Map、Affinity、快照 | `**hello**` 显隐及全部映射边界测试通过 |
| E. Layout Engine | GPUI shaping、wrap、bidi、hit test、Block cache | 显隐重排、点击、选区无偏移 |
| F. Input/Interaction | IME、键盘移动、拖选、剪贴板、快捷键、Undo | macOS 真实输入法矩阵通过 |
| G. File App | 打开/保存/冲突处理/恢复/菜单/主题 | 文件可靠性与端到端回归通过 |
| H. 性能/可访问性 | 视口虚拟化、缓存、无障碍、压力测试 | 目标性能和正确性门槛通过 |

### 15.1 优先验证的技术风险

1. GPUI 当前版本的文本布局对象能否提供足够精细的 glyph/caret geometry 和 hit-testing 能力。
2. GPUI macOS 输入协议与自定义 Element 如何正确连接。
3. 投影后的软换行与双向文本如何映射回 Source Anchor。
4. 精确 Markdown delimiter ranges 的解析质量与增量稳定性。
5. 辅助功能 API 是否满足文本编辑器级别要求。

如果某个 GPUI 公共接口不足，应考虑对 GPUI 提交通用能力扩展或维护明确隔离的底层 patch；不以外层视觉遮盖的方式伪造编辑器正确性。

## 16. 不采用的架构

- **每个 Block 一个 TextInput**：破坏全局 Selection/IME/Undo 一致性。
- **每个 Inline 一个 GPUI Element**：跨 Run 软换行、命中测试和选择几何难以统一。
- **所有 Block 获得正文焦点就展开整段 Markdown**：不符合 Hybrid View；局部源码由明确入口进入，Inline 仍精细显隐。
- **仅靠高亮把 `**` 设置为透明**：字符仍占宽度，软换行与光标几何错误。
- **先排版再删除语法字符**：Wrap 与显示文本不一致。
- **复制一份富文本作为第二数据源**：产生双向同步与持久化歧义。
- **每次光标移动全量解析/布局**：性能架构错误。

## 17. 最终结论

选择 **纯 GPUI + 自研 Document / Editor / Markdown Projection / Layout 内核**，采用 Typora 风格 Hybrid View。统一 Rope 保存 Markdown；InlineVisibilityPolicy 管理行内显隐，BlockPresentationPolicy 管理稳定结构与源码入口，经 FlowBlocks、Source maps 和布局快照完成绘制与编辑。IME、Undo、Selection、默认 Markdown 复制与文件保存围绕同一份源码和版本化快照设计。

**质量判断标准不是“能否显示 Markdown”，而是：在任何语法显隐、软换行、复杂 Unicode、输入法组合与跨 Block 选区条件下，源码、光标和视觉位置始终一致。**

---

### 附录：实施前必须锁定的事实

本文的 Rust 结构体和 trait 是**设计接口**，不是对 GPUI 现有 API 的声称。开始编码前应在选定 GPUI commit 上核查：输入接口签名、文本 shaping/line breaking API、Element 生命周期、focus 与 accessibility 接口、平台剪贴板与文件对话框接入；再将设计接口映射为可编译实现。
