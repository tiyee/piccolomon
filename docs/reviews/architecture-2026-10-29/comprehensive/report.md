# 当前代码的架构、逻辑、扩展性与性能审查

日期：2026-10-29；代码基线：`ae1a2e0`。开始时工作区干净。
本次仅新增审查报告、可重复运行的隔离探针和验证日志，不修改生产代码或依赖。

## 主要发现

确认 5 项 P2：3 项新增证据（C1–C3），2 项既有未关闭问题（R5、R7）。
本轮没有确认新的 P0/P1。现有九包依赖方向、源码唯一所有权、候选事务、快照索引、
派生投影与原子发布应继续保留；当前证据支持局部修复和渐进优化。

| 项目 | 触发条件与影响 | 证据类型 |
| --- | --- | --- |
| C1 · P2 | 裸 CR 文档的围栏后标题已结构化呈现，点击却不能展开标记 | 真实会话调用，H1/H6/Setext，LF/CRLF 对照 |
| C2 · P2 | 每个公式重复筛选全局表格替换事件，形成 O(F×S) 工作 | 当前源码临时副本，仅添加谓词计数 |
| C3 · P2 | 保留的自动结束标记被复制到每步历史，特定连续输入形成平方级保留内存 | 真实输入、导航、撤销/重做及 history_memory 查询 |
| R5 · P2 | 控件批次身份变化会使正文布局失效，即使正文和预留空间未变 | 当前会话身份探针 + UI 布局键调用链 |
| R7 · P2 | 实际高亮同步运行，超出缓存的未变代码在新投影中反复解析 | 当前高亮器副本中的已有解析计数器 |

### C1：标题交互与呈现使用了不同语义视图

位置：[commands/headings.rs:94](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/commands/headings.rs:94)。

最小输入（用转义表示真实换行字节）：

```text
```\rcode\r```\r\r# title\r\rtail
```

解析适配层已经为裸 CR 提供同字节宽度的 `semantic()` 视图，主投影使用该视图，
能够呈现围栏后面的标题。但 `reveal_clicked_heading()` 直接读取原始 `parse.index().headings`
和 `parse.nodes`，没有使用同一个语义视图。该输入下原解析结果缺少后续标题，因此点击
找不到节点。`heading_caret()` 的标题查询也直接使用传入快照，需要一并检查。

会话探针在正文 `title` 上选择并调用真实点击显隐命令：H1、H6、Setext 三种标题在 LF、
CRLF 时均得到 reveal；裸 CR 三种均没有 reveal。投影中始终有 heading 样式，源码身份、
dirty 和历史均未改变。因此这是结构呈现与交互不一致，不是保存格式变化；全文/块源码入口
仍可用于编辑原始标记。

建议将标题、祖先及容器查询收敛到语法层的统一语义接口；查询使用 normalized semantic，
创建显隐 Session 时仍绑定原始 Source，不能把临时语义 Source 的身份传给文档。
回归覆盖围栏后 H1–H6/Setext、引用/列表嵌套、混合换行、点击后编辑标记、离开收起和撤销。
该问题不要求修改或发布外部解析包。

### C2：公式与表格事件存在交叉扫描

位置：[presentation.rs:384](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/presentation.rs:384)，
事件来源：[presentation/table.rs:44](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/presentation/table.rs:44)。

`for formula in &formulas` 内调用 `insertions.retain(...)`，每个公式遍历整份文档的表格
替换事件。列分隔 `\t` 永远保留，所以不会随着循环缩短。F 个公式、S 个替换事件产生
O(F×S) 次谓词检查；公式不必在对应表格内。公式渲染失败或返回源码也会支付该成本。

三列表格，每个正文行一个 `$x$`，只计这条 retain 的调用次数：

| 正文行 / 公式数 | 检查次数 |
| ---: | ---: |
| 100 | 20,200 |
| 200 | 80,400 |
| 400 | 320,800 |
| 800 | 1,281,600 |

探针使用返回 None 的公式服务，排除了公式排版引擎成本；生产算法副本仅插入计数器，
投影仍通过正式校验。这是确定性工作量，不是 release 延迟或卡顿时长。

建议保持公式范围和替换事件有序，在公式遍历之外一次过滤；只过滤落在公式内的非列分隔
事件。可用双指针或范围目录，避免逐公式 retain。回归需覆盖公式内 `\\|` / `<br>`、
单元格代码字面内容、空格与空单元格、公式失败回退以及公式在表格外的混合文档。

### C3：补全状态导致历史内存放大

位置：[session/lib.rs:27](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/lib.rs:27)，
[commands.rs:163](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/commands.rs:163)，
[commands/inline.rs:29](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/commands/inline.rs:29)。

反复输入 `*`、`a`，自动得到 `*a*`，再用导航离开结束标记、输入空格。未经键入越过的
结束标记持续保存在 `closers` 中；之后每次编辑的 `Checkpoint::from` 都完整克隆它。
当标记数和编辑次数一同增长，历史里保存的范围数为 O(K²)。前台计划也反复克隆和映射
当前的完整 closers，但本次未把该时间成本量化。

| 格式片段数 | 最终源码字节 | 撤销条数 | 历史拥有分配字节 |
| ---: | ---: | ---: | ---: |
| 100 | 400 | 300 | 328,160 |
| 200 | 800 | 600 | 1,136,320 |
| 400 | 1,600 | 1,200 | 4,192,640 |

对照：键入自动结束的 `*` 来越过标记，最终源码同为 1,600 字节、同为 1,200 条历史，
拥有分配为 355,840 字节。两组均验证了一次 Undo/Redo 后源码一致。
上述数值是现有 `history_memory()` 的限定账目，包含历史容器、逆向编辑和 checkpoint，
不含共享源码/解析快照、布局、资源和分配器元数据，不能当作进程 RSS。

这比“历史没有总预算”更具体：许多很小的输入也会放大，不需要粘贴大段内容。
建议为补全状态设计差量历史或可共享的持久结构，并明确已离开标记的生命周期。
单纯把 Vec 改为 Arc 后每次整体复制，仍会保留 K 份越来越长的状态；单纯限制历史条数
会改变用户可撤销范围。保留键入越过、回到旧位置编辑、跨范围修改及 Undo/Redo 的语义，
再结合已有内存账目决定裁剪或合并策略。

### R5：动作状态仍与正文布局失效耦合

位置：[UI 接收控件:454](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/lib.rs:454)，
[ControlIndex:79](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/text_layout.rs:79)，
[LayoutKey:120](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/text_layout.rs:120)。

复跑当前会话探针：未声明缓存依赖的空控件扩展在十次选择变化中产生十个新批次 Arc，
正文投影 Arc 始终复用。UI 每次收到新批次就生成新控件代次，而该代次进入正文 LayoutKey，
下一帧需要完整布局。动态提供者仅改变动作 enabled/checked 时也沿用这条失效路径。
未知提供者重新生成控件本身是安全的默认行为，问题是其成本传播到未变化的正文排版。

建议拆分正文度量/预留空间、目标几何、动作状态的版本；复用正文布局时仍重建受影响的
命中关系，并在执行时重新校验 enabled。不能只删 control_generation，因为可见目录与
控件目标也依赖该代次。实施验收需要真实窗口的布局构建计数：空扩展、仅 enabled 改变、
同宽不同 target、预留宽度改变四种情况。本轮未测这些扩展场景的 GUI 延迟。

### R7：实际高亮仍没有总预算

位置：[highlight.rs:272](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-services/src/highlight.rs:272)，
[Tree-sitter 调用:421](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-services/src/highlight.rs:421)。

50 ms / 每轮八项限制只约束自动语言识别；显式语言缓存未命中时同步调用高亮器并消费全部
事件。当前 64 项缓存满后，超出部分每次新投影仍重复解析。

| 不同 Rust 块数 | 首轮解析 | 同正文第二轮解析 |
| ---: | ---: | ---: |
| 64 | 64 | 0 |
| 80 | 80 | 16 |
| 128 | 128 | 64 |

第二轮 `pending=false`。成功命中整个正文投影的普通移动不会触发高亮；编辑、显隐变化或
其他投影失效后才重新进入。R3 的识别预算中断重试已修复，不能将其与本项混淆。

建议把实际解析和事件消费纳入有界任务，允许先显示普通代码，按正文/语言/配置/任务身份
发布结果。优先可见块并保证后台任务收敛，避免无限增大缓存。缓存内存还应计入 spans、
字符串容量和容器开销；当前正文 2 MiB 上限不是高亮器总内存上限。

## 扩展能力与架构建议

以下是已知能力边界或后续设计，不额外计为新增缺陷：

1. **按实际贡献冻结导出。** [MarkdownFeatures::snapshot](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/lib.rs:78)
   对任何 additional 模块整体拒绝；纯命令、菜单或空控件模块也会阻止 PDF。
   为贡献区分“不影响输出”“可冻结输出”“不可冻结输出”，组合冻结后的呈现和资源依赖，
   继续拒绝无法稳定冻结的输出依赖。
2. **把装饰与菜单的扩展协议做成可执行接口。** 当前 `Slot::Decoration` 只有注册描述；
   `BlockContribution` 是 features 内部类型，document 主呈现与 block-menu 独占，
   `BlockCommand` / `InsertBlock` 为封闭枚举。优先为具体需求开放范围装饰和命令 ID 菜单，
   定义顺序、冲突、映射归属、缓存依赖与导出行为；不必先做动态插件 ABI 或任意 Widget。
3. **统一语义查询和借用生命周期。** C1 证明直接暴露原始 AST 不只增加性能风险，也会
   使交互与呈现理解同一源码的方式不同。优先提供标题、容器、条目和目标查询；热点逐步
   从 `list::items` / `table::tables` 的整批克隆转为借用或单目标值，保留扫描 oracle。
4. **收敛可替换服务的 UI 故障边界。** 后台打开/保存/导出/更新已有失败回执。公式服务的
   UI `set_font` 和 `Assets::load → render_asset` 仍直接调用替换服务；在引入新的公式服务
   前补这两个入口的故障注入与回退测试。本轮没有证明内置服务会在这些入口 panic。
5. **扩大诊断覆盖，但保持生产默认关闭。** 现有布局计数、历史/控件拥有内存是有用基础，
   还缺输入至显示的 p50/p95/p99、阶段耗时、在途快照、分片/聚合/UI 控件副本和峰值 RSS。
   控件动作目录可共享静态描述，减少每格及每层索引克隆整份动作数组；先测分配再修改协议。

大文档仍有明确边界：内容修改和 IME 临时状态全文解析/投影，新正文布局仍全文精排。
当前视口绘制、导航索引和光标缓存不等于端到端增量编辑。先处理 C2、C3、R5、R7 和测量，
再推进 P3 块级布局/可见块精排/高度索引，随后处理 P4 局部语义与 P5 局部 IME。
没有证据支持为了这些问题先更换解析器、引入 rope 或重写九包架构。

## 已修复项复核与实施顺序

- N2：实际符号链接打开、修改、异步保存回执通过；链接保留，目标更新，dirty 清除。
- R1：当前普通发布与历史共享 validate_plan；默认历史验证、豁免、拒绝原子性、失败诊断
  由现有全量测试覆盖，本次不再列为未修复。
- R6：声明源码依赖的预览提供者，1,000 围栏冷轮 1,000 次查询，十次移动新增 0 次。
- R3/R4：预算中断后完整预算重试成功；100/200/400/800 围栏分别只访问同数量语言候选。
- R2/R8/R9、单目标动作验证：复核实现及现有回归。旧探针日志的 `table_action_validation`
  仅打印完整批次大小，不能据此声称执行一个动作仍重建完整批次。
- 当前共享 PDF 布局已使用 `code_row_index()`；AGENTS.md 中“PDF 仍沿用原代码行查询”
  的旧描述与本基线不一致，本次按代码判定，不重复报告已消除的扫描。

建议先修 C1（统一标题语义视图）和 C2（一次过滤事件），两者范围较小且可以定向回归。
然后分别设计 C3 的补全历史表示、R5 的布局失效协议；R7 单独推进高亮任务预算和生命周期。
导出冻结、装饰组合随具体扩展需求落地，避免一次性大规模接口重写。

## 验证与边界

检查覆盖九包的主要边界和输入/事务/历史、语义索引、投影校验、扩展注册与缓存、
布局/命中、IME、保存、后台服务、公式/图表/高亮及导出关键路径。它不是逐行穷尽证明。

- 架构检查通过：9 个私有 workspace 包，生产依赖方向符合当前约束。
- `cargo test --locked`：329 项通过，0 失败。
- `cargo fmt --check`、配套 stable Clippy 全目标 `-D warnings`、diff 检查通过。
- 新隔离探针和既有边界/高亮探针通过，日志保存于本目录。
- 原生窗口冒烟结果见 `checks.json` / `smoke-output.txt`；不等同人工鼠标、输入法候选窗口
  或 Windows 实机验收。本次未测 release GUI 延迟或峰值 RSS。

新探针运行：`python3 docs/reviews/architecture-2026-10-29/comprehensive/run.py`。
它构建当前锁定依赖，在系统临时目录复制 features 源码，仅为 C2 添加谓词计数；
C1/C3 通过真实会话接口执行。所有断言都在无窗口探针内，不向原生事件循环注入 panic。
探针对当前已确认现象作断言，修复对应问题后应更新期望，不能把这些断言当作产品正确性标准。
原有 Cargo 配置弃用和第三方未来兼容性提示仍在，未修改全局配置或依赖。
