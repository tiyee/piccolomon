# 当前工作区架构、逻辑、扩展性与性能复查

日期：2026-10-27。范围为九个 workspace 包、应用装配、GUI 输入/排版/绘制、
扩展注册与宿主、解析/展示索引、后台图表、文件保存和两种 PDF 导出。
包含审查开始时已有的未提交修改；没有修改生产代码、依赖、原有测试或原有审查记录。
新增文件仅位于本目录。

后续状态：F1/F2 两项 P1 已实施并完成验收，见[实施记录](../../../37_P1_Repair_Implementation.md)。
下文保留审查时的复现和测量事实，不代表修复后的结果；其余 P2 项仍是后续工作。

旧报告 R1–R10 及三批修复按当前代码重新核对；局部源码控件键、语言识别收敛、
左右 grapheme 查询、命令内映射共享、表格行内查询、PDF 样式游标、dirty 缓存等，
没有作为当前未修复问题重复报告。

结论：现有源码唯一真相、候选事务发布、不可变解析/展示快照、服务端口与九包依赖方向合理。
值得优先改进的是扩展输出不变量、历史发布与普通提交的一致性，以及缓存命中后仍运行的交互热点。
无需先改包名、引入动态插件系统或更换文档存储。

证据类型：**复现**为当前生产接口的独立探针；**测量**为指定环境的无窗口诊断；
**静态路径**为实际调用链与复杂度分析，未测量相应 GUI 延迟。
故障注入不等于正常 Markdown 已可触发同一错误。

## 优先处理的七项发现

### F1 / P1：投影校验没有覆盖排版需要的顺序、包含关系和前进条件

位置：[validation.rs:117](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-projection/src/validation.rs:117)、
[validation.rs:137](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-projection/src/validation.rs:137)、
[text_layout.rs:284](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/text_layout.rs:284)、
[layout.rs:134](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/layout.rs:134)。

**复现校验缺口，静态确认后续风险。** 取内置呈现生成的正常表格，仅把 `TableView.range`
改为 `0..0`，`Display::validate` 仍成功。单元格不要求包含在表格展示范围内，
表格范围也不要求消费至少一个展示字节。GUI 的排版循环在 `start == 0` 命中它后，
执行 `start = table.range.end` 又回到 0，能无限重复生成表格行。

另取三个正常公式，交换后两项后，公式位置为 `[0, 8, 4]`，校验仍成功。
`shape_inline` 依赖排序做 `partition_point`，并按迭代顺序切片
`text[offset..formula.display.start]`；逆序项可形成反向范围，导致排版阶段 panic。
该阶段在 `project_checked` 的故障边界之外。

本次只在独立探针中验证错误输出能通过校验，没有把无限循环或 panic 注入原生事件循环；
也没有发现内置 Markdown 呈现对普通输入会生成这些输出。

建议在发布前显式验证或规范化表格/公式/图表的排序、重叠策略、父子范围包含关系和对象范围，
对跳跃式排版要求 `next_start > start`。排版消费处再做可恢复的前进保护，错误时回退可编辑源码。
不要依靠外层 `catch_unwind` 解决无限循环。

验证应加入：空表格展示范围、单元格越出所属表格、逆序/重叠公式、重复对象位置，
并确认发布失败保留源码、不缓存错误结果、不会进入 GUI 排版死循环。

### F2 / P1：块类型查询让普通重绘重新扫描全文并分配行数组

位置：[block.rs:95](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-syntax/src/block.rs:95)、
[block.rs:181](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-syntax/src/block.rs:181)、
[block.rs:216](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-syntax/src/block.rs:216)、
[layout_frame.rs:374](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/layout_frame.rs:374)。

**静态路径和测量。** `prepaint_frame → active_block → focused_block → block::at`
在编辑器聚焦、没有悬停覆盖等条件下每帧运行。正文布局即使命中缓存，光标闪烁仍可进入这条路径。
普通段落/标题/引用分支调用 `code_block::lines(text)`，重新扫描全文并创建 `Vec<Range<usize>>`，
随后线性找行；此前还线性检查全部代码块和表格。列表分支也线性查条目并扫描 AST 判断任务类型。

本目录探针对已预热解析及惰性索引的普通段落，在文末调用真实 `block::at`，
每组 5 个样本，每个样本 10 次查询。环境为本机 x86_64 macOS、Rust 1.99.1、debug 库；
最终复测在本次 GUI 冒烟结束后独立执行，没有同时运行其他本次基准或编译任务。
结果记录在 [probe-output.txt](probe-output.txt)：

| 实际源码字节数 | 预热后单次查询 P50 / ms |
| ---: | ---: |
| 65,550 | 9.209 |
| 262,154 | 30.802 |
| 1,048,593 | 123.866 |

这些是查询耗时，不能作为 release GUI 帧耗时或加速倍数。

建议直接借用 `ParseSnapshot.index().lines` 二分定位；为代码/表格范围、Item ID、
外层列表首项及任务类型建立快照级目录。焦点块也可按源码身份及位置缓存；
hover、blink 等纯视觉事件不应重复查询源文档结构。

验收应覆盖普通段落、标题、引用、多小代码块和大列表，验证旧查询结果一致，
并记录缓存命中重绘中的全文访问次数和分配量。

### F3 / P2：Undo/Redo 绕过验证器及部分候选状态校验

位置：[lib.rs:232](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/lib.rs:232)、
[lib.rs:452](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/lib.rs:452)、
[lib.rs:488](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/lib.rs:488)。

**复现。** 注册一个可动态关闭的 `Validator`：先允许输入，再关闭验证器。
普通插入返回错误，但 Undo 和 Redo 仍改变源码，验证器调用增量均为 0。
历史路径直接调用 `core.publish_history`，没有经过普通提交的 validators。
历史导航规划的结果只校验源码/解析身份与字符边界，没有复用普通路径对 closers、
reveal、heading_reveal 和 block_source 的完整候选检查。

这会使未来基于只读状态、会话约束或外部条件的验证扩展，对不同提交来源表现不一致。
目前没有据此推断普通内置编辑已经损坏数据。若产品希望某些验证器对历史豁免，
也应由显式适用范围表达，而不是所有验证器都被跳过。

建议提取共享候选一致性检查和发布前验证阶段，引入提交来源（编辑/IME/Undo/Redo），
让验证器明确选择适用范围；历史恢复继续绕过输入补全。
拒绝历史发布时必须保持两条历史栈、源码、检查点和选区不变，并向 UI 返回错误。

### F4 / P2：控件身份回调仍能使健康提供者一起失效

位置：[render_actions.rs:217](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-extensions/src/render_actions.rs:217)、
[render_actions.rs:231](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-extensions/src/render_actions.rs:231)、
[lib.rs:463](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/lib.rs:463)。

**故障注入复现。** 一个健康提供者返回 1 个有效控件；另一个提供者启动注册成功，
之后 `id()` 回调 panic。`cached_control_batch` 在提供者局部捕获之外调用 `id()`，
于是整个批次返回 `render_control_batch_cached callback panicked`。
GUI 接收整个批次错误时清空所有控件与索引，健康提供者的结果也丢失。

会话外层已捕获该 panic，本次不是“panic 逃出会话/原生事件循环”的结论。
缺陷是承诺的按提供者隔离没有覆盖身份元数据调用。

建议注册成功时冻结 provider ID、命令目录和实例身份，运行时直接使用注册记录，
不再通过扩展回调获取标识；其他运行时元数据回调统一走提供者局部边界。
回归应同时验证缓存、非缓存和执行命令通路中，故障不会移除健康控件。

### F5 / P2：控件结果更新与正文 shaping 共用失效代次

位置：[lib.rs:456](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/lib.rs:456)、
[text_layout.rs:81](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/text_layout.rs:81)、
[text_layout.rs:101](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/text_layout.rs:101)、
[layout_frame.rs:15](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/layout_frame.rs:15)。

**静态路径。** 一个依赖选区的控件，哪怕只改变菜单 enabled/checked，
或者一个未声明缓存的提供者每次返回同样的空集合，都会产生新的聚合 Arc。
`accept_render_controls` 只按 Arc 身份判断，重建 `ControlIndex` 并分配新 generation。
这个 generation 进入 `LayoutKey`，使完整 `TextLayout::build` 重跑。
因此第三批的提供者分片缓存虽然避免健康提供者重复生成，仍无法隔离正文排版成本。

建议将正文 shaping/宽度依赖，与控件锚点/可见索引、菜单动作状态拆开。
只有保留宽度、文字/字体/资源度量改变才重做正文 shaping；
锚点集合改变时重建相应几何目录，菜单状态改变只更新覆盖层。
不能直接删除现有 generation 检查，否则可见目录中的控件下标可能指向旧批次。

回归应使用 selection-dependent enabled 控件、恒定空提供者、同宽不同目标控件，
统计 TextLayout 构建次数，同时验证执行时目标和旧帧保护。

### F6 / P2：上下导航仍线性遍历整个视觉行数组

位置：[layout.rs:693](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/layout.rs:693)、
[layout.rs:733](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/layout.rs:733)。

**静态路径。** 普通行的 `row_for` 已得到正确行，随后仍用
`rows.iter().position(ptr::eq)` 从头找它的下标。表格分支为单元格内下一行过滤全文，
离开单元格时又遍历全文选上下最近行。文末上下键仍是 O(视觉行数)，不受正文排版缓存改善。

建议让 `row_for` 返回/携带已知行下标，普通相邻导航 O(1)；
复用或补充单元格到视觉行下标目录、行带的上下邻接目录。
保留同一表格行多列的优先规则，不能假定所有 rows.top 全局严格递增。
验收应覆盖软折行、硬换行、公式动态行高、不同高度单元格和跨表格边界导航。

### F7 / P2：长段落的公式混排仍存在“折行数 × 公式数”扫描

位置：[layout.rs:165](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/layout.rs:165)、
[layout.rs:205](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/layout.rs:205)。

**静态路径。** `shape_inline` 已经二分取出当前段落的公式，
但对每个折行再次 `formulas.iter().filter(...)` 扫描该段所有公式。
长单段落含 F 个公式、折成 R 行时，仅归属过滤就是 O(R×F)。
正文每次变化或宽度变化都会重建该布局。

建议在有序公式切片和有序折行边界上使用推进游标，每个公式只分配一次；
保留对象原子边界、基线与跨 span grapheme 规则。测试不仅覆盖“许多短公式段落”，
还应覆盖“一个很长段落包含许多公式”和窗口缩放。
本次没有测量 GUI 中该场景的绝对耗时，不能把复杂度直接换算成延迟。

## 扩展机制的两项后续设计

以下是已知能力限制/改进方向，不与上面的缺陷混为一谈。

1. **按实际贡献冻结导出依赖。** 当前
   [MarkdownFeatures::snapshot:78](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/lib.rs:78)
   发现任何 additional module 都返回 None。增加只处理快捷键的模块，甚至无贡献模块，
   也会关闭两种 PDF 导出。拒绝使用未冻结的实时服务是正确的；下一步应区分 command-only、
   UI-only 与真正改变导出结果的呈现/资源贡献，为后者提供显式 snapshot/capability 协议。
   不应通过退回实时服务来恢复导出。
2. **让装饰贡献具有真实执行协议。** 当前 `Slot::Decoration` 有注册描述，
   但 registry 没有对应执行提供者；内置 code/list/quote/table 的 `BlockContribution`
   仍是 features 内部私有组合，主呈现独占。新增结构呈现往往需要改主组合器，
   目前是静态模块化，尚不具备开放的呈现贡献组合能力。
   建议先为只读语义范围上的样式/可见性/资源贡献定义受校验的组合协议，
   明确冲突顺序、源码映射归属、缓存依赖和导出快照，再考虑动态加载或任意 Widget。

## 大文档路线与性能验收

P1/P2 和三批修复已改善多处查询及派生开销，但
[commands.rs:229](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/commands.rs:229)、
[composition.rs:232](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/composition.rs:232)
仍对修改/预编辑构建全文解析，正文仍全文精排。
这些是现有实现明确保留的限制，没有在本次重新冒充新发现。

建议顺序：

1. 修复 F1/F3/F4，稳定候选与扩展边界。
2. 修复 F2/F5/F6/F7，先把缓存命中下的交互开销降下来。
3. 落地块级排版缓存与视口布局（P3），区分文字、装饰、控件及资源度量的失效原因。
4. 再推进局部语义/投影（P4）与局部 IME 临时投影（P5）；任何派生结果继续绑定源码身份。
5. 高亮后续可按内容任务调度、设置高亮本身的协作预算。
   当前 50 ms 仅约束语言识别；显式语言的 Tree-sitter 高亮仍同步运行，
   超过 64 项/正文键预算后有些正文每次新投影会重新高亮，不能把识别预算当总刷新预算。

不应以一个平均 `display()` 时间代表编辑器性能。建议固定 release 语料，分别记录：
按键到提交、刷新、正文布局、绘制、P50/P95/P99、峰值 RSS、分配量、缓存命中率及构建次数。
语料包含普通段落、很多小代码块、格式密集表格、长单段公式、引用/列表、
CRLF/CR、超长 grapheme，并在文档前中后重复操作。
本次没有提供 release GUI P95、峰值 RSS 或 Windows 实机兼容性结论。

## 本次验证

- `python3 scripts/check-architecture.py`：通过，9 个私有包的生产依赖边界通过。
- `cargo test --locked`：通过。
- `cargo fmt --check`、`git diff --check`：通过。
- 独立复现：`python3 docs/reviews/architecture-2026-10-27/followup/run.py`。
  该脚本读取 Cargo JSON 产物路径，在临时目录编译探针，不增加 Cargo target、不改生产代码。
- `cargo run --locked -- --smoke-test`：完整原生窗口冒烟通过，阶段 0–295，退出码 0；
  日志为 [smoke-output.txt](smoke-output.txt)。没有终止用户已打开的应用窗口。

探针覆盖 F1 的错误投影接受、F2 的热查询、F3 的验证器调用与 F4 的健康控件丢失。
F5–F7 为静态路径分析；现有通过的测试不能替代这些新增边界和规模场景的回归。
