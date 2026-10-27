# 当前代码复查：架构、逻辑、扩展性与性能

基线：`a5de8e7`，2026-10-28。开始时工作区干净；本轮只增加审查资料与独立探针，
不修改生产代码、依赖或已有审查结论。此前已修复项以当前实现为准。

## 结论

现有九包分层、源码唯一真相、候选事务、派生快照、旧帧检查与服务端口值得保留。
本轮确认九项 P2 改进项，没有确认新的 P0/P1。主要问题集中在统一发布边界、提供者
故障隔离，以及缓存命中之外仍然发生的工作。下表区分既有未关闭项与新增证据。

| 编号 | 优先级 | 问题 | 证据 | 与此前审查关系 |
| --- | --- | --- | --- | --- |
| R1 | P2 | Undo/Redo 绕过验证器和部分候选校验 | 当前会话探针 | A1 仍存在 |
| R2 | P2 | 一个控件提供者的身份回调失败，使整批控件失效 | 当前会话探针 | A2 仍存在 |
| R3 | P2 | 自动语言识别把超时缓存成稳定的无法识别结果 | 隔离副本、预算注入 | 新增 |
| R4 | P2 | 代码语言控件逐块从头查 AST，形成平方级扫描 | 原函数计数副本 + 调用链 | 新增 |
| R5 | P2 | 控件动作状态变化使正文完整排版缓存失效 | 当前 UI 调用链 | A5 仍存在 |
| R6 | P2 | 正文投影命中缓存后，预览候选仍扫描全部代码块 | 当前会话计数探针 | A6 仍存在 |
| R7 | P2 | 高亮工作集超过缓存后，未变代码反复同步解析 | 隔离副本、现有计数器 | 此前性能限制的量化 |
| R8 | P2 | 表格内上下导航仍扫描全文视觉行 | 当前 UI 调用链 | A7 的剩余分支 |
| R9 | P2 | 打开、导出、更新任务尚未统一服务异常的终态清理 | 当前后台调用链 | A4 扩展检查的剩余部分 |

已复核关闭：重复注册保留原提供者、异步保存 panic 转失败回执、普通行上下导航重复查找、
单段公式按折行反复过滤。旧报告 A3/A4/A8 与 A7 普通行分支不再作为未修复缺陷。
R9 不包含已经修好的异步保存。

## R1：让历史恢复进入明确的验证协议

位置：[session/lib.rs:452](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/lib.rs:452)，
对照 [普通发布校验:216](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/lib.rs:216)。

`undo()` 直接发布历史候选；普通提交中的 validators、closers 和显隐 Session 校验并未
全部复用。探针先编辑，再让验证器拒绝修改：普通输入被拒绝，Undo/Redo 仍改变正文，
两次历史操作的验证器调用增量为零。这会让将来的只读策略、文档约束扩展出现入口差异。
尚未证明内置历史会因此损坏正文。

建议抽取候选一致性检查与发布前验证；传入 Edit/Undo/Redo 来源，让验证器显式声明
是否约束历史恢复。恢复已通过验证的历史与动态策略之间需要明确语义，不能简单强加所有
输入规则。历史拒绝应返回可报告错误，并保留双栈、源码、选择与显隐状态。

## R2：在注册时保存稳定身份

位置：[render_actions.rs:217](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-extensions/src/render_actions.rs:217)。

提供者目录在局部保护之外调用 `id()`；依赖键、诊断和命令查找也会再次调用。
一个提供者在注册后发生身份回调异常时，外层保护返回整批错误，UI 清除健康提供者的控件。
探针验证故障前有一个健康控件，故障后整个批次失败；这属于隔离粒度问题，
并非已经观察到 panic 逃出原生事件循环。

建议注册记录持有稳定 ID、命令目录和实例 Arc；运行时以记录身份做分片缓存和局部捕获。
回归同时覆盖缓存、未缓存、故障诊断及执行路径，健康贡献者必须继续可用。

## R3：区分识别完成与预算中断

位置：[highlight.rs:269](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-services/src/highlight.rs:269)，
[language.rs:37](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-services/src/language.rs:37)。

`detect_before()` 对超时、解析中止、歧义与没有明确语言都返回 `None`。调用方将其写入
`detections`，且没有因本次超时设置 `deferred`。后续新投影即使有完整预算也命中负缓存。

隔离副本把当前轮剩余识别预算设为正的 1 ns，以确定性模拟前面的代码块刚好耗尽预算：
首次尝试无高亮、pending=false；下一轮尝试数为零、仍无高亮；新识别器对同一 Rust 正文
生成八个高亮片段。这证明预算中断被缓存，并不代表日常输入每次都会触发该条件。

建议内部返回 `Detected / Undetermined / Deferred` 等区分结果；只对完成的判定做负缓存。
对单块超过整轮预算的情况，采用可恢复阶段、有限重试或明确终态，避免无限的 16 ms 重排循环。
验证恢复、取消、内容变更、顺序变化和长期超时情况下的收敛。

## R4：消除代码语言控件的逐块 AST 全扫

位置：[code_actions.rs:72](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/code_actions.rs:72)，
[批量调用:105](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/code_actions.rs:105)。

`controls()` 遍历所有代码块，每块调用 `language_range()`，其中从 `parse.nodes` 开头
寻找对应代码节点。很多小围栏时，块数 B、节点数 N 的成本为 O(B×N)，该语料下是平方级。
控件缓存按源码身份失效，所以正文修改后仍会支付该成本；渲染动作执行前重新验证控件也会进入此路径。

原函数副本只插入谓词访问计数，得到：

| 围栏数 | AST 节点 | 节点谓词检查数 |
| ---: | ---: | ---: |
| 100 | 201 | 10,100 |
| 200 | 401 | 40,200 |
| 400 | 801 | 160,400 |
| 800 | 1,601 | 640,800 |

建议复用快照的代码节点目录，批量有序配对或按范围查询；更长期可由语法层提供
围栏信息字段范围，避免功能层重新查找节点。保留容器前缀、缩进代码排除、裸 CR、
围栏长度与尾随信息的既有语义。先用扫描 oracle 对照，再加入访问规模断言。

## R5：拆开正文度量与动作状态的失效

位置：[UI 接收批次:459](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/lib.rs:459)，
[ControlIndex 代次:79](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/text_layout.rs:79)，
[LayoutKey:118](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/text_layout.rs:118)。

聚合 Arc 改变就创建新的控件索引代次，代次又进入正文 LayoutKey。一个选区相关提供者
只改变 enabled/checked，或未知提供者每次返回同样的空列表，也会使下一帧重新完整排版。
贡献者分片缓存已存在，但没有隔离正文 shaping 的成本。

建议分别跟踪正文宽度/度量、锚点几何与菜单动作状态。只在实际影响字体、资源度量或
预留宽度时重新排版。不能直接删除 generation：可见索引还可能持有旧控件下标。
实施时需要真实窗口的布局构建计数，覆盖空提供者、仅 enabled 变化及同宽不同目标。
本轮为调用链确认，未测 GUI 延迟。

## R6：按声明依赖复用预览候选

位置：[UI refresh:528](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/lib.rs:528)，
[候选遍历:393](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-extensions/src/registry.rs:393)。

每次 refresh 在正文投影缓存查询之前调用 preview_inputs。没有图表的普通围栏文档
会遍历所有代码块和贡献目录；有图表时还会重新提取正文，然后才查询内容缓存。

当前会话探针中，100/1,000 个普通代码块的十次光标移动都命中正文投影 Arc，
但返回 None 的预览提供者分别被调用 1,000/10,000 次。以上是工作量，不是帧耗时。

建议注册时形成预览提供者目录并声明适用语言。把源码/配置相关的候选和正文提取，
与选区、光标、冻结、局部源码相关的可见性分开。按源码身份、实例和声明代次缓存前者；
未知扩展仍安全重算，不能忽略 PreviewProvider 目前能读取整个 EditingState 的事实。

## R7：给实际高亮工作设预算

位置：[highlight.rs:291](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-services/src/highlight.rs:291)，
[Tree-sitter 调用:375](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-services/src/highlight.rs:375)。

50 ms 预算只限制自动识别。显式语言高亮仍同步执行；64 项/2 MiB 正文缓存之外的块
每次新投影又被解析。独立副本启用源码已有 parses 计数器，不修改算法：

| 不同 Rust 代码块 | 冷解析数 | 同一正文第二轮解析数 |
| ---: | ---: | ---: |
| 64 | 64 | 0 |
| 80 | 80 | 16 |
| 128 | 128 | 64 |

这不会发生在成功复用整个正文投影的普通移动中；但正文编辑、显隐变化或其他投影失效
会重新进入高亮。很多自动代码块的多轮识别也可能多次重复高亮超出工作集的正文。

建议独立调度高亮计算，按正文/语言身份和当前文档代次发布，先显示普通代码；
预算同时覆盖实际高亮，优先可见块并保证其他块不会饥饿。避免简单无限增大缓存。
缓存字节统计也应考虑 spans 与键的开销；当前 2 MiB 主要统计正文，不等于总内存上限。

## R8：让表格导航复用几何目录

位置：[layout.rs:708](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/layout.rs:708)。

普通行分支已修复；单元格分支仍过滤完整 rows 查同格相邻行，离开单元格再扫描全文
寻找最近上下行。表格出现在长文档后部、按住上下键时，每次导航都有线性工作。

建议在 TextLayout 内维护单元格到视觉行范围、行带邻接关系。可复用 HitIndex 的
单元格分组信息，但需明确布局拥有权。表格多列共享 Y，不能假定整个 rows 按 top 严格
递增。回归涵盖折行、公式动态行高、不同高度单元格、跨表格边界和两个 affinity。

## R9：统一后台服务调用的失败回执

位置：[打开文件后台任务:171](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/files.rs:171)，
[导出后台任务:119](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/export.rs:119)，
[检查更新:203](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/menus.rs:203)。

这些调用直接进入可替换的 FileStore::read、PdfExporter::export、ArtifactStore::write、
UpdateChecker::check；操作标志在正常 await 返回后才清除。它们没有沿用 SaveRequest
现在已有的 unwind 转失败回执边界。替换实现一旦 panic，便没有保证能执行终态清理；
不同执行器可能传播或中止任务。普通 Result 错误已有清理，不能混为一谈。

建议在每类任务边界统一转换失败、携带操作身份并总是完成清理，再由 UI 应用当前操作
的回执。同步 save/open 的端口策略也应说明。该项由调用链确认，本轮没有在原生事件循环
注入故障，也没有发现当前内置文件服务的普通 I/O 错误会触发此问题。

## 扩展能力与后续路线

这些是能力演进建议，与上述缺陷分别管理：

1. **按贡献声明导出依赖。** 当前 [snapshot:78](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/lib.rs:78)
   对任何 additional 模块拒绝导出，纯命令模块也受影响。可区分影响呈现/资源的模块
   与只提供命令/UI 的模块，为前者提供冻结协议，后者明确不参与。保持拒绝不稳定依赖的原则。
2. **定义可执行的装饰组合。** Decoration 目前是描述，document 主提供者仍独占；
   features 内部的范围贡献尚未成为外部静态模块可调用的协议。需要先定义映射归属、
   冲突、顺序、缓存依赖和导出等价性，再按具体功能开放，不必先做动态插件 ABI。
3. **逐步收敛语义查询。** ParseSnapshot 的 Deref 仍暴露 nodes/tokens，R4 就是绕过
   已有索引的实际例子。优先补围栏信息范围、祖先与容器查询，并审计新增线性扫描。
4. **按目标验证操作。** [动作执行:343](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-extensions/src/render_actions.rs:343)
   为验证一个请求重建该提供者的全部 controls。可增加目标相关的能力查询，保留默认
   全量验证兼容未知提供者；不能直接信任旧 enabled 或跳过范围校验。
5. **为历史配置可观测的预算。** core 两条历史栈目前无条数/字节预算，大段替换的逆向
   正文会长期保留。先统计历史字节、在途快照与峰值 RSS，再确定裁剪策略；不应默默丢弃
   用户认为仍可撤销的操作。dirty 的全文比较已移出重绘，但同长度修改在发布时仍可能
   比较长公共前缀，需纳入提交阶段测量。

性能建议按以下顺序推进：

- 先修 R3/R4 这两项可独立回归的问题；R1/R2 明确扩展协议后分别修复，R9 统一任务终态。
- 再处理 R5/R6/R8，减少缓存命中和导航过程中的全文工作；R7 单独设计任务预算与资源生命周期。
- 建立真实窗口输入至显示的 p50/p95/p99、阶段耗时、排版构建次数、命中率、分配量、
  峰值 RSS 与在途快照数。当前计数证据不能换算成 GUI 延迟。
- 随后推进既有 P3 块级排版缓存、可见块精排与高度索引，再推进 P4 保守局部语义/投影、
  P5 局部 IME 投影。代码多块、单个长代码块、大表格、列表/引用、长单段公式、Unicode、
  LF/CRLF/CR 与文档首中尾位置分别验收。

内容修改和 IME 临时状态仍全文解析/投影，正文仍全文精排，这是已知保留限制。
本次没有证据支持优先更换解析器、引入 rope 或重写分层架构。

## 覆盖、验证与复现

检查覆盖 core 事务/历史/保存基线，syntax 快照/查询目录，projection 映射/校验，
extensions 注册/回调/缓存，features 命令/输入/呈现，session 发布/IME/任务/导出，
UI 刷新/几何/导航/文件状态，以及高亮、公式、Mermaid、原子写入和共享打印布局关键路径。
这是源码关键路径审查与针对性复现，不代表穷尽全部输入组合。

- 架构检查：通过，九个私有包的生产依赖边界符合脚本约束。
- `cargo test --locked`：全量通过。
- `cargo fmt --check`、`git diff --check`：通过。
- 当前 stable 的 `/usr/local/opt/rust/bin/cargo-clippy clippy --locked --all-targets -- -D warnings`：通过。
  默认 `cargo clippy` 误用了旧 1.81 驱动，首次因 MSRV 失败；改用现有 1.99.1 配套驱动后通过，
  没有修改全局工具链配置或依赖。仍有既有第三方依赖的未来兼容性提示。
- 原边界探针重新执行：通过，见 [boundary-output.txt](boundary-output.txt)。同时验证已修复注册/保存行为。
- 新探针：通过，见 [highlight-output.txt](highlight-output.txt)，涵盖 R3/R4/R7。
- 未新运行原生窗口冒烟、Windows 实机、真实 IME 人工检查或 release GUI 延迟/RSS 测量。
  R5/R8 的实际收益应在实施时补充窗口计数和端到端基准。

复现命令：

```sh
python3 docs/reviews/architecture-2026-10-28/run.py
python3 docs/reviews/architecture-2026-10-28/followup/run.py
```

新 runner 在临时目录中编译源码副本：高亮部分仅启用原有 parses 计数器，预算用探针注入；
代码语言部分提取当前 language_range 原函数，只增加节点访问计数。生产源文件不修改，
没有新增 Cargo target，没有对用户文档执行写入。算法或函数签名变更后，应复查探针提取逻辑。
