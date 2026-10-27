# 架构、逻辑、扩展性与性能审查（2026-10-28）

后续状态：A3、A4、A8 及 A7 普通行分支已修复，见[低风险修复记录](../../38_Low_Risk_Architecture_Repairs.md)。
下文保留审查基线的问题描述；`probe-output.txt` 为修复前输出，探针已调整为检查修复后的注册/保存行为。

审查基线：当前分支 `4d888da`，开始时工作区干净。本次不修改生产实现或依赖，
仅增加审查报告和独立复现工具。

## 结论与范围

九个包的分层、源码唯一真相、候选事务发布、不可变派生快照和旧帧隔离方向合理，
可以继续在现有结构上演进。当前主要问题是部分入口没有共享同一套宿主约束，
以及投影缓存、控件缓存和排版缓存的失效范围仍不完全一致。

本次确认 **8 项 P2 问题**，其中 5 项是此前复查 F3–F7 仍然存在的问题，
3 项是本轮新增。5 项有独立探针复现，3 项由实际调用路径确认；不把静态复杂度
换算成未经测量的 GUI 延迟。本轮没有确认新的 P1，也没有发现需要推倒分层架构的依据。

审查覆盖 core 的事务/历史/保存基线，syntax 的快照和查询目录，projection 的映射和校验，
extensions 的注册、缓存、回调边界，features 的命令和主呈现，session 的发布、IME、
图表任务和导出，以及 UI 的刷新、排版、导航、平台输入、文件操作和服务适配。
同时核对高亮、公式缓存、Mermaid、原子文件写入和共享 PDF 排版。
这是一轮关键路径审查，不代表穷尽全部输入组合。

此前 F1/F2 的投影消费校验及块查询修复已经在当前代码中存在；本次不将它们列为未修复问题。

| 编号 | 优先级 | 问题 | 证据 | 与旧审查关系 |
| --- | --- | --- | --- | --- |
| A1 | P2 | Undo/Redo 跳过验证器与部分候选一致性校验 | 探针 + 静态 | F3 仍存在 |
| A2 | P2 | 控件身份回调失败使整个批次失效 | 探针 | F4 仍存在 |
| A3 | P2 | 重复注册返回错误，却已覆盖旧执行提供者 | 探针 | 新增 |
| A4 | P2 | 文件端口 panic 不生成失败回执，保存状态无法恢复 | 探针 | 新增 |
| A5 | P2 | 动态控件使正文完整排版缓存失效 | 静态 | F5 仍存在 |
| A6 | P2 | 投影缓存命中时，图表候选仍扫描全部代码块 | 计数探针 | 新增 |
| A7 | P2 | 上下导航仍遍历完整视觉行数组 | 静态 | F6 仍存在 |
| A8 | P2 | 单段公式混排仍有折行数 × 公式数的扫描 | 静态 | F7 仍存在 |

## A1：统一历史与普通提交的验证边界

位置：[session/lib.rs:452](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/lib.rs:452)、
[普通发布检查:206](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/lib.rs:206)。

`undo()` 准备反向事务后直接调用 `core.publish_history()`，不经过 validators。
历史导航规划只检查源码/解析身份与光标字符边界，没有复用普通发布对 closers、
reveal、heading_reveal、block_source 的全部检查。

探针先允许输入，再关闭验证器：普通输入被拒绝；Undo 和 Redo 仍改变源码，
两次历史操作的验证器调用总增量为 **0**。
因此未来只读策略、文档约束等扩展对不同修改来源会表现不一致。
这不等同于已证明内置历史会破坏正文。

建议提取统一的候选一致性检查和发布前验证阶段，显式传递提交来源，
让需要历史豁免的验证器声明适用范围。历史恢复继续绕过输入补全。
`undo()` 应返回可报告错误；拒绝时源码、双栈和选择状态均不改变。

## A2：注册时冻结控件身份，补齐提供者隔离

位置：[render_actions.rs:217](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-extensions/src/render_actions.rs:217)、
[未缓存错误分支:193](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-extensions/src/render_actions.rs:193)。

`cached_control_batch()` 构建当前提供者目录时，在局部故障边界之外调用 `id()`；
缓存键、错误诊断与命令查找等处也反复回调身份元数据。
启动注册成功后，只要一个提供者的 `id()` panic，外层 GuardedFeatureEngine
就返回整个批次错误，UI 随后清除健康提供者的控件。

探针确认：故障前健康控件数为 1，故障后整个批次返回错误。
panic 已由外层捕获，本项结论是故障隔离粒度错误，不是 panic 逃出原生事件循环。

建议注册记录直接保存稳定 ID、命令目录和实例 Arc，运行期不再回调身份。
仍可能失败的依赖声明、controls 和 plan 按提供者局部捕获；故障只删除自身贡献。
回归同时覆盖缓存、未缓存和命令执行路径。

## A3：失败的注册操作不能替换已有实现

位置：[registry.rs:129](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-extensions/src/registry.rs:129)。
同样模式存在于 controls、preview、block_ui、templates 和 presentation 注册入口。

当前使用 `map.insert(id, provider).is_some()` 检测重复，检测时旧值已经被替换。
若模块处理可选注册的失败并继续返回 `Ok(())`，staged 中只保留第一次贡献描述，
执行映射却指向第二次被“拒绝”的实现，最终 Registry 仍可构建成功。

探针注册 handler 1，再尝试同 ID 的 handler 2；第二次确实返回错误，
构建后实际执行的却是 **handler 2**。正常使用 `?` 的内置模块会中止整次构建，
因此本项主要影响会恢复注册错误的扩展模块。

建议所有注册入口使用 `BTreeMap::entry`，仅在 Vacant 时写入并追加 staged。
Occupied 返回错误且不改动任何状态。增加“注册失败后继续构建”的回归，
不只测试失败立即向上传播的情况。

## A4：文件服务异常必须落成终态回执

位置：[session/files.rs:33](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/files.rs:33)、
[保存状态入口:524](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/lib.rs:524)、
[UI 后台任务:239](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/files.rs:239)。

`SaveRequest::run()` 直接调用可替换 FileStore，没有转换 panic 的宿主边界。
write 一旦 panic，就没有 SaveReceipt，`complete_save()` 无法清除 active/saving。
后续保存只会不断替换 queued；正常 UI 的打开和关闭也会一直遇到“正在保存”。

探针在独立进程注入 panicking FileStore，并在外部捕获 unwind：
run 未返回回执，`saving=true`，再次 queue_save 没有派发请求。
这证明会话状态不能自行恢复；没有在 AppKit 中注入 panic，也未据此宣称现有
AtomicFileStore 的普通 I/O 错误会触发此问题。普通 Result 错误已有正确回执路径。

建议在任务服务调用边界将 unwind 转换为 `io::Error` 并始终生成对应失败回执，
让既有串行队列统一完成清理与后续调度。同步 save/read 入口也应采用一致策略。
另外检查 PDF exporter、ArtifactStore、UpdateChecker 的后台任务终态清理；
它们也不应依赖每个替换实现自行捕获 panic。

## A5：把控件动作状态与正文排版的失效拆开

位置：[UI 接收批次:458](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/lib.rs:458)、
[ControlIndex 代次:80](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/text_layout.rs:80)、
[LayoutKey:120](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/text_layout.rs:120)。

UI 按聚合 Arc 身份判断控件变化。一份依赖选区的贡献只改 enabled/checked，
或未声明缓存的提供者每次返回同样的空 Vec，都会生成新聚合 Arc，重建 ControlIndex
并取得新 generation。该代次直接进入正文 LayoutKey，下一帧完整 TextLayout::build 重跑。

分片缓存减少了健康贡献者回调次数，却没有隔离正文 shaping 成本。
这会使简单的动态菜单扩展把普通光标移动重新变成全文排版。

建议区分正文宽度/度量键、锚点几何键和菜单动作状态。
仅保留宽度、正文样式或资源度量变化重做 shaping；动作状态作为帧覆盖层更新。
锚点变化独立重建相应索引。不能仅删除 control_generation：旧的可见索引仍可能
按旧控件下标取值，必须同步设计引用与代次。

新增窗口计数回归：恒定空提供者、只改变 enabled 的选区提供者、
同宽不同目标控件。核对布局构建次数、目标执行与旧帧拒绝。

## A6：投影命中后仍存在图表候选的全文工作

位置：[UI refresh:527](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/lib.rs:527)、
[session diagram_views:373](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/lib.rs:373)、
[registry preview_batch:388](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-extensions/src/registry.rs:388)、
[内置输入构建:36](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/diagrams.rs:36)。

每次 refresh 在查询正文投影缓存之前，都调用 diagram_views → preview_inputs。
Registry 为每个代码块遍历贡献目录，询问各预览提供者；只有找到 32 个实际输入才提前停止。
因此只有普通代码、没有图表的文档，恰好会扫描全部代码块。
存在图表时，内置输入还会重新克隆 CodeBlock、收集行并拼接正文，然后才查询内容缓存。

独立计数探针模拟 UI 调用次序，在普通 tail 段落交替移动光标：

| 普通代码块数 | 正文投影 Arc 命中 | 返回 None 的预览提供者调用 |
| ---: | ---: | ---: |
| 100 | 10/10 | 1,000 |
| 1,000 | 10/10 | 10,000 |

这是工作量复现，不是 GUI 延迟测量。热结构目录避免了重新解析，但没有消除候选遍历。

建议注册时预先形成可执行预览目录；内置按规范化语言筛选候选，避免遍历无关贡献。
将与源码/配置有关的候选及正文提取，与光标、选区、冻结、局部源码产生的可见性判断分开。
前者按源码身份、提供者实例及显式依赖代次复用；后者只处理有界候选。
PreviewProvider 目前可读取整个 EditingState，不能直接强行按源码缓存所有第三方提供者；
需要像控件一样显式声明依赖，未知实现保留安全重算路径。

## A7：上下导航复用已知视觉行下标

位置：[layout.rs:695](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/layout.rs:695)、
[layout.rs:734](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/layout.rs:734)。

普通行通过 row_for 找到行后，仍从头使用 `position(ptr::eq)` 找下标，
文末每次上下键为 O(视觉行数)。单元格分支过滤全文视觉行查找同格下一行，
离开单元格时再扫全文寻找上下最近行。正文排版缓存无法消除这些扫描。

建议 row_for 返回或携带下标；普通上下邻接直接访问。
补充单元格到视觉行范围和行带邻接目录，复用 TextLayout 生命周期。
表格多列可能共享 Y、行高不等，不能把 rows 当作全局 top 严格递增数组。
回归覆盖自动折行、公式行高、不同高度单元格和跨表格边界导航。

## A8：公式归属使用有序游标

位置：[layout.rs:165](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/layout.rs:165)、
[layout.rs:206](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/layout.rs:206)。

shape_inline 先取出当前段落的公式切片，再对每个折行用 filter 扫描该段所有公式。
R 个折行、F 个公式的归属检查为 O(R×F)，长单段的公式密集内容尤为明显，
输入和窗口宽度变化均会重新执行。

建议有序折行边界与公式切片共用推进游标，每个公式分配一次，归属工作 O(R+F)。
保持公式原子范围、基线和 grapheme 约束；用“单个长段包含很多公式”进行窗口缩放回归，
不能仅用很多短公式段落验证。

## 扩展能力的后续设计

以下是已明确保留的能力限制，与上面的缺陷分别管理：

1. **按贡献冻结导出依赖。**
   [MarkdownFeatures::snapshot:78](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/lib.rs:78)
   对任何 additional 模块返回 None，纯命令模块也关闭 PDF 导出。
   建议注册时区分是否影响导出结果，为呈现和资源提供冻结能力；纯命令/UI 模块可明确不参与导出。
   保留未冻结依赖拒绝导出的原则，不能退回实时服务。
2. **让 Decoration 成为可执行组合协议。** 当前只是注册描述，
   document 主提供者独占，code/list/quote/table 的 BlockContribution 仍是 features 内部组合。
   开放前先定义只读范围贡献、优先级、映射归属、冲突处理、缓存依赖和导出等价性。
   先支持所需的静态结构扩展，再考虑动态加载；无需为了扩展性提前引入任意 Widget。
3. **收敛语义查询入口。** ParseSnapshot 仍 Deref 暴露解析器的 nodes/syntax，业务可继续绕过目录。
   为后续增量语义更新逐步通过 SemanticQuery 封装实际需要的祖先、范围、容器和内联查询。
   保留现有解析快照做差分基线，避免一次替换解析器、源码存储和投影协议。

## 性能实施顺序

建议先处理 A1–A4，稳定提交、注册和后台终态；再做 A5–A8，
消除缓存命中场景里的重复工作。两组适合拆成独立、可验证的小改动。

随后按既有大文件路线推进：

- 先补真实窗口输入至显示的阶段计时、布局构建次数、缓存命中率、分配量、峰值 RSS 和在途快照数。
  当前 RYNE_PROFILE 的 refresh/layout_frame/paint/text_layout_build 作用域有用，
  但还不是完整输入端到端分布。
- P3：稳定块身份、块级排版缓存、可见范围精排及高度索引；优先证明长单段和表格内的导航/IME 几何。
- P4：语义查询门面、保守局部解析与分块投影一起推进。只优化解析后仍拼接全文 Display，收益会受限制。
- P5：预编辑局部覆盖与有界后台任务，继续绑定完整源码/临时代次；不能发布新源码后用旧 AST 解释输入。

现有内容修改、历史恢复、IME 临时投影仍全文解析，正文仍全文精排。
这些在 AGENTS 和大文件设计中已经明确，不作为本轮新发现。
Tree-sitter 的 50 ms 预算只覆盖语言识别；显式语言高亮仍同步执行，
64 项高亮缓存之外的正文可能在下一次新投影中再次高亮。
后续可按内容调度高亮任务并预算实际高亮阶段，不能把识别预算当总刷新预算。

性能验收应分别覆盖 LF/CRLF/CR、普通段落、很多小围栏、单个长代码块、
大表格/多小表格、列表/引用、长单段公式、Unicode，记录冷/热状态及文档前中后位置。
当前没有足够证据选择更换 rope 或解析器作为第一步。

## 本次验证与交付

- `python3 scripts/check-architecture.py`：通过，9 个私有包生产依赖边界通过。
- `cargo test --locked`：全量通过。
- `cargo fmt --check`、`git diff --check`：通过；审查新增文件另校验链接和行尾空白。
- `/usr/local/opt/rust/bin/cargo-clippy clippy --locked --all-targets -- -D warnings`：通过，
  无本项目 Clippy 警告；Cargo 仍提示既有依赖的未来兼容性通知。
- 独立探针：`python3 docs/reviews/architecture-2026-10-28/run.py`：退出码 0；
  输出见 [probe-output.txt](probe-output.txt)。断言用于确认当前问题可复现，不是修复完成的验收标准。
- 本轮没有执行新的原生窗口冒烟、Windows 实机、真实输入法人工验收或 release GUI 性能/RSS 测量。
  A5/A7/A8 仍需实施时补窗口计数或规模回归，不能用现有全绿测试代替。

本目录 runner 读取 Cargo JSON 确定依赖产物，在临时目录编译探针，
不新增 Cargo target、不写用户文档；panic 注入只在独立进程执行并被捕获。
修复时应将相应正确性断言转入正式回归。
