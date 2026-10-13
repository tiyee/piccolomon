# 渲染模式交互扩展设计与可行性验证

日期：2026-10-13。基线：[分层架构 v2](17_Ryne_Layered_Plugin_Architecture_v2.md)、
[实际实施记录](18_Ryne_Architecture_Implementation.md)。

状态：本文保留初始架构设计与无窗口原型验证。后续正式表格菜单和可执行扩展通路已接入，见
[21 · 表格交互实施记录](21_Render_Mode_Table_Interactions.md)。列表前置按钮及条目操作已沿同一通路接入，
见 [22 · 列表交互实施记录](22_Render_Mode_List_Interactions.md)。任意 Widget 和新呈现原语仍未交付。

## 1. 目标与结论

扩展可以在渲染模式下贡献自己的展示、交互区域、控件和操作处理器。表格 cell 的菜单、
列表项的添加按钮、代码块工具栏和自定义预览的操作都通过同一条协议接入。
新增操作只增加扩展实现与启动装配，不修改编辑器中的 Markdown 类型分支。

当前架构可以承载这一系统：不可变快照、`CommandPlan`、`EditorSession::publish_plan`、
源码历史和 GPUI 元素事件已经具备。需要补齐可执行的注册、语义子目标、布局锚点和事件路由。
原型已经编译并直接使用真实 Markdown 功能、解析、投影、会话校验与撤销，验证了操作链路。
真实 GPUI 几何接入、焦点往返和键盘菜单操作仍需后续窗口验收。

## 2. 当前实现的缺口

| 当前落点 | 限制 | 设计调整 |
|---|---|---|
| `extensions/registry.rs` 的 `Contribution` | 只有描述和排序，没有可执行的 UI/命令提供者 | 注册描述与处理器一起校验、原子发布 |
| `AdornmentSpec` / `BlockCommand` | 块菜单及三个固定命令 | 作为通用渲染交互协议的兼容适配器 |
| `features/lib.rs::block_ui` | 聚合生成菜单 | 按渲染上下文查询多个提供者 |
| `ui/lib.rs::block_action` | UI 匹配业务命令 | 改为统一激活与命令分派；宿主服务处理复制等效果 |
| `ui/layout_frame.rs` 的 table cell 矩形 | 绘制期间临时保存，未绑定完整语义目标 | 发布带目标身份的命名布局区域 |
| `ui/layout.rs::Geometry` | 已有 snapshot、frame_epoch、源码模式校验 | 复用并覆盖滚动、弹层和渲染交互版本 |
| `FeatureEngine::plan(EditIntent)` | 新命令需要中央枚举分支 | 保留基础输入；新增可注册的扩展命令通道 |

仅开放更多 `BlockCommand` 变体，或只在 UI 中增加回调，都不能满足上述目标。

## 3. 扩展单元与分层

一个功能按需注册以下提供者，不要求实现一个包含所有能力的大 trait：

```text
RenderBundle（功能 ID / 依赖 / API 版本 / 消息与资源）
  PresentationProvider  语义与视图 → 主呈现 / 装饰 / 源码映射贡献
  InteractionProvider   语义目标与呈现角色 → 控件 / 区域 / 事件绑定
  CommandHandler<P>     只读快照、已解析目标、参数 → 命令计划

语义及显示贡献 → GPUI 布局和命名区域 → 控件绘制与命中
  → 激活令牌 → 命令路由与重新解析 → 源码计划 → 会话统一提交
```

一个节点只有一个主呈现拥有者；多项装饰、工具栏和交互贡献可以组合。
表格呈现提供 cell 区域，另一个功能可向该区域贡献操作，不必接管表格排版。
渲染变体使用稳定 `RenderVariantId`，如代码正文和图表预览；主呈现选择先服从显式配置和
依赖规则，再校验唯一拥有者。操作提供者声明支持哪些变体，切换变体使旧交互失效。

| 包 | 新职责 |
|---|---|
| `ryne-projection` | 呈现角色、区域标识、原语及源码映射；不认识按钮业务 |
| `ryne-extensions` | 目标、交互贡献、命令描述、处理器和宿主效果的类型协议 |
| `ryne-features` | 具体语义目标解析、展示贡献、控件配置、可用性和命令规划 |
| `ryne-session` | 持有不可变注册表；解析目标、路由命令、校验并提交；无 GPUI 依赖 |
| `ryne-ui` | 注册通用控件适配器，生成当帧几何，管理命中、弹层、焦点和临时状态 |
| 应用装配 | 注册提供者、服务、翻译与图标；决定启用哪些功能 |

当前继续静态编译、启动注册，不引入动态卸载或公开 SDK；进程内扩展不构成隔离边界。

## 4. 语义目标：不以光标代替点击对象

```text
TargetRef = DocumentKey + OwnerRef + SemanticRole + PartPath
ResolvedTarget = 同一快照的语义对象 + SourceRange + 完整容器 scope
RegionKey = ProviderId + ComponentId + TargetRef + RegionRole
```

`SemanticRole` 允许功能声明自己的命名空间，如 `table.cell`、`list.item` 或 `custom.badge`，
不在宿主维护一份封闭的目标类型枚举。`PartPath` 是已声明 schema 的部件定位信息：
表格可使用行列路径，列表可使用当前修订中的节点位置；业务解释归目标解析器。
源码范围必须包含文档身份。部件路径和 AST ID 只在对应修订内有效，不宣称跨编辑稳定。

点击时将实际目标写入激活令牌。执行时验证 DocumentKey，并由所属解析器重新确认拥有者、
角色、路径和容器；不能根据当前光标猜目标，也不能仅检查范围没有越界。
首版任何源码修订都使旧菜单失效，不自动将旧行列索引迁到新表格。后续如需保留弹层，
必须显式提供重绑定算法并再次确认身份，不能只做字节偏移平移。

## 5. 展示与交互贡献

以下是目标协议草图，不是已经发布的 Rust API：

```text
InteractionContribution {
  id, provider, target, render_variants,
  anchor: RegionKey,
  placement: InsideTrailing | Above | Below | Leading | Overlay,
  space: ReserveInline(width) | ReserveBlock(height) | Overlay,
  visibility: Always | HoverTarget | FocusWithin | HoverOrFocus,
  control: Button | MenuTrigger | Toolbar | RegisteredWidget,
  accessible_name: MessageKey,
  bindings: [EventBinding], order, group
}
EventBinding = Activate(CommandRef) | OpenMenu(MenuSpec) | InteractionReducerRef
MenuItem = CommandRef + MessageKey + IconRef? + Availability + group/order
```

业务扩展声明语义区域，不提交绝对屏幕坐标。呈现提供者使用已有结构原语标记区域，
GPUI 文字排版完成后统一产出 `RegionKey → Bounds`。表格 cell、列表项正文与整个子树
分别有区域；控件可以靠首行对齐，也可以挂在完整子树后。

需要占正文空间的入口在 shaping 前预留尺寸，悬停时只改变可见性，避免文字折行跳动。
悬浮入口必须避免正文命中冲突。控件、菜单与正文共用本帧内容栏、滚动和缩放变换。
装饰区域不添加投影字符，不改变源码映射；空单元格仍有可编辑文本锚点。

同槽多个贡献按依赖、分组、显式顺序与稳定 ID 排序；不足空间时进入工具栏或溢出菜单，
不能静默丢失低优先级命令。相互矛盾的独占布局必须诊断并撤回该提供者贡献。
原型暂时诊断碰撞，尚未实现正式分组与溢出布局。

### 5.1 真正的新展示或控件

常见操作由平台无关的按钮、菜单、工具栏和输入控件组合。需要新的形态时，添加
`WidgetKind` 与对应的 GPUI 适配器，并注册其测量、绘制、命中和事件实现；功能层仍不返回
任意 GPUI View。适配器在 UI 包或依赖 GPUI 的私有适配包，不能逆向导入功能实现。

自定义呈现仍须提供源码映射、导航入口及失败回退；不能将交互控件变成另一套富文本编辑器。
当前 `Display` 是有限结构模型，任意新布局需要扩展展示原语及其映射校验，不能承诺未知
渲染形态只注册元数据就能完成。已支持原语内的新增操作则应无需修改通用宿主。

## 6. 命令：注册实际处理器

```text
CommandDescriptor<P> {
  id: FeatureId + LocalCommandId,
  parameter_schema: P,
  target_roles, execution_kind, message_keys
}
CommandHandler<P>::availability(snapshot, resolved_target, params) → Availability
CommandHandler<P>::plan(snapshot, resolved_target, params) → CommandPlan
```

注册器绑定命令 ID、参数验证器和实际处理器，不只是保存字符串。
`P` 是命令自己的参数类型，例如列对齐或插入数量；类型擦除仅发生在注册层。
跨边界用有限、可校验的值或不透明注册令牌传递，不允许调用者传任意 `Any` 对象。
菜单命令、快捷键和命令面板可以复用同一处理器；后两者也必须明确解析目标来源。

可用性有 `Hidden`、`Disabled(MessageKey)`、`Enabled`；生成控件和执行命令时都检查。
删除表头或最后一列如何处理由表格功能定义，宿主不包含这类 Markdown 规则。
跨块选区如何处理也由命令声明：支持完整选区、保留选区，或禁用；不能悄悄按起点操作。

编辑结果复用 `CommandPlan::Edit/Select/View/Consume`。正式接入时提取共享候选构建器，
统一变换选区、显隐范围及补全检查点；不能让每个扩展复制这些算法。普通输入继续沿用
`EditIntent` / `InputDraft`，操作命令不再经过自动补全管道。

复制、打开链接、文件选择等使用明确的宿主效果请求，由 UI/应用适配器执行；
外部效果无法与源码撤销原子化。首版一次命令选择编辑计划或宿主效果，避免把不可回滚
效果放在可能被验证器拒绝的事务前。长任务使用现有有界任务端口，回执校验目标和输入版本。

## 7. 事件、状态与一致性

```text
InteractionFrameKey = DocumentKey + ProjectionKey + RenderVariant/ConfigGeneration
                    + LayoutGeneration + ViewportGeneration
ActivationToken = 注册处理器身份 + CommandRef + TargetRef + FrameKey
```

FrameKey 由可信宿主维护，不由功能回调随意指定。滚动、窗口尺寸、字体、缩放、资源度量、
显隐或渲染变体变化均使相应几何失效；仅闪烁不使它失效。完整设计可将菜单目标身份与
几何身份拆开，滚动后重新定位；首版采用更保守的关闭并重新激活策略。

事件顺序：最上层弹层 → 命中的操作控件 → 允许的渲染组件事件 → 正文编辑回退。
`Handled` 停止传播，`Pass` 继续；正文区域不因控件旁边的鼠标移动而丢失光标与选区。
按钮按下消费事件，不先移动正文光标；激活后按照贡献声明保留或恢复编辑焦点。
拖拽类组件如需要捕获指针，使用宿主捕获令牌；宿主在失焦、源码变化或关闭时释放。

`InteractionState` 仅保存菜单、悬停、按压、控件草稿等临时状态，按视图/提供者/目标/组件标识
隔离并限制数量。源码变化、文档关闭、源模式切换使相关状态失效，不写入 Markdown 或内容历史。
需要编辑数值或语言的控件草稿必须确认后才生成事务；取消恢复无修改状态。

IME 预编辑和正文拖选期间不激活结构操作，也不进行非必要显隐重排。确认候选的 Enter
只由平台输入桥接处理，不能同时激活菜单项。全文源码隐藏渲染操作，局部源码隐藏对应 scope
及其后代的入口。所有这些状态在执行时再次检查，不能只靠按钮已隐藏。

键盘入口保留可访问名称、控件角色、顺序和可用性。菜单支持方向键、Enter、Escape，
关闭后恢复正文焦点与原选区。指针和键盘最终产生同一种激活令牌。
消息和图标跟随扩展注册，语言切换由宿主解析 MessageKey，不添加逐命令的翻译 match。

## 8. 注册、失败和预算

启动先验证功能依赖、内部 API、重复 ID、命令处理器、消息/图标引用、目标解析器、
控件适配器及独占呈现冲突，全部成功后发布不可变注册表。共用宿主命令需显式引用其描述；
不能伪装其他提供者身份。依赖存在不表示可以操作其全部语义目标，适用角色仍需校验。

展示回调只读，不提交命令或重入构建。失败在可恢复调用边界转成诊断，撤回该提供者本次贡献；
主呈现失败时同步撤回其隐藏及替换，保留可编辑源码。UI 回调不能让 panic 穿过 AppKit。
静态进程内回调不能被安全强制中断，耗时预算是测量和诊断策略，不承诺硬截止。

按可见语义区域懒生成控件，限制每区域控件和菜单项数、临时状态数及资源量；
生成贡献按语义/配置版本缓存，悬停通常只更新显示状态。原型为简化遍历全文并线性查找区域，
没有建立大文档性能结论。正式实现必须使用区域索引并测量表格/列表密集文档。

## 9. 独立验证原型

文件位于 [reviews/render_interactions](reviews/render_interactions/README.md)。
仅在 `ryne-session/Cargo.toml` 注册一个 example，不改变生产模块、依赖版本或正式 GUI。

```sh
cargo run --locked -p ryne-session --example render-interactions
cargo test --locked -p ryne-session --example render-interactions
```

原型宿主不匹配 `table/list/custom.badge` 的业务类型，只按注册提供者路由。
表格和列表通过真实 AST 提取目标；处理器只拿到不可变 `EditingState`，返回候选计划。
提交使用真实 `EditorSession::publish_plan`，编辑结果由真实 Markdown 投影重新呈现。
第三个 badge 扩展注册新的角色及非编辑选择命令，无需修改宿主。

| 验证项目 | 原型证据 |
|---|---|
| 内部语义目标 | 点击另一个 cell 删除对应行，光标仍可在表格外 |
| 菜单与直接按钮 | 表格多操作菜单；列表单操作直接激活 |
| 插入边界 | 表头下插入在分隔行之后；表头删除禁用 |
| 容器及源码 | 引用中的表格；引用有序列表含嵌套子树；Unicode、转义管道 |
| 换行与末尾 | LF/CRLF/CR；有/无末尾换行；未修改字节保持 |
| 历史 | 操作一次撤销；Redo；撤销恢复原选区和 clean |
| 纯视图行为 | 贡献、布局、打开菜单不修改源码、选区或历史 |
| 旧身份 | 旧修订、另一文档、相同源码的新布局代次拒绝 |
| 输入状态 | 全文/局部源码、拖选、真实 composition 状态拒绝 |
| 提交失败 | 真实会话验证器拒绝后无部分修改 |
| 扩展注册 | 新目标角色与命令、重复提供者、执行前重新检查可用性 |
| 几何回退 | 缺失/非法几何和控件碰撞报告错误；不静默吞掉操作 |
| 语法回退 | 不规则表格不贡献结构操作，源码保持可编辑 |

几何来自确定性 fixture；原型验证坐标命中和版本比较算法，不验证真实 GPUI 度量，
也没有实现弹层焦点、键盘导航、实际预编辑候选窗、通用控件状态机和辅助功能。
同一目标的多个命名子区域、参数 schema、共享宿主效果、注册 Widget、主呈现选择及溢出布局
属于设计协议，未在原型中实现。
这份证据支持操作链路与既有会话兼容，不能据此宣称任意自定义 UI 已可直接接入。

## 10. 正式接入顺序与完成标准

1. 在 extensions/session 建立可执行的提供者、目标解析器、命令注册和共享候选构建器。
   将原型的旧身份、原子提交和历史用例迁为生产契约测试。
2. 为现有投影发布命名区域，在 GPUI 当帧布局绑定目标及几何；先将现有类型角标、
   块源码/选择/复制菜单迁入新通路，保留现有产品行为与七语言文案。
3. 增加按钮、菜单、工具栏适配器与控件焦点状态，完成同槽组合、正文空间预留和键盘激活。
   用测试扩展验证内部目标入口，之后再独立交付具体表格或列表业务操作。
4. 按真实新形态需求增加 Widget 和呈现原语；测量可见区域生成和密集结构场景。

机制验收：新增同类渲染操作只增加提供者、处理器、资源/消息和应用注册；
不修改 UI 或会话中的业务类型 match；编辑只有一个事务、一个正文和一套选区。
新布局原语允许有明确的宿主改动，但需要相应源码映射、导航和回退测试。

GUI 验收至少覆盖真实 cell/列表几何、滚动缩放后命中、控件不遮正文、按钮不抢正文选区、
菜单键盘操作与焦点恢复、IME/拖选冻结、嵌套局部源码和无 panic 失败退出。
按项目约定执行架构、格式、测试、Clippy 与真实窗口冒烟；冒烟不代替完整人工交互验收。

本次实际执行的命令与结果见 [原型验证记录](reviews/render_interactions/README.md)。
