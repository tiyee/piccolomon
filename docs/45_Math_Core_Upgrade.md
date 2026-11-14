# math-core 0.9.0 升级与回归

验证日期：2026-11-14。升级前使用工作区当时的未提交状态作为基线，保留已有 PDF、解析器、区块树及界面改动。

## 版本与范围

- 正式工作区 `math-core =0.5.5` → `=0.9.0`，同时更新内部 renderer 至 0.9.0、stable-arena 至 0.3.0，新增 kstring 2.0.5。其余已锁定包没有版本变化。
- 版本依据：[官方 0.9.0 发布说明](https://github.com/tmke8/math-core/releases/tag/v0.9.0)、[API 文档](https://docs.rs/math-core/0.9.0/math_core/)，并通过 `cargo info math-core@0.9.0 --registry crates-io` 核实。该版本要求 Rust 1.96；本次使用 Rust 1.99.1。
- Formulary 0.3.0、两套数学字体、GPUI、源码映射及 PDF 编码器版本保持原工作区状态。
- `docs/reviews/formulary_renderer` 是独立、排除于工作区的历史评估工具，继续锁定 0.5.5，用于复现旧评估，不作为正式应用依赖。

## 适配与回退

`LatexToMathML::new(config, vec![])` 显式传入空外部宏目录，使用 `convert_with_local_state` 获取 `ConvertResult`。每个公式单独转换，不共享宏、标签或公式编号；字体仍参与缓存键和资源路径，PDF 仍冻结服务快照。

0.9.0 的函数语义输出包含 `&ApplyFunction;` / `&InvisibleTimes;`。Formulary 的 XML 解析器不识别这些 HTML 命名实体，未经适配会使 `\sin`、`\log`、`\lim` 等回退。适配器仅把这两个实体替换成对应 U+2061/U+2062，不二次解码 `&amp;` 包含的字面文本。

转换成功也可能带未解析引用警告；任何转换警告都保留完整 LaTeX。Formulary 的未知元素/结构警告、缺字、变体和原有尺寸限制继续生效。另增加 256 KiB 中间 MathML 上限。

Formulary 0.3.0 会静默忽略 CSS。对于输出标签带 `style` 或 `class` 的公式，适配器保守回退，不丢弃颜色、负间距、矩阵分隔线或对齐要求。`cases`、部分带对齐的矩阵/方程环境也可能因此回退；普通 `matrix` / `pmatrix` 仍可呈现。`\boxed` 通过既有未知 `menclose` 警告回退。

宏回归发现短输入 `\def\nest{\sqrt{\nest}}\nest` 在允许展开、即使限制 64 次时也会使 debug 测试线程栈溢出并 abort；`catch_unwind` 无法捕获。正式适配器在解析前拒绝新版的七类定义/赋值命令：`def`、`gdef`、`let`、`global`、`newcommand`、`renewcommand`、`providecommand`。扫描按控制词和转义控制符区分，保留此前“不支持用户宏定义时显示源码”的行为。内置 `\cdots`、`\iff`、`\pmod` 等仍允许展开，采用 1000 次预算。此策略不等于对任意上游递归输入作强制隔离或中断保证。

## 自动回归覆盖

新增三个数学服务测试和一个跨层会话测试：

- 21 个受支持表达式 × 两种字体，覆盖函数、Unicode、分数、组合数、上下标、根式、矩阵、括号、向量、分组及内置宏；检查 SVG 可解析与有限度量。
- 未解析引用、CSS 依赖、未知命令、中文缺字的完整源码回退与失败缓存。
- 自定义宏、直接/间接递归及定义隔离，后续普通公式仍可呈现。
- LF / CRLF / CR × 两种字体：段落、引用内列表、表格内公式；逐字节验证 ASCII 回退源码的双向位置映射，包含相邻中文与 emoji；点击对应的光标选中、局部/全文源码切换不产生 dirty/历史；修改后一次撤销/重做恢复源字节。
- 同一跨层测试冻结字体后切换实时服务，生成六份真实 PDF 字节流并验证文件首尾；导出不修改文档身份或历史。

既有测试继续覆盖 4096 字节/嵌套限制、两字体缓存、256 项以外活资源复用、12–36 px SVG 基线及 1×/2× 栅格、IME、保存和原始换行。

## 升级前后渲染对照

正式适配器的 `math-fonts` 示例分别在升级前、升级后运行，产物位于本地 `target/math-core-upgrade/before` 和 `after`。

8 个公式 × 2 种字体 × 5 种字号产生 80 个 SVG 与 160 个 PNG。70 个 SVG、140 个 PNG 完全相同；差异全部来自二次方程公式 `\frac{-b \pm \sqrt{b^2-4ac}}{2a}` 的运算符间距。两种字体高度不变，宽度分别从 4.809022 / 5.413473 em 变为 4.1868 / 4.79125 em。已查看升级后并排栅格：正文、上下标、根号和分数线完整，未发现新增裁切。

既有 `\overrightarrow{AB}` 箭头穿过字母的 Formulary 布局限制仍存在，本次未改变。Lete 的 `\hat{y}` 和 STIX 的 `\mathcal{F}` 仍按字体覆盖回退；升级不承诺所有 LaTeX 语法或出版级排版。

## 检查记录

日志位于本地 `target/math-core-upgrade/`：

- `python3 scripts/check-architecture.py`：通过，10 个私有包边界有效。
- `cargo fmt --check`：通过。
- `cargo test --locked`：451 项通过，2 项原有忽略（外网图片下载、万文件侧栏性能采样），0 失败。
- `/usr/local/opt/rust/bin/cargo-clippy clippy --locked --all-targets -- -D warnings`：通过。
- `cargo run --locked -- --smoke-test`：通过，执行至 stage 395，退出码 0；包含公式几何/绘制、原生字体菜单、IME/拖选延迟、表格/列表/引用与区块树布局回归。

真实窗口冒烟使用编辑回调与实际排版/绘制，不等同于人工鼠标键盘或所有输入法候选窗口验收。本次运行平台为 macOS，未进行 Windows 实机测试。
