# 脚注与屏外焦点可行性复核

日期：2026-11-15。主程序基线 HEAD：`e361ebeeba1263a5c4a20533569fa73c2012f379`，工作区含规划文档修改。工具链为 Rust 1.99.1 / Cargo 1.99.1，macOS 本机运行无窗口探针。

结论：**总体有条件可行，现有证据不要求改成可编辑块树或更换解析器；原规划遗漏了几项必要接口和布局改造，不能作为小型 UI 功能直接开工。** 本轮已修正[脚注规划](../../49_Footnotes_Design_And_Plan.md)与[视口设计](../block_tree/DESIGN.md)。当前只完成部分 FN0 后端证据和静态接口核查，生产脚注、B3、平台 IME 仍未交付。

## 1. 主要发现与处理

| 优先级 | 原规划不足 | 当前代码证据 | 修正 |
| --- | --- | --- | --- |
| P1 | 缺几何时返回 None、下一帧补排，不能保证候选框正常 | GPUI 0.2.2/macOS 的 `first_rect_for_character_range` 将 None 转为原点零矩形；Windows `handle_ime_position` 缺位置则跳过更新 | 活动输入范围提前精排；正常查询不能缺矩形；真实屏外矩形需真机验收，未通过时预编辑滚动保留候选锚点可见 |
| P1 | 保留焦点句柄不等于保留平台输入 | [paint.rs](../../../crates/ryne-ui/src/paint.rs) 的 `paint_frame` 每帧注册 `window.handle_input` | 正文宿主保持逐帧输入注册，全占位帧也注册；独立验证 focus、输入桥接和几何三项 |
| P1 | 解析配置“统一”没有列出实际迁移面 | [EditingState::new](../../../crates/ryne-extensions/src/lib.rs)、[命令提交](../../../crates/ryne-features/src/commands.rs)、[临时 IME](../../../crates/ryne-session/src/composition.rs)、[导出重建](../../../crates/ryne-session/src/export.rs)均调用现有数学解析入口 | FN1 先引入值类型配置并贯通候选、历史、格式化、裸 CR 与导出；默认 GFM 构造行为保持不变 |
| P1 | 编号替换缺少原子映射和上标混排支持 | [Display](../../../crates/ryne-projection/src/lib.rs) 与 [IndexedDisplay](../../../crates/ryne-projection/src/indexed.rs) 的等长 span 特判；[MixedLine](../../../crates/ryne-ui/src/layout.rs) 只支持 Text/Math | FN2 分成原子映射/混排和定义编辑两步；补停靠点、命中、缓存、验证及打印，不能把多位数字当普通文本或伪装为公式 |
| P1 | 返回记录无法仅凭当前通知跟随编辑 | [SessionEvent](../../../crates/ryne-extensions/src/lib.rs) 没有位置变换；[发布与历史路径](../../../crates/ryne-session/src/lib.rs) 内部才持有 PositionMap | FN3 增加会话跟踪锚点表，候选提交时原子映射；UI 只存句柄与视口偏移，不长期保留 old/new Source |
| P1 | “IME 时禁用跳转”与现有鼠标路径顺序冲突 | [mouse_down](../../../crates/ryne-ui/src/lib.rs) 首先 `commit_composition` | 修饰键导航预检先于普通点击；禁用时不提交候选、不改选区，不降级为普通点击 |
| P2 | 定义内部结构不能直接复用现有容器前缀 | [syntax.rs](../../../crates/ryne-parser/src/syntax.rs) 仅扫描 BlockQuote/Item 前缀；后续代码/列表查询依赖 token | FN1 增加定义头部/续段前缀来源；区分 lazy continuation、尾部空行与字面代码缩进，FN2b 才接续段命令 |
| P2 | 只读跳转、任意行内命中和键盘菜单不是已有接口 | [CommandPlan](../../../crates/ryne-extensions/src/lib.rs) 仅 Edit/Select/View/Consume；[RenderControlPlacement](../../../crates/ryne-extensions/src/render_actions.rs) 仅 Header/Leading/Trailing/Marker | FN3 明确新增类型化只读导航结果、行内目标命中和键盘入口，禁止借 Select 改选区冒充滚动 |
| P2 | 不支持定义与引用的回退、空定义边界不够具体 | 探针实际发出嵌套和空定义事件，正文引用仍会绑定到它们 | 先收集所有定义判重再过滤资格；空/不支持目标与关联引用整组保留源码；回退压过内部隐藏规则 |

P1/P2 是本次审查优先级，不是大文件优化阶段。上述问题是规划前置工作缺失，不表示已经发现或修复主程序中已发布脚注功能的 bug。

## 2. 锁定后端探针

本目录是独立 workspace，不加入 Ryne 的 10 个生产包。[Cargo.toml](Cargo.toml) 与主程序保持相同的 pulldown-cmark 0.13.6 和 feature 配置（关闭默认 feature，启用 html）；独立锁文件中的共享传递版本与主工作区一致，包括 unicase 2.10.0、bitflags 2.13.2 和 memchr 2.8.3。没有改主工作区锁文件。

[探针源码](src/main.rs)固定 22 组输入，检查每组引用/定义事件数量、所有事件范围能合法切片 UTF-8 原文，以及默认 GFM 子集不发出脚注事件；同时记录默认路径中的普通链接事件。标签具体值与嵌套层次见[原始输出](backend-events.txt)，本轮人工核对；不把事件数量断言称为完整标签匹配性质证明。

复现（在仓库根目录）：

```sh
cargo run --offline --locked --manifest-path docs/reviews/footnotes/Cargo.toml > /tmp/ryne-footnote-backend-events.txt
diff -u docs/reviews/footnotes/backend-events.txt /tmp/ryne-footnote-backend-events.txt
cargo fmt --manifest-path docs/reviews/footnotes/Cargo.toml --check
```

没有缓存依赖的环境可去掉 `--offline`，仍保留 `--locked`。这是 CLI 探针，无 GUI 事件回调，不启动应用或修改用户文档。

| 样例 | 实际观察 | 设计影响 |
| --- | --- | --- |
| `basic` | 脚注选项下 2 个引用、1 个定义；默认路径将引用解析为指向 `note` 的普通链接 | 编辑器启用脚注是显式方言变化，默认解析基线必须独立保持 |
| `undefined` / `unused` | 未定义引用只有 Text；无引用定义仍发出定义事件 | 无需扫描 Text 生成未定义脚注目录；无引用定义保持源码 |
| `duplicate` | 两个同名定义事件均保留，范围包含各自正文 | 可以实现歧义策略，但必须包含所有容器内定义再判重 |
| `case` / `unicode_case` | `X`/`x`、`STRASSE`/`straße` 均识别，事件仍保留不同大小写 | 后端私有 FootnoteLabel 使用 UniCase；不能照搬 VeloType 按大小写敏感字符串绑定 |
| `spaces` / `escape` / `table_pipe` | 事件标签折叠 ASCII 空白；一般反斜杠保留；表格内转义管道符被还原为管道字符 | 原始标签范围与语义键分开，采用事件标签加后端一致的相等性 |
| `multiline` / `lazy` | 定义含多段、列表、代码；不缩进续行仍可在同一段内；容器范围可能包含尾部空行 | 不能逐行去固定缩进或以首个无缩进行终止；必须补容器来源适配 |
| `empty` | 有定义容器但无正文节点 | 隐藏头部会产生不可编辑空块；首版整组保留原文 |
| `nested_quote` / `nested_list` | 定义是容器内部子节点，外部引用仍发出绑定事件 | 不支持嵌套定义时还要回退它的使用点，不能只丢目录项 |
| `recursive` | 定义内部引用仍产生事件 | 首版目录应明确排除定义内出现位置，不递归编号或形成无限回跳 |
| `literal` / `inline_math` | 代码、转义、HTML 块、数学内未产生脚注引用 | 使用后端节点判定资格，避免全局文本匹配 |
| `crlf` / `cr` | 两种原始换行的定义与引用范围均为合法 UTF-8 字节切片 | 只是后端证据，Ryne 裸 CR 归一视图仍需继承配置并保留公开 Source 身份 |
| `table_ref` / `definition_order` | 单元格发出引用；定义按源码顺序出现 | 全局显示编号需另外按正文引用顺序计算，不能采用定义事件顺序 |
| `link_collision` | 没有空行的 `[x]: /url` 被吞入前一脚注定义的 lazy paragraph | 不可假定相邻引用式定义一定在脚注之外；边界必须服从解析器并纳入格式化/续段测试 |

后端源码另外核实了 `linklabel.rs::FootnoteLabel = UniCase<CowStr>`、`scan_link_label_rest` 的空白/表格处理和 `parse.rs` 对未定义引用的判断。这些结论限定于本次锁定版本，不声称兼容所有脚注方言。

## 3. 实施依赖修订

1. **FN0 补齐字段来源探针**：验证定义头部、原始标签、续行前缀、空白/Tab、容器嵌套和三种换行的可恢复边界。当前只验证了事件范围；lossless token、字段来源、往返仍未验证。
2. **FN1 统一配置与容器语义**：修改所有派生解析入口，增加节点与前缀适配，再做只读目录、判重与资格。不能只开后端 flag 就接 UI。
3. **FN2a 原子对象及混排**：普通/IndexedDisplay 同步改造，覆盖 `[^x]` → `1000` 等长情况、数字内部停靠禁止、当前帧命中、缓存与上标文字度量。
4. **FN2b 定义结构编辑**：接正文、嵌套命令、首位置保护、空续段退出和完整源码入口；完整布局路径先通过差分。
5. **FN3 跟踪锚点与只读动作**：会话表在普通事务/Undo/Redo/IME 确认中原子更新，UI 只持有句柄；先调整鼠标预检顺序，再接导航与返回。
6. **FN4/FN5 导出与平台集成**：PDF 原位输出或完整源码回退；B3 启用后重验未测目标、屏外焦点、候选与滚动。没有真实窗口记录不能放行这些阶段。

会话跟踪锚点不依赖阶段 D 的位置树，脚注完整布局路径不依赖 B3；B3 也不依赖脚注。两者共享导航目标与几何协议，但不存在先后互相等待的环。

## 4. 本轮验证与限制

- 已运行锁定后端 22 例事件/数量/合法范围检查；原始输出可复现。
- `--offline --locked` 重跑与保存输出逐字节一致；独立探针 fmt、文档差异空白检查通过。本轮检查 8 份 Markdown 的 75 个本地链接/标题锚点及表格列数。
- 已静态核查解析配置、容器 token、投影映射、混排与缓存、提交/历史、鼠标事件、平台输入注册和 GPUI 候选定位。
- 未运行生产脚注集成测试、原子编号混排差分、FN3 锚点实现或虚拟布局窗口测试；未做人工 macOS/Windows 输入法验收。
- 未测量 1/5/10 MiB 性能、跳转延迟或 RSS；不能从 22 例探针推导容量或性能保证。

生产实现阶段必须保留旧默认解析基线，并通过规划中各阶段的退出条件。本轮仅放行继续做 FN0 范围来源验证及 B2/B3 的分阶段设计，不能将 FN1–FN5 或屏外 IME 支持标为完成。
