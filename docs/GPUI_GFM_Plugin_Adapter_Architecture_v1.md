# Rust + GPUI GFM 编辑器：插件化与适配器架构设计 v1.0

> 本方案已与第 16 份分层方案合并为 [Ryne 分层与插件架构实施方案 v2](17_Ryne_Layered_Plugin_Architecture_v2.md)。后续架构实施以 v2 为基线，本文保留为原始设计参考。

## 1. 结论

采用 **微内核 + 受控同步 Hook + 类型化提交后事件总线 + 服务适配器 + 声明式 UI 扩展**。不要把所有编辑行为抽象成可随意执行的事件回调。

目标：保持 Rope SourceText 为唯一事实来源，支持 GFM Live Markdown、Smart Completion、块左上角标识、文末虚拟插入锚点、Knuth–Plass 平衡断行和 formulary 数学排版。

## 2. 不变量

1. 只有 `SourceTransaction` 能修改 Rope；插件只能返回 `EditPlan` 或发送命令。
2. `DocumentRevision` 与 `ProjectionRevision` 分离；异步结果需校验版本。
3. Undo/Redo、IME composition、SourceAnchor、SourceMapping 属于核心一致性边界。
4. PostCommit 事件不能取消事务；回调不得同步重入编辑器。
5. Layout 插件不写源码，UI 插件不直接操纵 GPUI 内部 View 树。
6. GFM 为基础语义；LaTeX 数学属于显式扩展语法。

## 3. 架构

```text
GPUI Shell (keyboard / IME / mouse / accessibility)
  ├── UI Extension Host (adornments / menus / insertion anchor)
  └── Editor Facade
        ├── CommandDispatcher
        │     ├── ordered Input Hooks / Validators
        │     └── SourceTransaction -> Undo + Anchor Transform
        ├── Typed Event Bus (post-commit)
        ├── Document Core (Rope / GFM / Projection / Selection)
        └── Service Registry
              ├── ParserAdapter (conformance-gated)
              ├── ParagraphBreaker (Greedy / Balanced)
              ├── MathLayoutAdapter (formulary)
              ├── FileStorageAdapter
              ├── ImageProvider / LinkOpener
              └── Highlighter / Exporter
```

## 4. 扩展机制职责

| 机制 | 用途 | 时序 | 能否修改源码 |
|---|---|---|---|
| Command | 插入、转换、保存、Undo | 同步调度 | 仅通过核心事务 |
| Input Hook | 智能补全、输入规范化 | 提交前，同步确定性 | 返回 EditPlan |
| Validator | 校验允许拦截的用户命令 | 提交前 | 不直接修改 |
| Event Bus | 文档、选区、焦点、保存状态 | 提交后 | 不允许同步修改 |
| Service Adapter | 解析、数学、断行、文件 | 按契约调用 | 仅 Storage 负责持久化 |
| UI Extension | 块角标、菜单、虚拟插入点 | GPUI 渲染周期 | 仅发送 Command |

## 5. 插件 API 草案

```rust
pub trait EditorPlugin: Send + Sync + 'static {
    fn manifest(&self) -> PluginManifest;
    fn activate(&self, registrar: &mut PluginRegistrar<'_>)
        -> Result<(), PluginError>;
    fn deactivate(&self) -> Result<(), PluginError> { Ok(()) }
}

pub struct PluginManifest {
    pub id: &'static str,
    pub api_version: u32,
    pub dependencies: &'static [&'static str],
    pub priority: i16,
    pub capabilities: &'static [Capability],
}

pub enum Capability {
    ObserveDocument, RegisterCommand, TransformEdits,
    ContributeUi, ProvideLayout, FileAccess, OpenExternalUrl,
}
```

`PluginRegistrar` 暴露 `register_command`、`register_input_transform`、`subscribe::<T>`、`register_adornment`、`register_menu_provider`、`provide_service`。注册返回 RAII `RegistrationToken`，卸载时撤销。依赖拓扑排序；相同优先级按 plugin ID 和注册序号稳定排序。重复 ID、循环依赖和不兼容 API 版本拒绝激活。

首版只支持**可信静态链接插件**。Rust trait object 不具有稳定动态库 ABI；未来第三方扩展优先 WASM/进程隔离与能力授权。

## 6. 输入 Hook 与原子事务

```rust
pub trait InputTransform: Send + Sync {
    fn transform(
        &self,
        snapshot: &EditorSnapshot,
        intent: &InputIntent,
        ctx: &TransformContext,
    ) -> TransformOutcome;
}

pub enum TransformOutcome {
    Pass,
    Replace(EditPlan),
    Reject(EditorError),
}

pub struct EditPlan {
    pub expected_revision: DocumentRevision,
    pub edits: Vec<SourceEdit>,
    pub selection_after: Option<SourceSelection>,
    pub undo_group: UndoGroupPolicy,
}
```

**单阶段单一获胜者**：第一个 `Replace` 结束本阶段，避免多个插件连续改写同一段源码。若需要多步处理，使用显式阶段 `NormalizeInput -> SmartComplete -> ValidatePlan`，每阶段重新校验范围与修订；禁止隐式串联不相容的 EditPlan。

执行顺序：

1. GPUI 将键盘/IME 提交转成 InputIntent。
2. Dispatcher 读取不可变 Snapshot。
3. 有序 Hook 生成 EditPlan。
4. Core 校验修订、权限、边界、非重叠 SourceEdit。
5. 原子提交 Rope，更新 SourceAnchor、Selection 和 Undo。
6. 生成 DocumentChanged、SelectionChanged 等事件。
7. 排队更新 Parser、Projection、Layout、GPUI。

**IME 组合期间不触发结构性 SmartCompletion**；仅在 composition commit 后运行一次。Undo/Redo 使用专用核心路径，不重新触发自动补全。

## 7. 类型化事件总线

建议事件：

- `DocumentOpened`、`DocumentChanged`、`DocumentClosed`
- `SelectionChanged`、`CaretMoved`、`FocusChanged`
- `CompositionStarted`、`CompositionUpdated`、`CompositionCommitted`
- `ProjectionUpdated`、`LayoutInvalidated`、`ViewportChanged`
- `HoveredBlockChanged`、`BlockMenuOpened`
- `CommandExecuted`、`UndoStackChanged`
- `SaveRequested`、`SaveCompleted`、`SaveFailed`
- `PluginActivated`、`PluginFailed`

每个事件包含 `editor_id`、`document_id`、`sequence_no`、`document_revision`、`projection_revision`、`origin` 和精简 payload。

- `DocumentChanged` 保持事务顺序，不静默丢失；后台订阅者落后时发送 `ResyncRequired`。
- `SelectionChanged`、`HoveredBlockChanged`、`LayoutInvalidated` 可按帧合并。
- 事件在核心提交完成且释放写锁后发布。
- 监听器若要编辑，只能投递**下一轮** `EditorCommand`，不能同步递归调用。
- GPUI 相关订阅只在 UI 线程运行；后台任务使用有界队列。

## 8. Command 与快捷键

```rust
pub enum EditorCommand {
    InsertText(String),
    ApplyEdits(EditPlan),
    SetHeadingLevel(u8),
    ToggleStrong,
    InsertCodeFence { language: Option<String> },
    InsertMath,
    InsertImage,
    InsertBlockAfter { anchor: SourceAnchor },
    Undo,
    Redo,
    Save,
    Plugin { id: CommandId, payload: CommandPayload },
}
```

快捷键按 `Normal / Selection / Code / Math / Composition` 上下文解析；冲突可诊断。第三方插件不能覆盖系统 IME 或辅助功能保留快捷键。

## 9. 服务适配器

```rust
pub trait ParagraphBreaker: Send + Sync {
    fn layout(&self, input: &ParagraphInput, ctx: &LayoutContext)
        -> Result<ParagraphLayout, LayoutError>;
}

pub trait MathLayoutAdapter: Send + Sync {
    fn layout(&self, latex: &str, style: &MathStyle)
        -> Result<MathLayout, MathError>;
}

pub trait GfmParserAdapter: Send + Sync {
    fn parse(&self, source: &SourceSnapshot) -> ParseResult;
}

pub trait FileStorageAdapter: Send + Sync {
    fn load(&self, uri: &DocumentUri) -> StorageFuture<LoadedDocument>;
    fn atomic_save(&self, request: SaveRequest) -> StorageFuture<SaveReceipt>;
}
```

适配器是**算法/服务替换边界**，插件是**注册和生命周期单元**。两者不要混淆。

- `GreedyBreaker` 是默认实现；`BalancedBreaker` 采用 Knuth–Plass 思想，仅处理合法断点并可回退。
- `FormularyMathAdapter` 执行 LaTeX → MathML → formulary → MathDrawList；GPUI 只负责绘制。
- Parser 替换需通过 lossless ownership、GFM 语义和 SourceMapping 一致性测试，不能允许任意 AST 形状。
- 切换 Adapter 增加 `service_generation`，废弃旧缓存；正在执行的布局保留旧 adapter handle 至结束。

## 10. 声明式 UI 插件

```rust
pub trait BlockAdornmentProvider: Send + Sync {
    fn adornment(
        &self,
        block: &BlockViewInfo,
        ctx: &UiQueryContext,
    ) -> Option<AdornmentSpec>;
}

pub struct AdornmentSpec {
    pub kind: AdornmentKind,
    pub visibility: AdornmentVisibility,
    pub action: CommandId,
}
```

每个展示 Block **左上角最多一个标识**：标题 H1–H6，图片/代码/表格图标；普通段落可只在 hover 显示。若多插件贡献同一槽位，Host 按稳定优先级选一个。嵌入段落中的图片不自动生成独立 Block 标识。

`DocumentInsertionAnchor` 是文末的虚拟 Interaction/Layout 节点，**不写入 Rope、GFM AST**。默认透明但有 hit-test 区域，hover 时显示添加菜单；点击只发送 `InsertBlockAfter` Command。

菜单贡献只声明 `group / order / command / visibility predicate`；GPUI Host 负责位置、键盘焦点、无障碍和生命周期。

## 11. 三个完整用例

### Smart Completion

输入 `###H`：SmartCompletion 生成 `### H` 的单事务计划；Parser 识别合法 H3；Projection 隐藏 `### `；BlockAdornment 显示 H3；Undo 一次撤销整个用户动作与补全。

输入 `**H`：生成闭合 `**`，光标 SourceAnchor 留在闭合标记前；自动配对状态保存在 Interaction 层；粘贴已有闭合符不得重复补全。

### formulary 数学插件

数学扩展必须显式启用，因为 GFM 本身不定义 `$...$`。解析时避开代码跨度/围栏、转义等 GFM 结构。MathAdapter 输出 width/ascent/descent 和精确 glyph/rule draw list；正文断行器将行内公式作为原子对象。公式编辑失败时显示源码或容错占位，绝不丢失文本。

### Balanced Line Breaking

`BalancedBreaker` 只影响 VisualLineLayout；不修改源码。使用 Unicode/CJK 合法断点与字体测量；在 IME、代码块、超长段落或性能超限时回退 `GreedyBreaker`。光标位置由 SourceAnchor 重新映射。

## 12. 并发、性能与安全

- Core 修改在串行调度器执行；GPUI 输入热路径不做 I/O 或网络。
- 后台解析/布局任务读取不可变 Snapshot，携带 revision、service_generation、cancel token；过期结果丢弃。
- 缓存 key 至少考虑局部文本哈希、字体版本、尺寸、宽度、ProjectionRevision、AdapterGeneration。
- 输入 Hook 目标 p95 小于 1ms **仅作为待验证预算**，不是已测数据。
- 同步 Rust Hook 无法安全强制抢占；超时先记录，之后禁用慢插件。
- 失败回退：SmartCompletion → 原始输入；Balanced → Greedy；Math → 源码/占位；UI 插件 → 隐藏；存储失败 → 保留未保存状态。
- 未来不可信第三方插件必须隔离进程或 WASM，显式能力授权，不使用 Rust trait ABI 直接跨 `.dylib`。

## 13. Crate 建议

```text
crates/
  editor-types/
  editor-core/
  editor-gfm/
  editor-projection/
  editor-plugin-api/
  editor-plugin-host/
  editor-services/
  editor-layout/
  editor-gpui/
  plugins/
    smart-completion/
    block-adornment/
    append-anchor/
    math-formulary/
    balanced-linebreak/
    image-preview/
```

依赖方向保持 `types → core/services/api → host/layout → gpui`，插件只依赖公开接口，不直接依赖 App 的私有状态。

## 14. 测试与验收

1. **确定性**：相同输入、插件集合与注册顺序得到相同源码、选区和 Undo。
2. **冲突**：两个插件同时转换输入时，唯一获胜者稳定可预测。
3. **原子性**：EditPlan 中任一非法编辑导致整个事务失败。
4. **IME**：组合中不结构改写；提交后只补全一次。
5. **事件**：DocumentChanged 有序；高频 hover 合并；异步落后可 resync。
6. **版本**：旧后台布局不可覆盖新布局；Adapter 切换使缓存失效。
7. **UI**：Block 左上角不重复；滚动与嵌套正确；文末锚点不污染源码。
8. **数学**：公式未闭合、复杂矩阵、缺字、GPUI glyph 绘制有明确回退。
9. **断行**：中英文混排、CJK 禁则、grapheme、选区和源码映射正确。
10. **性能**：记录 Hook p95/p99、缓存命中、事件积压、滚动掉帧、公式重算次数。

## 15. 分阶段实施

**P0 核心契约**：CommandDispatcher、SourceTransaction、Undo、SourceAnchor、revision 不变量。

**P1 插件宿主**：静态注册、Manifest、依赖、Hook、Typed Event Bus、诊断；用 SmartCompletion 验证。

**P2 UI 扩展**：块角标、块菜单、文末隐形插入锚点，验证 GPUI hit-test/虚拟化。

**P3 服务适配器**：Greedy/Balanced 断行、formulary 数学、存储适配器与回退。

**P4 第三方插件**：稳定协议、权限模型、WASM/进程隔离，等 API 稳定后再做。

## 16. 最终架构决策（ADR）

- **ADR-001**：Rope SourceText 唯一事实来源。
- **ADR-002**：Hook 决策在事务前；Event Bus 只做提交后通知。
- **ADR-003**：输入转换按阶段、单一获胜者，禁止无序叠加。
- **ADR-004**：GPUI Host 统一渲染声明式 UI 扩展。
- **ADR-005**：数学和断行通过服务适配器接入，并有可靠回退。
- **ADR-006**：首版静态可信插件，暂不开放 Rust 动态库 ABI。
- **ADR-007**：GFM 为基础规范，数学为可选扩展语法。

**一句话原则：核心一致性封闭，扩展能力开放；命令负责修改，事件负责通知，适配器负责替换实现，插件负责组合功能。**
