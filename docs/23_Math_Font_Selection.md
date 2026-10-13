# 数学字体切换与真实渲染对照

## 行为与范围

Ryne 菜单新增“数学字体”子菜单，提供 **Lete Sans Math** 和 **STIX Two Math**。
默认 Lete；当前选项带勾选，切换立即更新已收起的公式，包括列表和表格中的公式。
菜单标题及偏好保存错误使用既有七语言目录，字体家族名称保留原名。
macOS 原生菜单和 Windows 窗口内菜单共用命令树。

选择独立保存到语言配置同目录的 `math-font` 文件（macOS 为
`~/Library/Application Support/Ryne/math-font`，Windows 为 `%APPDATA%/Ryne/math-font`）。
缺失或未知值回退 Lete。写入使用同目录临时文件与重命名；失败保留本次界面选择并报告错误。
字体设置属于应用偏好，不写入 Markdown，不增加 dirty 或撤销记录。

IME 预编辑与拖选期间先保存选择，待交互结束后的正常 refresh 再更新公式投影，
保持正在使用的文字、选区与几何稳定。刷新重建基线、行高和折行，旧帧不能解释新投影；
字体重排复用源码滚动锚点。源码模式继续显示原始 LaTeX。

## 字体来源与许可

| 字体 | 固定版本 | 内嵌资源 |
| --- | --- | --- |
| Lete Sans Math | 0.63，提交 `5672e4573903c5658fb74d96b4282bfb4ed2404f` | `assets/fonts/lete-sans-math/LeteSansMath.otf` |
| STIX Two Math | 2.13 b171，保留原文件 | `assets/fonts/stix-two-math/STIXTwoMath-Regular.otf` |

Lete 来自[官方仓库](https://github.com/abccsss/LeteSansMath/tree/5672e4573903c5658fb74d96b4282bfb4ed2404f)，
字体文件使用 SIL OFL 1.1；原始版权与许可证随文件保存，没有修改或重命名字体。
SHA-256 为 `8e6f7468e12c19668943e71353f8ae0769682547146f11f3471da6f5f9db68b6`。
macOS、Windows ZIP 和复用 ZIP 的 MSIX 打包均包含两种字体许可。用户无需安装系统字体。

## 实现边界

- `ryne-extensions::MathFont` 定义稳定 ID、名称、默认值及可选字体目录；
  `FormulaRenderer` 提供当前选择、切换、固定设置的快照及资源读取端口。
- `ryne-services::math` 持有两份字体字节，按选择提供给同一版 Formulary，
  字形覆盖预检也使用所选字体。生产适配器不再固定使用 STIX。
- 缓存键为 `(字体, LaTeX)`，共享原有 256 项上限，含失败结果；切换后不复用另一字体的结果。
  资源路径为 `math:<font-id>:<latex>`，GPUI SVG 缓存同样区分字体。
  即使旧资源稍后加载，也按路径中的字体生成，不能混用新度量与旧字形。
- 尺寸继续由呈现主题控制：适配器在 18 px 布局，SVG 坐标放大至 1000 em 单位后输出，
  归一化度量交给 UI 与 PDF 使用。字体切换不引入独立富文本数据或文档事务。
- `FeatureEngine::snapshot` 固定公式渲染设置。导出启动时冻结字体，后台生成时
  即使界面切换也保持原选择；两个现有 PDF 编码器使用同一份公式矢量路径和度量。
  本次没有引入 Typst 或修改 PDF 分页引擎。

新增字体时，应固定文件与版本并附许可，扩展 `MathFont` 目录及服务层字节映射，
再加入覆盖预检、布局、资源身份与视觉对照验证。未知字符继续保留完整源码，
不偷偷混入另一数学字体；当前未提供逐字形字体回退。

## 可复现对照

```sh
cargo run --locked -p ryne-services --example math-fonts
```

输出到 `target/math-font-comparison/index.html`：两列分别为 Lete / STIX，
使用正式 `render_with_font`，对照分数、上下标、三次根号、积分、求和、矩阵、
伸缩括号、宽重音和箭头。字号为 12、14、16、18、36 px；每项包含 SVG、
1×/2× PNG、基线及 width/ascent/descent。`comparison.png` 汇总 14/36 px 的 2× 栅格，
左侧 Lete，右侧 STIX，各行按共同基线排列。

已检查生成的并排栅格：分数、根号、上下标、积分、求和、矩阵和括号均有可见字形；
Lete 为无衬线风格，度量与 STIX 不同。测试验证有限正度量、SVG 尺寸与基线一致、
脚本缩放不归零及各字号栅格非空。这些检查不能证明所有 OpenType MATH 算法正确。

**已知限制：**当前 Formulary 0.3.0 对 `\overrightarrow{AB}` 的箭头垂直位置不理想，
两种字体都可见箭头穿过字母；换字体不修复此布局问题。12–16 px 复杂多层公式的
可读性仍受小字号和当前布局引擎限制，不能把字体切换测试视为出版级排版验收。
Lete 可以渲染本语料中的 `\mathcal{F}`，STIX 在现有转换结果下缺字并继续回退源码；
字体覆盖差异允许产生不同的渲染/源码回退结果。

## 验证

- 数学服务回归：两字体 MATH 表、独立缓存/资源身份、固定字体快照、缺字回退、
  12–36 px SVG 度量及 1×/2× 栅格。
- 菜单和偏好：七语言标签、两项动作与唯一勾选、默认值、持久化及写入失败。
- 会话导出：段落/列表/表格公式固定字体，源码、Unicode/CRLF、选区与历史不变；
  两个 PDF 编码器均覆盖两种字体。
- 真实窗口冒烟：原生菜单动作可用、两种字体的公式资源和新布局、旧帧拒绝、
  IME/拖选延迟、偏好落盘及源码/选区/dirty/撤销历史不变。
  自动回调冒烟不等同于完整人工鼠标、键盘或输入法候选窗口验收。

未调整 GPUI、解析器、grapheme、Formulary、math-core 或其他依赖锁定版本。
