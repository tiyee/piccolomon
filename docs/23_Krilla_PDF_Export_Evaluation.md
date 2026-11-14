# krilla PDF 导出方案 B：适配性评估

> 2026-11-14 更新：正式导出已统一使用 Krilla，入口为“导出 PDF”，实现位于
> `crates/ryne-services/src/pdf.rs`，分页布局位于 `print_layout.rs`。下文保留历史评估和原双后端记录。

评估日期：2026-10-13。范围：当前工作区实现、krilla 0.8.2、krilla-svg 0.8.1。
本次只做代码与上游 API/发布包核查，不替换正式导出后端，不声称完成编译或 PDF 视觉验收。
当前工作区包含尚未提交的 PDF 导出实现；下文以读取到的工作区代码为准。

后续实现：按用户要求，正式菜单现已并存“导出 PDF”和“导出 PDF v2”，后者使用 krilla。
实现位于 `crates/ryne-services/src/pdf_v2.rs`，复用 `pdf.rs::layout_pages` 的 A4 SVG 场景，
通过独立 usvg 0.47 解析并交给 krilla-svg，保留正文文本嵌入。共用现有快照、后台任务、路径保护和原子写出，
v2 默认文件名追加 `-v2`，七种界面语言均提供独立入口。原 GPUI/解析器/resvg/grapheme 版本保留。
这交付了下文阶段 1 所对应的编码后端接入，尚未交付新的 shaping/断行/分页引擎、Mermaid 矢量资源或 Tagged PDF。
下文“本次”指初始研究，不是该后续实现的验证范围。

## 结论

**方案 B 适合 Ryne 的长期导出方向，可以进入独立原型；不适合仅替换编码库后就宣称实现出版级排版或与屏幕完全一致。**

项目已经拥有独立只读导出快照、平台无关结构投影、公式 SVG 和基础分页，
因此无需从零搭建，也无需为接入 krilla 修改 Markdown 数据模型或引入第二份可编辑正文。
但尚未拥有跨 GPUI 与 PDF 共用的字体/字形布局结果，当前分页也只是基础实现。
主要投入将落在字体解析、文本 shaping、合法断行与分页，而不是 PDF 对象编码。

建议先改进导出的排版边界并做 krilla 对照原型；是否替换现有后端，以文本正确性、
视觉差异、资源占用和新增 PDF 能力的实测结果决定。当前没有充分理由同时交付 Typst 与 krilla 两套正式后端。

## 1. krilla 能做什么

krilla 在 pdf-writer 上提供字形、路径、图片等绘制抽象，并提供字体子集嵌入、
链接、书签和 PDF 标准相关能力。它明确把段落排版、表格布局、分页、页眉页脚排除在范围之外。
因此它适合接收已经排好版的页面，不能替代 Page Layout Adapter。
参见 [上游定位与范围](https://github.com/LaurenzV/krilla#scope)。

其文本 API 需要区分：

| API | 能力与边界 | Ryne 用法 |
|---|---|---|
| `draw_text` | 单行 shaping；只接受给定字体，不执行字体回退或完整 Bidi 解析，限制为单一 script | 只适合已确认字体覆盖的简单文本片段 |
| `draw_glyphs` | 接收外部排好的字形；Bidi、回退、布局由调用方承担 | 正式中外文混排的主要候选入口 |

不能直接把中文、拉丁字母、Emoji 混排的一整段交给 `draw_text`，就认为获得完整段落排版。
参见 [Surface 文本 API](https://docs.rs/krilla/0.8.2/krilla/surface/struct.Surface.html)。

krilla 支持多种 OpenType 字体和彩色字体表，但这不代表自动执行 OpenType MATH 公式布局。
公式仍由现有 Formulary/math-core 负责。
参见 [字体能力](https://docs.rs/krilla/0.8.2/krilla/text/index.html)。

## 2. 项目已经具备的基础

当前链路为：

```text
Source 的只读快照
  → ExportSnapshot::render
  → 折叠态 Display（样式、列表、表格、公式、图表）
  → pdf.rs 自有 A4 排版与分页
  → 每页 SVG
  → usvg + svg2pdf
  → pdf-writer 拼接页面
```

| 代码位置 | 已有能力 | 接入方案 B 的复用方式 |
|---|---|---|
| `crates/ryne-session/src/export.rs` | 隔离编辑状态的快照、图表生成与预算、失败回退 | 保留，不读取 live View |
| `crates/ryne-extensions/src/lib.rs` 中 `PdfExporter` | `Send + Sync` 的独立导出服务端口 | 可直接注入 Krilla 实现，无需先改成多格式插件系统 |
| `crates/ryne-projection/src/lib.rs` | 字节映射、样式、列表/表格/代码范围、公式和图表对象、flow 信息 | 作为导出适配输入，逐步补充所需语义 |
| `crates/ryne-projection/src/resources.rs` | 公式的 em 指标与 SVG；图表像素 | 复用公式基线指标与矢量 SVG；图表先保持现状 |
| `crates/ryne-services/src/pdf.rs` | A4 基础分页、列表缩进、长代码与长单元格分片 | 保留作为对照，提取布局规则而非整段照搬 |
| `crates/ryne-ui/src/export.rs` | IME 确认、后台导出、路径保护、临时文件与重命名写出 | 保留现有交互与写出流程 |

已有源码 4 MiB、500 页、图表最多尝试 32 块/32 MiB 等约束应继续保留。
增加新的字体/布局缓存时，需要单独计量其内存，不能把图表预算当作整个导出的内存上限。

## 3. 当前距离“高保真”的具体差距

### 3.1 屏幕与导出不是同一个排版器

屏幕 `layout.rs::shape_inline` 使用 GPUI `shape_text`、`shape_line` 和 `line_wrapper`。
导出 `pdf.rs::wrap` 则逐 grapheme 迭代，并累加每个字符的字体 advance，超宽即换行。
最终 SVG 文本又由 usvg shaping。

因此目前测量宽度与最终绘制宽度可能不同，尤其是 kerning、连字、组合字符和复杂 script；
按 grapheme 不拆字符，也不等于遵守英文单词边界与中文标点禁则。
**将 svg2pdf 换成 krilla 不会自动修复这些差异。**

`docs/GPUI_GFM_Paragraph_Line_Breaking_Architecture_v1.md` 仍标为建议方案，
主张先 Standard、后可选 Balanced；当前代码没有交付该共用段落引擎。
不能把文档中的 Knuth–Plass 思路视为已经可复用的生产实现，也不必等 Balanced 完成才开始 PDF 原型。

### 3.2 GPUI 字形结果缺少可直接复用的字体桥接

核查锁定的 GPUI 0.2.2 源码：`ShapedRun` 暴露 `FontId`，`ShapedGlyph` 暴露字形 ID、
位置和文本索引。但 `TextSystem` 没有提供把任意回退后的 `FontId` 导出为字体字节、
TTC face index 和 variation 坐标的完整公开接口。
`get_font_for_id` 返回字体描述，不能等同于精确的字体二进制身份。

krilla 需要用具体字体数据构造 `Font`。字形 ID 必须对应同一字体 face；
只按 family 名重新寻找字体，再复用屏幕 glyph ID，并不可靠。
连字和组合字形还需要准确的 cluster 文本范围，才能保证 Unicode 提取。

可选路径：

- **近期：导出端独立 shaping。** 明确字体文件、face、回退和 cluster，输出定位字形；
  目标是风格与指标接近编辑器，接受两个 shaping 系统的差异。
- **长期：共用字体与段落布局服务。** 在不依赖 GPUI 的接口中表达字体资源身份、
  字形布局和文本范围，再由 UI 与 PDF 消费。让 GPUI 使用相同结果的绘制可行性仍需原型，不能提前承诺。
- **直接搬运 GPUI 布局：暂不作为首选。** 需要补平台字体桥接或修改依赖边界，
  与维持 GPUI 0.2.2、不把服务层绑到 GUI 的目标冲突较大。

上游发布包中的 `examples/parley.rs` 展示了外部布局结果接入 `draw_glyphs`，
可作为原型参考；它不是完整文档分页器，也不能证明 Parley 与 GPUI 的断行一致。
本次读取了 [krilla 0.8.2 发布包](https://static.crates.io/crates/krilla/krilla-0.8.2.crate) 中该示例。

### 3.3 样式与单位尚未统一

屏幕字体主题在 `ryne-ui/src/theme.rs`，使用逻辑 px；PDF 在 `pdf.rs` 另定义 pt 数值。
例如正文 18/30 px 对照 PDF 12/19.8 pt，表格 17/28 px 对照 11/19 pt；
标题在屏幕用半粗，PDF 当前使用 700。块间距的 flow 规则也没有被 PDF 布局完整复用。

建议把可共享的字号比例、字重、颜色、块间距放入平台无关的值类型；
打印配置单独指定页尺寸、页边距和缩放。若采用 96 逻辑 px/in 的产品约定，则按 0.75 转成 pt，
不要乘 Retina backing scale，也不要暗示逻辑 px 等于显示器物理长度。

屏幕栏宽最大 760 px，而 A4 内容宽约 499.28 pt（按上述约定约 665.7 px）。
同字体也会产生不同断行。默认目标应是**同一语义与视觉风格，在纸张宽度下重新排版**；
若额外提供固定屏幕栏宽导出，需要明确整栏缩放与分页策略。

### 3.4 分页规则仍需完善

当前 `room` 主要依据剩余高度换页，长表格按单元格视觉行绘制网格片段。
尚未形成独立规则来处理标题与后文同行留页、段落孤行/寡行、列表条目拆页、
表头重复、普通表格行尽量完整留页、超高行的连续边框等。
当前能够跨页输出长内容，不等于已经具备出版级的分页策略。

应先实现确定、可测试的 Standard 排版和基本分页约束；
Balanced 是后续可选增强。代码块继续保留字面内容、缩进与源码换行语义。

### 3.5 公式与 Mermaid 的资源质量不同

公式 `Formula` 已保留自包含 SVG、width/ascent/descent，可保持矢量并沿用基线。
保留当前不支持语法、缺字和超限回退；无需为 krilla 更换公式引擎。
路径公式的清晰度与可搜索性是不同问题，不能宣称公式文字会自动变成可复制 Unicode。

Mermaid 服务内部生成 SVG 后光栅化；公开 `Diagram` 只有像素、尺寸与预算信息。
现有 PDF 也嵌入 PNG。因此只换 krilla，图表仍是位图。
若目标是放大或印刷时清晰，应让只读图表资源额外保留经限制的矢量数据，
同时保留屏幕像素缓存；新资源要纳入预算，失败仍回退源码。

### 3.6 可搜索文本、链接与 Tagged PDF 需要显式保留语义

普通正文应以字体字形写出，保留 cluster → Unicode 映射，不能为了外观一致把整页轮廓化或截图。
现有 svg2pdf 链路已经支持嵌入文本字体；可搜索正文不是 krilla 独有的收益，实际提取正确性须对照验证。

当前 `Style.link` 只有布尔标识，`Display` 没有独立链接目的地址集合。
若要添加可点击链接，需要在投影/导出语义中携带 URL 与文字范围，再映射到各页注释矩形。

krilla 提供 tagging，但不会从绘制矩形自动识别表格或阅读顺序。
需要保留 heading/list/table/cell 等结构、设置语言、构建 tag tree，并为公式和图表补替代文本；
PDF/UA 不能作为接入即获得的承诺。
参见 [上游 tagging 文档](https://docs.rs/krilla/0.8.2/krilla/tagging/index.html)。

## 4. 依赖与原生集成

本次核查 docs.rs 和 crates.io 发布包的规范化 Cargo.toml：

| 项目 | 当前项目 | 上游核查版本 | 影响 |
|---|---|---|---|
| Rust | 本机 1.99.0 | krilla/krilla-svg MSRV 1.92 | 工具链满足声明要求 |
| PDF 编码 | pdf-writer =0.12.1 | krilla 0.8.2 依赖 pdf-writer ^0.15.0 | 可考虑多版本共存，不能混用两版对象类型 |
| SVG | resvg/usvg 0.45.x | krilla-svg 0.8.1 依赖 resvg/usvg ^0.47.0 | `usvg::Tree` 不可直接跨版本传递 |
| 文本 | GPUI 平台 shaping | krilla `simple-text` 使用 Rustybuzz | 不能假定 shaping 结果相同 |

版本差异本身不是 Cargo 必然解不出的冲突。近期原型可在适配器内部从 SVG 字节重新解析，
让 0.45 与 0.47 共存；代价是依赖、体积、编译和解析开销，需要测量。
不应为原型擅自升级项目锁定的 GPUI、解析器、grapheme 或 resvg 版本。

来源：[krilla manifest](https://docs.rs/crate/krilla/0.8.2/source/Cargo.toml)、
[krilla-svg manifest](https://docs.rs/crate/krilla-svg/0.8.1/source/Cargo.toml)。
本次没有进行完整依赖解析或编译，因此“可共存”是接口与 Cargo 版本规则层面的判断，尚非集成验收。

`PdfExporter: Send + Sync` 可以继续使用无状态服务，任务内部创建 Document/Page/Surface；
不要求把绘制中的 Surface 在多个线程共享。导出与错误报告继续留在后台任务边界，
绘制栈和 tagging 配对应由适配器保证，失败不能穿过 AppKit 回调。

## 5. 推荐架构与实施顺序

```text
ExportSnapshot → 折叠 Display / 必要的只读语义
  → ExportDocument（块结构、原始文字、样式、对象、链接、语言）
  → PrintLayout（字体回退、shaping、合法断行、表格与分页）
  → PagedScene（定位字形、矢量、位图、链接区域、语义关联）
  → KrillaPdfExporter → PDF
```

这些都是可丢弃的导出产物，不构成第二份可编辑正文。`ExportDocument` 保留语义，
`PagedScene` 保存已经布局的结果；若未来增加 Typst，它应从语义输入分支，不能接收 krilla 的绘制列表。

接口仍放在 `ryne-extensions`，具体算法与 krilla 实现在 `ryne-services`；
平台无关资源/场景值可在 `ryne-projection` 表达。UI 只选择选项与发起任务，
会话只生成快照，不导入 krilla，不读取实时光标或旧窗口几何来导出。
暂不必引入 `id()`、后端注册表和切换菜单；现有服务端口足以承载原型与后续替换。

1. **对照原型。** 独立于主 workspace 锁文件，固定版本；复用当前分页生成的 SVG，
   改由 krilla-svg 写出。验证 SVG/字体兼容、文件体积、耗时与多阅读器结果。
   此步骤只评估编码替换，不代表完整方案 B，也预期不会改善断行和分页。
2. **真实方案 B 原型。** 导出端使用明确字体资源与 shaping，生成定位字形，
   通过 `draw_glyphs` 写出；公式用现有 SVG，先完成 Unicode/CJK、表格和基础分页。
3. **资源与语义增强。** 保留 Mermaid 矢量、代码高亮快照、链接 URL、书签；
   共享主题值，扩充跨页规则。Tagged PDF 作为独立交付项验证。
4. **有需求再共用布局。** 按段落设计文档引入 Standard/Balanced，评估屏幕绘制适配，
   明确不同宽度下的重排行为；不把该重构作为交付基本 PDF 的先决条件。

阶段 1 工作量较小，收益主要是验证后端；阶段 2 为中高投入，主要成本是字体与打印布局；
阶段 4 影响 UI、字形资源和交互映射，投入明显更高。本次没有原型计时，不提供未经验证的工期。

## 6. 原型通过标准

| 方面 | 必须验证的样例/结果 |
|---|---|
| 正文正确性 | 中文禁则、中英混排、英文单词、NBSP、长 URL、组合音标、连字与 Emoji；不丢字、不重叠 |
| 字体 | 中日韩回退、粗斜体、TTC face；实际测量与输出使用同一字体与 shaping 结果 |
| 结构 | H1–H6/Setext、嵌套列表、引用当前源码回退、行内与围栏代码的字面内容 |
| 分页 | 标题随文、孤行策略、跨页代码、表格长行/重复表头、末页和超宽对象 |
| 数学/图表 | 行内公式基线、上下标、表格内公式；有效 Mermaid 与错误/超限源码回退 |
| PDF 文本 | 正文复制/搜索，连字与组合字符提取；不要把视觉正确代替 Unicode 正确 |
| 编辑隔离 | LF/CRLF/CR 源码字节、dirty、路径、统一选区、历史、源码模式不因导出改变；IME 沿用确认策略 |
| 成本 | 同语料比较峰值内存、耗时、页数、PDF 大小与应用包体积；字体跨页重复嵌入情况 |
| 平台/阅读器 | macOS 与 Windows 字体解析；至少 Quartz/Preview 与 PDFium 或 Poppler 对照 |

若阶段 1 视觉相同且没有明确需要的 PDF 能力，保留当前后端是合理结论。
若目标是编辑器风格一致、可控的 Markdown PDF，推进方案 B；若目标改为成熟的论文分页、
复杂交叉引用和专业排版模板，应单独评估 Typst，而不是期待 krilla 补齐这些上层能力。

## 7. 本次验证范围

已读取现有 PDF、快照、主题、布局、投影、图表资源与导出示例；核查 GPUI 0.2.2 本地源码；
在线核查 krilla/krilla-svg API、版本、许可与依赖声明，并读取正式发布包 manifest 和 Parley 示例。
本次仅新增本文，不改应用行为、依赖或用户已有修改，未运行应用构建、测试、GUI 冒烟或 PDF 视觉对照。

## 8. 双后端实现验证（2026-10-13）

后续实现保留原导出入口和 `Cmd+Shift+E`，新增独立 `ExportPdfV2` action。
macOS 原生菜单与 Windows 窗口内菜单使用同一七语言命令树，v2 复用原有后台快照、
IME 确认、并发保护、覆盖确认和同目录临时文件重命名。实现只增加新后端及适配依赖，
未升级现有 GPUI、解析器、grapheme、usvg/resvg 0.45 或 pdf-writer 0.12。

已通过：架构检查、Rust 1.99 格式检查、`cargo test --locked`、
`cargo test --locked --example export-pdf`、Clippy all-targets（warnings denied）和真实窗口 `--smoke-test`。
本机 Cargo 的默认 fmt/clippy 子命令落到了旧 rustup 1.81，验证时显式使用 Homebrew Rust 1.99
的 cargo-fmt/rustfmt 与 cargo-clippy/rustc，没有修改全局工具链配置。

回归覆盖两种后端的混合 Markdown、公式、嵌套列表、代码字面内容、表格、Mermaid 成功与失败回退、
长段落/代码/单元格分页、原编辑状态隔离，以及 v2 空文档 A4 页面和输入预算/控制字符拒绝。
菜单测试覆盖七种语言的不同 action，窗口冒烟检查系统菜单中的两个入口；
这些不代表完成了人工保存对话框或所有 Windows/输入法验收。

另外用同一份 CRLF 混合样例导出两种 PDF，由 pypdf 读取并以 Poppler 渲染全部 5 页对照：
均为 A4，中文、代码字面内容、错误图表源码与末尾 token 均可提取；
去除提取空白后两种输出正文一致。检查了公式、网格、列表、代码、图表与全部页面连续性。
该单一样例 v1 为 241,550 字节，v2 为 104,594 字节；这不是通用体积或性能承诺。
公式仍为矢量路径，图表仍为位图，未声称实现新的排版引擎或跨阅读器完整兼容性验收。

已运行 `sh scripts/bundle-macos.sh debug` 生成 `target/Ryne.app`，未终止用户已有窗口。
`--smoke-test-failure` 按预期报告检查失败并以退出码 1 正常结束，没有 panic 穿过原生事件循环。
