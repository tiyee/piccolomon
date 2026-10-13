# GPUI Live Markdown — 第四份：Markdown Syntax Engine 深度架构与实现规范

> 2026-10-12 Hybrid 修订：交互路线与阶段范围以[第十一份](./11_GPUI_Typora_Hybrid_View_Spec.md)为准。Inline 按光标显隐；Block 保持结构化，显式进入局部/全文源码；模式切换不改源码或历史。下文领域 API 为设计草案，须纳入 Hybrid presentation 版本依赖后实施。

> **版本**：0.1 设计规格  
> **定位**：衔接前三份文档，定义增量 Markdown 解析、精确 delimiter 范围、Block/Inline 语法树、语义与源文本一致性、错误恢复及跨模块协议。  
> **目标**：纯 GPUI、自研编辑器内核、单一 Rope、Typora 风格 Hybrid View：Inline 显隐与 Block 结构化。  
> **约束**：不以开发资源为架构妥协理由；也不为了“自研”而放弃经过验证的解析算法。本文中的 Rust 代码是接口设计/伪代码，不保证可直接编译。第三方解析器的实际行为与增量能力应通过锁定版本的源码和测试确认。

---

## 目录

1. 目标、非目标与不可变约束
2. 解析器选型及推荐策略
3. 语法方言与兼容性契约
4. 解析模型：Block、Inline、Delimiter、Trivia
5. Source Range、Unicode 与换行
6. Block 解析算法
7. Inline 解析算法与分隔符配对
8. Markdown Projection 语义输出
9. 增量解析与依赖传播
10. 稳定 Node ID 与增量树复用
11. 编辑中间态、错误恢复与未闭合语法
12. 嵌套、冲突与特殊语法
13. Link、Image、Autolink 和 URL
14. List、Quote、Heading 与代码围栏
15. StyleRun、可见性与 Token 分类
16. 与 Document/Editor/IME 的事务协议
17. 性能、缓存与并发
18. 测试矩阵与属性测试
19. Rust 接口草案
20. 工作包、验收关卡与开放决策

---

## 1. 目标、非目标与不可变约束

### 1.1 目标

Markdown Syntax Engine 不只是将 Markdown 转换为 HTML AST。它必须同时回答：

- 这段源码在 CommonMark/GFM 语义上是什么？
- 哪些字节属于可见内容，哪些属于可隐藏的 Markdown 标记？
- 哪些标记是配对的？哪些是不完整的用户输入？
- 光标进入某个 Inline 时，应展开哪些标记？
- 一个源码替换后，最小需要重新解析哪些 Block 与 Inline？
- 如何把解析结果稳定地交给 Projection，而不让 UI 依赖某个解析库的私有 AST？

### 1.2 硬性不变量

1. **Rope 是唯一权威文本**。AST 是派生索引，不拥有可编辑文本。
2. **所有语法范围均指向同一 DocumentRevision 的 UTF-8 源码字节**。
3. **不能凭正则猜测成对标记**。尤其是 `*`、`_`、链接、转义、代码跨度与嵌套。
4. **源码必须可无损恢复**。AST/Projection 不得修改原文空格、换行或标点。
5. **语法不完整时优先显示原文**，不允许误隐藏导致用户无法修复。
6. **解析失败不阻止输入**，可回退到该范围的 RawText。
7. **语义解析与标记范围归属必须一致**，不能两个解析器分别认定不同节点。
8. **解析与 UI 不互相修改状态**。Parser 只消费快照并返回版本化结果。
9. **IME overlay 是临时呈现输入**，不能直接混入已提交的 AST。
10. **不允许使用过期解析结果覆盖新文档**。

### 1.3 非目标

第一阶段不支持完整 HTML DOM 渲染、MDX、LaTeX 解析、Pandoc 全部扩展、富文本表格编辑器或嵌入式附件语义；但语法模型必须支持可扩展的方言模块和 Inline Widget 输出。

---

## 2. 解析器选型及推荐策略

### 2.1 候选路线

| 路线 | 优点 | 风险 | 判断 |
|---|---|---|---|
| 完全自研 CommonMark/GFM parser | 语法范围与增量机制完全可控 | 规范复杂，极易在 delimiter/list/link 细节出错 | 不建议从零实现全部规范 |
| `tree-sitter-markdown` 作为主解析器 | 树节点与增量编辑接口适合交互式场景 | 需验证语法覆盖、节点范围、GFM 方言和错误恢复 | 候选主引擎 |
| `pulldown-cmark` 作为主解析器 | CommonMark 语义成熟，事件模型清晰 | 需验证增量解析与 delimiter 精确标记输出 | 候选语义基准 |
| 两套解析器同时控制 AST | 可相互补充 | 分歧导致语义与显隐不一致 | **禁止** |
| 主解析器 + 独立 conformance oracle | 能校验规范兼容性 | 测试工程更多 | **推荐** |

### 2.2 推荐架构：一个权威 Parser + 一个验证 Oracle

**推荐以增量解析能力和源码范围精度为准选定单一权威解析器**，并构建 `SyntaxAdapter` 统一输出自己的 AST。

实际落地的优先调查顺序：

1. 锁定 `tree-sitter-markdown` 的具体 Rust crate/grammar commit。
2. 验证 Block 和 Inline 是否分别解析、编辑后如何执行增量更新。
3. 检查 `**bold**`、`***nested***`、`[x](url)`、反斜杠转义等标记是否可获得**精确源码区间**。
4. 验证 GFM task list、strike、table、autolink 等功能。
5. 若存在结构性缺口，用**受 AST 约束的 delimiter extractor**补充，不使用无上下文字符串匹配。
6. 用 `pulldown-cmark` 的规范语义输出作为差分测试 oracle；**运行时只有一个权威 AST**。

**选型关卡**：若 Tree-sitter grammar 无法可靠表示关键语义或精确范围，则不应为了增量性能勉强采用它。可选择更准确的主解析器，并自建 Block 级增量调度；正确性高于局部性能。

### 2.3 禁止的设计

```text
tree-sitter 负责找 `**` 的位置
pulldown-cmark 负责判定 Strong
两者各自产生节点，UI 临时拼接
```

这会在强调嵌套、转义、代码跨度和不完整输入时产生无法统一的语义。

允许的设计：

```text
Authoritative Parser
   → SyntaxAdapter
   → Canonical SyntaxSnapshot
   → Projection / Style / Editing Commands

Independent Oracle
   → test-only semantic comparison
```

---

## 3. 语法方言与兼容性契约

建议定义显式方言配置，而不是散落的 feature flags：

```rust
pub struct MarkdownDialect {
    pub commonmark: CommonMarkVersion,
    pub gfm_strikethrough: bool,
    pub gfm_task_lists: bool,
    pub gfm_tables: bool,
    pub gfm_autolinks: bool,
    pub footnotes: bool,
    pub wikilinks: bool,
    pub math: bool,
}
```

第一版建议以 CommonMark 为基准，启用经过测试的 GFM 常用扩展：删除线、任务列表、自动链接。表格在 H1 解析并保留原文，**H2 实现网格与单元格编辑**；未支持的扩展应显示原文而不是误解释。

### 3.1 文档级配置不可隐式漂移

- 每份 Document 绑定一个 Dialect。
- 切换 Dialect 必须递增 `SyntaxConfigRevision` 并触发重解析。
- AST、Projection 和布局快照都携带该配置版本。
- 插件方言不得直接改写源 Rope。
- 语法扩展冲突必须有明确优先级和测试。

### 3.2 规范版本与回归

建立固定的 CommonMark/GFM conformance fixture 集合，锁定规范版本与第三方 parser commit。依赖升级必须跑全量差分测试，避免编辑器升级后 Markdown 语义悄然变化。

---

## 4. 解析模型：Block、Inline、Delimiter、Trivia

### 4.1 树与索引并存

语法树需要表达嵌套关系；编辑器还需要按源码偏移快速定位节点。

推荐：

```rust
pub struct SyntaxSnapshot {
    pub document_revision: DocumentRevision,
    pub syntax_config_revision: u64,
    pub tree: SyntaxTree,
    pub block_index: BlockIntervalIndex,
    pub inline_index: InlineIntervalIndex,
    pub diagnostics: Vec<SyntaxDiagnostic>,
}
```

`SyntaxTree` 是语义树；`BlockIntervalIndex` 与 `InlineIntervalIndex` 是派生查询索引，不是第二个权威 AST。

### 4.2 Node 结构

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct NodeId(pub u64);

pub struct SyntaxNode {
    pub id: NodeId,
    pub kind: SyntaxKind,
    pub full_range: SourceRange,
    pub content_range: Option<SourceRange>,
    pub markers: Vec<SyntaxMarker>,
    pub children: Vec<NodeId>,
    pub parent: Option<NodeId>,
    pub completeness: Completeness,
}

pub enum Completeness {
    Complete,
    Incomplete,
    ErrorRecovered,
}

pub struct SyntaxMarker {
    pub range: SourceRange,
    pub role: MarkerRole,
    pub visibility_class: VisibilityClass,
}
```

`full_range` 包含节点所有源码；`content_range` 仅表示主要内容，可能不存在或由多个不连续片段构成。**链接、图片、代码围栏、列表等不应强行压成一个连续 content_range**，应支持 `content_spans: Vec<SourceRange>`。

### 4.3 SyntaxKind

```rust
pub enum SyntaxKind {
    Document,
    Paragraph,
    AtxHeading { level: u8 },
    SetextHeading { level: u8 },
    BlockQuote,
    BulletList,
    OrderedList { start: u64 },
    ListItem,
    TaskListItem { checked: bool },
    FencedCodeBlock { info: Option<String> },
    IndentedCodeBlock,
    ThematicBreak,
    HtmlBlock,
    Table,
    TableRow,
    TableCell,

    Text,
    Strong,
    Emphasis,
    Strikethrough,
    CodeSpan,
    Link,
    Image,
    Autolink,
    SoftBreak,
    HardBreak,
    HtmlInline,
    EscapedChar,
    Entity,
    RawText,
}
```

`RawText` 表示无法安全解析或方言不支持的源码片段，**必须原样可编辑**。

### 4.4 MarkerRole

```rust
pub enum MarkerRole {
    OpenDelimiter,
    CloseDelimiter,
    LinkLabelOpen,
    LinkLabelClose,
    LinkDestinationOpen,
    LinkDestinationClose,
    LinkDestination,
    LinkTitle,
    EscapeBackslash,
    CodeFenceOpen,
    CodeFenceClose,
    HeadingPrefix,
    HeadingUnderline,
    ListBullet,
    OrderedListNumber,
    TaskCheckbox,
    QuotePrefix,
    HardBreakMarker,
    WhitespaceTrivia,
}
```

**不是所有 Marker 都默认隐藏**。语法角色与可见性策略必须分离。

例如 `# ` 是标题语法标记，但标题前的空格与缩进可能影响用户对结构的理解；是否隐藏属于 `BlockPresentationPolicy`，不是 Parser 的决定。

---

## 5. Source Range、Unicode 与换行

### 5.1 统一使用半开区间

`[start, end)`，单位 UTF-8 bytes。

```rust
pub struct SourceRange {
    pub start: SourceByte,
    pub end: SourceByte,
}
```

不变量：

- `start <= end`。
- 范围端点是 UTF-8 code point 边界。
- 所有子节点位于父节点 full_range 内。
- sibling 区间的重叠必须由语法模型明确解释。
- `SyntaxSnapshot` 中所有范围属于同一个 `DocumentRevision`。

### 5.2 例子

```markdown
abc **hello** xyz
```

UTF-8 byte 范围：

| 片段 | 范围 | 角色 |
|---|---|---|
| `abc ` | `0..4` | Text |
| `**` | `4..6` | OpenDelimiter |
| `hello` | `6..11` | Text |
| `**` | `11..13` | CloseDelimiter |
| ` xyz` | `13..17` | Text |

含中文：

```markdown
中**文**字
```

字节范围：

| 片段 | 范围 |
|---|---|
| `中` | `0..3` |
| `**` | `3..5` |
| `文` | `5..8` |
| `**` | `8..10` |
| `字` | `10..13` |

**绝不能把 Unicode 字符索引当成 byte offset。**

### 5.3 换行

必须明确三种视图：

1. 原始文件字节：可能包含 `\n`、`\r\n`、少见的 `\r`。
2. Document 内部规范：可以保留原始换行，或规范化为 `\n` 并记录序列化策略。
3. Parser 行索引：需要准确反映其所见的字节序列。

建议第一版 **Document 内部规范化为 LF，同时 FileSession 保留原始换行策略**；但混合换行文件需要保真策略，不能简单把“原始是 CRLF”当作全文件统一事实。若要求逐字节无损往返，应改为保留原始换行或维护逐行 EOL 元数据。

无论选择哪一种，必须将其写入 `DocumentContract` 并纳入文件往返测试。

---

## 6. Block 解析算法

### 6.1 Block 是增量解析与布局的主要单元

典型层次：

```text
Document
 ├─ Heading
 ├─ Paragraph
 │   ├─ Text
 │   └─ Strong
 ├─ List
 │   ├─ ListItem
 │   │   └─ Paragraph
 │   └─ ListItem
 └─ FencedCodeBlock
```

**注意：Block 不是简单“按空行切分”。**

以下结构可能跨越多行并相互影响：

- 列表缩进与懒续行（lazy continuation）。
- 引用块。
- 围栏代码块。
- Setext 标题。
- HTML block。
- 紧凑/松散列表。
- Link reference definition 对其他段落的引用解析。

### 6.2 Block 解析状态

```rust
pub struct BlockParseState {
    pub open_containers: Vec<ContainerState>,
    pub fence: Option<FenceState>,
    pub indentation: IndentationState,
    pub reference_definitions: ReferenceIndex,
}
```

按行扫描时，输入不仅是当前行文本，还包括上游容器状态。一个行修改可能改变后续许多行的解释。

### 6.3 解析边界与稳定点

增量更新应从受影响行前面的**可证明安全检查点**开始，而不是机械地“前后各扩一行”。

检查点需要保存：

- 当前 Block 容器栈。
- 代码围栏状态。
- HTML block 状态。
- 引用定义作用域/版本。
- 影响后续解析的语法状态摘要。

当新旧状态在某一检查点一致，并且对应源码区间未变，才可复用后续 Block。

对于最坏情况（例如删除一个开围栏导致整个文件重新解释），**允许全文重解析**，不能为了局部性能输出错误 AST。

---

## 7. Inline 解析与 Delimiter 配对

### 7.1 为什么不能用正则

输入：

```markdown
***bold and italic***
foo_bar_baz
\*not italic*
`**literal**`
**a *nested* b**
[**label**](https://example.com/a_(b))
```

这里涉及：

- delimiter run 长度。
- 左/右 flanking 条件。
- `_` 的词内限制。
- 反斜杠转义。
- 代码跨度的优先级。
- 链接标签和目的地中的括号。
- 多重强调的配对规则。

因此 Inline Parser 必须遵循权威解析器的语义，不能用 `find("**")` 定位结束标记。

### 7.2 Delimiter Run 结构

```rust
pub struct DelimiterRun {
    pub range: SourceRange,
    pub character: char,
    pub count: usize,
    pub can_open: bool,
    pub can_close: bool,
    pub context: DelimiterContext,
}
```

`can_open/can_close` 应依据 CommonMark 对前后 Unicode 标点、空白、字母数字的规则判定。

### 7.3 标记提取必须以语义结果为约束

若权威解析器没有直接输出 marker span，可以：

1. 先取得解析器确认的 Strong/Emphasis/Link 节点范围。
2. 在该节点的局部源码范围中进行**受限 delimiter 分析**。
3. 确认 opening/closing token 与解析器节点严格匹配。
4. 若无法无歧义确认，标记为 `MarkerConfidence::Unknown`。
5. 对 Unknown 节点**不隐藏语法标记**。

这比“猜一个最像的 `**`”更安全。

### 7.4 CodeSpan 优先级

```markdown
`**not strong**`
```

其中 `**` 是代码内容，不能生成 Strong 子节点。

代码跨度以反引号 run 配对，内容内部的反引号长度、换行和空白规范化由 CommonMark 语义决定；但源码必须保持原始字节。

---

## 8. 向 Projection 提供的语义输出

Parser 不直接决定“哪些字符隐藏”，而是提供**可投影候选标记**。

```rust
pub struct ProjectionSyntax {
    pub revision: DocumentRevision,
    pub nodes: Vec<ProjectionNode>,
    pub markers: Vec<ProjectableMarker>,
}

pub struct ProjectableMarker {
    pub source_range: SourceRange,
    pub owner: NodeId,
    pub role: MarkerRole,
    pub confidence: MarkerConfidence,
}
```

### 8.1 MarkerConfidence

```rust
pub enum MarkerConfidence {
    Exact,
    Conservative,
    Unknown,
}
```

- `Exact`：经过 AST 与源码一致性验证，可按 Policy 隐藏。
- `Conservative`：部分标记可判定，策略应优先保留。
- `Unknown`：始终可见。

### 8.2 输出不是 HTML

Parser 的输出应是：

```text
Source ranges + Semantic nodes + Marker ranges
```

而不是：

```text
Markdown → HTML → HTML parser → GPUI
```

HTML 转换会丢失原始 delimiter、空白和精确源码位置，不适合本项目要求的源码保真 Hybrid 编辑。

### 8.3 语义样式

Parser 输出 StyleIntent：

```rust
pub enum StyleIntent {
    Strong,
    Emphasis,
    Strikethrough,
    InlineCode,
    Link,
    Heading(u8),
    Quote,
    CodeBlock,
}
```

Projection 将源码范围映射到 Display 范围后生成 StyleRun；Layout 使用 GPUI shaping 计算字形几何。

---

## 9. 增量解析与依赖传播

### 9.1 输入

```rust
pub struct ParseRequest {
    pub previous: Option<Arc<SyntaxSnapshot>>,
    pub document: DocumentSnapshot,
    pub changes: Vec<NormalizedChange>,
    pub dialect: MarkdownDialect,
}
```

### 9.2 增量更新管线

```text
Document Transaction
   ↓
NormalizedChangeSet
   ↓
Map old ranges to new ranges
   ↓
Find affected Block ancestors
   ↓
Expand to safe parse checkpoint
   ↓
Incremental Block parse
   ↓
Reparse Inline for changed semantic blocks
   ↓
Recompute reference dependencies
   ↓
Reuse unaffected syntax subtrees
   ↓
Rebuild interval indexes (incremental)
   ↓
Publish SyntaxSnapshot
```

### 9.3 参考链接的非局部依赖

例如：

```markdown
See [docs][rust].

...

[rust]: https://www.rust-lang.org
```

修改文末定义可能改变文首 `[docs][rust]` 的解析或语义。

因此依赖图至少应记录：

```rust
pub struct ReferenceDependencyIndex {
    pub definitions: HashMap<NormalizedLabel, DefinitionId>,
    pub usages: HashMap<NormalizedLabel, Vec<NodeId>>,
}
```

定义变更后，必须重新解析/更新相关引用节点，即使它们不在编辑行附近。

### 9.4 失效粒度

分四级：

| 级别 | 示例 | 处理 |
|---|---|---|
| Inline-local | 段落内修改普通文字 | 该段落 Inline 重新解析 |
| Block-local | 改标题 `#`、列表项文本 | 该 Block 及邻近依赖 |
| Container-propagating | 列表缩进、引用、围栏 | 扩展到容器稳定点 |
| Document-wide | 全局引用定义/极端结构变化 | 更新依赖节点或全文解析 |

### 9.5 正确性优先的回退

任何增量解析结果都必须能与同版本**全文解析**得到的规范化语义树一致（忽略仅用于缓存的 Node ID）。

开发阶段随机抽样做 full-parse differential；CI 运行更高覆盖率的差分测试。

---

## 10. 稳定 Node ID 与子树复用

### 10.1 为什么需要稳定 ID

光标处于 Strong 节点时，Visibility Policy 可以记录活动语义节点。但用户在段首输入字符，不应导致整个文档所有节点 ID 改变。

### 10.2 ID 生成策略

不要使用 `(kind, source_start, source_end)` 作为唯一 Node ID；插入文本后范围变化会造成 ID 全部重建。

建议：

- 解析器能够复用的节点继承 ID。
- 无法复用的节点通过局部结构匹配（kind、父节点、内容指纹、相对位置）尝试保留 ID。
- 发生语义重组时允许 ID 变化，不强求所有节点永远稳定。
- ID 只在一个 Document 生命周期内唯一，不作为磁盘持久化标识。
- Selection 始终基于 Source Anchor，不能只依赖 Node ID。

### 10.3 增量树复用

```rust
pub struct ReuseResult {
    pub retained: Vec<NodeId>,
    pub inserted: Vec<NodeId>,
    pub removed: Vec<NodeId>,
    pub changed: Vec<NodeId>,
}
```

这个差异集可直接驱动 Visibility/Projection/Layout 的精确失效。

---

## 11. 编辑中间态与错误恢复

### 11.1 用户输入天然不完整

输入过程可能经历：

```text
*
**
**h
**he
**hello
**hello*
**hello**
```

不完整输入不是异常，不应生成会隐藏半个结构的假节点。

### 11.2 恢复规则

- 未闭合强调：显示原始 `*`/`**`。
- 未闭合链接：保留 `[`, `]`, `(` 等源码字符。
- 未闭合代码围栏：根据规范可能合法地延续到文末；但其“隐藏围栏”策略要保证可编辑性。
- 不完整转义：保留反斜杠。
- 未识别扩展语法：原样显示。
- 解析器异常：对受影响 Block 降级 RawText，并记录诊断。

### 11.3 SyntaxDiagnostic

```rust
pub struct SyntaxDiagnostic {
    pub range: SourceRange,
    pub kind: SyntaxDiagnosticKind,
    pub severity: Severity,
    pub message: &'static str,
}
```

诊断用于开发、调试与安全回退，不一定直接显示在编辑器 UI。

---

## 12. 嵌套、冲突与特殊语法

### 12.1 嵌套强调

```markdown
**bold *and italic* text**
```

结构：

```text
Strong
  ├─ Text("bold ")
  ├─ Emphasis("and italic")
  └─ Text(" text")
```

光标进入 `italic` 时：

- 必须依据策略确定是否展开 Emphasis 和 Strong 两层标记。
- 所有被展开 marker 均来自精确源码范围。
- 同一个 marker 不得被多个 Projection Node 重复隐藏/展开。
- Marker 归属应有明确的 owner。

### 12.2 三重星号

```markdown
***hello***
```

可能解析为嵌套 Strong/Emphasis。需要确保 `***` run 被分配给对应语义节点时，源码 marker 区间不重叠或具有可验证的共享策略。

**建议**：将单个 delimiter run 拆成多个不重叠 `MarkerAtom`，并记录它们在 run 中的 byte 范围。不能让两个节点都宣称拥有同一个 `0..3` 并各自独立隐藏。

### 12.3 转义

```markdown
\*literal\*
```

`*` 不应成为 Emphasis；反斜杠可能在预览中隐藏，但只有在正确转义成立时才允许隐藏。光标进入附近时应显示原始 `\`。

### 12.4 HTML 与实体

```markdown
&copy;
<span>hello</span>
```

实体解码属于**替换投影**而非简单 HiddenSyntax，可能涉及一个源码范围对应一个不同 Unicode 字符。第一版若只支持隐藏，不应偷偷把 `&copy;` 替换成 `©`；可保留原文，直到 Widget/Replacement Projection 模型就绪。

原生 HTML Inline/Block 可以作为 RawText 显示，避免 DOM/HTML 渲染引入独立布局语义。

---

## 13. Link、Image、Autolink 与 URL

### 13.1 Inline Link

```markdown
[OpenAI](https://openai.com "Title")
```

应识别：

- label open `[`
- label content `OpenAI`
- label close `]`
- destination delimiters `(` / `)`
- URL source span
- optional title source span
- whitespace trivia

普通显示策略：

- 显示带链接样式的 `OpenAI`。
- 隐藏 `[`, `]`, `(`, `URL`, title, `)`，前提是 marker 精确可靠。
- 光标进入 label 时，展开整个链接语法（或按交互规范分级展开）。
- 鼠标普通点击用于编辑定位；打开链接使用明确修饰键或命令，避免误触。

### 13.2 Reference Link

```markdown
[OpenAI][oa]

[oa]: https://openai.com
```

链接节点与定义节点分离，并建立 ReferenceDependencyIndex。编辑定义时更新所有引用的语义和链接目标。

### 13.3 Autolink

```markdown
<https://example.com>
```

可将尖括号标记视为候选可隐藏语法，但 URL 正文本身必须保留。

### 13.4 Image

```markdown
![alt](image.png)
```

第一版建议显示源码或显示 alt 文本但保留明确的编辑入口；未来图片 Widget 必须使用 Source-backed Replacement Projection。**不能只靠 HiddenSyntax 实现真实图片占位**。

---

## 14. Block 语法的精确 marker

### 14.1 ATX Heading

```markdown
## Heading ##
```

标记可能包括：

- opening `##`
- opening 后的分隔空格
- optional closing `##`
- closing 前的空白

识别结果交给 BlockPresentationPolicy；Structured 标题的前后缀保持隐藏，正文获得光标不展开。BlockSource 显示完整语法，不应把正文中的 `#` 错当 closing marker。

### 14.2 Setext Heading

```markdown
Heading
-------
```

下划线是独立行，语义确认后由 Hybrid BlockPresentationPolicy 隐藏并呈现标题；修改或删除 underline 通过块源码入口。必须保留跨行源码映射，语义/几何无法可靠确认时回退原文。详见第十一份 H1 规格。

### 14.3 Lists

```markdown
- item
1. item
- [x] done
```

ListItem marker 包括缩进、项目符号或编号、任务复选框及必要空白。

不要把整个列表标记统一删除为零宽字符。列表布局需要保留 marker 对应的结构性缩进和悬挂缩进（hanging indent）。这属于**Block Decoration / Structural Layout**，不是简单 Inline Elision。

建议输出：

```rust
pub struct BlockPresentation {
    pub content_spans: Vec<SourceRange>,
    pub prefix: Option<BlockPrefix>,
    pub indent: IndentSpec,
    pub hanging_indent: f32,
}
```

Structured 列表的 marker 保持隐藏，正文获得光标不展开；圆点/数字、任务复选框、悬挂缩进由 Block Layout 决定。修改语法走结构命令或 BlockSource。

### 14.4 BlockQuote

```markdown
> quoted
```

Structured 引用以左侧引用线和缩进呈现，正文获得光标不展开 `>`；结构命令和 BlockSource 保留源码编辑入口。多层引用维护每层 prefix 归属。

### 14.5 Fenced Code

````markdown
```rust
fn main() {}
```
````

围栏开闭、info string、代码内容是不同语法范围。Structured 代码块隐藏围栏，正文直接编辑；语言字段/BlockSource 可修改 info string 与围栏。正文不进行普通 Inline 解析；代码高亮不得改变 Markdown 语义。

### 14.6 Hybrid 解析与呈现边界

Parser 不消费焦点、Enter 或模式状态；输入语义确认后由呈现层按第十一份选择 RawDraft/Structured/BlockSource。Table 解析为 row/cell/source spans，H2 接入网格与单元格编辑；H1 表格原文呈现。打开已有合法块不要求补按 Enter；纯模式切换不触发重解析。

---

## 15. StyleRun、可见性与 Token 分类

### 15.1 Parser 输出语义，不输出 GPUI 样式

```rust
pub struct SemanticSpan {
    pub source_range: SourceRange,
    pub styles: Vec<SemanticStyle>,
}
```

`Strong`、`Emphasis` 可以嵌套；最终 StyleRun 必须在 Projection 后计算显示范围。

### 15.2 分层样式

```text
Base Typography
  + Block Semantic Style
  + Inline Semantic Style
  + Link / Code Decoration
  + Selection / IME Overlay
```

选区和 IME 是绘制层覆盖，不应写入 Syntax Tree。

### 15.3 Syntax Token 分类

| Token | Parser 角色 | Presentation |
|---|---|---|
| `**` | Strong delimiter | 光标感知显隐 |
| `*` | Emphasis delimiter | 光标感知显隐 |
| `` ` `` | CodeSpan delimiter | 光标感知显隐 |
| `[` `]` `(` `)` | Link delimiters | 光标感知显隐 |
| `#` | Heading prefix | Block 策略 |
| `-` / `1.` | List marker | Block Decoration |
| `>` | Quote prefix | Block Decoration |
| `` ``` `` | Fence | Code Block 策略 |
| `\` | Escape marker | 条件性显隐 |
| `&copy;` | Entity source | Replacement Projection（未来） |

---

## 16. 与 Document / Editor / IME 的事务协议

### 16.1 Source Transaction

```text
Editor Engine
  → Document.apply(ChangeSet)
  → AppliedChange { before, after, normalized_changes }
  → SyntaxEngine.update(DocumentSnapshot, AppliedChange)
  → SyntaxSnapshot
  → InlineVisibilityPolicy(selection, composition)
  → BlockPresentationPolicy(HybridPresentationState) / FlowBlocks
  → ProjectionSnapshot
  → LayoutSnapshot
```

Parser **不修改 Selection**。当 Node ID 变化时，Selection 仍由 Source Anchor 保持稳定。

### 16.2 光标移动

```text
Selection changes
  → query InlineIntervalIndex
  → recompute active nodes
  → update visibility
  → invalidate only changed projection blocks
```

光标移动本身不重新解析 Markdown。

### 16.3 IME Overlay

IME 组合期间：

- Parser 使用最后已提交的 DocumentSnapshot。
- Composition Overlay 由 Projection/Layout 处理。
- 若组合区间跨越语法标记，强制显示相关源码，避免隐藏 marked text。
- 组合提交后产生一次正式 Document Transaction，触发解析。
- 如确需组合期间语义反馈，可以对 overlay 生成**临时派生解析**，但不能覆盖 committed SyntaxSnapshot。

### 16.4 异步解析结果

```rust
pub struct ParseResult {
    pub source_revision: DocumentRevision,
    pub config_revision: u64,
    pub snapshot: Arc<SyntaxSnapshot>,
}
```

发布条件：

```text
result.source_revision == current_document_revision
AND
result.config_revision == current_syntax_config_revision
```

否则丢弃或重基准化。不能用过期 AST 生成最新 Projection。

---

## 17. 性能、缓存与并发

### 17.1 三类缓存

1. **Block parse cache**：源码片段指纹、容器状态、方言版本、子树。
2. **Inline parse cache**：Block 内容指纹、引用定义版本、Inline AST。
3. **Index cache**：区间索引、引用依赖索引、Node ID 复用映射。

### 17.2 更新成本

| 操作 | 预期更新 |
|---|---|
| 段落内输入一个字 | 当前段落 Inline |
| 修改 `**` | 当前 Inline / Block |
| 修改列表缩进 | 相关 List 容器及后续状态 |
| 删除代码围栏 | 直到容器状态稳定，可能全文 |
| 修改引用定义 | 定义及所有依赖引用 |
| 仅移动光标 | Parser 零工作 |
| 修改字体 | Parser 零工作，Layout 失效 |
| 切换方言 | 全文重新解析 |

### 17.3 调度策略

- 普通短文档同步解析可提供最低延迟。
- 长文档可将不影响当前光标的解析安排到后台。
- 当前正在编辑的 Block 必须有可用的正确语法快照或安全 RawText 降级。
- 后台任务持有不可变 DocumentSnapshot。
- 使用取消/版本检查丢弃过期结果。
- 不要因为后台解析延迟而阻塞文字输入。
- 性能优化必须通过全文解析差分测试证明语义不变。

### 17.4 内存

- 共享源码快照，避免每次解析复制整个文件。
- AST 字符串优先使用 SourceRange，只有需要规范化标签等场景才持有独立字符串。
- 回收不可见 Block 的重型 Inline 索引，但保留足够的 Block 索引支持导航。
- 对引用依赖建立按 label 的反向索引，避免每次编辑扫描全文。

---

## 18. 测试矩阵与属性测试

### 18.1 CommonMark/GFM 规范测试

使用固定版本的官方规范用例。比较：

- Block 类型。
- Inline 类型与嵌套。
- 语义文本。
- 链接目标。
- 代码跨度。
- HTML/实体处理。
- 不完整语法恢复。

**不要仅比较 HTML 字符串**：HTML 相同不代表 delimiter range 正确。

### 18.2 Marker 黄金用例

| 源码 | 关键断言 |
|---|---|
| `**hello**` | 2 个准确 Strong marker |
| `***hello***` | delimiter run 拆分无重叠 |
| `**a *b* c**` | 父子嵌套正确 |
| `foo_bar_baz` | 不误识别强调 |
| `\*text*` | 转义不产生错误强调 |
| `` `**x**` `` | code 内无 Strong |
| `[x](a_(b))` | URL 括号配对准确 |
| `[x][ref]` | 引用依赖正确 |
| `# title` | 标题前缀范围准确 |
| `- [x] done` | 列表/任务标记分类正确 |
| `中**文**字` | UTF-8 byte offset 正确 |
| `**unfinished` | 不隐藏未闭合语法 |

### 18.3 增量解析差分测试

随机执行：

```text
Insert / Delete / Replace / Undo / Redo
```

每次：

1. 对旧 AST 做增量更新。
2. 对当前完整源码做全文解析。
3. 规范化两个 AST（忽略临时 Node ID）。
4. 比较语义结构、source ranges、marker ownership、引用依赖。
5. 对所有 marker 验证源码切片。
6. 若不同，记录最小复现输入和变更序列。

### 18.4 Projection 合同测试

- 每个隐藏区间都必须对应 `Exact` Marker。
- 隐藏区间不可越界。
- 不完整语法不得被隐藏。
- 同一个源码字节不可被多个无约束 marker 重复隐藏。
- StyleRun 映射后不得落在非法 display 边界。
- Node ID 变化不导致 Source Selection 漂移。
- Parser 失败时文档仍可编辑并保存。

### 18.5 Fuzzing

推荐三类 fuzz target：

```text
fuzz_parse_source
fuzz_incremental_edit_sequence
fuzz_marker_projection_contract
```

尤其覆盖：

- 任意 Unicode 字节合法文本。
- 深层列表/引用嵌套。
- 大量 `*`、`_`、反引号。
- 极长行与大文件。
- 非常多引用定义与重复标签。
- 插入/删除代码围栏。
- 输入中间态反复切换。

---

## 19. Rust 公共接口草案

### 19.1 Syntax Engine

```rust
pub trait MarkdownSyntaxEngine {
    fn parse_full(
        &mut self,
        document: &DocumentSnapshot,
        dialect: &MarkdownDialect,
    ) -> Result<Arc<SyntaxSnapshot>, SyntaxError>;

    fn parse_incremental(
        &mut self,
        document: &DocumentSnapshot,
        previous: &SyntaxSnapshot,
        changes: &[NormalizedChange],
        dialect: &MarkdownDialect,
    ) -> Result<Arc<SyntaxSnapshot>, SyntaxError>;
}
```

### 19.2 Syntax Query

```rust
pub trait SyntaxQuery {
    fn deepest_node_at(
        &self,
        position: SourceByte,
        bias: Bias,
    ) -> Option<NodeId>;

    fn ancestor_chain(&self, node: NodeId) -> Vec<NodeId>;

    fn intersecting_nodes(
        &self,
        range: SourceRange,
    ) -> Vec<NodeId>;

    fn markers_for(&self, node: NodeId) -> &[SyntaxMarker];

    fn block_at(&self, position: SourceByte) -> Option<NodeId>;
}
```

### 19.3 Projection Adapter

```rust
pub trait ProjectionSyntaxProvider {
    fn projectable_markers(
        &self,
        blocks: &[NodeId],
    ) -> Vec<ProjectableMarker>;

    fn semantic_spans(
        &self,
        blocks: &[NodeId],
    ) -> Vec<SemanticSpan>;

    fn structural_presentations(
        &self,
        blocks: &[NodeId],
    ) -> Vec<BlockPresentation>;
}
```

### 19.4 增量差异

```rust
pub struct SyntaxDelta {
    pub before: DocumentRevision,
    pub after: DocumentRevision,
    pub retained: Vec<NodeId>,
    pub inserted: Vec<NodeId>,
    pub removed: Vec<NodeId>,
    pub changed: Vec<NodeId>,
    pub affected_blocks: Vec<NodeId>,
    pub changed_references: Vec<NormalizedLabel>,
}
```

### 19.5 查询性能约束

- `deepest_node_at` 应基于区间索引，而非每次扫描全部 AST。
- `markers_for` 应返回同一快照内稳定借用数据。
- `SyntaxDelta` 不应包含可变 Rope 引用。
- `parse_incremental` 必须验证 previous 与 changes 的版本链；不匹配返回错误或全量解析。
- 所有 API 明确使用 UTF-8 SourceByte。

---

## 20. 工作包、验收关卡与开放决策

### S0 — Parser 技术验证

**交付**：

- 锁定 Tree-sitter Markdown、pulldown-cmark 及相关 grammar 版本。
- 验证 CommonMark/GFM 语义覆盖。
- 验证 marker ranges、增量解析、错误恢复。
- 输出能力矩阵与不能可靠支持的语法清单。

**通过条件**：关键 Inline 语法具备可信 marker range；否则重新选择权威解析器。

### S1 — Canonical Syntax Model

**交付**：

- `SyntaxSnapshot`、Node/Marker、Block/Inline Index。
- `SyntaxAdapter` 与源码范围校验。
- 基础 golden fixtures。

**通过条件**：语义树与源码范围一致，不存在未定义 marker 重叠。

### S2 — Block Incremental Parsing

**交付**：

- 容器状态检查点。
- 安全重解析边界。
- Block 子树复用。
- 全量/增量差分测试。

**通过条件**：结构性编辑与全文解析结果一致。

### S3 — Inline Delimiter Extraction

**交付**：

- Strong、Emphasis、Strike、CodeSpan、Link marker。
- 嵌套 delimiter ownership。
- Incomplete/Unknown 回退。

**通过条件**：任何被隐藏的 marker 都能精确映射到源码字节。

### S4 — References + GFM

**交付**：

- ReferenceDependencyIndex。
- TaskList、Autolink、Table 解析支持。
- 方言配置与依赖传播。

**通过条件**：非局部引用更新正确。

### S5 — Projection / Editor 集成

**交付**：

- `ProjectionSyntaxProvider`。
- 光标感知显隐。
- StyleRun 与 BlockPresentation。
- 编辑后增量失效和过期结果保护。

**通过条件**：复杂嵌套 Markdown 中移动光标不会隐藏错误标记或破坏选区。

### S6 — 规范、压力与稳定性

**交付**：

- 官方规范测试。
- Fuzzing。
- 真实大文档 benchmark。
- Parser/Projection 错误降级测试。

**通过条件**：不存在已知会导致源码丢失、语法误隐藏、位置错位的路径。

### 20.1 编码前必须明确的决策

1. **解析器权威性**：最终选择哪一个解析器作为唯一 AST 来源？
2. **Markdown 方言**：CommonMark/GFM 哪些扩展启用？
3. **原始换行保真**：Document 内部规范化还是保留原始字节？
4. **Setext Heading（已确认）**：Structured 隐藏 underline，BlockSource 编辑，失败安全回退原文。
5. **列表 Marker（已确认）**：结构装饰保持，源码由块入口编辑；映射与命中按第十一份验收。
6. **实体替换**：何时支持 Replacement Projection？
7. **Inline Link 展开**：进入 label 是否展开整个 URL？
8. **未知语法**：统一 RawText，还是启用用户配置扩展？
9. **HTML**：是否始终以源码形式显示？
10. **增量语义差分**：如何定义与全文解析等价的规范化 AST？

---

## 21. 与前三份文档的关系

| 文档 | 职责 |
|---|---|
| 第一份：整体架构 | 单文件产品边界、模块与全局原则 |
| 第二份：Projection + Layout | Source/Display 映射、显隐、字形布局 |
| 第三份：Editor Engine | Document、Anchor、Transaction、Selection、Undo、IME |
| **第四份：Syntax Engine（本文）** | **增量 AST、精确 Marker、CommonMark/GFM、解析依赖、错误恢复** |

完整数据流：

```text
GPUI Input
   ↓
Editor Engine
   ↓
Document Transaction
   ↓
DocumentSnapshot + AppliedChange
   ↓
Markdown Syntax Engine
   ↓
SyntaxSnapshot + SyntaxDelta
   ↓
Visibility Policy
   ↓
ProjectionSnapshot
   ↓
Block LayoutSnapshot
   ↓
GPUI Renderer
```

### 最终架构结论

**Markdown Syntax Engine 的职责不是“生成漂亮的预览”，而是生成一份能够精确追溯到原始 Markdown 字节的、可增量维护的语义索引。**

只有在 AST、delimiter 范围和编辑版本完全一致的前提下，Typora 风格 Hybrid View 的语法显隐才可以成为稳定、可测试、可维护的编辑器核心能力。

> **下一份建议**：第五份《GPUI Native Rendering、Viewport Virtualization、Hit Testing、Scroll Anchoring 与 Accessibility》，专门解决原生绘制、百万字文档滚动、可视行缓存、输入候选框几何和 macOS 辅助功能。
