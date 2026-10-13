# GPUI Live Markdown — 第八份：端到端编辑行为、交互语义与验收规范

> 2026-10-12 Hybrid 修订：交互路线与阶段范围以[第十一份](./11_GPUI_Typora_Hybrid_View_Spec.md)为准。Inline 按光标显隐；Block 保持结构化，显式进入局部/全文源码；模式切换不改源码或历史。下文领域 API 为设计草案，须纳入 Hybrid presentation 版本依赖后实施。

> 版本：v0.1（产品交互规范与测试契约）  
> 平台：macOS / Rust / GPUI / 单一 Markdown 源文件  
> 目标：将前七份架构文档中的设计约束转化为**用户可观察、可复现、可自动验收**的行为。  
> 状态说明：用户已确认采用 **Typora 风格 Hybrid View** 和默认复制 Markdown；其余未确认细节为本项目建议默认，不是对 Typora 内部实现的断言。块状态、触发与源码模式以[第十一份规格](./11_GPUI_Typora_Hybrid_View_Spec.md)为准。macOS 键位细节仍需真实系统核验。

---

## 1. 规范的地位与优先级

本文定义 Editor Engine 对外的行为契约。发生冲突时：

1. 不丢失、不误改 Markdown 原文；
2. Unicode、IME、系统输入协议正确；
3. Source Anchor、选区与 Undo 可预测；
4. Live Markdown 的视觉反馈；
5. 性能与动画优化。

每条行为使用 ID `EB-xxx`，测试使用 `CT-xxx`。标注：

- **MUST**：不可违反的验收条件。
- **SHOULD**：默认产品策略，允许通过正式 ADR 更改。
- **MAY**：可选增强，不能破坏 MUST。

本文的“源码”指 committed Rope 的 UTF-8 Markdown；“显示文本”指经 Projection 后的视觉内容；“组合文本”指尚未提交的 IME overlay。

## 2. 测试坐标与 Fixture 约定

所有规范测试统一使用以下形式：

```yaml
id: CT-001
source: 'ab**cd**ef'
selection:
  anchor_byte: 4
  head_byte: 4
  affinity: downstream
visibility: idle
command: move_right
expect:
  source: 'ab**cd**ef'
  document_revision_delta: 0
  history_delta: 0
  dirty_changed: false
```

测试中 `|` 仅作为说明性光标符号，不是 Markdown 源码的一部分。UTF-8 偏移使用**字节**，绝不混用 Unicode scalar、grapheme、UTF-16 或屏幕字符序号。对于视觉命中测试，应额外指定 Layout Fixture（字体、宽度、方向、坐标），避免凭字符串推测像素。

### 2.1 状态模型

```rust
struct ObservableEditorState {
    committed_source: String,
    selections: Vec<SourceSelection>,
    primary_selection: usize,
    document_view_mode: DocumentViewMode,
    block_view_modes: Vec<BlockModeObservation>,
    visible_text_by_block: Vec<String>,
    revealed_marker_ranges: Vec<SourceRange>,
    composition: Option<CompositionObservation>,
    undo_depth: usize,
    redo_depth: usize,
    dirty: bool,
    viewport_anchor: Option<SourceAnchor>,
}
```

规范测试只比较对外可观察状态；缓存 ID、AST 节点分配和渲染内部结构不属于产品行为。

---

## 3. Live Markdown 标记显隐

### EB-001：非活动 Inline 隐藏精确语法标记（MUST）

源码 `ab**cd**ef`，光标远离 Strong 节点时，显示 `abcdef`，`cd` 仍为粗体。源码、Revision、Undo、Dirty 不因显隐而改变。只有经 Syntax Engine 验证为 `Exact` 的 marker 可隐藏。

### EB-002：光标进入 Inline，展开成对标记（MUST）

光标进入 `cd` 的源码内容范围后显示 `ab**cd**ef`，保留 `cd` 粗体。退出该节点后恢复隐藏。展开/隐藏均不得产生编辑事务。

### EB-003：边界采用 Source Anchor + Affinity（MUST）

在 `ab**cd**ef` 的视觉 `b|c` 边界，点击先由旧 LayoutSnapshot 命中 Source Anchor，再更新 Visibility、重新布局，最后把光标画到**同一 Source Anchor**。禁止显隐后重新按旧像素点击。

### EB-004：嵌套节点展开祖先链（SHOULD）

`**a *b* c**` 中进入 `b`，默认展开 Strong 与 Emphasis 两层的成对 marker，确保用户能理解当前嵌套语法。离开最内层但仍在 Strong 内时只收起 Emphasis。实现应允许将来调整为局部展开，但必须保持对称且可预测。

### EB-005：非折叠选区（SHOULD）

选区与某个语义 Inline 的内容或 marker 相交时，展开相交 Inline 节点及必要 Inline 祖先，避免用户无法判断选区是否包含隐藏标记。选区始终保存 Source 范围；标题、列表、引用等外层 Block 不随选区自动显示前缀。拖选期间冻结显隐集合，MouseUp 后统一重算并保留两个 Source 端点。

### EB-006：不完整语法原文可见（MUST）

`**unfinished`、`[name](`、孤立反引号等输入中间态不得凭猜测隐藏标记。合法未闭合 fenced code 的语义由 CommonMark 规则决定，但应提供可编辑的围栏入口。

### EB-007：稳定的结构化 Block（MUST）

- Heading：已确认的 ATX 前缀或 Setext underline 隐藏；光标进入正文仍保持标题，修改原始语法走块源码入口或级别命令。
- List / Task List：bullet、编号、复选框与悬挂缩进保持；正文聚焦不显示结构 marker。缩进、类型、复选框命令修改准确源区间。
- Quote：引用线与缩进保持；正文聚焦不展开 `>`。
- Fenced Code：围栏保持隐藏；代码内容直接编辑，不解析普通 Markdown Inline；语言字段与块源码入口允许修改 info string/围栏。
- Table：H2 以网格和单元格编辑，普通聚焦不展开整表；H1 暂保持可编辑原文，不能宣称已实现网格。

输入中的 RawDraft、类型相关确认时机和未知语法回退由第十一份规定。普通正文聚焦不等于进入源码态；内部 Inline 仍按 EB-001—005 显隐。

### EB-008：投影变化保持视觉语义（MUST）

显隐引发换行时，Source Selection 不变；新光标位置由新布局计算；主动输入优先保证光标可见，主动滚动优先保证阅读锚点稳定。

### EB-009：局部与全文源码模式（MUST）

`EditBlockSource` 显示精确块源码；全文 Source 显示整个 Markdown。两者共享 Rope、Source Selection、事务、IME 和 Undo，不维护第二份可编辑正文。纯模式切换不改 Dirty/History；退出按最新 AST 恢复结构或安全回退原文。Escape 只退出视图，不撤销已提交编辑。IME 组合或拖选期间协调/推迟切换，并更新同版本几何。

### CT-001 至 CT-012：显隐验收

| ID | 初始源码 | 操作 | 必须验证 |
|---|---|---|---|
| CT-001 | `ab**cd**ef` | 光标在远离 Strong 的段落 | 显示 `abcdef`；源码不变 |
| CT-002 | `ab**cd**ef` | 点击 `c` | 成对 `**` 展开；光标 Anchor 稳定 |
| CT-003 | `ab**cd**ef` | 从 `c` 移到 `e` | 标记收起；Undo 不变 |
| CT-004 | `**a *b* c**` | 光标进入 `b` | 两层展开，样式保留 |
| CT-005 | `**a *b* c**` | 光标从 `b` 移到 `c` | 内层收起，外层保留 |
| CT-006 | `***x***` | 光标进入 `x` | marker ownership 不重叠，显示源码正确 |
| CT-007 | `` `**x**` `` | 光标进入代码 | `**` 不被当作 Strong |
| CT-008 | `\*x*` | 光标进入 `x` | 转义标记不误识别为强调 |
| CT-009 | `**unfinished` | 任意光标 | 不隐藏未闭合标记 |
| CT-010 | `ab**cd**ef` | 光标移动导致软换行 | Source Anchor 不变 |
| CT-011 | `# title` | 进入/离开标题，再进入块源码 | 正文编辑时前缀隐藏；源码态可编辑；纯切换无历史 |
| CT-012 | `- [x] task` | 正文聚焦、切换 checkbox | 列表结构保持；checkbox 仅修改准确源范围 |

---

## 4. 光标、选区与导航

### EB-010：字符导航以 Grapheme 为最小单位（MUST）

左右移动与 Backspace/Delete 不得停在 UTF-8 continuation byte、combining sequence、emoji ZWJ cluster 内部。文本塑形产生的合法视觉 caret stop 是左右移动的依据。

### EB-011：左右键采用视觉方向（MUST）

混合 LTR/RTL 段落中，Left/Right 根据当前视觉行的 CaretStop Graph 导航，不直接做 `source_byte ± 1`。相同逻辑位置可因 Affinity 产生不同视觉光标。

### EB-012：上下键保留 preferred_x（MUST）

跨视觉行移动时保持上一次期望的 x；移动到短行后再移动到长行，应尽可能回到原 x。显式鼠标点击和水平移动应重置 preferred_x。

### EB-013：Shift 扩展选择保留方向（MUST）

`anchor` 为固定端，`head` 为活动端。Shift+方向键改变 head；反向缩短后继续移动时行为稳定。选择方向可被 Undo 恢复。

### EB-014：单击/双击/三击（SHOULD）

- 单击：定位最近合法 caret stop。
- 双击：按 Unicode word boundary 选择单词；链接标签和 Inline Code 的边界遵循语义规则。
- 三击：选择逻辑段落内容（**不含末尾段落分隔符**作为默认策略）；该策略应在 macOS 实测后 ADR 确认。
- 点击后标记展开时不得二次 hit-test。

### EB-015：Home/End 与 macOS 键位（SHOULD）

- `Cmd+Left/Right`：逻辑行首/尾；
- `Option+Left/Right`：词边界；
- `Cmd+Up/Down`：文档首/尾；
- `Home/End`：若物理键存在，默认滚动视口或遵循 macOS 系统行为，需通过 Keymap 配置与实机确认；
- `Up/Down`：视觉行；
- 所有快捷键经 Keymap 统一配置，不能散落在 GPUI Event handler。

### EB-016：跨 Block 选择（MUST）

选区可以从标题跨过列表进入代码块；逻辑选区仍是全篇连续 Source Range。渲染层按可见 VisualLine 输出多个矩形，剪贴板依据复制模式序列化。

### EB-017：滚动不改变光标（MUST）

用户滚动仅改变 viewport，不自动重定位 Source Selection。键入时再按需要滚动到光标。

### CT-013 至 CT-024：导航验收

| ID | 场景 | 预期 |
|---|---|---|
| CT-013 | `a😀b` 逐次左右移动 | 不进入 emoji 的 UTF-8 内部 |
| CT-014 | `e\u{301}x` | 组合字素作为一个导航单元 |
| CT-015 | 家庭 emoji ZWJ | 不拆开 emoji cluster |
| CT-016 | Arabic + English | 视觉左右导航稳定 |
| CT-017 | Bidi 边界双 Affinity | CaretStop Graph 正确区分 |
| CT-018 | 软换行后 Up/Down | preferred_x 保留 |
| CT-019 | Shift+Left 后 Shift+Right | 选区收缩、方向正确 |
| CT-020 | 跨三个 Block 拖选 | Source Range 连续 |
| CT-021 | 点击 Strong 边界 | 点击前后 Anchor 相同 |
| CT-022 | 双击中文/英文混排 | Unicode word boundary 正确 |
| CT-023 | 三击段落 | 遵循确定的段落分隔符策略 |
| CT-024 | 手动滚动后停留 | 不改变 Selection |

---

## 5. 插入、删除与换行

### EB-020：普通输入的唯一入口（MUST）

平台 `KeyDown` 不直接插入普通字符；文本提交来自 GPUI/macOS 文本输入回调。一个按键最多生成一次 committed 输入事务。快捷键和非文本命令走独立分发路径。

### EB-021：选区替换（MUST）

输入文本时，若存在非折叠选区，替换完整 Source Selection；多区间操作基于同一个旧 Revision，并从后向前提交。选区末端定位到新文本之后，Anchor Bias 必须一致。

### EB-022：Backspace/Delete 的最小语义（MUST）

无选区时删除前/后一个**合法 grapheme**；有选区时删除 Source Selection。隐藏 Inline 边界处先展开可编辑语法，再由 Source Anchor 执行删除，不能悄悄删除远端 delimiter。结构化 Block 边界采用明确的结构命令（如空列表退出）或源码入口；不能套用 Inline reveal 来逐字删除隐藏的块前缀。

### EB-023：删除配对标记（SHOULD）

如果光标明确位于 `**` 源码标记内部，Backspace 按源码字素逐步删除；不能自动同时删除另一端 `**`，除非有明确的“成对删除”模式且能安全识别节点。第一版不启用隐式配对删除。

### EB-024：Return 与 Block 结构（SHOULD）

按第十一份的模式与块类型分发；Enter 首先交给 IME，确认候选不能同时拆块：

- Hybrid 普通段落：Return 拆分为两个段落，写入合法 Markdown 段落边界；Shift+Return 写入两个尾随空格及换行，形成硬换行。
- 局部/全文 Source：Return 插入单个 `\n`，Shift+Return 不隐式改写 Markdown 语法。以下结构快捷行为仅用于 Hybrid 结构化内容。
- 列表非空项：继承相同类型的 marker 与缩进。
- 空列表项：退出当前列表层级；保留用户可撤销的完整源码变更。
- 引用块非空行：继承 `> `；空引用行：退出引用。
- fenced code：插入普通换行并保持代码行缩进，不自动关闭已有围栏。
- Heading：RawDraft 中 Return 确认标题；结构化标题中 Return 创建后续普通段落，补齐合法边界，不继承 `# `。
- Table（H2）：合法 delimiter row 后 Return 确认网格；单元格内 Return/Tab 走表格命令，具体规则须先补齐 H2 fixture。

复杂 CommonMark lazy continuation、嵌套列表与多行缩进由结构编辑器基于 SyntaxSnapshot 决策，不用正则匹配行首。

### EB-025：Tab / Shift+Tab（SHOULD）

在列表中缩进/反缩进当前项或选中项组，作为单一可撤销事务；在代码块中按用户设置插入缩进（空格或 Tab）。普通段落的 Tab 行为应由编辑器设置明确决定，避免与焦点导航冲突。

### EB-026：自动配对符号（MAY）

`*`、反引号、括号和链接配对属于增强功能。默认不自动插入会改变 Markdown 语义的复杂配对；如果启用，应保存配对状态并支持 Undo、IME、粘贴与选区替换，不可凭附近字符盲删。

### CT-025 至 CT-036：编辑验收

| ID | 操作 | 预期 |
|---|---|---|
| CT-025 | 在普通文本输入 `a` | 一次事务、一次插入 |
| CT-026 | 选中 `bc` 输入 `X` | 只替换选择范围 |
| CT-027 | Backspace 删除 emoji | 删除完整 grapheme |
| CT-028 | Backspace 删除组合字素 | 不留孤立 combining mark |
| CT-029 | 隐藏 `**` 边界 Backspace | 先呈现明确编辑位置，无隐式远端删除 |
| CT-030 | Hybrid 段落 Return / Shift+Return，Source Return | 分别为段落边界、硬换行、单个源码换行；可原子撤销 |
| CT-031 | 非空列表 Return | 新项 marker 与缩进正确 |
| CT-032 | 空列表 Return | 退出列表，Undo 一步恢复 |
| CT-033 | Quote Return | 引用延续或退出符合规则 |
| CT-034 | Heading Return | 新段落非标题 |
| CT-035 | 选中多列表项 Tab | 一次事务、所有项结构正确 |
| CT-036 | 多光标同时输入 | 旧版本区间无重叠且稳定提交 |

---

## 6. 格式化与链接交互

### EB-030：格式化操作是 AST-aware Transaction（MUST）

`Cmd+B`、`Cmd+I`、删除线等不能通过字符串 `find("**")` 猜测语法。选中内容后加标记或移除已配对 marker，必须由精确 AST marker ranges 驱动。

### EB-031：非空选区 ToggleStrong（SHOULD）

- 未加粗：在 Source Selection 两端插入 `**`，选择范围保持指向内容本身。
- 已完整加粗：删除精确成对 `**`，内容不变。
- 部分交叠：不尝试危险的跨节点自动改写；应按确定的语义归一化策略处理，无法保证时拒绝结构性切换并保持源码。

### EB-032：空选区 ToggleStrong（SHOULD）

默认采用**待输入格式状态**（pending mark），下一次正常输入时产生合法 Strong 源码；按下快捷键本身不立即插入 `****`。该状态不是 Document 内容，不设置 Dirty，不进入 Undo。光标移动、取消、IME 开始时有明确状态转换。

> 待输入格式是较复杂的产品决策。实现前应与“立即插入成对标记”进行交互原型对比，并以 ADR 锁定；若实现成本不是约束，仍应以用户体验与语义一致性决定。

### EB-033：链接普通点击用于编辑（SHOULD）

单击链接文字定位光标；`Cmd+Click` 或上下文菜单打开链接。悬停可以展示目标 URL。默认不因单击就跳出编辑器。

### EB-034：链接地址编辑（SHOULD）

进入 Link label 时展开链接源码，包括 `[]()` 和 destination；编辑 URL 不应改变 label 的 Source Anchor，除非编辑操作实际涉及它。

### EB-035：不支持语法安全保留（MUST）

未知扩展、无法确认 marker ownership 的嵌套语法、部分损坏链接均保留原文，禁止格式化命令无提示地“修复”或删除内容。

### CT-037 至 CT-045：格式化验收

| ID | 初始条件 | 操作 | 预期 |
|---|---|---|---|
| CT-037 | `hello` 全选 | Cmd+B | `**hello**`，内容选区不含 marker |
| CT-038 | `**hello**` 内容全选 | Cmd+B | `hello`，Undo 可恢复 |
| CT-039 | `**a *b* c**` | 在 `b` 切换格式 | AST-aware，无错配 marker |
| CT-040 | `hello` 空选区 | Cmd+B | Source 不变，pending mark 激活 |
| CT-041 | pending strong | 输入 `x` | 生成合法 Strong 源码 |
| CT-042 | `[label](url)` | 单击 label | 不自动打开链接 |
| CT-043 | `[label](url)` | Cmd+Click | 打开经过验证的链接目标 |
| CT-044 | `[label](url)` | 编辑 URL | label Source Anchor 正确迁移 |
| CT-045 | 不完整 `**abc` | Cmd+B | 不执行危险 marker 猜测 |

---

## 7. 复制、剪切与粘贴

### EB-040：默认复制 Markdown（MUST）

**用户已确认默认复制 Markdown。** `Cmd+C` / `Copy as Markdown` 序列化选中内容的 Markdown 片段，保留受支持的格式和链接目标；结果不依赖 Selection 引发的 marker 展开，也不直接复制 Projection 字符串。这是本产品决策，不是对 Typora 剪贴板行为的推断。

- 仅选择格式节点的内容或部分内容：按 AST 保留必要祖先样式，在输出片段中补齐成对标记。例如 `**hello**` 中选择 `ell`，输出 `**ell**`；只选择链接标签 `label`，输出 `[label](url)`。
- 选区已经包含完整节点与其 marker：保留原有语法，不重复包裹。全选文档时复制完整 Markdown 源码，不改写无关格式、空白或换行。
- 引用式链接的内容片段：序列化为带已解析目标和可选 title 的行内链接，避免只复制标签而丢失定义；全选文档则保留原始引用写法和定义。
- 内容选区跨越多个节点：保留选中内容对应的格式嵌套与块边界，不带入未选中的正文。需要合成 delimiter 时验证片段语义；无法可靠保留语义则回退为精确源码切片。
- 选区端点切入 marker、URL、title 等语法编辑区域，或涉及未知/不完整语法：回退为精确 Source Selection 切片，不自动修复、不扩展选择。这一判断基于源码范围与 AST，不基于标记当前是否可见。
- **Copy Source** 是独立的精确源码复制命令：始终复制 Source Selection 原始字节，不补齐格式。可选 **Copy as Plain Text** 根据语义生成无格式文本，必须显式调用。
- `text/plain` 剪贴板载荷也写入同一 Markdown 字符串；平台支持时同时提供等价 `text/markdown`。首版默认复制不依赖 HTML/RTF 或主题样式导出。
- 空选区复制不修改剪贴板；任何复制都不修改 Source、Selection、Dirty 或 Undo。

Clipboard Serializer 消费同版本 Source、Syntax 和 Selection；缺少可靠语义快照时回退为源码切片。精确源文本保真由 Document 和 Copy Source 保证，合成的 Markdown 内容片段以语义保留为目标。

### EB-041：剪切与复制一致（MUST）

`Cmd+X` 默认按 EB-040 生成 Markdown 剪贴板，再对原 Source Selection 执行一次删除事务。复制片段时补齐的标记不扩大删除范围，也不触发选区外的语法清理。剪贴板写入失败时不删除文档；空选区不删除、不覆盖剪贴板。Undo 精确恢复剪切前源码与选区；剪切后的语义 Markdown 再粘贴不保证逐字节恢复原始写法。

### EB-042：纯文本粘贴（MUST）

`text/plain` 按普通输入插入。包含 `**` 的粘贴内容是 Markdown 源码文本的一部分，随后由 Parser 决定语义，不做额外不可撤销转换。

### EB-043：Markdown 粘贴（SHOULD）

若剪贴板提供 `text/markdown`，默认优先使用；否则读取 `text/plain`，按源码原样插入。自身复制的两种载荷包含相同 Markdown，不能再次添加格式标记或重复转义。显式“粘贴为纯文本”只选择 plain 载荷，不解析并剥除其中的 Markdown；自身复制的 plain 载荷本身就是 Markdown 字符串。

### EB-044：HTML 粘贴（MAY）

富文本 HTML→Markdown 转换必须是独立、可测试、可禁用的模块。不能执行 HTML 中脚本；远程资源不应未经用户允许自动下载。转换后的插入作为一个 Undo group。

### EB-045：换行与文件序列化（MUST）

复制粘贴不得静默修改无关段落的换行或文件编码。保存时保留约定的 BOM/EOL 策略；混合 EOL 需要明确保真测试。

### CT-046 至 CT-053：剪贴板验收

| ID | 场景 | 预期 |
|---|---|---|
| CT-046 | `**hello**` 仅选中 `hello`，Cmd+C | 输出 `**hello**`，显隐变化不影响结果 |
| CT-047 | 相同内容选区，Copy Source | 精确输出 `hello`；可选 Copy as Plain Text 也输出 `hello` |
| CT-048 | Cmd+X 且剪贴板写入失败 | 源码不变 |
| CT-049 | 粘贴多行 Markdown | 一次 Undo 可撤销 |
| CT-050 | 粘贴 Unicode emoji | Grapheme 边界正确 |
| CT-051 | 粘贴 HTML | 受控转换，不执行脚本 |
| CT-052 | 粘贴到链接 label | Source Range 正确 |
| CT-053 | CRLF 文件中粘贴 LF | 保存遵循明确 EOL 策略 |

### CT-095 至 CT-101：Markdown 默认复制补充验收

编号 CT-094 保留给第十三节已有 DSL 样例。

| ID | 场景 | 预期 |
|---|---|---|
| CT-095 | `**hello**` 仅选 `ell` | Cmd+C 输出 `**ell**`；Source/Selection/Undo 不变 |
| CT-096 | `[label](url)` 仅选 `label` | 输出 `[label](url)`；marker 显隐不影响结果 |
| CT-097 | 同一文档全选后 Cmd+C | 完整源码逐字节复制，嵌套 marker 不重复添加 |
| CT-098 | 选择 `**hello**` 开标记中的一个 `*` | 回退源码切片 `*`，不补齐或扩大范围 |
| CT-099 | `**hello**` 仅选 `ell` 后 Cmd+X | 剪贴板 `**ell**`，源码 `**ho**`；Undo 精确恢复 |
| CT-100 | 自身复制后粘贴到空文档 | plain 载荷含 Markdown；支持时 markdown 载荷相同，粘贴不重复包裹 |
| CT-101 | `[label][id]`，定义 `[id]: https://example.com`，仅选 `label` | 输出 `[label](https://example.com)`；目标来自同版本已解析定义 |

---

## 8. Undo / Redo 行为

### EB-050：语义事务撤销（MUST）

Undo 恢复 Source、Selection 的 anchor/head 及方向；Redo 重新应用。每次 Undo/Redo 都产生新的单调递增 DocumentRevision。

### EB-051：合并输入组（SHOULD）

连续普通文字输入可合并；光标移动、鼠标点击、格式化、粘贴、跨 Block 结构编辑、IME 提交都应形成明确边界。合并不仅依赖时间阈值，还依赖连续性和命令类型。

### EB-052：显隐不进入历史（MUST）

仅移动光标、展开/隐藏 Markdown marker、滚动、主题切换、caret blink 不得增加 Undo 深度。

### EB-053：Undo 后新编辑清空 Redo（MUST）

Undo 后执行新的 Source Transaction，丢弃原 Redo 分支；单纯移动光标不清空 Redo。

### EB-054：格式化和列表结构操作原子撤销（MUST）

一次 Cmd+B 的两端 marker 插入、一次多行缩进、一次退出空列表项，均应在一次 Undo 中完全恢复。

### CT-054 至 CT-061：历史验收

| ID | 场景 | 预期 |
|---|---|---|
| CT-054 | 连续输入 `abc` | 合理合并为一次 Undo |
| CT-055 | 输入后移动光标再输入 | 两个历史组 |
| CT-056 | Cmd+B 添加双 marker | 一次 Undo 全部撤销 |
| CT-057 | 多行粘贴 | 一次 Undo |
| CT-058 | IME 多次 marked 更新后提交 | 一次 Undo |
| CT-059 | 光标进入/退出 Strong | Undo 深度不变 |
| CT-060 | Undo 后移动光标 | Redo 仍存在 |
| CT-061 | Undo 后输入新内容 | Redo 清空 |

---

## 9. macOS IME 与 Composition Overlay

### EB-060：IME 组合态不污染 committed Rope（MUST）

采用第三份方案的 Composition Overlay。marked text 更新只改变临时呈现；提交时产生一次正式 Source Transaction；取消时恢复原视图且不改变 Source/Dirty/Undo。

### EB-061：组合范围与 UTF-16 转换（MUST）

所有系统 UTF-16 range 显式转换。遇到 surrogate pair 中间偏移，按定义的 BoundaryPolicy 处理，不能把 UTF-16 数字当 SourceByte。

### EB-062：组合文本始终可见（MUST）

IME overlay 的字符不得被任何显隐策略吞掉；Inline 组合范围锁定可见决策，源码模式中 marker 保持可见。Structured Block 的外层前缀仍保持结构呈现，不因组合展开；实际组合的语法候选安全原文呈现。候选框跟随真实 shaped caret geometry。

### EB-063：组合期间编辑命令（MUST）

平台输入法有机会优先处理 Backspace、方向键、候选切换；引擎不得抢先执行普通 Source 删除。由平台协议决定提交、更新或取消，状态机串行处理。

### EB-064：失焦、保存与关闭（SHOULD）

失焦和关闭必须显式请求平台结束组合或按确认过的系统行为处理；保存操作不得悄悄遗漏屏幕上可见但未提交的 marked text。若不能安全提交，阻止保存并反馈用户，不允许猜测。

### EB-065：Composition 的 Undo（MUST）

一个已提交的输入法组合通常对应一个撤销组；取消组合不产生历史条目。若输入法产生多个独立提交事件，则按平台真实提交边界记录，不能人为合并不相关会话。

### CT-062 至 CT-075：IME 验收

| ID | 场景 | 预期 |
|---|---|---|
| CT-062 | 拼音输入多个候选更新 | committed Source 不变 |
| CT-063 | 拼音候选提交 | 一次 Source Transaction |
| CT-064 | 取消组合 | Source、Undo、Dirty 不变 |
| CT-065 | 日文假名→汉字转换 | marked range/候选框正确 |
| CT-066 | 韩文音节组合 | 不拆开 grapheme |
| CT-067 | 英文 dead key | 只插入最终正确文本 |
| CT-068 | Emoji surrogate pair | UTF-16 ↔ UTF-8 正确 |
| CT-069 | 在 Strong 内容中组合 | 语法标记不遮挡 marked text |
| CT-070 | 在隐藏 marker 边界组合 | Source Anchor 与 overlay 一致 |
| CT-071 | 组合中 Backspace | 不与普通删除重复执行 |
| CT-072 | 组合中滚动/窗口 resize | 候选框位置更新 |
| CT-073 | 组合中切换焦点 | 状态按平台协议安全结束 |
| CT-074 | 组合中保存 | 不遗漏可见 marked text |
| CT-075 | 提交后 Undo/Redo | 组合一次撤销，恢复选区 |

---

## 10. 文件打开、保存与冲突

### EB-070：打开单一 `.md` 文件（MUST）

保留 Markdown 源码语义、编码信息、BOM、换行策略及外部文件身份。无法按预期解码时不能静默丢弃非法字节；应提示编码问题或以安全只读方式打开。

### EB-071：Dirty 与 SavePoint（MUST）

Dirty 取决于当前可序列化内容是否等于最近成功保存的内容，而非简单 `revision != saved_revision`。Undo 恢复到保存状态时 Dirty 应消失。

### EB-072：原子保存（MUST）

以不可变 DocumentSnapshot 写入同目录临时文件，按所选文件系统策略 flush/fsync、原子替换并维护权限/元数据策略。保存期间若继续编辑，只能将对应快照标记为 saved，不得错误地把更新后内容标记已保存。

### EB-073：外部修改冲突（MUST）

文件外部变化且编辑器也有未保存修改时，不能静默覆盖磁盘。提供重新载入、保留本地、另存为、比较/合并等受控路径；具体 UI 可迭代，但不得无提示丢数据。

### EB-074：自动保存（SHOULD）

建议支持可配置自动保存；触发时须协调 IME、文件锁/身份与冲突检测。自动保存不是绕过冲突检查的理由。

### CT-076 至 CT-084：文件验收

| ID | 场景 | 预期 |
|---|---|---|
| CT-076 | UTF-8 BOM 文件 | 打开/保存策略保真 |
| CT-077 | CRLF 文件 | 往返保真 |
| CT-078 | 混合 EOL 文件 | 按明确策略保真或提示 |
| CT-079 | 保存后 Undo 回保存内容 | Dirty 正确 |
| CT-080 | 保存中继续输入 | 不把新内容误标记为已保存 |
| CT-081 | 保存写入失败 | 原文件与 Dirty 保留 |
| CT-082 | 外部修改 + 本地 Dirty | 不静默覆盖 |
| CT-083 | 外部修改 + 本地 Clean | 安全重新载入/提示 |
| CT-084 | IME 组合中触发保存 | 遵循 EB-064 |

---

## 11. 焦点、鼠标、滚动与辅助功能

### EB-080：单一逻辑编辑焦点（MUST）

一个 Document 对应一个编辑焦点与统一 SelectionSet；不同 Block 不是互不相干的 TextInput。窗口失焦不丢失 Source Selection。

### EB-081：滚动锚定（MUST）

光标附近显隐、窗口宽度变化、离屏 Block 实测高度修正都通过 Source-backed ScrollAnchor 调整。主动滚动时不强制跳回光标。

### EB-082：无障碍文本范围（MUST）

Accessibility 必须有统一的文本坐标模型；所报告的选区与 range bounds 使用相同版本的布局。若平台无法直接暴露某个离屏范围，按需布局或返回明确不可见状态，不能伪造矩形。

### EB-083：VoiceOver 与语义（SHOULD）

标题、列表、链接、代码块的语义来自 SyntaxSnapshot；文本几何来自 LayoutSnapshot。VoiceOver 可按文本和语义导航，且不会触发意外源码修改。

### EB-084：大文档虚拟化（MUST）

只布局可视范围及 overscan；单个超长 Block 必须支持进一步按 VisualLine/chunk 虚拟化。视口外 Block 不可因缺少绘制对象而丢失逻辑选区或 Accessibility 访问能力。

### CT-085 至 CT-093：UI/Accessibility 验收

| ID | 场景 | 预期 |
|---|---|---|
| CT-085 | 点击后 Strong 展开 | 不发生二次 hit-test 漂移 |
| CT-086 | 拖选跨 Block | 固定端 Source Anchor 稳定 |
| CT-087 | 软换行引起高度变化 | caret 保持可见 |
| CT-088 | 主动滚动 | Selection 不变 |
| CT-089 | 离屏 Block 高度修正 | 阅读锚点稳定 |
| CT-090 | VoiceOver 读取标题 | 语义与文字正确 |
| CT-091 | Accessibility 请求离屏 range | 按需布局/安全返回 |
| CT-092 | 单个超长段落 | 无界全量布局被禁止 |
| CT-093 | Retina DPI 切换 | 命中与候选框几何正确 |

---

## 12. 冲突情境优先级矩阵

| 冲突 | 优先级规则 | 禁止行为 |
|---|---|---|
| IME vs 普通按键 | IME 协议优先 | 重复插入 |
| Selection vs Visibility | Source Selection 优先 | 根据新像素重定位 |
| Syntax vs 不完整输入 | 源码可编辑优先 | 猜测隐藏 |
| Undo vs Projection | Undo 仅恢复源码事务 | 视觉显隐入历史 |
| Save vs IME Overlay | 显式协调提交 | 静默漏存可见文本 |
| External file vs Dirty | 数据保护优先 | 静默覆盖 |
| Async Layout vs 新 Revision | 最新一致快照优先 | 旧几何写入新文档 |
| Scroll Anchor vs caret | 按主动操作选择策略 | 滚动时强制回光标 |
| Performance vs correctness | 正确性优先 | 错误复用缓存 |
| Accessibility vs 虚拟化 | 按需查询逻辑文档 | 把未渲染当不存在 |

---

## 13. 可执行测试 DSL 设计

建议测试仓库建立 `tests/behavior/*.yaml`，配套 Rust fixture runner。DSL 使用强类型 schema 验证，不允许未知字段静默忽略。

```yaml
schema_version: 1
id: CT-094
title: "Cursor reveal is non-destructive"
platform: portable
initial:
  source: 'ab**cd**ef'
  selection: { anchor_byte: 0, head_byte: 0, affinity: downstream }
  saved_source: 'ab**cd**ef'
steps:
  - action: set_selection
    anchor_byte: 6
    head_byte: 6
    affinity: downstream
  - assert:
      source: 'ab**cd**ef'
      visible_text: 'ab**cd**ef'
      undo_depth: 0
      dirty: false
  - action: set_selection
    anchor_byte: 0
    head_byte: 0
    affinity: downstream
  - assert:
      source: 'ab**cd**ef'
      visible_text: 'abcdef'
      undo_depth: 0
      dirty: false
```

### 13.1 事件类型

```rust
enum BehaviorAction {
    SetSelection { anchor: SourceByte, head: SourceByte, affinity: VisualAffinity },
    InsertText(String),
    DeleteBackward,
    DeleteForward,
    Move { direction: MoveDirection, unit: MoveUnit, extend: bool },
    ToggleStrong,
    ToggleEmphasis,
    Return { shift: bool },
    EditBlockSource { block: BlockId },
    ExitBlockSource,
    SetDocumentViewMode(DocumentViewMode),
    SetHeading { level: Option<u8> },
    ToggleTask { item: BlockId },
    TableCommand(TableEditCommand), // H2
    BeginComposition { replacement: SourceRange },
    UpdateComposition { text: String, selected_utf16: Range<Utf16Unit> },
    CommitComposition { text: String },
    CancelComposition,
    Undo,
    Redo,
    Copy { mode: CopyMode },
    Paste { mime: MimeType, data: String },
    Resize { width: f32, height: f32 },
    ScrollTo { y: f32 },
    Save,
}
```

### 13.2 Assertion 分类

- **Document**：源码、revision、dirty、savepoint、外部文件状态。
- **Selection**：anchor/head、方向、affinity、preferred_x。
- **Projection**：显示文本、Inline marker 显隐、Source/Display 映射。
- **Hybrid**：Document/Block 模式、结构装饰、源码入口、类型相关触发；表格二维导航仅在 H2 启用。
- **History**：undo/redo depth、合并组、逆向可恢复。
- **Geometry**：caret rect、hit-test、selection fragments、scroll anchor。
- **IME**：composition state、marked range、UTF-16 映射、候选框。
- **Accessibility**：可见文本、选区、range bounds、语义树。
- **Safety**：失败操作不得修改源码，过期结果不得提交。

### 13.3 测试层次

| 层次 | 运行环境 | 重点 |
|---|---|---|
| Unit | 任意 CI | ChangeMap、Anchor、Selection、History |
| Property | 任意 CI | 随机事务、往返、映射单调性 |
| Syntax Differential | 任意 CI | 增量解析与全文解析等价 |
| Behavior DSL | 任意 CI | 上述 EB 契约 |
| Geometry Golden | 固定字体/布局环境 | Caret、Bidi、wrap、hit-test |
| Native Integration | macOS runner | GPUI 输入/焦点/剪贴板 |
| Manual Platform Matrix | macOS 实机 | 中日韩 IME、VoiceOver、Retina |
| Performance | 固定基准机 | P50/P95/P99、内存、虚拟化 |

---

## 14. 属性测试与 Fuzzing

### P-001：编辑可逆性

对任意合法 Unicode 源码和有效 ChangeSet：`undo(apply(doc, change))` 在文本与约定的选区状态上等价于原状态。

### P-002：投影不改变源码

任意 Selection、InlineVisibilityPolicy、BlockPresentationPolicy、源码视图或 Viewport 的纯呈现变化，`DocumentSnapshot` 字节完全不变，Dirty/Undo 不变。用户实际输入和结构命令仍产生正常事务。

### P-003：源码映射合法

每个可见 Display caret stop 必须能解析到合法 Source grapheme boundary；对于隐藏 marker 的多对一映射，Affinity 规则确定且不越界。

### P-004：增量解析等价

对任意随机编辑序列，增量 SyntaxSnapshot 的规范化语义与全文解析一致；marker range 归属准确。

### P-005：历史与保存

输入→保存→输入→Undo 回保存内容时 Dirty 为 false；Redo 后为 true。保存中发生新输入不影响 SavePoint 对应快照。

### P-006：IME overlay 隔离

任意 marked text 更新序列在 commit 前不修改 committed Source；cancel 后观察状态恢复；commit 恰好生成一个符合平台提交边界的正式事务。

### P-007：几何往返

在明确的 Affinity 和同一 LayoutSnapshot 下，`source_anchor → caret_rect → hit_test` 的结果在定义的等价类内一致。

### P-008：异步结果安全

任何旧 Revision 的 Syntax/Projection/Layout 异步结果不能覆盖新状态。

Hybrid 模式另验证 Source anchors 与模式切换；表格 cell graph 只要求定义的命中/导航往返，不沿二维视觉路径要求源码字节单调。

建议 Fuzz targets：

```text
fuzz_document_transactions
fuzz_anchor_change_map
fuzz_selection_navigation
fuzz_projection_mapping
fuzz_incremental_markdown
fuzz_ime_composition_events
fuzz_undo_redo_savepoint
fuzz_layout_snapshot_races
```

随机测试失败必须自动输出最小复现：初始源码、完整事件序列、Revision、选区、方言和测试 seed。

---

## 15. 真实 macOS 测试矩阵

| 维度 | 覆盖 |
|---|---|
| 输入法 | 简体拼音、日文假名/汉字、韩文、英文 dead key、emoji |
| 文本 | ASCII、CJK、RTL、组合附加符、ZWJ emoji、长 URL |
| 文件 | LF、CRLF、混合 EOL、BOM、超长文件、外部修改 |
| 字体 | 系统默认、CJK fallback、emoji fallback、等宽代码字体 |
| 屏幕 | Retina、缩放、窗口 resize、跨显示器 |
| 操作 | 键盘、触控板、鼠标拖选、双击/三击、系统剪贴板 |
| 辅助功能 | VoiceOver、文本范围、可见范围、语义导航 |
| 性能 | 大量 Block、单个超长 Block、快速滚动、连续输入 |

每次平台回归记录：macOS 版本、GPUI commit、字体、输入法、屏幕比例、失败事件序列。不要用模拟输入法测试代替系统真实组合行为。

---

## 16. 产品决策与确认状态

用户已确认 Hybrid View 路线与默认 Markdown 复制；本项目的细化规则记录于本文及第十一份 HV-001—006，并不声称完全复刻 Typora。未标注确认的项目仍是建议默认，需交互原型或系统实测锁定；所有规则仍需实现验收。

| ADR | 决策 | 本文建议默认 |
|---|---|---|
| ADR-021 | 非折叠选区显隐 | 仅展开相交 Inline 及 Inline 祖先；拖选冻结，外层结构保持 |
| ADR-022 | 结构性 Block marker | **Hybrid 路线已确认**：正文聚焦保持结构；块源码入口显式编辑，见 HV-002 |
| ADR-023 | Setext 标记 | H1 确认语义后结构化；源码入口修改 underline，见 HV-003 |
| ADR-024 | 空选区 Cmd+B | Pending Mark |
| ADR-025 | 默认 Cmd+C | **用户已确认：Markdown**；保留内容格式，另设 Copy Source，Plain Text 为显式辅助命令 |
| ADR-026 | 单击链接 | 定位光标，Cmd+Click 打开 |
| ADR-027 | 三击 | 选逻辑段落，不含分隔符 |
| ADR-028 | 隐藏 marker 边界删除 | Inline 先展开再删除；Block 走结构命令/源码入口 |
| ADR-029 | IME 中保存/关闭 | 先请求安全提交，失败阻止 |
| ADR-030 | 自动保存 | 可配置且冲突安全 |
| ADR-031 | 混合换行保真 | 优先逐行保留或提示 |
| ADR-032 | Tab 在普通段落 | Keymap/设置明确决定 |
| ADR-033 | 原生 Home/End | 以 macOS 习惯实测确定 |
| ADR-034 | 富文本 HTML 粘贴 | 独立可禁用转换器 |

**禁止**在产品决策未确定时让不同模块各自实现不同默认。ADR 一旦变更，必须更新 Behavior DSL fixture 与文档版本。

---

## 17. 验收门槛与发布阻断规则

### Gate B1：核心编辑

必须通过 CT-013–036、CT-054–061 和 P-001；任何 Unicode 截断、事务不原子、Undo 损坏都阻断后续集成。

### Gate B2：Live Markdown

H1 必须通过 CT-001–012、CT-037–045、P-002–004，以及第十一份 CT-102–114；H2 增加 CT-115–117 和表格 IME/AX 回归。任何语法误隐藏、结构聚焦时意外退回源码、光标漂移或错误 marker ownership 都阻断相应阶段发布。

### Gate B3：剪贴板与文件

必须通过 CT-046–053、CT-076–084、CT-095–101、P-005；任何静默丢失原文、错误保存状态或覆盖外部文件都阻断发布。

### Gate B4：原生输入

必须通过 CT-062–075 和 P-006；必须在真实 macOS 环境覆盖中文、日文、韩文、dead key 与 emoji。不能只凭 mock 通过。

### Gate B5：原生布局与辅助功能

必须通过 CT-085–093 和 P-007–008；Bidi、软换行、候选框、VoiceOver 的错误几何须修复。

### Gate B6：性能与可靠性

在明确的目标机器上建立 P50/P95/P99 基线；百万字与极端长 Block 不能出现无界内存增长、长时间主线程阻塞或旧布局提交。

**零容忍阻断**：源码丢失、静默覆盖、IME 重复输入、非法 Unicode 边界、Undo 不可逆、过期快照编辑新文档、错误隐藏不可修复语法。

---

## 18. 推荐的实施顺序

1. 实现 `BehaviorAction`、`ObservableEditorState`、Fixture Runner；先有可执行契约。
2. 接入 Document/Anchor/Transaction/History，完成 Gate B1。
3. 接入 Syntax、InlineVisibilityPolicy、BlockPresentationPolicy、FlowBlocks、Projection 与源码模式，按 H1/H2 完成 Gate B2。
4. 接入 Clipboard/FileSession，完成 Gate B3。
5. 接入 Composition Overlay/GPUI Native Input，完成 Gate B4。
6. 接入 Shaping/HitTest/Viewport/Accessibility，完成 Gate B5。
7. 完成大文档性能与跨版本回归，完成 Gate B6。

测试体系应作为开发输入，而非最后一轮 QA 补丁。每个新行为必须先增加 EB 编号和对应 fixture，再进入实现。

---

## 19. 与前七份文档的关系

| 文档 | 主要责任 |
|---|---|
| 第一份 | 产品与整体架构 |
| 第二份 | Projection、DisplayMap、Block Layout |
| 第三份 | Document、Anchor、Transaction、Undo、IME |
| 第四份 | Markdown Syntax Engine 与增量 AST |
| 第五份 | GPUI 原生渲染、虚拟化、Accessibility |
| 第六份 | 工程实施、GPUI API 核验、CI、风险 |
| 第七份 | 核心算法与数据结构 |
| **第八份（本文）** | **用户可观察行为、交互冲突决策、端到端测试与发布验收** |

整体闭环：

```text
User Action / macOS Event
        ↓
Behavior Spec (EB-xxx)
        ↓
Editor Engine / Composition
        ↓
Committed Source Transaction
        ↓
Syntax → Hybrid policies / FlowBlocks → Projection → Layout
        ↓
GPUI Paint / IME / Accessibility
        ↓
Observable State Assertions (CT-xxx)
```

## 20. 最终结论

高质量 Live Markdown 编辑器的标准不只是“Markdown 渲染正确”，而是用户可以放心地进行任何编辑：光标进入隐藏语法、跨行选择、Unicode 删除、输入法候选、Undo/Redo、粘贴、保存和辅助功能操作，都不会改变自己没有明确要求改变的内容。

**第八份文档把前七份架构的抽象正确性转换为明确的交互行为和测试契约。** 在正式开发时，应将本文的 EB/CT 编号直接映射到 Rust 集成测试与 macOS 原生回归测试，并通过 ADR 锁定尚存的产品选择。

> 下一份可聚焦《GPUI macOS 真实 API 对照与最小可编译原型》，前提是联网核验并锁定 GPUI 具体 Git commit；它应提供真实可编译代码，而不再只是接口草案。
