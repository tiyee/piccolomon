# GPUI Live Markdown — 第六份：工程化实施、GPUI 能力核验与质量验收规范

> 2026-10-12 Hybrid 修订：交互路线与阶段范围以[第十一份](./11_GPUI_Typora_Hybrid_View_Spec.md)为准。Inline 按光标显隐；Block 保持结构化，显式进入局部/全文源码；模式切换不改源码或历史。下文领域 API 为设计草案，须纳入 Hybrid presentation 版本依赖后实施。

> 版本：0.1（工程设计草案）  
> 目标：macOS 原生单文件 Live Markdown 编辑器，Rust + GPUI，自主编辑器内核  
> 前置文档：01 总体架构；02 Projection/DisplayMap/Layout；03 Document/Anchor/Transaction/IME；04 Syntax Engine；05 Native Rendering/Viewport/Accessibility  
> 重要说明：本文件是**可执行工程计划与验证协议**，不是已完成的源码审计报告。未实际检出的 GPUI API、性能结果与平台能力均标记为“待验证”。

---

## 1. 工程目标与不可妥协条件

### 1.1 产品与内核边界

目标是单窗口、单 Markdown 文件的 Typora 风格 Hybrid View 编辑器。Inline 标记随光标智能显隐；Block 按类型即时渲染并在普通正文编辑时保持结构，原始语法通过局部/全文源码入口编辑。Enter 依模式与块类型决议，不是所有块的唯一触发。统一基础排版继续适用。完整状态、H1/H2 范围和 CT-102—117 见[第十一份](./11_GPUI_Typora_Hybrid_View_Spec.md)。

- 不使用 WebView/DOM 作为编辑器。
- 不以多个独立输入框拼接文档。
- 不以“透明字符但保留布局宽度”模拟隐藏语法。
- 不让 GPUI 视图层持有第二份可编辑文档。
- 不因输入法、Bidi、辅助功能困难而放弃原生行为。
- 不因增量解析困难而接受错误语义；可在安全条件下全文重算。
- 不让实现成本成为架构选择依据，但避免无价值的重复造轮子。

### 1.2 可验收的不变量

| ID | 不变量 | 证明方式 |
|---|---|---|
| INV-01 | Markdown 源码只有一个权威 Document | API 边界审查、静态依赖检查 |
| INV-02 | 所有源码编辑均通过原子 Transaction | 属性测试、错误注入 |
| INV-03 | Source Anchor 编辑后保持定义明确的位置语义 | 随机编辑序列 |
| INV-04 | Projection 显隐不改变源码、Undo、Dirty | 集成测试 |
| INV-05 | 语法标记仅在精确确定范围后隐藏 | Parser golden + fuzz |
| INV-06 | 绘制、点击、选区、IME 使用一致 LayoutSnapshot | 版本断言、事件重放 |
| INV-07 | UTF-8/UTF-16/grapheme 不混用 | 强类型、Unicode 测试 |
| INV-08 | 组合输入可提交、取消且撤销分组正确 | macOS 实机测试 |
| INV-09 | 过期异步结果不能覆盖新状态 | 并发调度测试 |
| INV-10 | 文件保存与已提交快照严格对应 | 故障注入、磁盘往返 |
| INV-11 | 大文档首屏无需全量塑形 | 性能跟踪 |
| INV-12 | 辅助功能可查询文本与正确范围 | VoiceOver/AX 集成测试 |

---

## 2. 工程仓库与依赖约束

建议使用 Cargo Workspace，模块按**稳定责任边界**划分，而不是按页面划分。初始建议：

```text
live-markdown/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── deny.toml
├── .cargo/config.toml
├── docs/
│   ├── architecture/
│   │   ├── 01-overview.md
│   │   ├── 02-projection-layout.md
│   │   ├── 03-editor-engine-ime.md
│   │   ├── 04-markdown-syntax.md
│   │   ├── 05-rendering-accessibility.md
│   │   └── 06-engineering-verification.md
│   ├── adr/
│   ├── contracts/
│   └── verification/
├── crates/
│   ├── editor-types/
│   ├── editor-document/
│   ├── editor-history/
│   ├── editor-engine/
│   ├── markdown-syntax/
│   ├── markdown-projection/
│   ├── editor-layout/
│   ├── gpui-compat/
│   ├── editor-gpui/
│   ├── file-session/
│   └── app/
├── probes/
│   ├── gpui-element/
│   ├── gpui-shaping/
│   ├── gpui-ime/
│   ├── gpui-accessibility/
│   └── gpui-window-coordinates/
├── tests/
│   ├── conformance/
│   ├── fixtures/
│   ├── integration/
│   ├── replay/
│   └── visual/
├── fuzz/
├── benches/
├── scripts/
└── .github/workflows/
```

### 2.1 依赖方向

```text
editor-types
    ↑
editor-document ─── editor-history
    ↑                   ↑
markdown-syntax       editor-engine
    ↑                   ↑
markdown-projection ───┘
    ↑
editor-layout
    ↑
gpui-compat → editor-gpui → app

file-session → editor-document / editor-types
```

该图表示逻辑依赖方向；实际 Cargo 依赖必须避免环。`editor-engine` 可以通过 trait 调用语法、投影和布局服务，必要时将 trait 放在低层 `editor-types` 或独立接口 crate。**不得为了画出这张图而引入循环依赖。**

### 2.2 Cargo Workspace 草案

```toml
[workspace]
resolver = "3"
members = [
  "crates/editor-types",
  "crates/editor-document",
  "crates/editor-history",
  "crates/editor-engine",
  "crates/markdown-syntax",
  "crates/markdown-projection",
  "crates/editor-layout",
  "crates/gpui-compat",
  "crates/editor-gpui",
  "crates/file-session",
  "crates/app",
]

[workspace.package]
edition = "2024"
license = "MIT OR Apache-2.0"

[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
correctness = "deny"
suspicious = "warn"
```

这只是结构草案，`resolver`、编译器版本及实际 GPUI 依赖要求需在锁定工具链后验证。若某些 macOS 桥接必须使用 `unsafe`，应仅在受审计的 `gpui-compat` 或平台 FFI crate 内局部允许，并配套安全契约，而非全局禁止后再通过规避方式实现。

### 2.3 依赖治理

- GPUI：固定**完整 Git commit**，不要跟随浮动分支；记录 fork upstream 与补丁集。
- Rust：固定 `rust-toolchain.toml`；升级必须运行完整测试矩阵。
- Rope：按编辑复杂度、快照共享、byte/line API、内存开销做基准选型。
- Markdown：锁定权威解析器和 grammar 版本；测试 oracle 独立。
- Unicode：使用经过验证的 grapheme、word boundary、line break、Bidi 设施。
- 序列化与文件 IO：优先标准库和清晰的 macOS 文件系统语义。
- `Cargo.lock` 提交到仓库；定期进行依赖安全与许可审查。

---

## 3. GPUI 能力核验矩阵（必须逐项完成）

**现阶段状态统一为“待源码验证/待实机验证”**；不宣称任何特定 GPUI 方法名已核实。

| 编号 | 能力 | 需核验的问题 | 核验产物 | 失败时的技术决策 |
|---|---|---|---|---|
| G01 | 自定义 Element | 测量、预布局、绘制、事件回调生命周期 | 最小 Element probe | 扩展/封装 GPUI Element 能力 |
| G02 | Text shaping | glyph、cluster、font fallback 可获取程度 | 多语种 shaping probe | fork/扩展文本塑形桥接 |
| G03 | Caret geometry | ligature、Bidi 双位置、行内插入点 | geometry probe + golden | 在 shaping 层补充精确 caret API |
| G04 | Soft wrap | 换行机会、行盒、缩进控制 | wrap probe | 自研 line breaker，复用底层 shaping |
| G05 | 输入焦点 | 自定义编辑器能否持有 text input focus | focus probe | 平台桥接增强 |
| G06 | IME | marked text、替换范围、selected range | 中文/日文/韩文 probe | 扩展 macOS 输入 handler |
| G07 | 候选框 | 窗口→屏幕坐标、滚动后更新 | candidate rect probe | 平台坐标适配层 |
| G08 | Pointer | 点击、拖选、capture、双/三击 | pointer probe | 自定义事件调度 |
| G09 | Scroll | 惯性滚动、剪裁、局部无效化 | scroll probe | Editor 内自主管理滚动几何 |
| G10 | Accessibility | AX 文本、range、bounds、actions | VoiceOver probe | 原生 AX 桥接/上游扩展 |
| G11 | Threading | shaping 和 GPUI 资源线程亲和性 | 并发 probe | 纯数据后台预处理，平台工作主线程 |
| G12 | DPI/Retina | 坐标系、屏幕切换、像素对齐 | 双屏 probe | 强类型转换与适配 |
| G13 | Window lifecycle | 关闭、失焦、输入组合提交/取消 | lifecycle probe | 显式生命周期协调器 |
| G14 | Clipboard | plain/Markdown/HTML 类型 | clipboard probe | 平台剪贴板桥接 |
| G15 | Menus/shortcuts | macOS 原生命令与键位 | menu probe | 自定义命令路由 |

### 3.1 每项核验的证据格式

```markdown
# G06 IME Verification
- GPUI revision: <full commit>
- Rust version: <rustc -Vv>
- macOS version: <version>
- Relevant source files: <path + commit>
- Actual API signatures: <copied from locked source>
- Minimal runnable probe: probes/gpui-ime/
- Test input methods: Pinyin / Japanese / Korean / dead key
- Observed behavior: <facts>
- Known gaps: <facts>
- Decision: use upstream / adapt / fork
- Reviewer: <name>
- Date: <date>
```

**只有“源文件 + 可复现 probe + 真实测试结果”才算已验证。** 文档中的接口示例不能替代证据。

### 3.2 源码审计路径

1. 选定上游仓库和完整 commit。
2. 浏览 crate 目录与 public API。
3. 追踪 TextInput、Element、TextLayout、Window、macOS platform backend 的调用链。
4. 记录 trait 生命周期、线程限制、所有权与事件顺序。
5. 运行独立 probe，不能只靠静态阅读。
6. 记录缺口和补丁边界。
7. 将证据提交到 `docs/verification/`。

---

## 4. gpui-compat：稳定的内部适配边界

不应让所有模块直接依赖随上游变化的 GPUI 类型。只让 `editor-gpui` 与 `gpui-compat` 了解真实平台 API。

```rust
pub trait NativeTextShaper {
    type ShapedBlock;
    type Error;

    fn shape(
        &self,
        text: &str,
        styles: &[StyleRun],
        width: LogicalPx,
    ) -> Result<Self::ShapedBlock, Self::Error>;
}

pub trait NativeInputBridge {
    fn sync_selection(&mut self, selection: EditableSelection);
    fn sync_composition(&mut self, composition: CompositionView);
    fn set_candidate_rect(&mut self, rect: WindowRect);
}

pub trait NativeAccessibilityBridge {
    fn publish(&mut self, snapshot: AccessibleSnapshot);
}
```

以上是**项目内设计接口**，不是 GPUI 现有 API 的断言。实际可把输入桥接实现为平台回调对象而非可变 trait，前提是保持职责不变。

### 4.1 何时 fork GPUI

以下能力若公开 API 不足，**可以直接 fork GPUI**，不必牺牲设计：

- 无法获得 shaped cluster 的合法 caret stop。
- Bidi / ligature 的 hit testing 精度不足。
- 原生 IME replacement range 或候选框几何无法完整实现。
- 自定义编辑器无法向 macOS AX 暴露正确的文本范围。
- 自定义 Element 无法获得所需的事件/绘制控制。

Fork 规则：

1. 补丁尽量落在基础能力层，不把 Markdown 逻辑写进 GPUI。
2. 新增接口需要明确安全与线程语义。
3. 保持 fork 与 upstream 的补丁差异可审查。
4. 每个 fork patch 对应 probe、回归测试和 ADR。
5. 若可向 upstream 提交通用补丁，优先推进，但项目不依赖上游接受后才能继续。

---

## 5. 核心契约：从 Document 到 RenderSnapshot

### 5.1 版本化数据链

```text
DocumentRevision
  └─ DocumentSnapshot
       └─ SyntaxSnapshot (document + dialect version)
            └─ ProjectionSnapshot (syntax + selection/visibility + IME)
                 └─ LayoutSnapshot (projection + typography + width)
                      └─ RenderSnapshot (layout + viewport + selection paint)
```

每个快照都包含依赖键，不能仅依赖一个模糊的 `generation`。当用户移动光标，DocumentRevision 不变，但 ProjectionRevision 可能变化；当窗口变宽，Syntax/Projection 不变，但 LayoutKey 必须变化。

### 5.2 强类型坐标

```rust
pub struct SourceByte(pub usize);
pub struct DisplayByte(pub usize);
pub struct Utf16Unit(pub usize);
pub struct LogicalPx(pub f32);
pub struct DocumentRevision(pub u64);

pub enum Bias { Before, After }
pub enum VisualAffinity { Upstream, Downstream }
```

- 编辑、历史、选择均以 Source Anchor 为权威。
- 显示布局使用 DisplayByte 和 caret stops。
- 输入法接口显式转换 UTF-16。
- 光标绘制和鼠标命中使用同一 LayoutSnapshot。
- 不允许长期保存 PixelPoint 代替 Source Anchor。

### 5.3 一次提交的事务时序

```text
Platform event
  → input adapter / composition state machine
  → EditorCommand
  → validate ChangeSet
  → Document atomic commit
  → transform Anchors + Selection
  → History append / merge
  → Syntax invalidation
  → Projection invalidation
  → Layout invalidation
  → consistent RenderSnapshot
  → paint / candidate rect / AX
```

**编辑事务原子性**不等于所有缓存重算都必须同步完成。缓存可以异步，但必须严格检查版本，并在没有新一致快照时保留旧帧或显示安全原文视图，不能混用新文本和旧光标。

---

## 6. 技术探针设计

### P01 — 最小自定义 Element

验收：一个 GPUI 窗口内自绘文字背景与光标矩形；点击后能获得准确窗口坐标；改变窗口大小可触发正确布局。

输出：`probes/gpui-element`、事件日志、实际 API 签名。

### P02 — 复杂文字塑形

输入集：

```text
hello fi ffi
中文 ABC 123
A😀中B
e\u{301}x
👨‍👩‍👧‍👦 family
English العربية English
אבג abc 123
```

验收：每个合法 grapheme 边界都能得到合理的 caret geometry；RTL 与 LTR 边界的 Affinity 可区分；改变字体和宽度后命中测试仍正确。

### P03 — 单段 Live Markdown

源码 `ab**cd**ef`；在未激活状态显示 `abcdef`；点击 `c` 进入节点后显示 `ab**cd**ef` 且 `cd` 保持粗体；点击前后 Source Anchor 不变；软换行变化后仍正确。

验收：不能通过透明标记或双 TextInput 实现。

### P04 — macOS IME

至少覆盖拼音、日文转换、韩文组合、dead key、emoji；必须测试候选框跟随滚动、换行、窗口缩放和多显示器移动。

验收：marked text 的更新、取消、提交与选区替换正确；一次 composition 是一组语义 Undo。

### P05 — Accessibility

用 VoiceOver 查询文本、移动光标、读取标题/链接、获取选区 bounds；滚动离屏后按需布局目标。

验收：AX 范围与编辑器文本坐标契约一致，不能出现“读到 A、实际删除 B”。

### P06 — 虚拟化与高度索引

生成 100k Block 文档、单个超长 Block、10 MB 混合 Markdown。测试首次可见内容布局、快速滚动、估计高度更新、Scroll Anchor 稳定。

验收：首屏不依赖全文 shaping；滚动位置在局部重排后稳定。

### P07 — 文件安全

打开 UTF-8 文件、修改、保存、Undo 恢复、外部修改、权限错误、磁盘空间不足、应用异常退出。

验收：不丢原文件；保存点绑定确定 DocumentSnapshot；错误时保留 Dirty 状态。

---

## 7. 分阶段实施：以正确性门槛驱动

不按“先做个看起来能用的编辑框”推进，而按可验证依赖顺序推进。阶段可以并行，但**未通过前置正确性关卡不得将其作为稳定基础**。

### Phase A — 平台能力与技术选型

- GPUI commit 锁定、源码核验矩阵。
- GPUI Text/IME/AX probe。
- Rope/Parser 基准与语义验证。
- 输出所有阻断性 API 缺口及 fork 决策。

**Gate A**：核心技术路线可行，缺失接口有明确可实现的扩展路径。

### Phase B — Document/Anchor/Transaction/History

- 强类型 SourceByte、AnchorStore、Bias。
- 原子多区间 Replace、逆向 ChangeSet。
- SelectionSet、Undo/Redo、SavePoint。
- 随机 Unicode 编辑属性测试。

**Gate B**：任意有效事务可逆；非法编辑无部分提交；Anchor 行为确定。

### Phase C — Syntax Engine

- 单一权威 Markdown Parser。
- 精确 marker spans、Block/Inline AST。
- 增量解析、引用依赖、RawText 安全降级。
- CommonMark/GFM conformance + 全量/增量差分。

**Gate C**：不存在已知错误隐藏 marker；增量与全文解析等价。

### Phase D — Hybrid Presentation / Projection / DisplayMap

- InlineVisibilityPolicy、BlockPresentationPolicy、RawDraft/Structured/BlockSource 与全文源码模式。
- FlowBlocks、结构装饰和 Source↔Display 分段映射。
- HiddenSyntax、可选 Replacement/Widget 结构。
- Affinity、合法 caret stop、显隐策略。
- IME overlay 接口。

**Gate D**：Inline 显隐、Block 模式切换与 Source Anchor 映射满足不变量；Structured 正文获得光标不展开前缀。

### Phase E — Layout/Native Rendering

- GPUI shaping 适配、Bidi、soft wrap。
- Block/VisualLine layout、hit test、selection geometry。
- Editor Element、绘制层、虚拟化、HeightIndex、Scroll Anchor。

**Gate E**：鼠标/键盘/软换行/Hybrid 模式使用一致几何；H1 通过 CT-102—114，H2 表格通过 CT-115—117；长文档首屏无需全量塑形。

### Phase F — IME/AX/File Session

- 完整 composition state machine。
- macOS 输入、候选框、AX 语义与范围。
- 原子保存、外部文件变更、Dirty/Undo 协调。

**Gate F**：真实 macOS 输入法和 VoiceOver 通过；故障注入不导致文件损坏。

### Phase G — 系统性稳定性

- 长时间随机事件重放。
- 大文档/长 Block 性能回归。
- 资源释放、内存预算、缓存一致性。
- 多显示器、字体 fallback、窗口 resize、睡眠恢复。

**Gate G**：所有正确性不变量、平台集成和性能基线均满足发布要求。

---

## 8. 测试体系：分层与责任

| 层级 | 测试内容 | 运行环境 |
|---|---|---|
| Unit | offset、Anchor、ChangeMap、History | 任意 Rust runner |
| Property | Unicode、随机事务、映射单调性 | Rust runner |
| Parser conformance | CommonMark/GFM 规范 | Rust runner |
| Differential | incremental vs full parse | Rust runner |
| Fuzz | AST、Projection、Transaction、IME event model | Rust fuzz runner |
| Geometry golden | caret stops、Bidi、wrap、selection rect | 固定字体/平台 |
| Integration | 源码→投影→布局→编辑命令 | macOS runner |
| Platform | IME、候选框、VoiceOver、AX | 真实 macOS GUI |
| File fault injection | 保存中断、权限、冲突 | macOS / 测试文件系统 |
| Performance | 大文件、快速滚动、输入 P99 | 固定基准机 |

### 8.1 必备 Property

**Document**：

```text
apply(T, S) = S'
apply(inverse(T), S') = S
```

**Projection**：

- Segment 有序且覆盖其声明的源码范围。
- HiddenSyntax 的显示宽度为零。
- Source→Display 映射单调，但不是双射。
- Display→Source 返回合法位置及 Affinity。
- 未闭合语法标记不可误隐藏。

**Layout**：

- 合法 caret stop 可通过 layout 查询得到几何。
- hit-test 在同版本布局上稳定。
- Source Anchor 在显隐/换行/滚动前后保持语义位置。
- SelectionGeometry 不越过所属视觉行。

**History**：

- Undo/Redo 后源码与约定选区恢复。
- 显隐不增加 History。
- IME 一次组合按策略形成一个 Undo group。

### 8.2 事件重放日志

设计结构化事件记录，禁止默认存储用户真实文档全文：

```rust
pub struct ReplayEvent {
    pub sequence: u64,
    pub document_revision: u64,
    pub event: EditorEvent,
    pub expected_source_hash: Option<[u8; 32]>,
    pub expected_selection: Option<SelectionDigest>,
}
```

回放可使用脱敏 fixture；对随机失败自动最小化事件序列，形成可复现回归用例。

### 8.3 失败分类

- `DATA_CORRUPTION`：源文档错误修改、文件损坏，发布阻断。
- `POSITION_CORRUPTION`：光标、选区、IME 范围错位，发布阻断。
- `SEMANTIC_MISMATCH`：AST 与规范不一致或误隐藏，发布阻断。
- `SNAPSHOT_MISMATCH`：混用过期布局，发布阻断。
- `ACCESSIBILITY_REGRESSION`：AX 操作影响错误文本，发布阻断。
- `PERFORMANCE_REGRESSION`：超出已设基线，需分析和决策。
- `VISUAL_POLISH`：非核心装饰偏差，可单独跟踪。

---

## 9. CI/CD 与自动化质量门槛

### 9.1 Pull Request CI

建议顺序：

```text
format
  → lint
  → compile workspace
  → unit + property smoke
  → parser conformance
  → integration
  → geometry golden
  → macOS GUI smoke (available runner)
```

命令示例（需随实际工具链调整）：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo deny check
```

**注意**：`cargo deny` 需要独立安装并维护策略文件；Fuzz 与 GUI 测试需要专门 runner，不应假定默认 `cargo test` 自动覆盖。

### 9.2 Nightly / Scheduled CI

- 长时 fuzz、属性测试扩展轮数。
- 全量增量解析差分。
- 大文件性能基准。
- macOS 多输入法真实 GUI 测试（可人工辅助）。
- 依赖审计与 fork upstream 差异检查。
- 文件保存故障注入。

### 9.3 发布候选检查

- 所有发布阻断类问题关闭。
- GPUI 依赖 commit 与补丁记录完整。
- 真实 macOS IME 与 VoiceOver 回归通过。
- 目标系统版本和芯片架构构建验证。
- 签名、公证、更新机制与崩溃诊断策略确定。
- 用户文档、隐私策略和恢复说明齐备。

---

## 10. 性能基准与观测

### 10.1 固定工作负载

| 场景 | 文档 | 操作 |
|---|---|---|
| W01 | 10 KB 普通 Markdown | 连续键入/删除 |
| W02 | 1 MB 技术文档 | 光标跨语法、软换行 |
| W03 | 10 MB 混合文档 | 打开、首屏、快速滚动 |
| W04 | 100k Block | HeightIndex 定位与滚动 |
| W05 | 单个 1 MB 段落 | 局部编辑、换行、命中 |
| W06 | 单个 1 MB code fence | 行虚拟化与滚动 |
| W07 | 中英阿混排、emoji | caret 与 Bidi |
| W08 | 长链接/嵌套强调 | Projection 重排 |
| W09 | 中文 IME 连续组合 | 候选框与输入延迟 |
| W10 | 窗口宽度连续变化 | layout cache 与 Scroll Anchor |

### 10.2 必测指标

- `keypress_to_consistent_frame_p50/p95/p99`
- `transaction_commit_latency`
- `incremental_parse_latency`
- `projection_rebuild_latency`
- `block_shaping_latency`
- `hit_test_latency`
- `scroll_frame_time`
- `layout_cache_hit_rate`
- `peak_memory` / `steady_memory`
- `visible_blocks` / `laid_out_visual_lines`
- `scroll_anchor_displacement`
- `ime_candidate_rect_error`
- `accessibility_range_query_latency`

**不预设未经实测的绝对性能数字**。先固定目标设备、刷新率、字体、系统版本与输入负载，建立 baseline，再确定 SLO。不得以平均延迟掩盖 P99 卡顿。

### 10.3 基准结果格式

```json
{
  "scenario": "W08",
  "gpui_commit": "<commit>",
  "rustc": "<version>",
  "macos": "<version>",
  "hardware": "<model>",
  "font": "<font/version>",
  "document_bytes": 1048576,
  "metrics": {
    "keypress_to_frame_p99_ms": null,
    "projection_p95_ms": null,
    "hit_test_p95_ms": null
  }
}
```

`null` 表示尚未测量，**绝不能填写臆测数值**。

---

## 11. macOS 文件系统、应用打包与恢复

### 11.1 FileSession 契约

- 文件读取为明确的字节流；检查 UTF-8、BOM、换行约定。
- 只读/无权限文件应允许安全只读呈现或明确错误。
- 保存使用已提交 DocumentSnapshot，保存成功后记录对应 SavePoint。
- 原子替换策略须考虑目标文件系统、同目录临时文件、flush/sync、rename 与元数据保留。
- 文件被外部修改时比较文件身份/版本，不静默覆盖。
- 文件不存在、符号链接、沙盒授权、网络卷行为分别测试。
- 保存期间的新编辑不能被误标为已保存。
- IME 组合中保存需要先完成明确的 commit/cancel 流程。

### 11.2 恢复与崩溃

用户未要求数据库，但**可靠的崩溃恢复并不等于必须引入数据库**。可以采用受控的临时恢复文件或轻量 journal；是否启用由产品规范决定。

若提供恢复：

- 恢复内容必须有文件身份、DocumentRevision、校验和。
- 不得覆盖用户原文件。
- 过期恢复文件有明确清理规则。
- 恢复内容的隐私与权限需与原文件同等级保护。

### 11.3 macOS 发布

- 确认最小 macOS 版本和 Apple Silicon / Intel 策略。
- App bundle、Info.plist、图标与文档类型关联。
- Developer ID 签名和 Apple notarization。
- 如启用沙盒，验证文件访问与安全范围书签生命周期。
- 崩溃日志避免默认包含用户文档正文。

---

## 12. 安全、可靠性与依赖风险

- Markdown 原文默认按文本处理，不执行内嵌 HTML/脚本。
- 外部链接由平台安全策略打开，不在编辑器内执行。
- 图片/附件未来采用明确 URI 权限与大小限制。
- 粘贴 HTML 转 Markdown 必须经过独立转换层，不执行脚本。
- 避免无界 AST 深度递归、超长 delimiter run、恶意引用定义引发资源耗尽。
- 对 parser 和 projection 采用 fuzz 测试与资源预算。
- 依赖升级需安全审计、许可证检查、回归测试。
- `unsafe` 仅存在于经审查的平台 FFI 边界，并为每处标注 safety invariant。

---

## 13. 架构决策记录（ADR）

每个不可逆或影响全局的决定都写 ADR，避免后续开发者在局部重构中推翻核心约束。

建议初始 ADR：

| ADR | 决策 |
|---|---|
| ADR-001 | 单一 Rope 与 Document 唯一写入者 |
| ADR-002 | 自研 Source Anchor + Bias |
| ADR-003 | 统一原子 Transaction + Undo History |
| ADR-004 | 单一权威 Markdown AST |
| ADR-005 | Cursor-aware Projection 位于 Shaping 前 |
| ADR-006 | Source/Display/Visual 坐标分离 |
| ADR-007 | IME Composition Overlay |
| ADR-008 | GPUI 自定义 Editor Element |
| ADR-009 | Block + VisualLine 两级虚拟化 |
| ADR-010 | LayoutSnapshot 是统一几何来源 |
| ADR-011 | GPUI commit pin 与受控 fork 策略 |
| ADR-012 | 原生 Accessibility 文本坐标契约 |
| ADR-013 | 原始换行/BOM 保真策略 |
| ADR-014 | 默认复制 Markdown（已确认）；详细行为沿用第八份 ADR-025 / EB-040 |

### ADR 模板

```markdown
# ADR-NNN: Title
Status: Proposed | Accepted | Superseded
Date:
Context:
Decision:
Alternatives:
Consequences:
Invariants:
Validation:
Rollback / Migration:
```

**建议优先关闭的开放决策**：换行保真、空选区格式化行为、IME 保存/失焦策略、AX editable range 对外坐标空间、GPUI fork 边界。默认复制 Markdown 已由用户确认，后续依第八份 EB-040—043 验证，不再作为开放选项。

---

## 14. 代码审查与发布阻断规则

### 14.1 每次 PR 必须回答

1. 这次修改是否影响 Source / Display / Visual 坐标？
2. 是否新增了需要版本化的缓存依赖？
3. 是否可能在输入组合中被调用？
4. 是否改变 Undo/Redo 的分组语义？
5. 是否改变 Parser 的 delimiter ownership？
6. 是否改变辅助功能可见文本范围？
7. 是否会在长文档或超长 Block 中退化为全文处理？
8. 是否引入了新 `unsafe`、线程共享或平台生命周期依赖？
9. 是否有最小可复现测试与失败时安全降级？
10. 是否需要更新 ADR 或跨模块契约？

### 14.2 不接受的捷径

- “当前测试用例没问题”就用字符索引代替字节索引。
- “只是显示问题”就把语法字符设为透明但保留宽度。
- “为了赶进度”把每个 Block 改成独立输入框。
- “大多数语言是 LTR”就忽略 Bidi。
- “macOS 会处理 IME”就不设计 composition state machine。
- “不影响用户”就用过期 LayoutSnapshot 做 hit test。
- “解析不稳定”就对未确认 marker 强行隐藏。
- “缓存失效复杂”就每次键入全篇重新 shaping。
- “保存成功了”就不区分保存快照和最新文档版本。

---

## 15. 工程交付清单

### 15.1 必须形成的文档

- [ ] GPUI API 能力核验矩阵与证据链接。
- [ ] Rust Workspace 依赖图与禁止依赖规则。
- [ ] Document/Anchor/Transaction 契约。
- [ ] Markdown AST/Marker 精确范围契约。
- [ ] Projection 双向映射与 Affinity 契约。
- [ ] LayoutSnapshot/HitTest/CaretGeometry 契约。
- [ ] IME Composition 与 UTF-16 契约。
- [ ] Accessibility 对外文本坐标契约。
- [ ] FileSession 原子保存与恢复契约。
- [ ] ADR 决策记录。
- [ ] 测试与性能基准报告。

### 15.2 必须形成的可执行验证

- [ ] GPUI Element probe。
- [ ] GPUI shaping/Bidi probe。
- [ ] macOS IME probe。
- [ ] Accessibility probe。
- [ ] Source/Display 映射 property tests。
- [ ] Document/History fuzz。
- [ ] Incremental/full parse differential。
- [ ] HitTest/Selection geometry golden。
- [ ] 100k Block 与单超长 Block benchmark。
- [ ] 文件保存故障注入。

### 15.3 完成定义（Definition of Done）

一个模块只有同时满足以下条件才算完成：

1. 接口与版本契约明确。
2. 正确性不变量有自动化测试。
3. 与上游模块集成并覆盖错误路径。
4. 对 macOS 平台行为有实机验证（适用时）。
5. 性能有可复现基线（适用时）。
6. 已知限制与安全降级写入文档。
7. 关键设计变化有 ADR。

---

## 16. 六份文档的完整闭环

```text
01 Overall Architecture
  ├─ 全局不变量、模块边界、单文件产品定位
  │
  ├─ 03 Editor Engine
  │    └─ Document / Anchor / Transaction / Undo / IME
  │
  ├─ 04 Markdown Syntax Engine
  │    └─ 增量 AST / 精确 delimiter / conformance
  │
  ├─ 02 Projection & Layout Contract
  │    └─ Source↔Display / Visibility / Affinity
  │
  ├─ 05 Native Rendering & Accessibility
  │    └─ Shaping / VisualLine / HitTest / Viewport / AX
  │
  └─ 06 Engineering & Verification（本文）
       └─ GPUI API 核验 / Workspace / CI / Fuzz / Benchmark / Gate
```

### 最终工程结论

**最优方案不是最复杂的方案，而是所有复杂度都服务于不可替代的编辑正确性。**

纯 GPUI + 自主编辑器内核的关键不是一开始写多少代码，而是：

- 在代码落地前确定严格的 Source、Projection、Layout、IME 契约。
- 通过真实 GPUI 源码和 macOS 实验核验平台能力。
- 对缺失的底层能力主动扩展 GPUI，而不是牺牲原生体验。
- 用属性测试、差分测试、fuzz、几何 golden 和真实输入法回归证明正确性。
- 让所有缓存和异步工作都服从版本化快照，而不是让 UI 偶然保持同步。

至此，前五份设计的抽象可以转化为可审查、可编译、可验证、可持续演进的工程实施体系。

> **下一份可选主题**：第七份《核心数据结构与算法规格：AnchorStore、ChangeMap、Projection Segment Tree、CaretStop Graph、HeightIndex》，给出复杂度证明、边界算法、Rust 参考实现和 property-test 生成器设计。
