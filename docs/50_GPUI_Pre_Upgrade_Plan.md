# GPUI pre 升级计划与可行性核验

日期：2026-11-15。目标版本：`gpui-pre =0.3.12`。工作分支：`codex/gpui-pre`。
基线提交：`bf8ebd5ef9271f599f56747e2b0c1c5ac626d9a7`。

2026-11-16：用户授权正式升级后，主工作区已采用 `gpui-pre` / `gpui-pre-platform =0.3.12`，
完成 UI 接口适配、原生粘贴统一、字符输入优先处理及架构守卫修复。
实际运行结果与尚未覆盖的平台项见 [正式升级记录](reviews/gpui-pre/implementation/README.md)。
本文保留 11 月 15 日的设计与放行条件；第 4 节的“本轮核验结论”是当日可行性快照，
不能代替最新验收结果。原始依赖探针、差异及复现方法见 [历史核验记录](reviews/gpui-pre/README.md)。

## 1. 目标与边界

升级原生窗口、输入、文字及 GPU 绘制所依赖的 GPUI，继续交付 macOS 和 Windows 桌面应用。
保留现有十个私有包的分层：GPUI 及平台实现只进入 `ryne-ui`；解析、投影、事务、服务与 PDF 不依赖 GPUI。

升级必须保持以下行为：

- Markdown 源码唯一、按事务编辑，保存保持原字节和 LF / CRLF / CR；显示切换不产生 dirty 或历史。
- 标题点击显隐、行内补全、列表/引用/代码/表格就地编辑、Alerts、公式、图片、Mermaid 及源码回退。
- UTF-8 / UTF-16 / grapheme 边界明确，候选确认一次提交；鼠标命中、光标和输入法使用同一版本几何。
- 单窗口打开保存、草稿恢复、侧栏、七语言、主题、字体与 PDF 导出的已有语义。
- macOS 系统窗口按钮与 Windows 自绘标题栏继续走原生窗口操作和安全关闭。

不同时实施脚注、区块树 B2/B3、完整 Bidi、完整辅助功能编辑、Linux/Web/iOS 支持或编辑器架构重写。
新版框架提供相关接口不代表 Ryne 已实现这些能力。也不在本次引入 `gpui-component` 或另一套文本编辑状态。

## 2. 版本及分发来源

2026-11-15 核验时，`gpui` 包最新发布是 0.2.2。`gpui-pre` 是另一个包名下的 Zed 快照分发，
0.3.12 发布于 2026-11-09，其包内 `package.metadata.gpui-pre.zed-rev` 为
`d2a4fb6ded7c470be25d1541281da3882495b65b`。不把它写成 `gpui =0.3.12`，
也不把包名里的 `pre` 当作 Cargo 的 `-pre` 预发布版本语义。

核验使用官方稀疏索引与发布 tarball，下载后逐项校验索引 SHA-256。
核心、平台、macOS、Windows 四个包均为 0.3.12，未被撤回。后续实施再次检查发布状态，
不能在同一轮适配中自动追随新版本。包内来源元数据和校验和证明本次审查对象的身份，
不构成已审计全部上游源码或维护者身份的声明。

来源：

- [gpui 当前发布](https://docs.rs/crate/gpui/latest)
- [gpui-pre 0.3.12](https://docs.rs/crate/gpui-pre/0.3.12)
- [gpui-pre 官方索引](https://index.crates.io/gp/ui/gpui-pre)
- [gpui-pre-platform 官方索引](https://index.crates.io/gp/ui/gpui-pre-platform)
- [固定 Zed 源码](https://github.com/zed-industries/zed/tree/d2a4fb6ded7c470be25d1541281da3882495b65b/crates)

## 3. 依赖与初始化设计

正式工作区采用以下声明，与 `Cargo.lock` 配套维护：

```toml
gpui = { package = "gpui-pre", version = "=0.3.12", default-features = false }
gpui_platform = { package = "gpui-pre-platform", version = "=0.3.12", default-features = false, features = ["font-kit", "runtime_shaders"] }
```

`crates/ryne-ui/Cargo.toml` 增加 `gpui_platform.workspace = true`，
现有 `gpui` dev-dependency 的 `test-support` 保留。源码继续使用 `gpui::` 别名。
`lib.rs` 的 `Application::new()` 改为 `gpui_platform::application()`，保留资源注册、
文件激活、窗口初始化与关闭回调。平台 facade 按目标系统选择后端；Windows 后端开启自己的 manifest 支持。

已经实际复现：把旧 `runtime_shaders` 直接放到新核心包上，Cargo 返回 101。
该 feature 现由 platform → macOS → Apple 渲染实现传递。
`font-kit` 也必须传到 platform，不能依靠核心包中同名的空 feature。
macOS 平台源码明确在未开启字体 feature 时使用 `NoopTextSystem`，因此“能编译但无真实字形”必须作为验收失败。

初次解析保留原锁文件，只让 Cargo 调整受约束的依赖，不先运行全量 `cargo update`。
实际解析保留 `pulldown-cmark 0.13.6`、`unicode-segmentation 1.13.3`、`image 0.25.10`、
`math-core 0.9.0`、`formulary 0.3.0`、`merman 0.10.0` 和 `krilla 0.8.2`。
GPUI 引入 `usvg/resvg 0.46.0`，可与服务层 0.45.1 和 PDF 0.47.0 共存；
跨层传递 SVG 字节和像素，不把不同版本的树对象混用。布局依赖由 Taffy 0.9.0 变为解析到的 0.15.1。

原 `scripts/check-architecture.py` 只检查真实包名 `gpui`，会漏掉重命名后的新依赖。
正式适配已识别真实包名和 Cargo alias 中的 `gpui`、`gpui-` / `gpui_` 包族，
涵盖普通、构建和目标条件依赖；新增 `scripts/test-check-architecture.py` 检查允许与拒绝路径。

## 4. 逐项可行性矩阵

“静态可行”表示已对照调用点和发布源码找到实现路径；“待运行”不能解释为通过。
编译核验的最终结果单独记录在核验记录中。每一项在合入前都必须满足本表退出条件。

| 编号 | 项目和涉及位置 | 本轮核验结论 | 实施与放行条件 |
| --- | --- | --- | --- |
| G01 | 版本、feature、锁文件 | 直接替换失败已复现；拆分声明后解析成功 | 两个直接包精确锁定同版；核心及平台图只含所选 GPUI 分发；关键业务依赖不变；解释全部锁文件差异 |
| G02 | 应用启动 `ryne-ui/src/lib.rs` | `Application::new` 已移除；platform 提供 `application` 和 `with_platform` 通路 | 改启动入口，真实窗口显示文字、资源和菜单，启动失败沿用可报告错误路径 |
| G03 | Element 生命周期 `element.rs`、`layout_frame.rs` | request_layout / prepaint / paint 基本签名仍可对应 | 以编译诊断检查新 trait 要求；保留本帧几何、裁剪、控件代次、正文独立焦点和持续输入宿主 |
| G04 | 文字排版 `layout.rs`、`text_layout.rs`、`inline_layout_cache.rs`、`theme.rs` | `WrappedLine` 不再 Clone；ShapedLine 绘制增加对齐参数，wrap_line 增加缩进策略；临时适配通过编译 | 用 Arc 保留共享布局和缓存；显式保持旧续行缩进；真字体下比较行数、基线、折行和命中，不能扩大容差掩盖映射错误 |
| G05 | SVG 及公式 `paint.rs`、`assets.rs` | `paint_svg` 增加 `data: Option<&[u8]>` 参数 | 保留已有 AssetSource 路径时传 `None`；所有图标、两种数学字体、行内与块公式、暗亮主题、1×/2× 正常 |
| G06 | 图片及 Mermaid `paint.rs`、`assets.rs` | `paint_image` 增加 `image_bounds`，内部按可见区域裁剪 | 区分原图放置区域和可见裁剪区域；未另行裁剪时两个区域相同；滚动到部分可见位置不拉伸或错切图片 |
| G07 | IME 桥 `input.rs`、`sidebar/filter.rs` | 原必需回调仍存在；新增多项带默认实现的回调及原生状态通知 | 审查两个输入宿主；确认/取消/选区/候选几何无回归；原生粘贴不能绕过统一粘贴语义；新增通知按实际平台实现接入 |
| G08 | 焦点、鼠标、键盘和滚动 | focus/blur 与修饰键判断需 App；track_scroll 改借用；KeyDownEvent 增加字符优先标记；临时适配通过编译 | 焦点不绑定窗口根；借用 focus 时先 clone 句柄再修改 App；验证 AltGr、正文/侧栏切换、拖选、旧事件和滚动锚点 |
| G09 | 原生标题栏及菜单 `element.rs`、`menus.rs` | Menu 新增 disabled；窗口关闭观察者新增 WindowId；macOS/Windows 原控制通路仍存在；临时适配通过编译 | macOS 复核重设标题 workaround 是否仍需要；Windows 实机拖动、双击、最小化/还原/关闭及未保存确认 |
| G10 | 异步任务和生命周期 `files.rs`、`recovery`、图表调度 | 框架仍提供前台/后台执行器；调度实现和依赖有变化 | 保存排队、过期回执、取消、超时、草稿锁与关闭等待均跑已有回归；退出不遗留任务或吞错 |
| G11 | 结构编辑与持久化 | 业务层不导入 GPUI，关键版本可保留 | 全 workspace 测试；列表/引用/表格/代码/Alerts 与行内显隐通过真实窗口冒烟，源码字节和撤销边界不变 |
| G12 | 导出与服务资源 | 冻结导出、SVG/像素边界允许新旧渲染依赖并存 | 运行 PDF 与图片/公式/图表现有回归；检查打印和屏幕使用相同源码快照，导出不改变编辑状态 |
| G13 | 测试框架与架构守卫 | `gpui::test` 仍可用；绘制清理 clear 现在需要 App；测试目标临时适配通过编译，未执行测试 | 新旧 TestPlatform 都可能使用 NoopTextSystem；模拟分发测试不能冒充字体或原生平台验收；增加 alias 边界正反例 |
| G14 | macOS 构建和分发 | Apple 构建包包含 vendor/gpui 与 runtime shader 拼接路径 | 两种架构 debug/release 和 app bundle；校验 shader、ICNS、字体许可、签名、最低系统版本与实际启动 |
| G15 | Windows 构建和分发 | 发布源码有 Win32 后端和 release HLSL 编译；需要 Windows SDK/fxc | x64/x86/ARM64 按已有发布承诺构建；静态 CRT、图标、manifest、便携 ZIP、MSIXBundle、文件激活和实机 GUI |
| G16 | 性能及发布回退 | API 更换不自动解决全文布局成本；本轮不承诺性能提升 | 同语料同环境比较耗时、布局缓存、内存和包体；保留完整升级前提交和成对 Cargo 文件回退路径 |

### 4.1 输入与剪贴板的额外约束

新版 EntityInputHandler 增加默认 `paste`，会把纯文本交给 `replace_text_in_range`。
必须核查 Ryne 的 action 粘贴与原生菜单/平台粘贴两条入口，防止列表多行粘贴、IME 确认和单次撤销语义分叉。
不能仅靠所有 trait 方法都有默认值就宣布输入兼容。

`Window::notify_text_input_changed` / `notify_text_selection_changed` 需要焦点句柄，
文本或选区由应用改变后用于通知原生服务。0.3.12 的 PlatformWindow 提供默认空实现；
本轮在 macOS/Windows 实现中未找到覆盖，因此不能把新增通知说成已修复桌面 IME 的证据。
实施时核对实际 backend，再决定通知接入点；不要为了调用它而从功能层引入 Window。
几何变动、候选框定位及 focus 重新注册仍须在真实系统验证。

新增 UIKit 相关长度、选区矩形等默认方法不扩大本项目平台范围。
原有 UTF-16 转换、grapheme 边界和当前帧校验继续保留。

`KeyDownEvent::prefer_character_input` 用于 AltGr 等字符输入优先于快捷键的情况。
临时探针只在人工构造的快捷键事件中补 `false`；正式适配必须检查正文 `render_shortcut`
与侧栏 `keys` 是否尊重实际事件的字符优先意图，不得覆盖平台提供的标志。
`Modifiers::secondary(cx)` 按 App 的运行平台信息判断修饰键；模拟其他系统的测试需显式设置平台身份。

### 4.2 绘制和字体的额外约束

`paint_svg` 的路径仍参与资源身份。临时适配只补 `None`，不同时改成传裸 SVG、替换缓存键或绕过资源校验。
`paint_image` 新参数表达完整图像位置；不能把可见区域重复当成完整区域导致局部图片缩放。
新版本在绘制中使用边界像素对齐，必须检查窄单元格、公式基线、选区、2 px 光标和缩放后的命中。

`gpui::test` 验证模拟事件和逻辑；真实字体与 Metal 的证明来自真实窗口冒烟/人工检查。
两者应并存，不通过移除几何断言、跳过失败测试或关闭原生渲染来获得绿色结果。

### 4.3 编译探针确认的改动清单

首轮仅调整依赖后，`ryne-ui` library 报告 53 条错误和 3 条弃用警告。
这些是编译诊断数量，部分属于同一根因的级联错误，不是 53 个独立设计缺陷。
进一步编译测试目标暴露 `clear(cx)` / `blur(cx)` 调用点。

| 接口变化 | 已验证的最小适配 | 必须补充的行为验证 |
| --- | --- | --- |
| Application 初始化 | `gpui_platform::application()` | Metal 与真实字体、菜单和文件激活 |
| `Window::focus` / `FocusHandle::focus` / `blur` | 增加 App；实体借用中的 FocusHandle 先克隆到局部变量 | focus/blur 回调时序、IME 与窗口按钮 |
| `WrappedLine: !Clone` | `Line` / `TypesetLine` 持有 `Arc<WrappedLine>`，构造时包装 | 缓存复用、拥有分配账目、旧帧隔离；不每帧重新 shape |
| `ShapedLine::paint` / `paint_background` | 为已定位到正文坐标的片段传 `TextAlign::Left, None` | 列表标签、Alert 标题、过滤输入和公式混排；原 WrappedLine 的表格对齐参数保留 |
| `LineWrapper::wrap_line` | 显式 `IndentAdjustment::SameIndent`，与旧实现保留前导缩进的意图对应 | 公式混排、含前导空白和嵌套列表的折行差分 |
| SVG 与图片绘制 | SVG 增加 None；图片显式给出完整放置区域 | 裁剪、像素对齐、颜色和资源身份 |
| Menu / 窗口关闭观察者 | `disabled: false`；闭包接收未使用的 WindowId | 原生菜单 enabled、关闭等待和最后窗口退出 |
| 滚动接口 | `ScrollEvent`、`on_scroll`、借用 UniformListScrollHandle | 滚动方向/单位、侧栏虚拟行、弹窗阻止传播 |
| 修饰键及测试输入 | `secondary(cx)`、人工事件增加字符优先字段 | Windows AltGr 及跨平台测试身份 |
| 绘制后的清理 | `window.draw(cx).clear(cx)` | 测试实际执行、实体释放、关闭和后台任务收尾 |

探针补丁保存在核验目录，仅用于复现编译可行性。正式实现还需架构守卫、输入语义、
运行测试和平台验收；不能直接把该补丁当成可发布升级。

## 5. 分阶段实施计划

| 阶段 | 前置条件 | 具体工作 | 退出条件与产物 |
| --- | --- | --- | --- |
| U0 固定证据 | 当前分支 | 记录基线、环境、来源和 checksum；建立独立副本；复现直接替换失败；检查拆分依赖和 API | 本文与核验目录；每项标明静态/编译/运行/待平台验证，不把原型写成已交付 |
| U1 锁定平台依赖 | U0 | 修改两份 Cargo manifest、启动入口和架构守卫；保留业务依赖锁定；核查 feature 传递与同版包族 | `cargo check --workspace --all-targets --locked` 通过；架构检查识别别名；锁文件差异可解释 |
| U2 接口和绘制适配 | U1 | 按完整编译错误清单适配 Element、SVG/图片、事件和测试调用；只修改 UI 适配边界 | 生产及测试目标全部编译；图片裁剪和 SVG 资源身份有明确实现；无借机重写正文布局 |
| U3 输入及编辑回归 | U2 | 审查两个输入宿主、粘贴和通知；执行全文/局部源码、跨块选择、IME、撤销和保存回归 | workspace 测试、完整真实窗口冒烟、专项冒烟、失败冒烟通过；原生 IME 另留记录 |
| U4 平台分发 | U3 | macOS 两架构；Windows SDK/CRT/HLSL/图标；便携包和 MSIX；窗口控制及文件激活 | CI 构建日志及实际产物；macOS 和 Windows 桌面人工操作清单；无用户数据损失 |
| U5 性能和文档 | U3/U4 | 同机对照冷/热启动、输入、排版、资源、滚动、内存和包体；同步 AGENTS 和实际依赖说明 | 性能差异报告、剩余限制、回退演练；所有必需 gate 通过才合入 |

U1 与 U2 可以是一个可编译的最小提交，但验收证据分开记录。
U3 发现接口兼容但行为变化时，先定位 UI 适配或上游差异，不能修改 Markdown 数据语义来迁就框架。
Windows release 构建不能用 macOS 的 `cargo check` 替代；MSIX 打包不能用 ZIP 成功替代。

建议提交拆分：依赖/平台/守卫、UI API/绘制、输入/测试适配、构建分发、验证及版本文档。
每批保留可回退点；不推送发布、不更新正式发布版本、不更改用户文件和全局 Cargo 配置。

## 6. 验收矩阵

| 场景 | 操作和必须满足的结果 |
| --- | --- |
| 原始字节和结构 | LF/CRLF/CR、中文、emoji/ZWJ、组合字符；列表嵌套/引用跨段/围栏及缩进代码/表格；保存与 Undo/Redo 字节一致 |
| 行内及标题 | H1–H6/Setext 只点击展开；悬停不改源码；补全、结束标记越过、标记增删及代码字面输入正确 |
| 列表与引用 | Enter 续项/续引用、空项退出一层、Tab/Shift+Tab、嵌套前缀、条目菜单、任务勾选及多行粘贴 |
| 表格 | 对齐/折行、Tab/Shift+Tab/Enter 追加、Shift+Enter 的 br、行列菜单、窄窗及跨单元格选区边界 |
| 代码 | 围栏补全与显隐、字面 Enter、语言下拉/auto、高亮、源码模式；焦点不破坏块级呈现 |
| 资源 | 图片成功/失败/data/file/FTP、部分可见图片、Mermaid 成功/失败/超限、行内及块公式、两种数学字体；后台结果只更新匹配身份 |
| 光标和布局 | 自然文字插入位置、删除线与代码背景连续、500 ms 闪烁、折行 affinity、缩放、不同 DPI、字体回退及旧帧拒绝 |
| IME | macOS 拼音、Windows 微软拼音；正文与侧栏；中文/emoji、确认 Enter、取消、候选移动、拖选/滚动/失焦、保存/关闭确认 |
| 交互 | 原生及 action 粘贴、复制、快捷键、各层菜单、侧栏筛选/拖宽/大纲跳转、全屏/窄窗/主题/七语言切换 |
| 文件生命周期 | 新建/打开/重载/另存/最近文件/符号链接、保存旧版本回执、失败与取消、草稿恢复、重复实例激活及未保存关闭 |
| 导出 | 只读冻结快照、PDF 正文可选、表格/长代码跨页、公式与 Mermaid/图片；失败回退不丢源码 |
| 失败路径 | `--smoke-test-failure` 正常退出 1；GUI 回调失败不 panic 穿过 AppKit；资源与任务失败留可编辑内容 |

### 6.1 检查命令

适配完成后，在候选代码和锁文件上执行，不把当前 0.2.2 的结果填入新版栏：

```sh
python3 scripts/check-architecture.py
python3 scripts/test-check-architecture.py
cargo fmt --check
cargo check --locked --workspace --all-targets
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo run --locked -- --smoke-test
cargo run --locked -- --smoke-test-failure
```

最后一项预期退出码 1，其余预期 0。Clippy 必须与当前 stable Rust 同版，沿用 AGENTS 中的驱动排查说明。
完整冒烟之外，分别运行现有专项开关：`RYNE_SMOKE_IMAGES_ONLY`、`RYNE_SMOKE_BLOCK_MATH_ONLY`、
`RYNE_SMOKE_ALERTS_ONLY`、`RYNE_SMOKE_THEME_ONLY`、`RYNE_SMOKE_CODE_LANGUAGE_ONLY`、
`RYNE_SMOKE_EXPORT_MENU_ONLY`、`RYNE_SMOKE_BLOCK_TREE_ONLY`；每次只开一个并记录退出码。

macOS 在隔离 target/bundle 中运行 `sh scripts/bundle-macos.sh debug` 和 `release`，
不覆盖或终止可能有未保存内容的现有窗口。检查签名、Info.plist、ICNS、字体许可与包内可执行文件。
Windows 沿用 `.github/workflows/package.yml`、`build-msix.yml`、`windows-store.yml` 的实际矩阵；
本次不执行商店提交。macOS 12 最低版本声明必须单独核验，macOS 15/26 runner 构建成功不等价于最低系统可运行。

### 6.2 性能比较方法

基线和候选使用同一机器、构建 profile、字体、文档、窗口大小和资源缓存条件，先预热，至少各运行五次。
记录中位数及范围；样本不足以声明稳定 p95 时不报告 p95。
复用 `examples/architecture-perf.rs` 与 `RYNE_PROFILE=1`，覆盖短文、长代码、大列表、大表格、公式和图片。
区分 refresh、layout_frame、paint、布局构建/命中计数、首次启动和热资源绘制；另外测量进程峰值 RSS 与包体。
已有拥有分配账目不等于 RSS。新增渲染依赖导致包体或内存变化必须解释。

建议把同机中位耗时超过基线 10% 的变化作为调查信号；这不是已有产品 SLA，也不能用它替代正确性验收。
明确的卡顿、排版循环、无界缓存增长、错误命中或候选框错位均阻止合入，不能以总分平均抵消。

## 7. 放行和回退

本轮核验支持进入有边界的升级适配；平台或运行项未完成时，不宣布可发布。
合入门槛是所有生产/测试目标编译、核心和窗口回归、双平台分发与人工 IME/窗口验收均有结果，
且没有新增源码丢失、历史异常、错误几何或关闭丢数据问题。

如果上游行为缺陷无法在 UI 适配层合理解决，记录最小复现并保留 0.2.2；
如需上游补丁，单列来源、固定提交、维护责任与退出条件，不静默引入未审查的 git HEAD。
回退必须成组恢复 Cargo.toml、UI manifest、Cargo.lock、平台/绘制/测试适配，不能只退一个版本号。
本次不改变文档格式、配置格式和草稿格式，回退后仍应可打开同一份源码。
