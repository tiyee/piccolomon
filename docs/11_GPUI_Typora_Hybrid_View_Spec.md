# GPUI 单文件 Markdown 编辑器：Typora 风格 Hybrid View 方案

版本：0.1，2026-10-12。状态：按用户要求修订的产品与架构规格，尚未实现。

本文规定当前交互路线：**Inline 按光标智能显隐，Block 保持结构化渲染，并提供局部与全文源码编辑入口。** 本文取代旧方案中“标题、列表、引用正文一获得光标就展开块前缀”的规则；标题点击例外见下文；第八份继续负责其余编辑行为，默认复制 Markdown 的决策继续有效。

## 1. 参考行为与本项目约定

Typora 官方说明：行内样式完成输入后可以即时预览，标记智能显隐；块样式可能在输入时或 Enter 后渲染，渲染后的块标记保持隐藏。[Quick Start](https://support.typora.io/Quick-Start/#live-preview) 官方也说明光标进入行内元素时会展开源码，并分别描述标题、列表、代码块和表格的操作。[Markdown Reference](https://support.typora.io/Markdown-Reference/)

因此本项目将用户描述中的“即时渲染、不可逆编辑”落实为**稳定的块级呈现**：普通光标进出不会让已渲染块反复退回源码；内容、结构与原始语法仍可编辑，也可 Undo。Enter 是部分块的确认或结构编辑事件，不是所有块的唯一渲染条件。

光标进入块正文时可以直接编辑内容；需要修改 `#`、`>`、围栏、表格分隔行等原始语法时，在块内通过“编辑块源码”入口进入局部源码态。进入任意正文不自动展开整个块。这是本项目的明确交互约定，不声称所有细节与 Typora 完全相同。

本项目采用 AST、增量失效和位置映射实现该体验。Typora 的专有内部 AST、映射表或渲染算法未在本次核实，不将这些描述作为上游实现事实或性能保证。

## 2. 两套呈现策略

| 对象 | 普通编辑 | 光标进入 | 光标离开 | 修改原始语法 |
|---|---|---|---|---|
| Strong / Emphasis / Strike | 隐藏已确认的成对 marker，保留样式 | 展开该节点及必要祖先 marker | 收起对应 marker | 直接编辑展开的源码 |
| Inline Code | 代码样式，隐藏反引号 | 展开反引号；内容不解析 Markdown | 隐藏反引号 | 直接编辑 |
| Link | 带样式的 label | 展开 label、destination、title 语法 | 收起语法 | 直接编辑；普通单击不打开链接 |
| Heading | 标题结构，悬停不展开 `#` 或 Setext underline | 点击展开可编辑标记；仅键盘进入保持隐藏 | 光标离开标题收起标记 | 点击标题、块源码入口或标题级别命令 |
| List / Task List | bullet、编号、复选框和悬挂缩进 | 编辑条目内容，结构 marker 仍隐藏 | 列表结构保持 | 缩进/类型命令或块源码入口 |
| Quote | 引用线与缩进 | 编辑引用正文，`>` 仍隐藏 | 引用结构保持 | 引用命令或块源码入口 |
| Fenced Code | 代码内容和代码块外观；围栏隐藏 | 直接编辑代码；语言字段有明确入口 | 代码块结构保持 | 块源码入口编辑围栏与 info string |
| Table（H2） | 网格与单元格内容；分隔语法隐藏 | 编辑单元格，不展示整个表源码 | 网格保持 | 表格操作或块源码入口 |
| 不完整/未知语法 | 原文可见 | 原文编辑 | 不猜测隐藏 | 原文编辑 |

标题字号与行高仍由统一主题控制；Hybrid View 不强制复制 Typora 的主题。行盒须容纳真实字体度量。

Inline reveal 在标题、列表、引用、表格单元格的内容中仍然生效，不影响外层 Block 的结构呈现。代码块正文不进入普通 Inline parser。

## 3. Block 呈现状态

设计类型如下，仅为领域接口，不是 GPUI 现有 API：

```rust
enum BlockViewMode {
    RawDraft,   // 当前正在输入且尚未确认呈现的结构候选
    Structured,
    BlockSource,
}

enum DocumentViewMode {
    Hybrid,
    Source,
}

struct HybridPresentationState {
    document_mode: DocumentViewMode,
    block_modes: BlockModeStore,
    inline_reveal: InlineRevealSet,
    revision: PresentationRevision,
}
```

`block_modes` 是稀疏 UI 状态，以文档身份、稳定 BlockId 和 Source anchors 关联；未保存状态的已支持合法块默认 Structured。节点消失或结构发生重组时重新决议，禁止让复用的 ID 继承另一块的源码态。

转换规则：

1. 新输入尚未满足结构识别条件时呈现 RawDraft，源码仍实时进入 Document。
2. 权威语义确认且满足该块的触发规则后转 Structured。模式转换自身不写入 Rope、History 或 Dirty。
3. 已有文件打开时，支持的合法块直接呈现 Structured，不要求用户逐块按 Enter。
4. 光标进入 Structured 正文、选中跨块范围或滚动，不切换 BlockSource；只有内部 Inline reveal 可能改变。
5. `EditBlockSource`、点击块源码入口或全文源码命令进入源码视图。局部源码使用现有 Source Selection、输入协议和 Undo。
6. 退出局部源码后按最新 AST 恢复结构化呈现；无效或不支持的部分保持原文。Escape 结束源码视图，不撤销已经提交的源码编辑。
7. 模式切换保留 Source anchors；原位置落在隐藏结构标记内部时，规范化到有定义的内容边界，并保留返回源码态的位置提示。不得用旧像素再次命中。
8. IME 组合或拖选期间推迟非必要模式切换；先完成平台组合生命周期或结束拖选，再发布一致新布局。

H1 同时只维护一个局部源码作用域。普通段落/标题/代码块使用完整语义块；位于嵌套列表或引用中时，提升到可独立呈现的最外层容器，保留完整前缀与缩进上下文。该作用域替代原有 FlowBlocks，不与结构化正文重复显示；退出时依据 Source anchors 和最新语义重建，不截取不完整子树单独解析。

全文 Source 视图显示所有 Markdown，并保持语法高亮和同一 Document；返回 Hybrid 时按当前源码重建呈现。它不形成第二份可独立编辑的正文。

## 4. 输入触发与结构编辑

| 块类型 | 本项目触发约定 | 渲染后 Return / 操作 |
|---|---|---|
| ATX Heading | 正在输入标题候选时可保留 marker；Return 确认后呈现标题，随后为普通段落 | 标题正文中 Return 创建普通段落，补齐合法段落边界 |
| Quote | 语义确认行首前缀后可即时呈现；Return 保证候选确认并延续/退出引用 | 非空行延续引用，空引用行退出当前层 |
| List / Task List | 合法行首 marker 与分隔符确认后即时呈现；未确认的前缀保持原文 | 非空项延续；空项退出；Tab 调整层级；复选框修改准确源区间 |
| Fenced Code | 合法 opening fence 行后 Return 建立代码内容区 | 代码内 Return 是代码换行，不自动退出；围栏修改由源码入口完成 |
| Table（H2） | 表头与合法 delimiter row 已确认，Return 结束 delimiter row 后进入网格 | Tab 在单元格间导航；增加行列、Return 语义由表格操作规范定义 |
| Setext Heading | underline 完成且语义确认后呈现标题 | 编辑正文保留标题；点击标题展开 underline 或走块源码入口修改 |

Enter 先交给 IME；输入法用于确认候选时不能同时创建新块。候选确认、换行和结构修改若改变源码，必须作为原子 Transaction，撤销恢复实际字节与选区。纯粹从 RawDraft 转 Structured 不单独产生历史条目。

普通段落采用 Typora 风格的可编辑段落操作：Hybrid 中 Return 拆分为两个普通段落，序列化为合法 Markdown 段落边界；Shift+Return 为显式硬换行（源码使用两个尾随空格加换行）。列表、引用、代码块有上述各自规则。局部/全文 Source 中 Return 插入单个源码换行，Shift+Return 不隐式改写语法。保存不会重新格式化未编辑的段落。

例如顶层普通段落 `ab|cd`，Hybrid Return 后源码为 `ab\n\ncd`，Shift+Return 后为 `ab  \ncd`；Source Return 后为 `ab\ncd`。这里的 `|` 是示意光标，`\n` 表示实际换行；容器内分隔还须保留引用前缀或列表缩进，不能机械地对所有块插入 `\n\n`。

任务复选框、列表缩进和标题级别变化走结构命令，将最小必要修改写回 Rope；没有独立富文本真相。转换块类型、退出列表、添加表格行列等都可以 Undo。

## 5. 数据与渲染管线

```text
DocumentSnapshot + Composition-aware EditableTextSnapshot
    ├─ committed Document → Authoritative SyntaxSnapshot
    └─ EditableTextSnapshot → composition spans
    → HybridPresentationState
         ├─ InlineVisibilityPolicy
         └─ BlockPresentationPolicy
    → FlowBlocks / content spans / structural decorations
    → ProjectionSnapshot + source maps
    → native shaping / layout / caret geometry
    → GPUI Editor Element / IME / accessibility
```

职责约定：

- Parser 只确认语义、marker 和源码范围，不消费焦点或决定是否呈现源码。
- InlineVisibilityPolicy 依据源码选区决定 inline marker 显隐。
- BlockPresentationPolicy 依据语义、触发事件和显式源码模式产生结构装饰，不将“光标进入正文”当作显示前缀的条件。
- FlowBlocks 将嵌套语义树转换成不重复计算高度的阅读顺序；HeightIndex 只索引布局单元，不重复计入 List 容器与内容段落。
- bullet、复选框、引用线等装饰有 Source-backed 命中动作或边界，没有可编辑正文副本，也不能吞掉选区所跨越的源区间。
- `ProjectionSnapshot` 的键必须含 Hybrid presentation 版本、composition、样式和文档依赖；布局切换后重新计算同版本 caret、selection、IME 与 AX 几何。

表格单元格以独立 spans 与布局节点表达。网格导航由 cell graph 决定，不能假定二维上下移动等价于源码偏移增减，或用单一线性 SegmentMap 替代整张表的几何映射。单元格可以有局部渲染缓存，但全篇只有一个逻辑焦点、Selection、composition 和历史。

## 6. 源码、剪贴板与历史

- `.md` 源码是唯一持久格式；打开、仅查看、模式切换和保存不把 AST 重新序列化为另一套格式。
- 默认 `Cmd+C` 继续复制 Markdown 片段，保留选中内容格式；`text/plain` 载荷同样包含 Markdown。Copy Source 提供精确源码切片，第八份 EB-040—043 继续适用。
- 混合视图中的结构命令可能修改 Markdown；源码视图中的输入直接修改相同 Rope。两者经过相同的事务和历史入口。
- 仅改变模式不入 Undo；Undo/Redo 修改文档后，重新决议相关 Block 模式与 AST，不能依靠旧布局或旧 BlockId 解释新源码。
- IME overlay 与组合内 caret 使用独立临时位置，不能强行映射到 committed Rope 中不存在的字符。模式切换和 Enter 均须协调 IME。

## 7. 分阶段交付与验证

| 阶段 | 范围 | 退出标准 |
|---|---|---|
| H0 | 固定 GPUI、复杂文本/IME/AX 探针 | 可编译与实机证据，平台缺口有明确适配路径 |
| H1 | Inline 显隐；标题、列表、任务、引用、代码、Setext 结构化；局部/全文源码模式；Markdown 复制 | CT-102—114、核心历史、跨块选择、文件与 IME 验收通过 |
| H2 | 表格网格、单元格编辑、增删行列和二维导航 | CT-115—117 及表格 IME/AX/复制用例通过 |
| H3 | 大文档、长块缓存与增量性能 | 固定机器实测，无源码损坏、过期布局编辑或无界缓存增长 |

表格是本方案目标能力，H1 的表格暂以原文可编辑，不能把它宣称为已经完成 Typora 风格表格。Image / Math 暂保持源码，另行扩展。

这一路线减少了块前缀随焦点反复显隐造成的重排，但仍保留 Inline 精细映射，并新增结构命令、局部源码作用域和表格二维导航。因此不能把完整 Hybrid View 等同于简单的“当前整块显示源码、其他块预览”，也不能据此认定整体更容易实现；H0 的原生文本、IME 与辅助功能验证仍是前置条件。

增量渲染是失效策略：Inline reveal 通常只影响相关布局单元，结构编辑可能影响容器或后续块；围栏和引用定义变化允许扩大解析范围。不能以“Hybrid”名称承诺任意长段落常数时间重排。源码映射是版本化关系，不是一个保证双向唯一的整数偏移表。

## 8. 行为验收补充

编号接续第八份 CT-101；同一 ID 只在本文定义，第八份门禁引用本节。

| ID | 场景 | 预期 |
|---|---|---|
| CT-102 | `**粗体**` 中进入/离开内容 | 仅 Inline marker 展开/隐藏；源码与历史不变 |
| CT-103 | 输入 `# 标题` 后 Return | 标题结构化、前缀隐藏；后续为普通段落；实际源码编辑可 Undo |
| CT-104 | 悬停、点击已渲染标题及离开标题 | 悬停不展开；点击显示可编辑 `#` / Setext 下划线；光标离开收起；源码和历史不变 |
| CT-105 | 引用正文进入/离开，Return 延续/退出 | `>` 不随焦点显示；结构命令最小修改源文本 |
| CT-106 | 列表正文进入/离开、Tab 缩进 | bullet/编号保持；缩进一次 Undo 恢复 |
| CT-107 | 切换任务复选框 | 只修改对应 `[ ]` / `[x]`，不创建独立编辑器 |
| CT-108 | 代码块内容编辑和 Return | 围栏保持隐藏；正文中 `**` 为字面内容；代码换行正确 |
| CT-109 | 进入/退出 EditBlockSource | 显示精确块源码，再按最新语义恢复结构化；纯切换不入历史 |
| CT-110 | Source 中破坏围栏/前缀后退出 | 按权威语义重新呈现或原文回退，不保留错误的旧结构 |
| CT-111 | 全文 Hybrid/Source 切换 | 同一 Rope/选区/Undo，未编辑内容逐字节保持 |
| CT-112 | 标题经列表拖选到引用 | 单一 Source Selection；外层前缀不因选择自动展开，复制 Markdown 保留结构 |
| CT-113 | IME 候选确认使用 Return / 组合中切换模式 | 候选确认不重复拆块；模式切换协调组合，几何无错位 |
| CT-114 | Hybrid 普通段落 Return / Shift+Return，Source Return | 分别生成段落边界、硬换行和单个源码换行；Undo 原子恢复 |
| CT-115 | 合法表头和分隔行后 Return（H2） | 显示网格，delimiter 隐藏；再次进入单元格不展开整表 |
| CT-116 | 表格增删行列、导航、Copy Markdown（H2） | 源码转义/对齐/单元格范围正确；结构操作一次撤销 |
| CT-117 | 表格进入源码、编辑、退出（H2） | 网格与新语义一致；无效表格安全回退源码，Selection/IME/AX 同版本 |

## 9. 决策归属

本文用 `HV-001`（Inline/Block 分策）、`HV-002`（结构化块与源码入口）、`HV-003`（类型相关触发和 Return）、`HV-004`（局部/全文源码模式）、`HV-005`（FlowBlocks 与表格映射）、`HV-006`（H1/H2 阶段范围）标识本次决策，不占用旧文档中已冲突的 ADR 编号。

当前 Hybrid 行为以本文为准；第八份的 Inline、复制、事务、IME 和文件契约继续适用。第十份仍记录平台与算法风险，采用 Hybrid 并不意味着这些风险已经验证解决。

## 10. 焦点/悬停块类型入口（后续明确需求）

鼠标悬停块正文时，无需点击即可在该块左上角、紧贴内容外侧显示类型标识；
编辑器获得焦点时，当前光标所在块也显示标识。悬停目标优先于光标所在块，不移动光标或选区。
标题使用 H1–H6，普通段落、列表项、引用、图片、代码、Mermaid、表格及原始块使用
各自的简短符号。默认不常驻；类型入口与正文共享排版版本，随滚动及嵌套定位。
列表入口位于编号或圆点外侧，代码入口与代码区域顶部对齐，表格入口位于网格外侧。

悬停标识打开块菜单，点击菜单项执行编辑/收起局部源码、选择整块、复制块源码。
局部源码保留完整外层容器；选择和复制使用原始源码范围。Escape、点击菜单外侧、
移开鼠标、移动光标或滚动关闭菜单；从标识跨过间隙进入菜单时保持展开。
编辑后旧菜单范围失效，不能用于操作新源码。
拖选、IME 预编辑及全文源码模式下隐藏入口，纯焦点、悬停和菜单显隐不修改源码或历史。
块类型转换、前后插入、删除与拖动为后续迭代。

标题仅在点击后显示可编辑的 `#` / Setext 下划线，光标离开标题后收起；仅悬停不展开，
鼠标移开不收起已点击标题的标记。这项最新要求优先于旧的悬停显隐与标记始终隐藏约定；
块类型标识仍可悬停显示，不影响标题标记的点击交互。
