# 架构、逻辑、扩展性与性能审查

审查日期：2026-10-27。对象是当前工作区，包括开始审查时已经存在的未提交修改。
本次没有修改生产代码、依赖、既有测试或用户原有文件；本目录是审查记录与独立复现工具。

后续状态：R1、R2、R7、R8 已在第一批工作中修复并验证，方案和结果见
[第一批修复记录](/Users/jiangtao/code/github.com/piccolomon/docs/33_First_Batch_Repair.md)。
第二批已修复 R3、R4、R5、R6，并提前完成 R9、R10，见
[第二批修复记录](/Users/jiangtao/code/github.com/piccolomon/docs/34_Second_Batch_Repair.md)。
下文及原始输出保留审查时的事实，不能当作修复后的当前结果；贡献者缓存声明已在[第三批修复记录](/Users/jiangtao/code/github.com/piccolomon/docs/35_Third_Batch_Repair.md)实施；P3–P5 仍留待后续处理。

## 判断

九包的分层方向合理，源码唯一真相、候选事务统一发布、不可变快照、旧帧保护和服务端口值得保留。
架构检查通过，但它只证明 Cargo 依赖方向，不能证明缓存依赖完整、交互热路径已经使用索引，或所有扩展回调都有故障边界。

当前最值得先做的工作是补齐缓存正确性和交互热路径。已有的 P1/P2 索引在一些真实 UI 入口没有被使用；
另外仍存在组合语法的平方扫描。直接开始大规模增量解析重构，会把这些成本和缓存缺陷一起带进新架构。

下面区分三种证据：**复现**是调用当前生产接口得到的结果；**测量**是本机小样本诊断；
**静态路径**是从实际调用链推导的复杂度，未声称测得对应 GUI 延迟。

## 需要修复的问题

### R1：局部源码范围没有进入控件缓存键

位置：[presentation_cache.rs:100](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/presentation_cache.rs:100)、
[render_actions.rs:183](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-extensions/src/render_actions.rs:183)。

**复现，优先修复。** 控件缓存只比较源码身份和全文模式，但 `provider_controls` 会按 `block_source` 过滤控件。
内置 Markdown 又声明这些控件可以缓存，因此切换局部源码不会使该缓存失效。

对 `- [ ] one\n- two\n\ntail`：先缓存再进入列表局部源码，缓存仍返回 3 个控件，重新生成应为 0；
先在局部源码生成缓存再退出，缓存仍返回 0，重新生成应为 3。两个方向都复用了同一个 Arc。
UI 仍会进行执行时校验，因此这不是已经证明的非法内容修改；问题是错误的控件集合、控件恢复和布局预留状态。

短期把局部源码范围加入缓存键。长期缓存未经过视图过滤的结构控件，再按本帧局部源码范围派生有效集合，
避免将“源码决定的控件”和“当前视图允许显示的控件”混用一个缓存契约。
回归应覆盖双向切换、嵌套局部范围及列表/任务标记/表格/代码语言入口，并比较缓存与完整重建结果。

### R2：自动语言识别在缓存容量之外无法收敛

位置：[highlight.rs:196](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-services/src/highlight.rs:196)、
[highlight.rs:210](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-services/src/highlight.rs:210)、
[presentation_cache.rs:67](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/presentation_cache.rs:67)。

**复现，优先修复。** 识别结果最多 64 项，每轮从文档开头访问，最多识别 8 个未缓存正文，淘汰时移除最早插入的项。
80 个不同的 `fn main_N() {}` 自动语言代码块，连续运行 16 轮，着色数量先达到 72，随后回落并保持在 64，
`presentation_generation()` 始终为 `None`。先前完成的正文被后来正文淘汰，下一轮又从头争用预算。

结果不仅是部分代码无高亮：未完成状态会持续禁止整个文档投影缓存。在真实 UI 中，普通重绘又不会自动调用 `refresh`，
所以单纯等待也不能保证剩余识别继续执行。

应将“当前文档的识别完成状态”与“昂贵高亮结果的保留缓存”拆开，使用有进度的去重任务队列与源码/内容身份，
为超出资源预算的块提供稳定的普通样式终态。用每轮总时间预算约束前台工作，并明确调度后续任务；
不能通过让 `None` 永远保持来表达资源不足。回归至少覆盖 8、64、80、128 个不同正文，以及删除、移动和撤销。

### R3：代码块悬停仍有 O(代码块数 × 排版行数) 扫描

位置：[lib.rs:1199](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/lib.rs:1199)、
[layout.rs:478](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/layout.rs:478)。

**静态路径，优先优化。** `update_block_hover` 对每个代码块重新过滤整个 `geometry.lines`，再寻找首末行；
在代码块之后的普通段落悬停时，通常会遍历所有代码块。代码块数量随行数增长时，这条鼠标移动路径呈平方增长。
随后 `hit_caret` 还会扫描全部行查找单元格、扫描全部视觉行查找命中行；点击和拖选也使用它。

现有 `FrameIndex.code_lines`、`Visibility.codes/lines/cells/rows` 已经拥有所需信息，但这些入口没有接入。
先按 Y 查候选代码面板、单元格和视觉行，再在少量候选中保持现有优先级、间隙和等距选择规则。
不要用对表格各列混排的 `rows.top` 直接二分替代区间查询，因为它并非全局单调。
验证应在文档前、中、末尾悬停与拖选，记录候选访问数量，覆盖很多小代码块、表格重叠行带及长面板。

### R4：左右方向键仍从文档开头做 grapheme 分段

位置：[lib.rs:646](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/lib.rs:646)、
[indexed.rs:61](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-projection/src/indexed.rs:61)。

**静态路径。** `horizontal` 的前后移动均从 `display.text.grapheme_indices(true)` 开始扫描。
光标位于末尾时，每次方向键或 Shift+方向键仍是 O(全文字节数)，即使投影和正文排版全部命中缓存。
发布投影已经构建 grapheme 位图，但目前没有向该调用者暴露相邻边界方法。

为 `IndexedDisplay` 提供严格前一个/后一个 grapheme 查询，并让左右移动与删除共享底层边界接口。
继续保持公式、图表的原子进入规则和隐藏标记两侧的 affinity；不要改成逐 Unicode scalar 移动。
用跨 span 结合符、ZWJ、区域指示符、超长 grapheme 和 EOF 对照原完整分段算法。

### R5：普通 Backspace 在刷新前就重复生成两份完整投影

位置：[list.rs:137](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/commands/list.rs:137)、
[paragraph.rs:103](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/commands/paragraph.rs:103)、
[commands.rs:36](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/commands.rs:36)。

**复现，优先优化。** 删除按表格、代码、列表、段落处理器委托。列表处理器即使没有任何列表行，也先执行 `self.display()`；
委托给普通删除后，`delete_grapheme` 再执行一次 `self.display()`。随后 UI 刷新还会对修改后的源码生成新投影。
这些规划阶段的投影不经过会话的发布投影缓存。

独立探针使用一处 `$x$` 和计数公式服务，在公式之外执行一次 Backspace；未调用 GUI 刷新，公式服务已经被调用 2 次，
确认了两次完整呈现调用。代码边界检查还会遍历代码行，段落合并会收集全文段落范围，增加普通删除成本。

先在各结构处理器中通过语义索引判断当前位置是否相关，再生成局部需要的映射。
中期给一次命令规划提供可复用的只读映射上下文，由同一源码与显隐身份校验；允许无 UI 的会话自己取得该映射。
不能直接信任 UI 旧帧，也不应让功能层读取可变 Editor。
回归应统计普通字符删除的投影次数，并覆盖隐藏前缀、空单元格、代码边界和跨块选择删除。

### R6：包含行内代码的多表格文档仍有平方扫描

位置：[presentation.rs:48](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/presentation.rs:48)、
[presentation.rs:60](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/presentation.rs:60)。

**静态路径和测量。** `editable_inline` 为表格内每个行内节点扫描所有表格；`inline_range` 为每个行内代码节点扫描所有表格、行和单元格。
普通段落中的行内代码也会进入这个单元格查找。全量投影对所有节点调用它们，故组合语法仍有 O(行内节点数 × 单元格数) 成本。

本机 Rust 1.99.1、debug、无窗口、解析及索引预热后，每种规模 3 次完整 `display()` 的 P50：

| 独立表格数 | 普通单元格 ms | 含两个行内代码单元格 ms |
| --- | ---: | ---: |
| 256 | 17.385 | 48.907 |
| 512 | 34.990 | 136.116 |
| 1024 | 74.871 | 422.468 |
| 2048 | 153.773 | 1477.456 |

最后一项源码仅 79,872 字节。行内代码组连续翻倍约为 2.78、3.10、3.50 倍，普通组接近线性。
这是 debug 诊断，不是 release GUI 延迟保证；结合代码可确认存在交叉扫描，不能将全部差值归于单一函数。

在 ParseSnapshot 建立有效表格身份集合、单元格源码区间目录或节点到所在单元格的映射；
对不在 TableCell 祖先中的代码节点直接返回原 envelope。投影和 `active_window` 共用该查询。
性能语料应加入“很多独立表格 + 行内代码/强调”，而不只测简单单元格。

### R7：扩展的预览输入回调能让 panic 越过会话边界

位置：[registry.rs:373](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-extensions/src/registry.rs:373)、
[lib.rs:360](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/lib.rs:360)。

**故障注入复现。** 已注册的 `PreviewProvider::input` 没有 catch 边界，`EditorSession::diagram_views` 也直接调用 `preview_inputs`。
注册一个在 `input` 中 panic 的静态功能，调用 `diagram_views`，panic 会到达外层探针。
实际 `Editor::refresh` 在原生输入回调中经过这条路径，所以当前隔离约定只覆盖了一部分扩展回调。
本次未向原生事件循环实际注入该 panic，也没有证明正常内置 Mermaid 输入会触发它。

按提供者捕获预览输入故障，保留代码和其他提供者结果，记录有界诊断。
同时梳理 `block_ui`、`block_window`、`snapshot`、渲染器 `generation` 等运行时回调，让所有执行型扩展使用一致的宿主调用边界。
默认故障边界仍是进程内防错，不构成动态插件沙箱。

### R8：观察者错误绕过了有界诊断通路

位置：[lib.rs:132](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/lib.rs:132)、
[lib.rs:286](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-session/src/lib.rs:286)。

**复现。** `record_diagnostics` 去重且限制 64 条，但 `notify` 捕获观察者 panic 后直接向公开的 Vec 追加。
一个持续失败的观察者配合 100 次选择，诊断数量为 100；每次选择、Normalize 和视图事件都可以继续增长。

统一经过有界诊断入口，保存提供者身份，并考虑将持续失败的观察者标记为不可用，避免每次输入继续重复失败。
补充观察者失败不回滚已发布事务、诊断有界及后续健康观察者仍被通知的回归。

### R9：两种 PDF 后端共享逐 grapheme 的全文样式扫描

位置：[print_layout.rs:145](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-services/src/print_layout.rs:145)。

**静态路径。** `wrap` 每个 grapheme 都从头遍历 `display.spans`，再从头遍历全部公式。
复杂度为 O(字素数 × 样式段数/公式数)，格式密集文档容易接近平方增长。
两种 PDF 编码器使用同一打印布局，因此切换 v2 不会消除该成本；后台执行只避免直接阻塞 UI，并不会减少工作量。

为每次单元格/段落 `wrap` 二分定位样式与公式起点，随后按顺序推进游标；
`document` 中的代码行、表格和列表查询也可复用已有只读索引。
这是独立的小规模优化，不要求引入新排版引擎。加入格式密集、公式密集及长代码导出规模测试。

### R10：光标闪烁与普通重绘会重复比较保存基线的全文字节

位置：[lib.rs:103](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-core/src/lib.rs:103)、
[element.rs:355](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-ui/src/element.rs:355)。

**静态路径，较低优先级。** 每次 `Editor::render` 为窗口标题查询 dirty；core 通过源码与保存的 String 比较字节。
干净文档以及在文末进行等长替换的文档会付出 O(全文长度)，500 ms 的光标闪烁也会触发这项工作。
长度不同的内容可以快速判断，不应把所有 dirty 查询都描述成全文扫描。

将精确 dirty 结果缓存到内容发布、撤销/重做和保存回执时更新；保存快照身份相同可以直接判定干净，
其他情况保留内容相等的正确语义。不能仅比较 revision，否则撤销回原文或重新输入相同内容后会误判。
窗口标题也只在路径或 dirty 变化时更新。

## 扩展性改进

1. **将缓存声明细化到贡献者。** [MarkdownFeatures:63](/Users/jiangtao/code/github.com/piccolomon/crates/ryne-features/src/lib.rs:63)
   在存在任意 `additional` 功能时禁用全部投影与控件缓存，连无呈现贡献的命令功能也一样。
   这是保守且有意的正确性策略，但扩展数量上升会直接损失已有性能优化。
   给各提供者声明源码、选区、显隐、配置或资源代次依赖，主呈现、装饰和结构控件分别聚合键；
   未声明的提供者继续不缓存，仅让它自己的结果失效。不要简单删除现有保护。

2. **明确注册能力的层级。** 当前命令、已有布局位置的菜单/控件、围栏预览和 PDF 后端有实际执行通路；
   但 `PrimaryPresentation("document")` 独占，代码/列表/引用/表格贡献器仍由 `PresentationBuilder` 直接调用，
   `BlockContribution` 是私有接口，布局和绘制也显式匹配各结构。
   新增全新块呈现仍需修改多个层级。建议先让内置块通过统一的可验证贡献协议进入 composer，
   再按真实需求扩展呈现原语。继续把“注册同类功能”与“新增任意 Widget/语法”区分清楚。

3. **拆分缓存数据与视图派生。** 源码结构、正文映射、控件定义、局部源码过滤、可见几何和菜单状态应有独立生命周期。
   本轮 R1 正是将两个生命周期合并的结果。现有不可变 Source/ParseSnapshot/IndexedDisplay/TextLayout 是合适的基础，
   不需要再维护一份可独立编辑的富文本正文。

4. **逐步减少对解析器节点细节的跨层依赖。** 功能层多处直接匹配 NodeKind、遍历 parent/children 与 tokens；
   SemanticQuery 目前还没有覆盖段落、单元格归属和边界编辑查询。
   优先把 R5/R6 所需查询补到 syntax 层，以同一快照索引集中处理容器和裸 CR；
   不必为了抽象而复制整套 AST，也不需要更改或发布相邻解析库。

## 建议实施顺序

| 阶段 | 工作 | 完成标准 |
| --- | --- | --- |
| 第一批 | R1、R2、R7、R8 | 缓存等价、识别可收敛、扩展故障不越过宿主、诊断有界 |
| 第二批 | R3、R4、R5、R6 | 鼠标/方向键接入已有索引；普通删除不重复全文投影；混合表格消除交叉扫描 |
| 第三批 | R9、R10、贡献者缓存声明 | 导出遍历按顺序推进；重绘不比较全文；简单扩展不使全部缓存失效 |
| 后续 | P3 块级排版、P4/P5 局部语义和 IME 投影 | 明确源码/显隐/字体/宽度/资源身份；保持滚动锚点、Unicode、容器及统一选区 |

内容编辑、撤销和 IME 临时状态仍全文解析/投影，正文仍全文精排，这是代码与 docs/32 已承认的限制。
本轮不将它们算作新发现。P3 应先复用未变化块的排版并维护高度索引，再逐步引入可见区域精排；
不能只裁剪绘制就宣称完成虚拟化。P4/P5 应以跨块语义影响范围、IME 临时身份和原始换行回退为验收边界。
保留目前锁定依赖和当前分支，不需要先更换 GPUI、解析器或 Markdown 数据模型。

性能验收补充真实入口：窗口末尾左右移动、Backspace、代码块后的悬停、跨屏拖选、持续中文预编辑；
同时记录缓存命中率、投影次数、访问行数与分配量。已有无窗口缓存查询基准不能替代这些操作的端到端延迟。

## 验证与复现

本机 macOS、Rust 1.99.1：

- `python3 scripts/check-architecture.py`：通过，9 个私有包。
- `cargo fmt --check`：通过。
- `cargo test --locked`：通过，255 项测试，未包含下列独立审查探针。
- `/usr/local/opt/rust/bin/cargo-clippy clippy --locked --all-targets -- -D warnings`：通过。
  当前 PATH 的 `cargo clippy` 是旧 1.81 驱动，因此直接使用已安装的 1.99 驱动，没有改全局配置。
- `cargo run --locked -- --smoke-test`：通过完整 macOS 真实窗口冒烟，含 P1/P2 专项、列表/表格/代码菜单、IME 几何等。
- 独立探针复现 R1、R2、R5、R7、R8；混合表格诊断支持 R6。

未执行 Windows 桌面验收、各真实输入法的人工候选窗口检查、release 端到端性能或峰值 RSS 测量。
上述成功不能排除缺少场景覆盖的问题；R1/R2 在现有全绿测试之外仍可复现。

本目录文件：`repro.rs`、`repro-output.txt`、`scaling.rs`、`scaling-output.txt`、`clippy.log`、`smoke.log`。
使用当前工作区代码重放（输出用于观察问题，探针本身不是要求通过的生产测试）：

```sh
python3 docs/reviews/architecture-2026-10-27/run.py repro
python3 docs/reviews/architecture-2026-10-27/run.py scaling
```

工具仅构建本地包，将探针链接到临时可执行文件；不会添加 Cargo target 或修改生产代码。
修复后应将对应断言纳入正式会话、服务和窗口回归，而不是把本目录的当前缺陷输出当作正确行为。
