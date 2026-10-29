# 当前代码的架构、逻辑、扩展性与性能审查

日期：2026-10-29。基线：`6b12d12`，开始时工作区干净。
本轮仅增加本目录中的审查资料与隔离探针，未修改生产代码、依赖和既有报告。

## 结论

现有九包分层可以继续演进。源码唯一所有权、候选事务、快照拥有的语义索引、平台无关
投影、不可变布局和具体服务端口的方向合理。当前没有证据支持先更换解析器、引入 rope
或重写 workspace。

复核确认 **4 项既有 P2 仍未解决：N2、R1、R5、R7**。本轮没有确认新的 P0/P1，
也不将已修复的列表交叉扫描和声明依赖后的预览候选重复工作重新计为缺陷。
另有目标级动作验证、导出冻结协议、装饰组合及资源计量等明确改进空间。

本轮审查覆盖九包主要边界及编辑、IME、投影、缓存、排版/导航、后台任务、保存/导出
关键路径；不是对每个分支和每种输入组合的穷举证明。

## 已确认且应安排修复的问题

| 项目 | 当前触发条件和影响 | 当前证据 |
| --- | --- | --- |
| N2 · P2 · 保存符号链接破坏链接关系 | 通过链接打开 Markdown 后保存，链接变为普通文件，实际目标仍是旧内容；会话认为保存成功且 clean | 临时目录中执行真实 `open → replace → queue_save → complete_save` |
| R1 · P2 · 历史恢复绕过验证协议 | 普通编辑受验证器限制，Undo/Redo 不调用同一验证器，也未完整复用候选一致性检查 | 当前边界探针：两次历史操作的验证器调用增量为 0 |
| R5 · P2 · 控件动作状态与正文排版失效耦合 | 未声明缓存的空扩展返回相同空列表，也会换聚合 Arc、控件代次和正文 LayoutKey | 十次移动产生十个新控件批次身份，正文投影身份保持不变；最终排版失效由当前调用链确认 |
| R7 · P2 · 实际高亮缺少总预算 | 显式语言同步高亮，工作集超出 64 项后，投影重建重复解析未变正文 | 64/80/128 个不同 Rust 块第二轮分别解析 0/16/64 块 |

### N2：让打开与保存绑定同一个文件目标

[files.rs:14](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-services/src/files.rs:14)
读取跟随链接，保存却在传入路径的目录建临时文件，并直接 rename 到该路径。
本轮复现 `link.md → target.md`：保存成功、dirty 清除，`link.md` 含新内容但不再是链接，
`target.md` 保持原内容。编辑文本仍可从 link.md 读到，不能描述成文本已经丢失。

建议在文件服务明确区分显示路径和实际保存目标；普通保存绑定实际目标，在其目录内
完成临时写入和原子替换。明确打开后链接被改指、目标被替换、悬空链接及另存为的行为。
不能只改成原地覆盖而失去原子保存。验收覆盖相对/绝对链接、跨目录目标、目标变化及
保存失败后的 dirty/重试。本轮仅复现 Unix 链接，不外推 Windows junction 和硬链接。

### R1：区分历史一致性检查和动态策略验证

[undo:451](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/lib.rs:451)
重建候选状态后直接发布历史；普通发布的验证器、closers 和显隐 Session 检查没有完整复用。
当前 `undo → ()` 还会静默返回失败，调用者难以区分没有历史、策略拒绝和提供者故障。

建议提取所有发布入口共享的结构一致性检查；策略验证显式接收 Edit/Undo/Redo 来源，
由验证器声明是否限制历史。历史恢复不重跑输入补全。返回可报告的结果；拒绝时保持
源码、双栈、选区、补全和显隐状态不变。此项影响只读和动态约束扩展，当前没有证明
内置历史会损坏正常文档。先决定协议，再改路径，避免把产品策略藏进重构。

### R5：按实际影响范围拆分控件代次

链路为 [聚合缓存](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-extensions/src/render_actions.rs:131)
→ [接收批次](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/lib.rs:454)
→ [ControlIndex / LayoutKey](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/text_layout.rs:79)
→ [布局复用判断](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/layout_frame.rs:19)。
提供者分片缓存已有，但动态分片的新身份仍会使整个正文重新排版。

建议独立跟踪影响正文的预留宽度/度量、控件目标和几何、动作 enabled/checked 状态。
只有度量变化进入正文布局键。目标改变仍必须刷新命中索引；执行仍重新验证当前 enabled，
不能直接删掉 generation。优先用空提供者、仅 enabled 改变、同宽不同 target 三个真实
窗口场景验收布局构建次数和命中正确性。本轮没有为扩展场景新增 GUI 计数，不能给出提速比例。

### R7：把真实高亮工作纳入调度

[highlight.rs:272](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-services/src/highlight.rs:272)
在缓存未命中时立即执行 Tree-sitter；50 ms / 八项限制用于自动语言识别，不能限制这部分。
128 块语料的第二轮重新解析 64 块，`has_pending_work=false`。成功复用整个正文投影的
光标移动不会进入该轮；正文编辑、显隐或其他投影失效后会再次支付成本。

建议先允许普通代码呈现，把高亮变为有总预算的任务；结果绑定正文、语言、配置和任务
身份，过期结果不得覆盖当前内容。优先可见块并保证非可见任务收敛；预算涵盖解析和
事件消费，缓存统计包含 spans 与键。仅增大 64 项上限不能解决同步总工作量问题。

## 扩展性与性能的下一批改进

1. **单目标动作验证。**
   [RenderActionRegistry::plan:366](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-extensions/src/render_actions.rs:366)
   为验证一次动作，调用目标提供者的全量 controls。内置
   [TableActions:61](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/table_actions.rs:61)
   会重建所有表格单元格和动作。探针语料为 800 正文行、3 列，加表头共 2,403 控件；
   当前每格 13 个动作，即一次完整构建产生 31,239 个动作条目，尚未计缓存/UI 索引克隆。
   建议增加 target 级能力查询，未知提供者保留全量验证作为默认实现，先迁移表格和列表。
   静态动作描述可共享，逐目标只保存动态状态。此处是对象数量和调用链证据，未测分配字节或延迟。

2. **按贡献冻结导出。**
   [MarkdownFeatures::snapshot:78](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/lib.rs:78)
   对任意 additional 模块拒绝快照；纯命令或空控件扩展也会阻止 PDF 导出。
   建议声明“不影响导出”“可冻结呈现/资源”“无法冻结”，按实际输出依赖组合快照。
   无法冻结的输出依赖继续明确拒绝。已有测试和本轮重跑的旧探针确认当前限制。

3. **提供可执行的装饰和菜单组合协议。**
   `Slot::Decoration` 只有注册描述；`BlockContribution` 仍在 features 内部，主文档呈现
   和 block-menu 独占，BlockCommand/InsertBlock 为封闭枚举。可优先支持范围装饰和
   命令 ID 菜单贡献，明确顺序、冲突、源码映射归属、缓存依赖及导出一致性。当前适合
   静态功能模块，尚不具备任意新块/Widget 的通用扩展能力；没有必要先建设动态插件 ABI。

4. **继续收敛 SemanticQuery 和只读借用。**
   快照拥有索引已经有效，但 ParseSnapshot 的 Deref 仍暴露原始节点，部分便利接口
   `table::tables`、`list::items` 返回完整克隆。逐步为热点补齐按目标和容器查询，优先
   使用借用切片；保留扫描 oracle 与工作量断言。不要仅以“已有索引”判断复杂度已经改善。

5. **补全内存账目和扩展故障边界。**
   [core 历史](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-core/src/lib.rs:119)
   没有条数/字节预算，逆向编辑和 checkpoint 持续保留。先统计历史字节、控件动作、
   快照和资源的活跃/保留内存，再设计裁剪规则。公式端口的 UI `set_font` 和
   [Assets::load](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/assets.rs:67)
   仍是直接服务调用，替换服务前应补专门的故障注入测试；本轮未证明内置服务在此崩溃。

6. **公式密集场景还有小型重复扫描，先测再排期。**
   [左右导航](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/lib.rs:674)
   对全部公式线性找精确边界；普通正文位置也可能扫到末尾。
   [每帧滚动锚点判断](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/layout_frame.rs:109)
   比较完整公式 Vec。可考虑复用不可变投影的边界目录/身份快速路径；顺序或资源改变时
   必须失效。这些是静态发现的优化候选，未量化影响，不列为新增 P2。

## 已修复项的复核

- N1 列表范围：800 条目 + 800 围栏，当前代码范围检查 17,600 次；旧记录为 1,280,000 次。
  计数插入当前函数副本，原生产算法不变；数量改善不等于端到端耗时同比改善。
- R6 候选缓存：声明源码依赖的提供者冷轮 1,000 次，随后十次移动新增 0 次。旧边界探针
  的未知提供者仍有 10,000 次调用，这是未声明依赖的兼容行为，不能当作 R6 修复失效。
- R2 身份冻结：运行期身份故障不再移除健康控件。
- R3 语言识别：注入剩余预算中断后，下一轮完整预算可恢复识别。
- R4 语言范围：100/200/400/800 围栏分别访问同数量代码候选。
- R8 表格导航、R9 后台失败回执及其他边界由本轮已有回归测试覆盖；未重新开展 Windows 实机故障注入。

## 实施顺序与验收

先分别处理 N2 文件目标身份和 R1 历史验证策略。随后拆分 R5 失效范围并增加目标级动作
验证；R7 单独设计任务预算和生命周期。按贡献冻结导出、装饰/菜单组合可随具体扩展需求推进。

当前内容编辑仍全文解析/投影；[IME 临时投影](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/composition.rs:232)
也重新解析，正文新布局仍全文精排。完成上述重复工作修复后，再推进 P3 块级布局复用、
可见块精排和高度索引，随后做 P4 保守局部语义与 P5 局部 IME。已有视口绘制和光标缓存
不能代表这些阶段已经完成。

性能验收使用独立 release 语料，覆盖普通段落、多小围栏、长代码、大表格、嵌套容器、
密集公式；分别测首中尾编辑、导航、滚动、IME 和 LF/CRLF/CR。记录输入至显示的
p50/p95/p99，parse/project/layout/paint 阶段、布局构建次数、命中率、分配和峰值 RSS。
当前计数和 debug 冒烟不用于宣称毫秒收益、最大文件大小或生产延迟保证。

## 验证与复现

架构检查、`cargo fmt --check`、`cargo test --locked`（319 项）、配套 stable Clippy
全目标检查以及 `git diff --check` 均通过。macOS 原生窗口 `--smoke-test` 通过至阶段 374；
它覆盖真实窗口几何与编辑回调，不等同 Windows 实机或人工输入法候选验收。
未测 release 大文档延迟与峰值 RSS。保留已有 Cargo 配置弃用和第三方未来兼容性提示，
未改全局配置或依赖。

最终检查结果和完整输出见本目录的 `checks.json` 及对应日志。
新探针通过 `python3 docs/reviews/architecture-2026-10-29/followup/run.py` 执行，使用当前
库编译临时调用器；符号链接测试文件仅建在系统临时目录并自动清理。
旧探针保持原文件，分别重跑 2026-10-28 边界/高亮探针与 2026-10-29 列表探针。
所有断言位于无窗口测试程序；没有向原生事件循环注入 panic。
