# GPUI Live Markdown — 第三份：Editor Engine、Document、Anchor、Transaction、Selection、Undo/Redo 与 IME 实现设计

> 2026-10-12 Hybrid 修订：交互路线与阶段范围以[第十一份](./11_GPUI_Typora_Hybrid_View_Spec.md)为准。Inline 按光标显隐；Block 保持结构化，显式进入局部/全文源码；模式切换不改源码或历史。下文领域 API 为设计草案，须纳入 Hybrid presentation 版本依赖后实施。

> **文档定位**：第三份独立技术设计，衔接《整体架构》与《Projection / DisplayMap / Block Layout 深度实现》。  
> **目标平台**：macOS；Rust + GPUI；单窗口、单文件、单一文档；不依赖 gpui-kit Editor。  
> **设计原则**：优先保证语义正确性、可验证性、长期演进和原生输入质量；不以工期为由牺牲基础模型。  
> **状态**：设计规格与参考伪代码。GPUI 具体 trait、生命周期、平台 API 必须在锁定 commit 后源码核验，文中示意接口不得直接视为可编译的 GPUI API。

---

## 0. 本文要解决的问题与边界

第一份定义了整体架构；第二份定义了 `Source → Projection → Layout`。本篇专注**所有会改变编辑状态的操作**：

1. 原始 Markdown 的唯一数据源与事务边界。
2. 编辑后稳定追踪的 Source Anchor。
3. 多选区、光标移动、隐藏语法边界、跨 Block 编辑。
4. Undo/Redo 的可逆事务、合并、分组与状态恢复。
5. macOS 原生文本输入：普通按键、IME 组合、替换范围、候选框、UTF-16。
6. 键盘/鼠标事件的确定性顺序与布局快照一致性。
7. 与增量解析、Visibility、Projection、Layout 的依赖与失效协议。
8. 端到端正确性测试、模糊测试和故障恢复。

**非目标**：协作 CRDT、多人编辑、LSP、多缓冲区、自动补全系统、数据库。接口可以预留能力，但不为假想需求引入不必要的分布式语义。

### 0.1 不可妥协的不变量

- **唯一真相**：Markdown 源码只由 `Document` 拥有；渲染缓存、输入组合快照不得成为第二份可独立编辑的正文。
- **源坐标是编辑权威**：`Selection`、`IME replacement range`、`History`、`Parser invalidation` 最终均指向源码位置。
- **事务原子性**：一次命令的所有替换、选区迁移、历史提交要么整体成功，要么完全不生效。
- **投影纯呈现**：隐藏/展开语法不改变源码，不写入 Undo，不触发文件 dirty。
- **快照一致性**：一个命中测试或候选框几何计算只能使用同一 `DocumentRevision + ProjectionRevision + LayoutRevision` 链。
- **Unicode 正确性**：编辑偏移是 UTF-8 字节；用户导航按扩展字素簇（grapheme cluster）；macOS 输入 API 的 UTF-16 坐标必须显式转换。
- **IME 独立状态机**：组合输入不能被普通字符插入路径随意拆分，也不能因为语法显隐丢失 marked text。
- **可复现**：给定初始文档、输入事件序列和配置，最终源码、选区、历史结果确定。

---

## 1. 模块与依赖方向

```text
app (FileSession, menus, window lifecycle)
   │
   ▼
editor-gpui (GPUI View/Element, focus, InputHandler, event adapter)
   │
   ▼
editor-engine (commands, navigation, selection, composition, history)
   ├──► editor-document (Rope, AnchorStore, ChangeSet, transactions)
   ├──► markdown-syntax (incremental AST)
   ├──► markdown-projection (visibility, mapping, snapshots)
   └──► editor-layout (shaping, hit test, caret/selection geometry)
```

依赖原则：

- `editor-document` 不依赖 Markdown 或 GPUI。
- `editor-engine` 不知道 GPUI 的具体事件类型；使用平台无关的 `EditorEvent`。
- `editor-gpui` 把 GPUI/macOS 输入协议转换成 `EditorEvent`，并把引擎输出转换为焦点、窗口重绘、IME 几何响应。
- `editor-layout` 可以使用 GPUI 的文本塑形基础设施，但不拥有编辑命令。
- `markdown-projection` 不修改 `Document`。
- **只有 Editor Engine 有权组织跨模块状态提交**，避免 Parser、Layout、IME 各自直接改写选区。

### 1.1 推荐 Workspace

```text
crates/
  editor-document/
    src/{lib,rope,offset,anchor,change,transaction,snapshot}.rs
  editor-engine/
    src/{lib,state,command,selection,navigation,history,composition,
         clipboard,dispatch,validation}.rs
  markdown-syntax/
  markdown-projection/
  editor-layout/
  editor-gpui/
    src/{view,element,input_adapter,focus,events,accessibility}.rs
  app/
tests/
  fixtures/
  property/
  integration/
  ime-manual/
```

---

## 2. Document：单一 Rope 与不可变读取快照

### 2.1 坐标类型

禁止在跨模块公共 API 中裸传 `usize`，防止 UTF-8 / UTF-16 / grapheme / display offset 混用。

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SourceByte(pub usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Utf16Unit(pub usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct DocumentRevision(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Bias {
    Before,
    After,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct AnchorId(pub u64);
```

所有 `SourceByte` 必须满足：

- `0 <= offset <= rope.len_bytes()`
- 指向 UTF-8 code point 边界。
- 对交互光标还应规范化到扩展字素簇边界；并非所有 Unicode scalar 边界都允许停靠。
- 与具体文档版本关联；跨版本使用须通过 `Anchor` 或 `ChangeMap` 解析。

### 2.2 只读快照与唯一写入者

```rust
pub struct Document {
    rope: Rope,
    revision: DocumentRevision,
    anchors: AnchorStore,
}

pub struct DocumentSnapshot {
    pub revision: DocumentRevision,
    // 只读 Rope 快照或共享不可变根，具体取决于所选 Rope 实现。
    rope: RopeSnapshot,
}
```

**单一 Rope**是指单一逻辑文档与唯一权威写入者；持久化 Rope 允许多个只读版本共享内部节点，这不构成多份可独立修改的文档。

性能与库选型要求：

- 大文档中间插入/删除的渐进复杂度。
- 支持高效 byte/char/line 查询。
- 快照成本与共享策略。
- 对编辑后 Anchor 变换的友好程度。
- CRLF/Unicode 的边界表现。

不预先把 `ropey` 或某个特定 Rope 实现锁死为长期答案；先制定契约，再用真实文档负载做基准比较。

### 2.3 原子修改入口

```rust
pub struct Replace {
    pub range: std::ops::Range<SourceByte>,
    pub insert: String,
}

pub struct ChangeSet {
    pub base_revision: DocumentRevision,
    pub edits: Vec<Replace>,
}

pub struct AppliedChange {
    pub before: DocumentRevision,
    pub after: DocumentRevision,
    pub changes: Vec<NormalizedChange>,
    pub inverse: Vec<Replace>,
    pub affected_lines: std::ops::Range<usize>,
}
```

约束：

1. 所有 `Replace` 基于**同一修改前版本**的坐标。
2. 编辑区间必须互不重叠；相同插入点需有显式确定性合并规则。
3. 先完成全部验证，再修改 Rope。
4. 推荐按源码偏移**从大到小**应用替换，避免前面替换移动后续坐标。
5. `inverse` 必须从修改前的原文准确提取；回滚所用的区间坐标必须在**修改后版本**中重新计算，不能直接复用原始区间。
6. 修改完成后统一递增 Revision，发布单一 `AppliedChange`。
7. 对不可恢复错误，不能暴露“文本已改、选区未改”的中间状态。

示意：

```rust
fn apply_changes(
    &mut self,
    changes: ChangeSet,
) -> Result<AppliedChange, EditError> {
    self.validate_revision(changes.base_revision)?;
    let normalized = normalize_and_validate(&self.rope, changes.edits)?;
    let inverse = build_inverse_in_post_edit_coordinates(&self.rope, &normalized)?;
    let affected_lines = compute_affected_lines(&self.rope, &normalized);

    for change in normalized.iter().rev() {
        self.rope.remove_bytes(change.old_range.clone());
        self.rope.insert_bytes(change.old_range.start, &change.insert);
    }

    let previous = self.revision;
    self.revision = DocumentRevision(previous.0 + 1);
    self.anchors.transform(&normalized);
    Ok(AppliedChange {
        before: previous,
        after: self.revision,
        changes: normalized,
        inverse,
        affected_lines,
    })
}
```

> 以上 `remove_bytes/insert_bytes` 是抽象操作名，不是对任何 Rope crate API 的声明。

---

## 3. Anchor：编辑后仍能追踪语义位置

### 3.1 为什么不能长期保存裸字节偏移

源码：

```text
abc|def
```

光标在 offset 3。若在文首插入 `"XX"`，逻辑光标应随原内容迁移到 offset 5。裸 `3` 不具备这种语义。

Anchor 由 `AnchorStore` 维护，读取时解析为指定版本的 `SourceByte`：

```rust
pub struct Anchor {
    pub id: AnchorId,
    pub bias: Bias,
}

pub trait AnchorResolver {
    fn resolve(
        &self,
        anchor: Anchor,
        at: DocumentRevision,
    ) -> Result<SourceByte, AnchorError>;
}
```

Anchor 可以通过两种实现策略维护：

- **立即更新式**：每次事务对存活 Anchor 执行区间变换。易于实现，但 Anchor 数量大时成本高。
- **延迟解析式**：Anchor 记录生成版本与偏移，基于 Change Log / 树结构按需变换。更适合大量语法锚点与快照并存，但需要版本回收和变更日志压缩。

长期建议设计抽象支持延迟解析，具体实现先由性能基准决定。不要把 AST 每个短生命周期范围都注册成持久 Anchor；稳定选区、滚动锚点和长期交互状态优先使用 Anchor，语法范围由 Parser Revision 维护。

### 3.2 插入点 Bias

在位置 `p` 插入长度 `n` 的文本：

| Anchor 位置 | Bias::Before | Bias::After |
|---|---:|---:|
| `a < p` | `a` | `a` |
| `a = p` | `p` | `p+n` |
| `a > p` | `a+n` | `a+n` |

注意：表中长度为**UTF-8 字节数**，不是字符数。

### 3.3 删除与替换

删除旧区间 `[s,e)`：

- `a < s`：保持原位。
- `a > e`：减去 `e-s`。
- `s <= a <= e`：折叠到 `s`，边界上的 Bias 决定后续同点插入归属。

替换等价于同一事务中**删除旧区间并插入新内容**，但对位于被替换区间内部的 Anchor，必须采用定义好的“塌缩 + 偏向”规则，不能依赖两次操作偶然的执行顺序。

对于多重编辑，`ChangeMap` 应能以单次遍历或二分索引完成旧→新位置映射。

### 3.4 Anchor 生命周期

- Selection、IME、Viewport 持有 Anchor 句柄。
- Document 对 AnchorStore 提供创建、克隆、释放或 RAII 引用计数语义。
- Undo 历史应保存可恢复的 Selection Snapshot，而不是持有永不回收的全部旧 Anchor。
- 文档关闭时所有 Anchor 失效，禁止跨 Document ID 使用。
- 溢出、非法版本、被回收历史必须返回明确错误，不能静默猜测位置。

---

## 4. Selection：全局范围、方向、亲和性

### 4.1 结构

```rust
pub struct Selection {
    pub anchor: Anchor,  // 选择起点，固定端
    pub head: Anchor,    // 活动端，移动端
    pub affinity: VisualAffinity,
    pub preferred_x: Option<f32>,
}

pub enum VisualAffinity {
    Upstream,
    Downstream,
}

pub struct SelectionSet {
    pub primary: usize,
    pub selections: Vec<Selection>,
}
```

**Anchor 的插入 Bias**与**视觉 Affinity**不是同一概念：

- Bias 决定文档修改时插入点的归属。
- VisualAffinity 决定同一个源码边界在软换行、Bidi 或隐藏语法位置上对应哪个可视光标停靠点。

即使第一版只允许一个选区，建议保留 `SelectionSet` 结构，确保核心 API 未来能扩展多光标而无需重写命令语义。

### 4.2 选择方向

选择范围规范化为 `[min(anchor, head), max(anchor, head))`，但**不能丢失方向**。

例如 Shift+Left 连续扩展时，固定端和活动端决定下一次移动方向。撤销重做应恢复方向。

### 4.3 选区归一化

- 多选区按源码起点排序。
- 重叠的非折叠范围合并。
- 相同位置的重复光标合并。
- 选区合并后明确主光标的归属。
- 不允许保存指向无效字素边界的交互光标。

### 4.4 隐藏语法边界的编辑语义

示例：

```markdown
ab**cd**ef
```

未激活时显示 `abcdef`。在 `b|c` 可视边界附近，可能存在多个源码位置映射到同一视觉位置。

原则：

1. **视觉点击先定位 Display caret stop**，再由 Projection + Affinity 选择 Source Anchor。
2. 若目标进入 `cd` 的语义节点，Visibility Policy 展开 `**`。
3. 显隐后保留已确定的 Source Anchor，重新布局并绘制；**不得根据旧像素位置重新命中测试**。
4. Backspace/Delete 按源码编辑命令定义，不按隐藏字符数量猜测。
5. 方向键导航使用合法视觉 caret stop 图，再映射到 Source；必要时展开语法，保证语法标记可以被正常编辑。
6. 双击词选择、三击段落选择应先确定 Source 范围，再按策略决定标记是否显示。

### 4.5 软换行与 Bidi

- `Left/Right` 是视觉导航；在 Bidi 文本中不等于简单 `byte_offset ± 1`。
- `Up/Down` 使用上一布局快照的 `preferred_x` 选择相邻视觉行的最近 caret stop。
- `Home/End` 在视觉行与逻辑行之间的语义需明确；macOS 默认键位行为由 Keymap 定义。
- `Option+Left/Right` 采用 Unicode word boundary；`Cmd+Left/Right` 可映射逻辑行首尾，具体遵循应用键位约定。
- 所有导航均不得停在 UTF-8 continuation byte 或 grapheme 内部。

---

## 5. Editor Command：平台无关的命令模型

```rust
pub enum EditorCommand {
    InsertText(String),
    ReplaceSelection(String),
    DeleteBackward,
    DeleteForward,
    DeleteWordBackward,
    DeleteWordForward,
    InsertNewline,
    Move(MoveDirection, MoveUnit, ExtendSelection),
    SelectAll,
    SelectWord,
    SelectParagraph,
    ToggleStrong,
    ToggleEmphasis,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste(ClipboardPayload),
}

pub enum MoveDirection {
    Left, Right, Up, Down, Start, End,
}

pub enum MoveUnit {
    Grapheme, Word, VisualLine, LogicalLine, Document,
}

pub struct CommandOutcome {
    pub document_changed: bool,
    pub selection_changed: bool,
    pub visibility_changed: bool,
    pub needs_layout: bool,
    pub needs_paint: bool,
    pub scroll_to_caret: bool,
}
```

**键盘事件与文字输入必须分流**：

- `KeyDown` 主要处理快捷键和非文本命令。
- 普通文本输入由平台文字输入回调提交，避免一个按键既触发 KeyDown 插入又触发 IME 插入。
- Dead key、重音组合、输入法预编辑由 IME 路径处理。
- 剪贴板操作先转成 `ClipboardPayload`，再统一交给编辑引擎执行。

### 5.1 命令执行管线

```text
Platform Event
   ↓
GPUI Adapter (focus / keymap / text-input protocol)
   ↓
EditorEvent
   ↓
Command Dispatcher
   ├─ validate revision & input mode
   ├─ normalize selection
   ├─ build source ChangeSet
   ├─ apply atomic Document Transaction
   ├─ transform Anchors & Selection
   ├─ record History (if eligible)
   ├─ update incremental Syntax
   ├─ recompute Visibility
   ├─ invalidate Projection & Layout
   └─ publish single EditorStateRevision
   ↓
GPUI repaint / scroll / IME caret rect
```

应避免 Parser 或 Renderer 在事务提交过程中同步回调编辑命令，防止重入。事件队列可以串行化变更；异步解析结果必须携带基准 Revision，过期结果不得覆盖新状态。

---

## 6. 编辑事务：把文档、选区与历史放进一个原子操作

### 6.1 事务请求

```rust
pub struct EditTransaction {
    pub base_revision: DocumentRevision,
    pub edits: Vec<Replace>,
    pub selection_after: SelectionPlan,
    pub origin: EditOrigin,
    pub history: HistoryPolicy,
}

pub enum EditOrigin {
    Keyboard,
    Mouse,
    Clipboard,
    Formatting,
    Ime,
    Undo,
    Redo,
    Programmatic,
}

pub enum HistoryPolicy {
    Record,
    MergeWithPrevious,
    ReplaceActiveComposition,
    DoNotRecord,
}
```

`SelectionPlan` 不应该简单保存“旧源码偏移 + 插入文本长度”。它需要支持：

- 将光标定位到新插入文本末尾。
- 保留选择方向。
- 根据 `AppliedChange` 变换原有 Anchor。
- 为格式化命令选择原内容而不是语法标记。
- 多光标插入后生成新的多选区。

### 6.2 提交步骤

1. 检查 `base_revision`。
2. 将 Selection Snapshot 解析到该版本。
3. 生成互不重叠的替换集合。
4. 提取旧文本，生成可逆历史操作。
5. 原子修改 Document。
6. 更新 AnchorStore 与选区。
7. 生成唯一的 `EditorCommit`。
8. 更新 History 与 dirty revision。
9. 通知 Parser / Projection / Layout 失效。
10. 一次性通知 GPUI。

如果步骤 5 之后某个缓存构建失败，**Document 不应回滚为旧内容仅为了维持缓存**；正确做法是缓存标记失效、重新计算，必要时使用安全的原始源码降级呈现。编辑事务的原子性应由不可失败的提交核心和提交前验证保证，缓存更新属于后提交派生过程。

### 6.3 删除与语法边界

`DeleteBackward` 不应简单删除 `offset-1..offset`：

- 先由导航服务计算前一个合法 grapheme。
- 若光标处于隐藏标记附近，按已定义的可视导航/展开策略决定要删除的源码范围。
- 删除可能破坏 Markdown 节点，应立即重新解析受影响范围。
- 不完整语法回退到原文显示。
- 删除操作不能因投影发生变化而重复执行。

### 6.4 格式化命令

选中 `hello` 后 `ToggleStrong`：

- 若已被完整 Strong 节点包围，则删除准确的开闭标记区间。
- 否则在源码选区两端插入 `**`。
- 选区保留在内容本身，避免把两侧新标记纳入选择。
- 多个替换必须作为一个事务。
- 嵌套与部分交叠时依据 AST 判断，不通过正则或简单字符串查找。

对于空选区的 `Cmd+B`，推荐采用“输入属性/待输入样式”状态或插入配对标记并定位中间的明确策略；必须在产品交互规格中二选一并保持一致，不能两种行为随机混用。

---

## 7. Undo/Redo：历史记录必须是语义事务

### 7.1 历史结构

```rust
pub struct HistoryEntry {
    pub forward: Vec<Replace>, // 相对于 entry.before 的版本
    pub backward: Vec<Replace>, // 相对于 entry.after 的版本
    pub selection_before: SelectionSnapshot,
    pub selection_after: SelectionSnapshot,
    pub before: DocumentRevision,
    pub after: DocumentRevision,
    pub group: HistoryGroup,
    pub origin: EditOrigin,
}

pub struct EditHistory {
    pub undo: Vec<HistoryEntry>,
    pub redo: Vec<HistoryEntry>,
    pub active_group: Option<HistoryGroup>,
}
```

重要：Undo/Redo 会产生**新的单调递增 DocumentRevision**，不能简单把文档版本号倒退。历史条目记录的是逻辑修改前后关系，而不是假设版本号永远可逆。

`backward` 的坐标必须相对于该条目应用后的文档状态；如果后续 Undo/Redo 操作或异步变更导致版本不匹配，需要通过 History 的当前栈顶状态或显式 ChangeMap 重新基准化。

### 7.2 合并策略

可合并：

- 同一连续输入会话的普通字符输入。
- 同一方向连续 Backspace。
- IME 一次完整 composition 的最终提交。

必须拆分：

- 光标显式移动。
- 鼠标选择变化。
- 粘贴。
- 格式化命令。
- 跨 Block 结构性编辑。
- 输入模式从 composition 切换到普通输入。
- Undo 后的新编辑（清空 Redo 分支）。

**不要仅依赖时间阈值**；应综合输入来源、选区连续性、事务类型、文本边界和 composition 生命周期。

### 7.3 Undo 恢复的状态

Undo 应恢复：

1. 原始 Markdown 源码。
2. 事务前的 Selection 位置和方向。
3. 必要的插入偏向和视觉亲和性。
4. 由当前 Selection 推导的 Visibility。
5. 与恢复状态一致的 Projection/Layout。

不应把完整 Projection Snapshot、像素坐标或滚动动画存入 Undo。滚动位置可以作为交互恢复提示，但不是文档历史的一部分。

### 7.4 IME 的 Undo 语义

IME 预编辑过程中可以有多次 marked-text 更新，但一次完整输入会话一般应表现为**一个可撤销的用户动作**。具体 Undo 分组须通过 macOS 原生输入测试确认，包括取消组合、替换既有选区、候选选择和连续输入。

---

## 8. IME：原生输入桥接的完整状态机

### 8.1 设计原则

IME 不是“每次收到字符串就插入 Rope”。

macOS 输入法可能反复更新 marked text、替换先前的 marked range、修改选区、提交候选文本，或取消组合。所有这些操作必须属于独立状态机。

GPUI 的具体 `InputHandler` / `PlatformInputHandler` trait 名称和签名随版本变化；在锁定版本后以源码为准。本文仅定义**应用内的稳定适配接口**。

### 8.2 组合状态

```rust
pub enum CompositionState {
    Idle,
    Active(CompositionSession),
}

pub struct CompositionSession {
    pub id: u64,
    pub base_revision: DocumentRevision,
    pub original_selection: SelectionSnapshot,
    pub original_source_range: SourceRangeAnchor,
    pub original_text: String,
    pub current_marked_range: SourceRangeAnchor,
    pub marked_selection_utf16: std::ops::Range<Utf16Unit>,
    pub visibility_lock: VisibilityLock,
}
```

`SourceRangeAnchor` 表示两个带 Bias 的 Anchor，不能简单保存会失效的裸 Range。

### 8.3 推荐状态转换

```text
Idle
  │ setMarkedText / begin composition
  ▼
Active
  ├─ updateMarkedText ────────────────► Active
  ├─ commitText / insertText ─────────► Idle (commit 1 history group)
  ├─ cancelComposition ───────────────► Idle (restore original text)
  ├─ focusLost / documentClosed ──────► explicit finalize policy
  └─ external conflicting edit ───────► reject / rebase / cancel policy
```

**不能默认 focus lost 等于提交**；必须根据 macOS 平台行为与产品预期定义，并用真实输入法验证。

### 8.4 Marked Text：两种可行内部模型

**模型 A：临时写入 Rope，组合期间不提交历史。**

- 每次 marked text 更新作为临时事务修改文档。
- Parser/Layout 能看到当前组合内容。
- 组合结束时，把从原始文本到最终提交文本的净变化合并成一条历史。
- 取消时恢复原始内容。
- 必须防止临时事务被文件保存、外部同步、Undo 或其他编辑操作误认为已提交内容。

**模型 B：Composition Overlay，不修改权威 Rope。**

- Document 保持已提交源码。
- Projection/Layout 在组合区间临时合成 marked text。
- 提交时一次性写入 Rope。
- 取消时丢弃 overlay。
- 所有命中测试、IME 几何和光标映射需要支持组合区间的双层坐标。

**本项目推荐模型 B 作为长期架构**：组合态是输入层的暂态 Overlay，不污染权威源码与 Undo，也不会意外保存未提交的候选内容。代价是 Projection 必须能接收受控的 composition overlay，并为其提供独立位置映射。

### 8.5 Composition Overlay 的数据流

```text
Committed Source Rope
       │
       ├──────────────► Markdown Syntax
       │
       ▼
Composition Overlay (temporary replacement)
       │
       ▼
IME-aware Projection
       │
       ▼
Block Layout / Caret Geometry
       │
       ▼
macOS candidate window rect
```

组合期间：

- 受影响语法节点保持可编辑的稳定显隐状态。
- Overlay 的范围不允许被语法隐藏。
- 组合文本可带下划线或输入法属性样式。
- 文档保存只读取 committed Rope；若用户在组合中触发保存，先按明确策略请求平台提交/结束组合，不能静默丢失可见内容。
- 组合提交后才创建文档事务及历史记录。
- 对用户可见的 marked text 必须与候选框位置、输入法报告的 selection range 一致。

### 8.6 UTF-16 ↔ UTF-8

macOS 文本输入范围通常以 UTF-16 code unit 表达；Rope 采用 UTF-8 byte offset。

例如：

```text
A😀中B
```

| 字符 | UTF-8 字节数 | UTF-16 单元数 |
|---|---:|---:|
| A | 1 | 1 |
| 😀 | 4 | 2 |
| 中 | 3 | 1 |
| B | 1 | 1 |

前缀边界：

| 边界 | UTF-8 byte | UTF-16 unit |
|---|---:|---:|
| 起点 | 0 | 0 |
| A 后 | 1 | 1 |
| 😀 后 | 5 | 3 |
| 中 后 | 8 | 4 |
| 末尾 | 9 | 5 |

转换接口：

```rust
pub trait TextIndexConverter {
    fn utf16_to_source(
        &self,
        offset: Utf16Unit,
        boundary: BoundaryPolicy,
    ) -> Result<SourceByte, IndexError>;

    fn source_to_utf16(
        &self,
        offset: SourceByte,
    ) -> Result<Utf16Unit, IndexError>;
}
```

`BoundaryPolicy` 必须明确 UTF-16 offset 落在 surrogate pair 中间时的处理方式：拒绝、向前取整或向后取整。不得把 UTF-16 数字直接当成 UTF-8 字节偏移。

优化策略：按行维护 UTF-8/UTF-16 前缀索引，编辑后仅更新受影响行；对长行再增加局部检查点。要注意 CRLF 在文档内部如何表示和计数，避免平台 API 与 Rope 规范不一致。

### 8.7 候选框几何

候选框矩形的来源必须是：

```text
Composition caret
  → composition-aware source/display anchor
  → projection snapshot
  → shaped visual caret stop
  → block-local rect
  → editor viewport transform
  → window/screen coordinate conversion
```

不能用“当前文本长度 × 平均字符宽度”估算。中日韩字体、emoji、ligature、Bidi 和软换行都会使这种估算失效。

### 8.8 IME 必测矩阵

- macOS 拼音：连续输入、候选切换、确认、取消。
- 日文输入：平假名、片假名、汉字转换。
- 韩文输入：组合音节。
- 英文 dead key：é、ü 等组合。
- Emoji：代理对、ZWJ 家庭 emoji。
- 选中一段 Markdown 内容后启动组合。
- 光标位于隐藏 `**` 边界后启动组合。
- 组合期间窗口缩放、滚动、焦点切换。
- 组合期间 Backspace、方向键、Cmd+Z。
- 组合期间保存、关闭窗口、外部文件变化。
- 组合结束后 Undo/Redo，验证只有一个语义撤销组。
- macOS Accessibility 文本查询与 marked range 查询。

---

## 9. 与 Projection / Layout 的协议

### 9.1 三类 Revision

```rust
pub struct EditorRevision {
    pub document: DocumentRevision,
    pub syntax: u64,
    pub presentation: u64,
    pub layout: u64,
}
```

- `document`：源码已提交变化。
- `syntax`：解析树变化。
- `presentation`：Inline visibility、Block/Document view mode、FlowBlocks、Projection、Composition Overlay 或样式变化。
- `layout`：排版宽度、字体、投影文本等影响几何的变化。

布局快照需记录它消费的版本：

```rust
pub struct LayoutSnapshotKey {
    pub document: DocumentRevision,
    pub syntax: u64,
    pub presentation: u64,
    pub viewport_width_bits: u32,
    pub typography_revision: u64,
}
```

布局版本必须与对应输入版本绑定，不能只比较一个全局递增数字。

### 9.2 文本输入后的顺序

```text
Input Commit
  1. validate source range and selection
  2. apply Document Transaction
  3. transform source Anchors
  4. update syntax incrementally
  5. recompute visibility from source selection
  6. rebuild affected projection snapshots
  7. reflow affected blocks
  8. derive caret geometry
  9. publish view invalidation
```

为保证响应速度，步骤 4–7 可分阶段执行，但用户可见的最终布局不能混用不同版本。若增量解析异步运行，应有明确的旧视图保留/同步兜底策略。

### 9.3 光标移动但源码不变

```text
Selection Changed
  → Visibility Policy
  → if projection changes: invalidate affected blocks
  → layout changed blocks
  → preserve source Anchor
  → update caret / scroll
```

这种操作不得：

- 增加 `DocumentRevision`。
- 设置文件 dirty。
- 写入 Undo。
- 重新解析整篇 Markdown。
- 依据重排后的屏幕坐标重新选择源码位置。

### 9.4 点击的时序

```text
MouseDown(pixel)
  → hit test using current committed layout snapshot
  → resolve Source Anchor + Affinity
  → update Selection
  → recompute Visibility
  → reflow if necessary
  → draw caret at same Source Anchor in new layout
```

**点击时只命中测试一次**。这是防止显隐触发光标跳动的重要不变量。

---

## 9.5 Hybrid 块模式与结构命令

Inline reveal 与 Block 模式使用独立策略。Structured 块的正文光标不展开结构前缀；EditBlockSource / ToggleDocumentSource 使用相同 Source Selection、Document、IME 和 History。模式切换只递增 presentation；设置标题、延续列表、切换复选框及段落拆分等实际修改源码时才形成 Transaction。Hybrid Return 与 Source Return 按第十一份分别决议，IME 先处理候选确认。源码态退出不撤销已经提交的输入；Undo 后以最新语义重建模式，不复用过期 BlockId。详见[第十一份](./11_GPUI_Typora_Hybrid_View_Spec.md)。

## 10. GPUI 适配层：职责清单

`editor-gpui` 负责：

1. 申请和维护文本输入焦点。
2. 注册键盘快捷键与命令分发。
3. 接收平台文本输入/marked text 更新。
4. 将平台坐标、UTF-16 range 转为引擎坐标。
5. 提供 IME 需要的文本片段、选择范围、候选框几何。
6. 绘制 caret、selection、composition underline。
7. 处理鼠标点击、拖选、双击/三击、滚动。
8. 提供 Accessibility 文本语义、选区和可视范围。
9. 在失焦、窗口关闭、文件切换时协调 composition 生命周期。

**必须源码核验的 GPUI 接口清单**：

- 文本输入 handler 的 trait、注册与生命周期。
- marked text、replacement range、selected range 的数据类型。
- IME 候选框的坐标约定。
- 字形塑形、行布局、caret hit test 能力与版本差异。
- 自定义 Element 的测量、布局、绘制与事件路由。
- 焦点切换与窗口关闭回调。
- macOS Accessibility 支持范围。

这些核验项是工程任务，不应在设计文档中假定当前 GPUI 已经提供某个精确方法签名。

---

## 11. Clipboard：复制/剪切/粘贴语义

### 11.1 默认复制 Markdown（用户已确认）

1. **Copy as Markdown / `Cmd+C`**：序列化选中内容的 Markdown 片段，保留格式与链接目标；对内容选区补齐必要的成对标记，不把临时展开的 marker 当作普通内容重复输出。
2. **Copy Source**：精确复制 Source Selection 的字节，不扩展范围、不补齐标记，供直接操作语法时使用。
3. **Copy as Plain Text**：可选的显式辅助命令，根据语义生成无格式文本，不作为默认行为。

例如 `**hello**` 中只选择 `ell`，默认复制 `**ell**`，Copy Source 则复制 `ell`。复制不修改文档、Selection 或 Undo。选区切入语法标记内部或无法可靠序列化时，默认复制回退为原始源码切片，不猜测修复；完整规则与验收以第八份 EB-040 为准。

剪贴板 `text/plain` 载荷为同一 Markdown 字符串，使普通文本应用也能粘贴出 Markdown；平台支持时可同时提供等价 `text/markdown`。`Cmd+X` 成功写入剪贴板后只删除原 Source Selection，复制时补出的标记不扩大删除范围。

### 11.2 粘贴

- `text/plain`：按源码普通文本插入。
- `text/markdown`：默认优先于 `text/plain`，作为 Markdown 原文插入；只有 plain 载荷时原样插入，不重复包裹或转义标记。
- `text/html`：若未来支持富文本粘贴，先转换成 Markdown，再以单个事务插入；转换器独立于编辑内核。
- 多行粘贴是一个 Undo group。
- 粘贴前结束或协调 IME composition，不能将粘贴直接混入 marked text。

---

## 12. 文件状态、Dirty 与保存一致性

`DocumentRevision` 递增不等于文件内容一定与磁盘不同：例如输入后 Undo 恢复到已保存内容。

建议使用：

```rust
pub struct SavePoint {
    pub content_fingerprint: ContentFingerprint,
    pub saved_revision: DocumentRevision,
}

pub struct FileSession {
    pub path: Option<std::path::PathBuf>,
    pub save_point: SavePoint,
    pub external_version: Option<FileIdentity>,
}
```

Dirty 的准确语义是**当前序列化内容是否与最后成功保存的内容相同**，不能简单用 `revision != saved_revision` 永久判定。可采用 History clean marker 加快速比较，并在不确定时通过内容指纹或字节比较验证。

保存与 IME 的关系：

- 若存在未提交 composition，必须显式处理提交/取消流程。
- 不允许只保存 committed Rope 而悄悄遗漏用户仍看得见的 marked text，除非 UI 明确表明组合尚未提交并阻止保存。
- 保存过程中应基于不可变 DocumentSnapshot 序列化，完成后只把对应快照标记为 saved。
- 若保存进行时又发生编辑，不能错误地把更新后的文档标为已保存。

---

## 13. 错误处理与可恢复性

| 错误 | 处理原则 |
|---|---|
| 过期 Revision 的编辑事务 | 拒绝或显式重基准化，绝不静默应用 |
| 非法 UTF-8 边界 | 返回错误，避免截断 |
| 过期 LayoutSnapshot | 重新布局或延迟事件，不用旧坐标写文档 |
| Parser 失败 | 保留源码，降级为原文显示 |
| Projection 构建失败 | 保留源码，使用无隐藏安全视图 |
| IME replacement range 无效 | 请求重新同步输入上下文，避免猜测替换 |
| Undo 历史不一致 | 停止应用历史，保留当前文档并记录诊断 |
| 保存失败 | 保留 dirty 状态和原磁盘文件 |
| 异步结果过期 | 丢弃结果，不覆盖最新状态 |

建议引擎内部提供结构化诊断事件，记录 revision、事务 ID、命令类型、受影响范围和缓存失效原因。不要默认记录完整用户文本，以减少隐私风险。

---

## 14. 关键测试设计

### 14.1 Anchor 属性测试

随机生成 Unicode 文本、Anchor 集合和互不重叠的 Replace，验证：

- Anchor 始终落在合法 UTF-8 边界。
- 插入点 Bias 正确。
- 单次变更与合成 ChangeMap 结果一致。
- 正向应用 + 逆向应用后，文本字节完全恢复。
- Anchor 在删除区间内的塌缩符合规范。
- 不同顺序的等价规范化操作结果一致。

### 14.2 Transaction / Undo 属性测试

- 任意有效 `ChangeSet` 应能构造准确的 inverse。
- `undo(apply(T, doc)) == doc`（比较文本与约定恢复的选择状态）。
- `redo(undo(apply(T, doc))) == apply(T, doc)`。
- 多区间替换不能发生 offset 漂移。
- Undo 后的新输入清空 Redo。
- 视觉 Projection 变化不增加历史条目。
- 源码恢复到 SavePoint 后 Dirty 正确复位。

### 14.3 Selection 黄金测试

| 源码 | 动作 | 预期 |
|---|---|---|
| `abc` | Left/Right | 按 grapheme 移动 |
| `a😀b` | Backspace | 删除完整 emoji |
| `e\u{301}x` | Backspace | 删除完整组合字素 |
| `ab**cd**ef` | 点击 `c` | 进入 Strong 并展开标记 |
| `ab**cd**ef` | 点击后重排 | 保持相同 Source Anchor |
| `**粗体**` | 跨标记选区 | 选择方向与范围正确 |
| 中英混排 + RTL | 左右方向键 | 按视觉 caret stops 移动 |
| 软换行段落 | Up/Down | preferred_x 保持 |

### 14.4 IME 端到端测试

除纯逻辑测试外，必须在 macOS 真机或真实 GUI 环境执行输入法集成测试；mock 不能证明候选框、系统选区和实际输入法协议正确。

测试结果至少记录：

- GPUI commit / macOS 版本 / 输入法。
- 初始源码、输入事件序列、最终源码。
- 组合开始/更新/提交/取消次数。
- 选区和 marked range 的 UTF-16 值。
- 候选框矩形与实际光标位置偏差。
- Undo/Redo 分组行为。

### 14.5 Fuzz / Differential Tests

- 与简单 `String` 参考模型比较 Rope 编辑结果。
- 与直接 UTF-16 遍历参考实现比较索引转换。
- 与纯文本不投影布局比较基础编辑行为。
- 随机移动光标、显隐、插入、删除、Undo/Redo，检查源码不变量。
- 在解析器无法识别的 Markdown 输入上验证“原文可编辑、无崩溃”。

---

## 15. 性能架构

### 15.1 目标复杂度

| 操作 | 目标 |
|---|---|
| 单次中间插入 | Rope 的局部编辑复杂度，避免整篇复制 |
| Anchor 解析 | 使用索引/变更日志，不全量扫描 |
| Source↔Display 映射 | Segment 二分查找 |
| 单次光标移动 | 不触发全文解析 |
| 单 Block 显隐 | 仅使受影响 Block 投影与布局失效 |
| 跨 Block 选区 | 仅绘制可视区域的选区几何 |
| Undo/Redo | 应用可逆事务，不重建整个 Document |
| UTF-16 转换 | 行级索引与局部检查点 |
| 长文档绘制 | 可视区域虚拟化与增量高度索引 |

### 15.2 内存生命周期

- Document snapshots 按引用计数/持久化节点共享。
- LayoutSnapshot 与 SourceRevision 关联，LRU 回收不可见 Block 的重型布局。
- AnchorStore 回收无引用 Anchor。
- History 设置可配置的内存预算；超预算时合并或淘汰最旧历史，并保留明确的 Undo 边界。
- Composition Overlay 在结束或取消后立即释放。
- 异步任务结果携带 Revision，过期结果不持有无限期文档快照。

---

## 16. 实施工作包与验收门槛

### E1 — Document + ChangeSet

交付：

- Rope Adapter、强类型坐标、不可变快照。
- 多区间原子 Replace、ChangeMap、inverse。
- Unicode / 随机编辑属性测试。

**门槛**：所有文本事务可逆，非法区间无部分提交。

### E2 — Anchor + Selection

交付：

- AnchorStore、Bias、选择方向、Affinity。
- Grapheme / Word / Visual navigation 接口。
- SelectionSet 规范化与跨 Block 选择。

**门槛**：编辑后 Anchor 正确迁移；Unicode 组合字符不可被拆开。

### E3 — Editor Commands + History

交付：

- 命令分发、格式化事务、历史合并。
- Undo/Redo、SavePoint、Dirty。
- 默认 Markdown 片段复制与精确源码复制；可选纯文本辅助命令。

**门槛**：任意有效编辑序列撤销重做后源码完全一致。

### E4 — Composition Overlay

交付：

- IME 状态机、Overlay、UTF-16 索引。
- Composition-aware Projection 接口。
- 组合会话的 Undo 分组。

**门槛**：逻辑层支持开始、更新、提交、取消与范围替换。

### E5 — GPUI Input Bridge

交付：

- 原生文本输入、焦点、候选框、键盘事件分流。
- 鼠标/拖选/快捷键、Accessibility 基础支持。
- 真实 macOS 输入法测试。

**门槛**：中文、日文、韩文、dead key、emoji 组合输入正确。

### E6 — 全管线集成

交付：

- EditorCommit 驱动增量 Parser/Projection/Layout。
- 版本一致的 hit test、caret geometry、scroll anchoring。
- 压力测试、长文档、错误降级。

**门槛**：Markdown 语法显隐不会造成光标漂移、重复输入、选区损坏或 Undo 污染。

---

## 17. 工程代码评审清单

- [ ] 是否存在第二个可独立编辑的 Markdown 正文副本？
- [ ] 所有 Source offset 是否声明版本与 UTF-8 边界？
- [ ] Anchor Bias 与 VisualAffinity 是否被混淆？
- [ ] 多区间替换是否基于同一个修改前版本？
- [ ] inverse 是否使用修改后的正确区间坐标？
- [ ] Undo/Redo 是否保持 DocumentRevision 单调递增？
- [ ] 语法显隐是否绝不写入 History 和 Dirty？
- [ ] IME marked text 是否与 committed Rope 明确隔离？
- [ ] UTF-16 中间代理单元是否有明确边界处理？
- [ ] 鼠标命中测试是否只使用一个布局快照？
- [ ] 显隐重排后是否保持 Source Anchor 而非旧像素位置？
- [ ] 过期 Parser/Layout 结果是否会被拒绝？
- [ ] 文件保存是否针对确定的 committed snapshot？
- [ ] 失败时是否优先保护原始 Markdown 数据？
- [ ] 是否有真实 macOS 输入法回归测试？

---

## 18. 需要在编码前明确的交互决策

以下属于**产品行为规范**。Hybrid 相关已确认决策以第十一份为准，其他事项可参考 Typora 实测或自主定义：

1. 光标位于 `**` 与内容之间时，标记何时展开？
2. 非折叠选区的 Inline 展开与拖选冻结策略；Structured Block 不因选区相交而展开外层 marker。
3. **已确认**：默认 `Cmd+C` 复制保留格式的 Markdown 片段；Copy Source 为精确源码切片，规则见第八份 EB-040。
4. 空选区 `Cmd+B` 是切换待输入属性还是插入 `****`？
5. Backspace 在隐藏标记边界时，先展开还是直接删除可见字素？
6. 点击链接文本时默认移动光标还是打开链接？是否需要修饰键？
7. IME 组合期间触发保存/关闭时，提交、取消还是阻止操作？
8. Home/End 与 Cmd+方向键在软换行、RTL 文本中的精确定义。
9. 单文件编辑器是否默认自动保存，还是显式 Cmd+S？
10. 文件外部修改时如何合并或提示冲突？

**建议**：将这些决策形成 `editing-behavior-spec.md`，作为黄金测试的依据。没有确定行为规范，测试只能证明实现自洽，不能证明它符合目标体验。

---

## 19. 三份文档的衔接

1. **第一份**《GPUI Live Markdown Architecture》：总体模块、单文件产品边界与全局不变量。
2. **第二份**《GPUI Markdown Projection & Layout Implementation》：Markdown 标记显隐、双向位置映射、布局与几何。
3. **第三份（本文）**：文档编辑、Anchor、事务、选区、历史、IME 和平台事件协议。

三个模块组之间最重要的契约是：

```text
Editor Engine 只提交 Source Transaction 与 Source Selection
        ↓
Syntax / Projection 只派生 Presentation Snapshot
        ↓
Layout 只派生 Geometry Snapshot
        ↓
GPUI 只负责平台输入、交互与绘制
```

任何层都不允许越过这些边界私自修改 Markdown 源码。

---

## 20. 最终技术结论

**编辑器内核的核心不是“把文本画出来”，而是维护一条始终正确的状态演进链：**

```text
Platform Input
    → Composition / Command
    → Source Transaction
    → Anchor + Selection
    → Incremental Syntax
    → Cursor-aware Projection
    → Block Layout
    → Native Caret / Selection / IME Geometry
```

只要 `Document`、`Anchor`、`Transaction`、`Composition` 与 `Projection` 的契约足够严格，Markdown 语法显隐就可以成为原生编辑器的正常呈现能力，而不是一个脆弱的 UI 特效。

**下一份适合深入的主题**：`Markdown Syntax Engine`——增量 AST、精确 delimiter ranges、CommonMark/GFM 语义、嵌套节点、未闭合语法与增量失效边界。
