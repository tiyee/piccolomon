# GPUI Live Markdown — 第七份：核心算法、数据结构与形式化不变量

> 2026-10-12 Hybrid 修订：交互路线与阶段范围以[第十一份](./11_GPUI_Typora_Hybrid_View_Spec.md)为准。Inline 按光标显隐；Block 保持结构化，显式进入局部/全文源码；模式切换不改源码或历史。下文领域 API 为设计草案，须纳入 Hybrid presentation 版本依赖后实施。

> 文档状态：架构级算法规格（v0.1）  
> 目标：Rust + GPUI，单一 Rope、原生 macOS、光标感知 Live Markdown  
> 关联：第 1–6 份方案  
> 范围：AnchorStore、ChangeMap、Projection Interval Map、CaretStop Graph、Block HeightIndex、快照与属性测试  
> 说明：Rust 代码为**参考接口与核心算法示意**，不是经过编译验证的成品。GPUI 的具体 API 必须另行核验。

---

## 1. 总体目标与架构边界

第七份不再重复模块架构，而是定义支撑前六份的底层算法契约。关键问题是：**源码发生多区间编辑、Markdown 标记显隐、文本重新塑形和视口高度变化时，如何保证所有位置映射仍然正确且复杂度可控。**

```text
Document(Rope) ──ChangeSet──► ChangeMap ──► AnchorStore
      │                                         │
      ▼                                         ▼
SyntaxSnapshot ──► Projection Segments ◄── Selection Anchors
                           │
                           ▼
                    Display Offset
                           │
                           ▼
                Shaped Visual Caret Graph
                           │
                           ▼
                    Block Geometry
                           │
                           ▼
                 HeightIndex / Viewport
```

全局硬性约束：

1. 编辑的唯一权威坐标是 `SourceByte`，单位 UTF-8 字节。
2. 每个映射必须声明输入/输出版本，不能混合不同 revision。
3. 显隐只修改 Projection，不修改 Rope、History 或 Dirty。
4. 编辑事务基于**同一个旧文档版本**，先规范化再提交。
5. 一个 Display offset 可能对应多个 Source offset；映射**不是双射**。
6. 文本布局后的光标移动由视觉 caret stops 决定，不能按字节简单加减。
7. Block 高度缓存允许估计，但编辑、点击和 IME 几何不得依赖未经确认的估计坐标。
8. 正确性先于增量性能；不满足复用前提时必须重新计算。

### 1.1 类型系统隔离坐标

```rust
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Debug)]
pub struct SourceByte(pub usize);

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Debug)]
pub struct DisplayByte(pub usize);

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct DocumentRevision(pub u64);

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct ProjectionRevision(pub u64);

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum Bias { Before, After }

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum VisualAffinity { Upstream, Downstream }
```

`Bias` 解决编辑插入点的锚定；`VisualAffinity` 解决同一逻辑位置在 Bidi、软换行和隐藏语法下的视觉歧义。**两者绝不合并成一个布尔值。**

---

## 2. ChangeSet：多区间原子编辑

### 2.1 数据模型

```rust
pub struct Replacement {
    pub old: std::ops::Range<SourceByte>,
    pub inserted: String,
}

pub struct ChangeSet {
    pub base: DocumentRevision,
    pub replacements: Vec<Replacement>,
}

pub struct NormalizedEdit {
    pub old_start: usize,
    pub old_end: usize,
    pub inserted_bytes: usize,
    pub removed_bytes: usize,
    pub new_start: usize,
    pub new_end: usize,
}

pub struct ChangeMap {
    pub before: DocumentRevision,
    pub after: DocumentRevision,
    pub edits: Vec<NormalizedEdit>,
}
```

`ChangeSet` 的所有 old ranges 均相对于修改前同一个 Rope snapshot。禁止一个 edit 的区间基于前一个 edit 已修改后的坐标。

### 2.2 规范化算法

1. 验证 `base == current_document_revision`。
2. 对所有区间验证 `start <= end <= len_bytes`、UTF-8 边界有效。
3. 按 `(start, end)` 升序排序。
4. 拒绝重叠替换；对于**同点零宽插入**，必须先按稳定的输入顺序合并成单一插入，避免排序不稳定。
5. 同点零宽插入与覆盖该点的非空替换不能靠排序偶然决定语义；调用方必须明确合并或拒绝。
6. 使用累积字节增量计算 `new_start/new_end`。
7. 预先提取旧文本，为 Undo 构造 post-edit 坐标系下的逆向编辑。
8. 从后向前对 Rope 应用原始替换，最后只递增一次 revision。

伪代码：

```rust
let mut delta: i128 = 0;
for edit in edits.iter_mut() {
    edit.new_start = checked_add_signed(edit.old_start, delta)?;
    edit.new_end = edit.new_start.checked_add(edit.inserted_bytes).ok_or(Error::Overflow)?;
    delta += edit.inserted_bytes as i128 - edit.removed_bytes as i128;
}
```

**注意**：`new_end` 表示插入内容在新文档中的末尾；旧范围的 `old_end` 经 ChangeMap 的位置映射，还要考虑边界偏向。使用 `i128`/checked arithmetic 只是示意，实际需检查 `usize` 上限及内存预算。

### 2.3 可逆性

对原文 `abcdefghi`，事务：

- `[1,3)` → `XYZ`（`bc` 替换为 `XYZ`）。
- `[6,8)` → 空（删除 `gh`）。

新文档：`aXYZdefi`。

映射表：

| Edit | old | new | inverse（新文档坐标） |
|---|---|---|---|
| 替换 `bc` | `[1,3)` | `[1,4)` | `[1,4)` → `bc` |
| 删除 `gh` | `[6,8)` | `[7,7)` | `[7,7)` → `gh` |

逆向事务的两处范围必须使用**新文档坐标**，再作为同一批 edits 规范化并逆序应用。不能简单把旧范围存起来用于 Undo。

### 2.4 变更位置映射

定义 `map_old_to_new(offset, bias)`。对不在替换范围内的位置，增加所有位于其前方 edit 的净长度差。对替换区间内部或边界，采用明确的偏向规则。

推荐**区间替换规则**：

- 旧位置 `p < s`：不受当前替换影响。
- `p > e`：加上 `inserted_len - (e-s)`。
- `s < p < e`：`Before` 映射到新 `s`，`After` 映射到新 `s + inserted_len`。
- `p == s`：`Before` 映射到新 `s`；`After` 映射到新 `s + inserted_len`。
- `p == e`：`Before` 映射到新 `s`；`After` 映射到新 `s + inserted_len`。

这里的边界规则适用于**替换关联语义**，但对于删除后的 `p==e` 位置，部分编辑器更希望其始终落在折叠点。两种方案都可实现；**必须由正式 `AnchorPolicy` 明确**，不能在不同模块使用不同定义。

#### 推荐更精确的 API

```rust
pub enum PositionMapping {
    Exact(SourceByte),
    Collapsed {
        before: SourceByte,
        after: SourceByte,
    },
}

pub fn map_old_to_new(
    map: &ChangeMap,
    old: SourceByte,
) -> Result<PositionMapping, MappingError>;
```

由 Anchor Bias 将 `Collapsed` 决议为最终位置。这个设计显式保留替换区间内的信息丢失，避免假装映射可逆。

### 2.5 算法复杂度

| 操作 | 目标复杂度 |
|---|---|
| 规范化 k 个 edits | `O(k log k)` |
| 计算所有新坐标 | `O(k)` |
| 单点旧→新映射 | `O(log k)`（二分定位 edit） |
| 批量有序 m 个点映射 | `O(m + k)` |
| 多区间 Rope 编辑 | 取决于 Rope 实现，避免整篇复制 |
| 生成 inverse | `O(被删除字节数 + k)`，不含 Rope 索引开销 |

---

## 3. AnchorStore：稳定位置与版本演进

### 3.1 两种架构的取舍

**Eager anchors**：每次事务立即调整所有 Anchor。容易证明，但若 AST/Selection/Viewport 共持有海量 Anchor，则单次编辑可能 `O(A)`。

**Lazy anchors**：Anchor 保存创建版本和位置，通过版本化 ChangeMap 链按需解析。若简单逐条回放，解析成本可达 `O(H log k)`（H 为版本链长度）；因此必须设计压缩、索引和快照。

推荐采用**分层 AnchorStore**：

- 长期持有的交互 Anchor：Selection、ScrollAnchor、Composition。
- 短期语法范围：直接归属于 SyntaxSnapshot，不注册全局 Anchor。
- Anchor 解析：以 `AnchorEpoch` 为单位压缩 ChangeMap 链。
- 对经常查询的 Anchor，可在新 revision 上做解析缓存。

### 3.2 Anchor 类型

```rust
pub struct SourceAnchor {
    pub document_id: DocumentId,
    pub id: AnchorId,
    pub bias: Bias,
}

struct AnchorRecord {
    born_revision: DocumentRevision,
    born_offset: SourceByte,
    cached: Option<(DocumentRevision, SourceByte)>,
}
```

`document_id` 防止不同文件的 Anchor 混用。`AnchorId` 不得因存储槽复用而意外引用新 Anchor；可以采用 `(slot, generation)`。

### 3.3 Epoch 压缩

每 N 次事务或 ChangeLog 达到内存阈值时：

1. 为仍存活的长期 Anchor 解析到最新 revision。
2. 更新这些 Anchor 的 `born_revision/born_offset`。
3. 丢弃不再被任何快照或 Undo 依赖的旧 ChangeMap。
4. 更新 epoch 的基准 revision。

不能在仍有旧 DocumentSnapshot、异步任务或历史记录需要映射时无条件删除旧版本。建议用 `RevisionLease`/引用计数描述最低可回收版本。

### 3.4 区间删除的 Anchor 语义

Anchor 位于被删范围内部时，旧字符已不存在。应按 Bias 折叠到替换后的左/右边界；这不是“恢复原位置”，而是一个明确定义的语义选择。

Undo 恢复原文本时，**不能假设所有折叠 Anchor 会自动回到原位置**。历史必须保存选区快照或语义位置恢复计划。

### 3.5 性能与回收

- Selection Anchor 数量通常小，解析应接近常数时间。
- 避免为每个 Markdown Token 创建持久 Anchor。
- AnchorStore 需提供生命周期跟踪和泄漏检测。
- 版本链必须有可控内存上界；不得无限累积。
- 性能指标：Anchor resolve P50/P99、epoch compaction 时间、live anchor 数量。

---

## 3.6 Hybrid 模式与结构映射

InlineVisibilityPolicy 与 BlockPresentationPolicy 分开决议，Hybrid presentation revision 必须进入快照依赖。结构前缀的隐藏绑定 Structured 模式而非正文光标；BlockSource 使用同一源码的无隐藏视图。FlowBlocks 为 HeightIndex 提供不重复占高的内容序列。H2 表格的 cell graph、结构命中与各 cell 的源区间映射独立于下文的线性 SegmentMap；二维视觉导航不承诺源字节单调。模式切换保留 Source anchors，语法内部位置按第十一份归一化，组合坐标继续使用 EditablePosition::Composition。

## 4. Projection：不是字符串替换，而是单调的分段关系

### 4.1 核心问题

源码：`ab**cd**ef`，UTF-8 byte offsets：

```text
Source:  a b * * c d * * e f
Offset: 0 1 2 3 4 5 6 7 8 9 10
Display: a b     c d     e f
Offset: 0 1 2     3 4     5 6
```

隐藏区间 `[2,4)`、`[6,8)`。可见文本 `abcdef`。

```text
Visible [0,2) source -> [0,2) display
Hidden  [2,4) source -> display boundary 2
Visible [4,6) source -> [2,4) display
Hidden  [6,8) source -> display boundary 4
Visible [8,10) source -> [4,6) display
```

因此 `source_to_display(2)`、`source_to_display(3)`、`source_to_display(4)` 都可能返回同一个 display offset 2；反向映射必须返回候选或按 Affinity 决议。

### 4.2 Segment 类型

```rust
pub enum ProjectionSegmentKind {
    Visible,
    HiddenSyntax { owner: NodeId },
    Replacement { owner: NodeId, widget_id: WidgetId },
    CompositionOverlay { session_id: u64 },
}

pub struct ProjectionSegment {
    pub source: Range<SourceByte>,
    pub display: Range<DisplayByte>,
    pub kind: ProjectionSegmentKind,
}
```

**单调映射**只适用于保持源码顺序的文本段；Replacement Widget/Overlay 需要明确映射契约，不能假定源字节与显示字节一一对应。

### 4.3 规范化 Segment 不变量

1. 所有 source ranges 以起点排序，且覆盖目标 Block 的全部源区间，无洞、无重叠。
2. `Visible`：`source.len_bytes == display.len_bytes`，且字节内容相同。
3. `HiddenSyntax`：display range 长度为 0。
4. Display ranges 连续且单调，不允许相互重叠。
5. Segment 边界必须是 UTF-8 code point 边界；可编辑 caret 还需是 grapheme boundary。
6. `HiddenSyntax` 必须来源于同版本 SyntaxSnapshot 的可信 Marker。
7. 不完整/不确定 Marker 不隐藏。
8. 相邻相同类型的 Segment 可以合并，但合并不得丢失不同 marker owner 的交互语义。
9. Snapshot 绑定 `DocumentRevision + SyntaxRevision + PresentationRevision`。

### 4.4 构造算法

输入：FlowBlock source range `[a,b)` 与可信、经 InlineVisibilityPolicy / BlockPresentationPolicy 决定隐藏的 marker ranges；此线性算法不承担 H2 表格二维导航。

1. 检查每个 marker 均在 Block 范围内且版本匹配。
2. 按 `start,end` 排序。
3. 合并可合法合并的隐藏范围；**不同 owner 重叠先报错，不盲目 union**。
4. 扫描 source，生成 Visible gap 与 HiddenSyntax Segment。
5. 同步累加 display offset。
6. 构建 source-start 与 display-start 二分索引。
7. 验证 `concat(Visible text) == projected display text`。

```rust
fn build_elision_projection(
    source: &str,
    hidden: &[HiddenMarker],
) -> Result<Vec<ProjectionSegment>, ProjectionError> {
    // 输入范围已完成版本校验、排序和冲突检查。
    let mut segments = Vec::new();
    let mut src = 0usize;
    let mut dst = 0usize;
    for marker in hidden {
        let s = marker.range.start.0;
        let e = marker.range.end.0;
        if s < src || e > source.len() || !source.is_char_boundary(s)
            || !source.is_char_boundary(e) {
            return Err(ProjectionError::InvalidRange);
        }
        if src < s {
            segments.push(ProjectionSegment::visible(src..s, dst..dst + (s-src)));
            dst += s-src;
        }
        segments.push(ProjectionSegment::hidden(s..e, dst, marker.owner));
        src = e;
    }
    if src < source.len() {
        segments.push(ProjectionSegment::visible(src..source.len(), dst..dst + source.len()-src));
    }
    Ok(segments)
}
```

以上伪代码使用 Block-local offsets；正式 API 需要在类型上区分 `BlockSourceByte` 与全局 `SourceByte`，或在构造时显式加上 Block 起点。

### 4.5 Source → Display

对源位置 `s`：

- 落在 Visible `[a,b)`：`d = display.start + (s-a)`。
- 落在 Hidden `[a,b)`：`d = display.start`。
- 位于 Segment 边界：可能有多个合法解释；输出 `DisplayPosition { offset, affinity }`。

```rust
pub struct DisplayPosition {
    pub byte: DisplayByte,
    pub affinity: VisualAffinity,
}
```

Source→Display 的**数值偏移**是单调非递减的，但位置语义并非可逆。

### 4.6 Display → Source：返回歧义集

对于 display offset 2，可能对应 source 2、3、4；中间的 3 是隐藏 marker 内部，不应成为用户 caret 停靠点。推荐 API：

```rust
pub struct SourceBoundaryCandidates {
    pub before_hidden: Option<SourceByte>,
    pub after_hidden: Option<SourceByte>,
    pub preferred: SourceByte,
}

pub fn display_to_source_candidates(
    &self,
    position: DisplayPosition,
) -> SourceBoundaryCandidates;
```

**不要将 `display_to_source(source_to_display(s)) == s` 作为所有 Source offset 的不变量**；它对隐藏区间内部必然不成立。正确的不变量是：

- 对可见且合法的 Source caret stop，按对应 Affinity 往返应得到相同**语义边界**。
- 对隐藏区间内位置，往返结果属于该隐藏区间的合法边界集合。
- 映射保持源码顺序，不跨越其他可见内容。

### 4.7 结构化 Marker 与多重隐藏

对于 `***hello***`，同一连续 delimiter run 可拆成归属不同语义节点的 `MarkerAtom`。Projection 不能让两个节点同时声称对同一个字节拥有独立隐藏权。

建议采用：

```rust
pub struct MarkerAtom {
    pub range: Range<SourceByte>,
    pub owner: NodeId,
    pub role: MarkerRole,
}
```

MarkerAtom 互不重叠。InlineVisibilityPolicy 与 BlockPresentationPolicy 分别决定所属 Atom 的显隐状态。嵌套节点是否联动展开属于交互策略，不属于分段映射算法。

### 4.8 索引与复杂度

| 操作 | 目标复杂度 |
|---|---|
| 构建 m 个隐藏区间 | `O(m log m + B)`，B 为构造输出段数 |
| Source→Display | `O(log m)` |
| Display→Source 候选 | `O(log m + c)`，c 为同一边界的候选数 |
| 更新一个 Inline 显隐 | 局部重建所在 Block，避免全文 |
| 拼接投影文本 | `O(显示字节数)`，应延迟到 shaping 时按需执行 |

**是否需要 Segment Tree？** 如果每个 Block 的 marker 数通常较小，`Vec<Segment>` + 二分索引更简单且缓存友好。只有真实基准显示单 Block 中频繁局部编辑/投影重建成本不可接受时，才引入带前缀长度聚合的 B-tree/rope-like projection tree。**“高级数据结构”不是目的；可证明正确和性能上限才是目的。**

---

## 5. CaretStop Graph：视觉导航不是偏移加减

### 5.1 为什么需要图

在以下场景，源码字节顺序、显示字节顺序、屏幕从左到右顺序不一致：

- 阿拉伯文/希伯来文与英文混排。
- 软换行边界。
- ligature（例如 `fi`）。
- emoji ZWJ 与组合字素。
- 隐藏 Markdown 标记的多对一投影。
- Inline Widget。

因此定义**每条视觉行的有序 caret stops**，并在行与行之间建立导航边。

### 5.2 核心结构

```rust
pub struct CaretStopId(pub u32);

pub struct CaretStop {
    pub id: CaretStopId,
    pub source_anchor: SourceAnchor,
    pub display: DisplayPosition,
    pub visual_line: VisualLineId,
    pub x: f32,
    pub y: f32,
    pub bidi_level: u8,
    pub affinity: VisualAffinity,
}

pub struct CaretEdges {
    pub left: Option<CaretStopId>,
    pub right: Option<CaretStopId>,
    pub up: Option<CaretStopId>,
    pub down: Option<CaretStopId>,
}
```

不建议在长期 LayoutSnapshot 内为每个 caret stop 创建全局持久 Anchor；可存 `SourceByte + LayoutRevision`，在对外生成交互 Selection 时创建 Anchor。上面 `SourceAnchor` 是语义示意，实际应避免海量 AnchorStore 负担。

### 5.3 构图步骤

1. 从 Projection 获取 Display 文本及合法 Source 边界。
2. 按 Unicode grapheme segmentation 获取允许导航的边界。
3. 通过字体 shaping 获取 cluster 与 glyph 几何。
4. 通过 Unicode Bidi 规则获取视觉行内顺序。
5. 为每个合法边界生成一到多个 `(display offset, affinity, x)` caret stops。
6. 按视觉 x 和 Bidi 行为定义左右边；在相同 x 的双亲和位置保留可区分节点。
7. 按 `preferred_x` 计算上下行最接近的合法 stop；`preferred_x` 是 Selection 的临时导航状态。
8. 行首/行尾、文首/文尾通过明确命令映射，而不是随意沿图走到第一个节点。

### 5.4 Bidi 特别注意

在 Bidi 交界，`left`/`right` 可能需要在同一 x 处切换 Affinity，随后再移动到下一个视觉 stop。具体规则应与 macOS 原生编辑行为对齐，并以平台实测为准。

不能保证“每次 Right 的 SourceByte 都增加”。能保证的是：在同一布局快照、相同导航策略下，视觉导航确定且不会停在非法字素内部。

### 5.5 Up/Down 算法

```text
current caret stop
   ↓
retain preferred_x (首次垂直移动时取当前 x)
   ↓
find previous/next VisualLine
   ↓
choose stop minimizing |x - preferred_x|
   ↓
resolve tie using affinity / bidi policy
   ↓
return Source position + affinity
```

复杂度：若行内 caret stops 按 x 排序，目标行二分查找 `O(log C)`；若行内同 x 多 stop，需处理局部等价组。`preferred_x` 在连续上下移动期间保持；横向移动、鼠标点击、输入等操作重置。

### 5.6 Graph 的生命周期

- 仅对已布局的视觉行建立完整导航索引。
- 远距离上下跳转可按需布局目标行。
- LayoutSnapshot 改变后，旧 `CaretStopId` 失效；持久 Selection 仍是 Source Anchor。
- 对同一 `LayoutSnapshot` 的 hit test、caret rect、上下导航使用相同 stop 集合。

### 5.7 几何往返属性

对任意可见合法 caret stop `c`：

```text
caret_geometry(c) → point near caret → hit_test(point)
```

返回的 Source 位置应与 `c` **在预定义的边界等价关系下相同**。在重叠 glyph、零宽边界和 Bidi 双亲和位置，不能强求每个像素点都唯一反推出原 stop；必须定义 tie-breaking 和等价类。

---

## 6. HeightIndex：动态 Block 高度与快速滚动

### 6.1 需求

文档有 N 个 Block，每个 Block 的高度可能随着：

- 投影显隐。
- 字体/字号/字重。
- 窗口宽度和软换行。
- 列表缩进与结构装饰。
- Inline Widget。

而变化。要求：

- 查询文档总高度。
- 查询第 i 个 Block 的文档 y。
- 根据滚动 y 找到 Block。
- 单个 Block 高度变化后快速更新。
- 插入/删除 Block 后保持性能。

### 6.2 Fenwick Tree：适用于稳定 Block 序列

若 Block 数量和顺序在一次编辑期间相对稳定，Fenwick Tree 支持：

- `prefix_sum(i)`：`O(log N)`。
- `update(i, delta)`：`O(log N)`。
- `lower_bound(y)`：`O(log N)`。
- `total_height()`：`O(log N)` 或缓存为 `O(1)`。

但**Fenwick Tree 不适合频繁在中间插入/删除 Block**：普通数组需要移动并重建索引，可能 `O(N)`。

### 6.3 长期推荐：带高度聚合的 B-tree

如果要求结构性编辑中也保持可控复杂度，建议用**隐式索引 B-tree / order-statistics tree**：

每个内部节点缓存：

```rust
pub struct HeightAggregate {
    pub block_count: usize,
    pub total_height: f64,
    pub measured_count: usize,
}
```

每个叶子存：

```rust
pub struct BlockHeightEntry {
    pub block_id: BlockId,
    pub height: f64,
    pub status: HeightStatus,
    pub layout_key: Option<BlockLayoutCacheKey>,
}

pub enum HeightStatus {
    Measured,
    Estimated,
}
```

可实现：

| 操作 | 平衡树目标复杂度 |
|---|---|
| `block_at_y(y)` | `O(log N)` |
| `prefix_height(i)` | `O(log N)` |
| `update_height(i,h)` | `O(log N)` |
| `insert_blocks(i, items)` | `O(log N + k)` 均摊，视具体 B-tree 实现 |
| `remove_blocks(range)` | `O(log N + k)` 或按子树删除优化 |
| `total_height` | `O(1)` 根聚合 |

**数值稳定性**：累计高度建议使用 `f64`，最终绘制时转换为平台逻辑像素类型；对于极长文档可降低 `f32` 前缀累计误差。高度必须有限、非负，拒绝 NaN/Infinity。

### 6.4 y → Block 的边界

定义 Block 的半开纵向区间 `[prefix(i), prefix(i+1))`。对于零高度 Block、恰好在边界上的 y，需要明确 tie-breaking：优先下一个非零高度 Block，文末 clamp 到最后合法 caret 位置。

### 6.5 Estimated vs Measured

HeightIndex 可以保存估计值用于滚动条和预取，但**鼠标命中/IME 不能使用估计高度直接生成文本坐标**。

若目标 Block 尚未布局：

1. 用 HeightIndex 找候选 Block。
2. 按需完成该 Block 的真实布局。
3. 更新高度并调整 Scroll Anchor。
4. 用真实 BlockLayout 完成 hit test。
5. 若更新改变 y→Block 归属，重新求解到一致或达到明确的同步布局边界。

### 6.6 Scroll Anchor 补偿

锚点表示 `(SourceAnchor, desired_viewport_y)`，布局变化前后：

```text
new_scroll_y = new_document_y(anchor) - desired_viewport_y
```

其中 `new_document_y(anchor)` 必须来自**新版本**布局。滚动偏移再 clamp 到 `[0, max(total_height - viewport_height, 0)]`。

**用户主动滚动期间**应优先保持视口顶部内容，而不是强制跳回编辑光标。

---

## 7. 版本化快照与增量失效 DAG

### 7.1 版本模型

```rust
pub struct SnapshotKey {
    pub document: DocumentRevision,
    pub syntax: u64,
    pub presentation: ProjectionRevision,
    pub hybrid: PresentationRevision,
    pub composition: u64,
    pub typography: u64,
    pub width: u64,
    pub layout: u64,
}
```

`DocumentRevision` 单调递增，Undo/Redo 也创建新 revision。其他版本仅表示派生状态更新。不同模块的缓存键必须包括所有影响结果的输入。

### 7.2 失效 DAG

```text
Document edit ──► Syntax affected nodes
      │                 │
      └────────────────► Projection affected blocks
                              │
                              ▼
                       Shaping/Wrap
                              │
                              ▼
                         Block height
                              │
                              ▼
                         HeightIndex
                              │
                              ▼
                         Viewport paint

Selection move ─► Inline visibility ─► Projection (only if changed)
Block/Document mode ─► Block presentation ─► FlowBlocks/Projection
Composition update ─► Editable text/Projection (no committed parse)
Caret blink ──────────────────► Paint only
Theme color ──────────────────► Paint only (unless font metrics change)
```

### 7.3 缓存键：不能过度全局化

若一个 Block 的源码、语义、投影和字体依赖均未变化，不能仅因为全局 `DocumentRevision` 增加就使它失效。

建议：

```rust
pub struct BlockCacheKey {
    pub block_stable_id: BlockId,
    pub source_fingerprint: u128,
    pub syntax_fingerprint: u128,
    pub projection_fingerprint: u128,
    pub block_presentation_fingerprint: u128,
    pub composition_fingerprint: u128,
    pub typography_revision: u64,
    pub width_bits: u32,
}
```

指纹只是缓存复用的候选依据；需要避免碰撞风险。对于关键正确性路径，使用内容等价校验或足够强的哈希并结合长度/结构版本。缓存结果仍携带来源快照身份。

### 7.4 异步任务发布规则

后台解析/布局完成后：

- 检查依赖快照键。
- 若当前状态不兼容，丢弃或仅复用可证明未变的局部结果。
- 绝不把旧 `CaretStopGraph` 用于当前输入事件。
- 当前正在输入的 Block 若没有一致布局，优先同步布局或保持旧完整帧，而不是“新文本 + 旧光标”。

---

## 8. Composition Overlay 的位置映射

IME 的临时文本不能作为已提交 Rope 的一部分。Composition Overlay 应以**临时替换段**进入 Projection：

```rust
pub struct CompositionOverlay {
    pub source_range: SourceRangeAnchors,
    pub marked_text: String,
    pub marked_selection_utf16: Range<Utf16Unit>,
    pub session_id: u64,
}
```

这时存在三套位置：

1. Committed SourceByte（权威 Rope）。
2. Composition-local UTF-16 / UTF-8（临时文本）。
3. DisplayByte（布局文本）。

Overlay 内部的 caret **不能强行映射成某个已提交 SourceByte**。需要联合位置类型：

```rust
pub enum EditablePosition {
    Source { anchor: SourceAnchor, affinity: VisualAffinity },
    Composition { session_id: u64, utf16: Utf16Unit },
}
```

组合结束后，将 overlay 净替换提交为单个 ChangeSet，再由 `SelectionPlan` 把 composition caret 映射为新的 Source Anchor。

### 8.1 为什么这是必要的

例如原文 `ab**cd**ef`，用户选中 `cd` 后输入法 marked text 为 `你好`。如果强行把组合内部第一个字与第二个字的光标映射到旧 SourceByte，旧 Rope 并不存在这些字符，候选框、删除与 UTF-16 range 会发生歧义。

### 8.2 Overlay 与 Visibility

- Overlay 范围永远可见。
- 相关 Markdown marker 进入稳定的展开策略。
- 组合更新不创建正式 History entry。
- 组合取消丢弃 Overlay；原 Rope 不变。
- 组合提交创建一个可撤销的编辑事务。
- 保存/关闭时必须先明确处理组合生命周期。

---

## 9. Rust 参考模块接口

```rust
pub trait ChangeMapper {
    fn old_to_new(&self, old: SourceByte) -> PositionMapping;
    fn new_to_old(&self, new: SourceByte) -> PositionMapping;
}

pub trait AnchorResolver {
    fn resolve(&self, anchor: SourceAnchor, revision: DocumentRevision)
        -> Result<SourceByte, AnchorError>;
}

pub trait ProjectionMapper {
    fn source_to_display(&self, source: SourceByte, affinity: VisualAffinity)
        -> Result<DisplayPosition, ProjectionError>;

    fn display_to_source_candidates(&self, display: DisplayPosition)
        -> Result<SourceBoundaryCandidates, ProjectionError>;
}

pub trait CaretNavigator {
    fn move_horizontal(&self, from: CaretStopId, dir: HorizontalDirection)
        -> Option<CaretStopId>;

    fn move_vertical(&self, from: CaretStopId, preferred_x: f32,
        dir: VerticalDirection) -> Option<CaretStopId>;
}

pub trait HeightIndex {
    fn total_height(&self) -> f64;
    fn block_at_y(&self, y: f64) -> Option<BlockIndex>;
    fn prefix_height(&self, index: BlockIndex) -> f64;
    fn update_height(&mut self, index: BlockIndex, height: f64)
        -> Result<(), HeightError>;
    fn insert(&mut self, index: BlockIndex, entries: &[BlockHeightEntry]);
    fn remove(&mut self, range: Range<BlockIndex>);
}
```

**代码审查要求**：接口不得接受无类型 `usize` 作为跨模块位置；若必须暴露裸索引，必须局限在内部、命名清楚并写明单位与版本。

---

## 10. 属性测试：数学不变量优先

### 10.1 ChangeMap

生成随机合法 Unicode 文本与互不重叠 edits：

- 应用 ChangeSet 与简单 `String` 参考模型结果一致。
- `apply(forward)` 后再应用生成的 inverse，原文字节完全恢复。
- 旧→新位置映射单调非递减。
- 区间外位置的映射符合累计长度差。
- 版本不匹配、越界、非 UTF-8 边界必须拒绝。
- 同点插入合并顺序确定。

### 10.2 AnchorStore

- Anchor 解析不产生越界或非法 UTF-8 边界。
- 插入点 Before/After 行为符合定义。
- 立即变换模型与 Lazy/Epoch 模型输出一致。
- 历史压缩前后所有存活 Anchor 解析结果一致。
- 不同 Document ID 不能交叉解析。
- Anchor 被删除范围覆盖时，折叠语义稳定。

### 10.3 Projection

- Segments 在 Source 维度完整覆盖 Block。
- Display 维度连续、单调、无重叠。
- 所有 HiddenSyntax 的显示长度为零。
- 所有 Visible 段的源码字节与显示字节相等。
- 隐藏 Marker 精确归属于可信 Syntax Node。
- Source→Display 数值映射单调。
- 可见合法 caret 的往返映射保持语义等价。
- 隐藏区间内部不会被错误宣称完全可逆。

### 10.4 CaretStop Graph

- 所有 stop 对应合法 grapheme boundary 或明确的 Composition position。
- 同一 LayoutSnapshot 内左右/上下导航确定。
- 左右移动不会停在 UTF-8 continuation byte。
- 视觉行内的 stop x 坐标符合塑形结果。
- `preferred_x` 在连续上下移动期间保持。
- Bidi 边界保留必要的 Affinity 区分。
- 重排后旧 CaretStopId 不能直接复用。

### 10.5 HeightIndex

- 前缀和与简单 `Vec<f64>` 参考实现一致（允许规定的浮点误差）。
- 任意合法 y 能定位正确 Block。
- 随机插入、删除、更新后总高度正确。
- 拒绝负数、NaN、Infinity。
- 估计→实测后 ScrollAnchor 的 viewport y 误差处于规定容差。
- 零高度 Block 的边界行为确定。

### 10.6 Fuzz Targets

```text
fuzz_changeset_inverse
fuzz_anchor_epoch_compaction
fuzz_projection_segment_map
fuzz_caret_navigation
fuzz_height_index_mutations
fuzz_ime_overlay_mapping
fuzz_full_editor_event_sequence
```

所有 fuzz failure 都应保留最小复现种子、初始源码、事件序列、相关版本号。

---

## 11. 黄金用例与预期

### 11.1 隐藏 Strong

```text
source:   ab**cd**ef
visible:  abcdef
```

断言：

- `Source[2,4)` 和 `[6,8)` 为 HiddenSyntax。
- `source_to_display(4)` 的 offset 为 2。
- `source_to_display(8)` 的 offset 为 4。
- `display_to_source_candidates(2)` 包含隐藏区间两侧合法边界 2 和 4。
- 光标进入 `c` 后展开标记，Source Anchor 不变，Layout 可改变。

### 11.2 Unicode

```text
source: 中**文**字
```

UTF-8 byte 边界：`0,3,5,8,10,13`。不得用 Unicode scalar index `0,1,3,4,6,7` 代替 byte offsets。

### 11.3 多编辑逆变换

```text
before: abcdefghi
after:  aXYZdefi
```

Undo 后精确恢复 `abcdefghi`；选区恢复通过历史 SelectionSnapshot，而不是指望被删除区间内 Anchor 自动回弹。

### 11.4 单 Block 长文本

一个 Block 含 1 MB 源码，编辑器应按视觉行/分段布局，不因 Block 虚拟化粒度过粗而整块无限重排。分段塑形需处理跨边界 ligature、Bidi 与 Unicode line breaking 的正确性，必要时扩大上下文或重排整个相关 shaping run。

---

## 12. 复杂度总表与性能预算

| 数据结构 | 查询 | 更新 | 备注 |
|---|---|---|---|
| ChangeMap sorted edits | `O(log k)` 单点 | `O(k log k)` 构造 | 同一事务 k 个 edits |
| Lazy AnchorStore | 依索引/epoch | 依版本压缩 | 禁止无界历史链 |
| Projection Vec segments | `O(log m)` | `O(m)` 局部 Block 重建 | m 为 Block marker 数 |
| Projection B-tree（可选） | `O(log m)` | `O(log m)` 局部操作 | 需实际 benchmark 证明收益 |
| CaretStop Graph | `O(1)` 横向邻接 | `O(C)` 构建 | C 为已布局 caret stops |
| Vertical caret search | `O(log C_line)` | 随布局构建 | 依 preferred_x |
| Fenwick HeightIndex | `O(log N)` | 高度更新 `O(log N)` | 中间插删不理想 |
| Aggregated B-tree HeightIndex | `O(log N)` | `O(log N)` 高度/结构更新 | 推荐长期方向 |

性能目标不写成脱离硬件的绝对毫秒承诺；需在锁定 GPUI、macOS、字体和目标机型后建立 P50/P95/P99 基线。

---

## 13. 实施顺序与验收关卡

### A1 — ChangeSet + ChangeMap

实现：强类型偏移、规范化、逆向编辑、版本链、随机测试。

**验收**：随机 Unicode 文本多区间事务可逆；错误事务不会部分提交。

### A2 — AnchorStore

实现：Bias、版本解析、Epoch 压缩、生命周期管理。

**验收**：Eager 参考模型与 Lazy 模型随机对拍一致。

### A3 — Projection Segments

实现：HiddenSyntax、Visible、双向候选映射、marker owner 检查。

**验收**：嵌套强调与 Unicode 下，映射不越界、不误隐藏、可见边界往返等价。

### A4 — CaretStop Graph

实现：与 GPUI shaping adapter 集成、Bidi Affinity、上下移动 preferred_x。

**验收**：CJK、RTL、emoji、ligature、软换行的导航与几何黄金测试通过。

### A5 — HeightIndex

实现：聚合 B-tree、估计高度、实测更新、ScrollAnchor 补偿。

**验收**：10 万 Block 随机插删改和滚动定位与参考模型一致。

### A6 — IME Overlay + 全链路

实现：CompositionPosition、临时投影、候选框几何、提交/取消。

**验收**：macOS 中日韩输入法真实测试，Undo 单组、Source Rope 不被预编辑污染。

### A7 — 压力、Fuzz 与性能回归

实现：长文档、超长 Block、连续显隐、快速滚动、后台任务取消。

**验收**：无源码损坏、无过期布局编辑、无无限增长的 Anchor/快照缓存。

---

## 14. 需要锁定的 ADR（架构决策记录）

| ADR | 决策 | 推荐方向 |
|---|---|---|
| ADR-015 | ChangeMap 边界语义 | 显式 Collapsed + Bias |
| ADR-016 | AnchorStore | 长期 Anchor + Epoch 压缩 |
| ADR-017 | Projection 存储 | Block-local Vec Segments，基准不足时升级树 |
| ADR-018 | Marker ownership | 无重叠 MarkerAtom |
| ADR-019 | Caret 导航 | Shaping 驱动 Visual Caret Graph |
| ADR-020 | HeightIndex | 带聚合高度的 B-tree |
| ADR-021 | IME 临时坐标 | EditablePosition::Composition |
| ADR-022 | Layout 缓存 | 版本键 + 局部指纹 + 失效 DAG |
| ADR-023 | Unicode 边界 | UTF-8 SourceByte + grapheme caret stops |
| ADR-024 | 异步发布 | Revision 检查与不可变快照 |

---

## 15. 架构审查中的反例

以下做法一律不应进入正式实现：

```rust
// 错误：把字节偏移当字符偏移。
let caret = source_offset + 1;

// 错误：认为隐藏语法的映射可以完全逆转。
assert_eq!(display_to_source(source_to_display(s)), s);

// 错误：Undo 直接复用 old range。
rope.replace(old_range, old_text);

// 错误：每次光标移动全量解析、全量塑形。
reparse_entire_document();
reshape_entire_document();

// 错误：IME marked text 无条件写入已提交 Rope。
document.insert(marked_text);

// 错误：用平均字符宽度估算候选框。
let x = chars * average_char_width;

// 错误：估计高度直接决定文本编辑坐标。
let source = guess_offset_from_estimated_block_y(y);
```

正确替代分别是：Unicode navigation、候选边界映射、post-edit inverse、局部失效、composition overlay、真实 shaped caret stops、按需实测布局。

---

## 16. 与前六份文档的完整闭环

1. **第一份**：整体架构与产品范围。
2. **第二份**：Projection/DisplayMap/Block Layout。
3. **第三份**：Document、Anchor、Transaction、Selection、Undo、IME。
4. **第四份**：Markdown Syntax、精确 Marker、增量 AST。
5. **第五份**：GPUI 原生渲染、视口、HitTest、Accessibility。
6. **第六份**：工程验证、API 探针、CI、性能验收。
7. **第七份（本文）**：关键数据结构、映射数学、复杂度、属性测试。

**结论**：稳定的 Live Markdown 不是靠在文本上“藏几个星号”，而是靠一组经过严格定义的、版本化的关系：

```text
ChangeMap
  → AnchorStore
  → Syntax Marker Ranges
  → Projection Segment Map
  → Visual CaretStop Graph
  → HeightIndex / ScrollAnchor
  → GPUI Native Rendering
```

只要这些关系在 Unicode、IME、嵌套语法、增量更新与异步布局下保持一致，就可以在不妥协原生体验的前提下实现Typora 风格 Hybrid View 单文件 Markdown 编辑器。

> **下一份建议**：第八份《端到端行为规范与测试样例：光标显隐状态机、键盘编辑语义、复制粘贴、链接交互、文件会话和真实 macOS 测试矩阵》，将架构中的可选策略固化为可验收的产品行为。
