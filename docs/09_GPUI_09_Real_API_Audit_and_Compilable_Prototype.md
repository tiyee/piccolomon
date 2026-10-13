# GPUI Live Markdown · 第九份

> 2026-10-12 Hybrid 修订：交互路线与阶段范围以[第十一份](./11_GPUI_Typora_Hybrid_View_Spec.md)为准。Inline 按光标显隐；Block 保持结构化，显式进入局部/全文源码；模式切换不改源码或历史。下文领域 API 为设计草案，须纳入 Hybrid presentation 版本依赖后实施。
# 真实 GPUI API 核验与最小可编译原型实施方案

**文档性质**：源码调查记录 + 实施设计 + 编译验收规范  
**版本**：0.1（2026-10-11）  
**项目**：Rust + 纯 GPUI + 自研编辑引擎的 macOS 单文件 Live Markdown 编辑器  
**设计原则**：架构正确性与原生体验优先；不采用 WebView，不把第三方输入框拼装成文档编辑器。  
**真实性声明**：本次已核查 Zed 官方仓库公开的 GPUI README、Cargo.toml、`examples/input.rs`、`examples/README.md` 和部分 Element/Accessibility 源码页面。**没有在本运行环境完成 macOS 上的 Cargo 编译、运行、IME 或 VoiceOver 测试，也没有锁定并下载一个可复现的完整 Git commit。**因此本文将“已从官方源码确认的接口”“仍待固定 commit 的接口”和“设计中的项目 API”严格区分。

---

## 目录

1. 本次实际核验的范围与证据等级
2. GPUI 版本策略和依赖锁定
3. 官方源码已确认的核心能力
4. 尚未证实的关键能力与阻断条件
5. 第九份与前八份的架构衔接
6. 最小可编译原型的范围定义
7. Workspace 目录与依赖 DAG
8. P0：官方 GPUI 基线运行
9. P1：最小自定义 Element
10. P2：输入事件、焦点与 IME 桥
11. P3：单行文本塑形与光标
12. P4：Rope 与事务内核接入
13. P5：Markdown Projection 最小闭环
14. P6：多行布局与命中测试
15. P7：虚拟化与滚动
16. P8：Accessibility 与 macOS 平台测试
17. `gpui-compat` 适配层契约
18. 版本一致性与主线程事件时序
19. 测试矩阵与自动化门禁
20. 性能基准、内存与可观测性
21. 风险清单、Fork 决策和 ADR
22. 可执行开发命令与诊断清单
23. 验收清单和失败处理
24. 下一步的实际执行顺序
25. 附录：官方资料、证据和示意代码

---

## 1. 核验范围与证据等级

### 1.1 证据等级

| 等级 | 含义 | 可以声称 |
|---|---|---|
| E0 | 设计推断、尚未查源码 | “计划使用”“需要验证” |
| E1 | 官方 README 或示例代码可见 | “官方示例展示了……” |
| E2 | 锁定 commit 后在完整源码中核对签名 | “在该 commit 中签名为……” |
| E3 | `cargo check` 在目标平台通过 | “此版本可编译” |
| E4 | macOS 实机交互测试通过 | “此环境下运行与交互已验证” |
| E5 | 多设备/系统回归与压力测试通过 | “满足项目发布门槛” |

**本次资料达到 E1，个别类型定义仅通过源码页面局部核对；不声称达到 E2–E5。**

### 1.2 本次查到的官方资料

- GPUI 官方 README：`https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md`
- 官方 Cargo.toml：`https://github.com/zed-industries/zed/blob/main/crates/gpui/Cargo.toml`
- 官方 examples README：`https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/README.md`
- 官方 `input.rs`：`https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/input.rs`
- 官方 `element.rs`：`https://github.com/zed-industries/zed/blob/main/crates/gpui/src/element.rs`
- 官方 accessibility 源码资料：`https://github.com/zed-industries/zed/blob/main/crates/gpui/src/_accessibility.rs`

这些 `main` 链接会随上游更新。工程仓库必须改用**完整 commit SHA 固定版本**，并记录核验日期、签名、编译结果。

### 1.3 核验记录模板

```yaml
api_audit:
  upstream: https://github.com/zed-industries/zed
  git_commit: "<40-char-full-sha>"
  rustc: "<rustc -V>"
  macos: "<sw_vers>"
  target: "aarch64-apple-darwin"
  api:
    - symbol: "gpui::Element"
      source_path: "crates/gpui/src/element.rs"
      status: "source-confirmed"
      compile_probe: "probe_custom_element"
      macos_probe: "probe_paint"
      owner: "rendering"
```

记录不可只写“已验证”，必须有源码定位与可重复的探针命令。

---

## 2. 版本策略与依赖锁定

### 2.1 已观察到的上游事实

官方 README 说明 GPUI 仍处于 pre-1.0 阶段，版本之间可能发生破坏性变更；官方 Cargo.toml 页面显示 `gpui` 包版本为 `0.2.2`，**但这不意味着该版本已被本项目编译或与任意 crates.io 发布包完全一致**。

官方文档示例使用：

```rust
use gpui::*;
use gpui_platform::application;

fn main() {
    application().run(|cx: &mut App| {
        // initialize app and window
    });
}
```

这只是**来自上游文档的 API 形态摘录**，不是本项目已编译的完整程序。

### 2.2 推荐固定策略

优先在工程中使用 Git 固定 commit：

```toml
# Cargo.toml：示意；<COMMIT> 必须替换为实际 SHA
[workspace.dependencies]
gpui = { git = "https://github.com/zed-industries/zed", rev = "<COMMIT>" }
gpui_platform = { git = "https://github.com/zed-industries/zed", rev = "<COMMIT>" }
```

**注意**：此配置仅表示预期方向。上游 workspace 依赖、平台 features、Rust toolchain 约束可能要求调整；必须以真实 `cargo metadata` 和 `cargo check` 验证。

必须提交：

- `Cargo.lock`。
- `rust-toolchain.toml`。
- `docs/upstream-gpui.md`：commit、上游仓库 URL、补丁列表。
- `docs/api-audit.md`：每个能力的源码定位和探针结果。
- `scripts/check-upstream.sh`：确认当前依赖与记录一致。

### 2.3 禁止做法

- `gpui = "*"` 进入正式生产代码。
- 随时跟踪 `main`，却不提交 lockfile。
- 在示例、文档和项目代码里混用不同年代的 `Application::new()` 与 `gpui_platform::application()` API。
- 用“网上某篇博客的代码”替代本地锁定版本源码。
- 通过 `#[allow(unused)]` 或跳过目标模块掩盖编译问题。

---

## 3. 官方源码已确认的能力

本节“确认”指官方公开源码页面展示相应代码（E1），不是项目已经运行。

### 3.1 Application 与窗口

官方 README 和示例展示：

- `gpui_platform::application()`。
- `Application::run(...)` 风格的应用入口。
- `App::open_window(...)`。
- `WindowOptions`、`WindowBounds`。
- `Render`、`Context`、`Entity` 等视图模型。

**项目推论**：足以设计一个原生 macOS 窗口作为 P0；具体初始化和平台 features 由编译探针确定。

### 3.2 Element 生命周期

官方 `input.rs` 展示自定义 `TextElement`，并实现：

```rust
impl IntoElement for TextElement { /* ... */ }
impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;
    // request_layout, prepaint, paint ...
}
```

可见 `request_layout`、`prepaint`、`paint` 的阶段划分。**完整 trait 签名必须从固定 commit 的 `element.rs` 复制核对**，不能将本文件中的伪代码直接当作实现。

### 3.3 原生输入接口

官方 `input.rs` 展示：

- `EntityInputHandler` 实现。
- `ElementInputHandler` 适配。
- `Window::handle_input(...)`。
- `UTF16Selection`。
- `selected_text_range(...)`。
- `text_for_range(...)`。
- `character_index_for_point(...)`。
- 与 marked range 有关的输入/绘制逻辑。

**关键推论**：GPUI 的官方输入示例已存在 UTF-16 与 UTF-8 转换逻辑。我们的项目必须用 Rope-aware 映射替代示例的简单字符串遍历，但不能跳过系统 UTF-16 约定。

### 3.4 文本塑形与几何

官方 `input.rs` 中可见：

```rust
let line = window
    .text_system()
    .shape_line(display_text, font_size, &runs, None);
let x = line.x_for_index(cursor);
let index = line.index_for_x(x);
```

这证明**官方示例至少展示了单行 shape、x_for_index、index_for_x 的调用方式**。

不代表已经证明：

- 多行复杂 Bidi 的全部 caret stops 都能公开访问。
- ligature 内所有 grapheme 边界都可准确定位。
- 同一 API 可直接用于百万字分段布局。
- 可跨线程调用 text system。
- macOS 的 Accessibility range geometry 已由此自动解决。

### 3.5 焦点与键盘

官方示例展示：

- `FocusHandle` / `Focusable`。
- `.track_focus(...)`。
- `.on_action(...)`。
- `KeyBinding::new(...)` 与 `App::bind_keys(...)`。
- 鼠标按下、抬起、移动事件注册。

**项目要求**：键盘输入最终转换为 `EditorCommand`，不允许在 GPUI View 内直接对字符串进行散乱修改。

### 3.6 视口虚拟化先例

官方 examples README 明确指向 `uniform_list` 作为虚拟列表例子。

这说明 GPUI 具备虚拟化相关示例，**但 Markdown 文档的动态高度与单个超长 Block 虚拟化仍须自建 HeightIndex 和 VisualLine 虚拟化**。不能直接把 `uniform_list` 当作完成的编辑器布局系统。

### 3.7 测试框架先例

官方 examples README 提到 `#[gpui::test]`、`TestAppContext`、focus/actions/window 测试示例。具体可用 API 与 feature 配置仍需固定 commit 后验证。

### 3.8 Accessibility 的现状

官方仓库存在专门的 accessibility 相关源码与文档，包含 `Role::Document`、`text!` ID 约束等示例。

**这不足以证明**完整的 macOS 文本编辑 Accessibility API（例如 selected text range、range bounds、VoiceOver 光标同步）均已满足。该项仍属于高风险 P8 探针。

---

## 4. 尚未证实的关键能力

| ID | 能力 | 当前证据 | 需要的验证 | 不通过的处理 |
|---|---|---|---|---|
| A01 | macOS 窗口启动 | E1 | E3/E4 | 修正依赖或平台入口 |
| A02 | 自定义 Element 绘制 | E1 | E3/E4 | 修正 Element 生命周期 |
| A03 | 单行塑形/HitTest | E1 | E3/E4 | GPUI API 适配 |
| A04 | 多行 shaping + Bidi | E0–E1 | 源码 + 实机 | 扩展布局层 |
| A05 | ligature 内 caret | E0 | 字体黄金测试 | 扩展 shaper |
| A06 | ZWJ emoji 边界 | E0 | Unicode 真实测试 | cluster 修正 |
| A07 | UTF-16 IME range | E1 | 中日韩实机 | 自研桥接 |
| A08 | IME candidate rect | E1（示例相关几何） | 实机录屏与断言 | 平台桥增强 |
| A09 | 动态高度虚拟化 | E0 | 10MB 文档 | 自研 HeightIndex |
| A10 | 单 Block 行级虚拟化 | E0 | 1MB 单段落 | 分段布局 |
| A11 | VoiceOver 语义 | 部分 E1 | 实机 | Accessibility 扩展 |
| A12 | AX text range bounds | E0 | 原生 API 测试 | macOS adapter |
| A13 | 纯数据后台 shaping | E0 | 线程安全审计 | 主线程分批 |
| A14 | 剪贴板格式 | 部分 E1 | macOS 验证 | clipboard adapter |
| A15 | 文本系统跨 DPI | E0 | 双显示器 | transform adapter |

**硬阻断**：A04、A07、A08、A11、A12 如无法满足，不能声称“原生编辑体验已经完成”。

---

## 5. 与前八份文档的接口衔接

```text
第一份：整体架构与产品约束
第二份：Projection / DisplayMap / Layout
第三份：Document / Anchor / Transaction / IME
第四份：Markdown Syntax Engine
第五份：GPUI Native Rendering / Viewport / AX
第六份：工程治理 / CI / 探针
第七份：核心算法 / 数据结构
第八份：编辑行为规范 / CT 验收
                         ↓
第九份：把上述约束绑定到真实 GPUI 源码、编译探针和可运行原型
```

第九份不能推翻既有不变量：

- 一份 `.md`、一条逻辑 Rope。
- 视觉隐藏不改变源文本。
- 所有交互使用 Source Anchor。
- Projection、Layout、Render 版本一致。
- IME composition overlay 不污染已提交文档。
- Inline 光标显隐与 Block 稳定结构化分策；局部/全文源码模式共享同一 Document，详见第十一份。
- 各 Block 同基础字号与行高，行盒容纳真实字体度量。
- 纯 GPUI 原生 UI，不使用 WebView。

---

## 6. 最小可编译原型的范围

### 6.1 最小原型不等于完整产品

最小原型应验证一条端到端路径：

```text
打开原生窗口
  → 自定义 Editor Element
  → 聚焦、键盘/IME 输入
  → Document.apply()
  → Syntax / Projection
  → Shaping / Layout
  → Paint
  → 鼠标点击回到 Source Anchor
  → Undo/Redo
```

最小原型保留 Strong 投影闭环；H1 另须验证标题、列表、引用、代码块、任务与源码模式，H2 验证表格。不能以 Strong 探针代替完整 Hybrid 产品验收。参见[第十一份](./11_GPUI_Typora_Hybrid_View_Spec.md)。

### 6.2 MVP 验收输入

```markdown
你好 **GPUI** world
这是 *Live Markdown* 编辑器。
`code` 与 [link](https://example.com)
```

需要验证：

- 光标离开 Strong 时 `**` 隐藏。
- 光标进入 Strong 时 `**` 展开。
- 原始 Rope 字节不变。
- 点击前后 Source Anchor 不跳。
- 中文输入法组合输入能正确提交。
- Backspace/Undo 不破坏 Markdown 源文本。
- Window resize 后正确换行。
- 保存时输出完整 Markdown，而不是显示文本。

### 6.3 不应在最小原型中伪造

- “先用透明 `**` 代替 Projection”。
- “先用普通 TextInput 接管全文，后续再换”。
- “先跳过 Unicode，把一个 char 当一个字形”。
- “先用 HTML 预览当编辑器”。
- “先把 IME composition 写进 Rope 再撤销”。

可以先支持小范围语法，但数据模型必须保持最终架构一致。

---

## 7. Workspace 结构与依赖 DAG

```text
live-markdown/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── .cargo/config.toml
├── crates/
│   ├── editor-types/
│   ├── editor-document/
│   ├── editor-syntax/
│   ├── editor-projection/
│   ├── editor-layout/
│   ├── editor-engine/
│   ├── gpui-compat/
│   ├── editor-gpui/
│   ├── editor-file/
│   ├── editor-testkit/
│   └── app-macos/
├── probes/
│   ├── p0-window/
│   ├── p1-element/
│   ├── p2-input/
│   ├── p3-shaping/
│   ├── p4-ime/
│   ├── p5-accessibility/
│   └── p6-virtualization/
├── tests/
│   ├── fixtures/
│   ├── golden/
│   └── conformance/
├── docs/
│   ├── api-audit.md
│   ├── adr/
│   └── upstream-gpui.md
└── scripts/
    ├── check-upstream.sh
    ├── check-architecture.sh
    └── run-conformance.sh
```

### 7.1 依赖方向

```text
editor-types
    ↑
editor-document       editor-syntax
    ↑                       ↑
editor-engine  ← editor-projection
    ↑                       ↑
    └──────── editor-layout
                  ↑
gpui-compat ──► editor-gpui
                  ↑
               app-macos
```

此图为概念示意；**实际 Cargo DAG 应保证无环**。更严格的分层：

- `editor-types` 不依赖 GPUI。
- `editor-document` 不依赖 GPUI / Layout。
- `editor-syntax` 不依赖 GPUI / View。
- `editor-projection` 不依赖 GPUI。
- `editor-layout` 优先依赖抽象 `TextShaper` trait，不依赖具体 GPUI。
- `gpui-compat` 是唯一直接接触 GPUI 底层的主要适配 crate。
- `editor-gpui` 依赖 `gpui-compat`、engine 与 layout 的公共 API。
- `app-macos` 只做组合、窗口、菜单和文件生命周期。

### 7.2 Cargo 依赖示意

```toml
[workspace]
resolver = "2"
members = [
  "crates/editor-types",
  "crates/editor-document",
  "crates/editor-syntax",
  "crates/editor-projection",
  "crates/editor-layout",
  "crates/editor-engine",
  "crates/gpui-compat",
  "crates/editor-gpui",
  "crates/editor-file",
  "crates/editor-testkit",
  "crates/app-macos",
]

[workspace.package]
edition = "2024"

[workspace.dependencies]
# 必须替换占位符，并以锁定 commit 的实际编译结果为准
gpui = { git = "https://github.com/zed-industries/zed", rev = "<COMMIT>" }
gpui_platform = { git = "https://github.com/zed-industries/zed", rev = "<COMMIT>" }
```

**注意**：不要直接复制这个片段运行；它是带占位符的配置规格。完成 E2 核验后才能发布“可编译模板”。

---

## 8. P0：官方 GPUI 基线运行

### 8.1 目标

先证明目标 Mac 的编译器、GPU/Window backend、字体和 GPUI 版本能够正常启动。

### 8.2 官方命令

官方 examples README 提供：

```bash
git clone https://github.com/zed-industries/zed.git
cd zed
git checkout <PINNED_COMMIT>
cargo run -p gpui --example hello_world
```

**这是待执行的验收命令，不是本次已执行结果。**

### 8.3 通过标准

- `cargo run` 成功退出编译阶段。
- 原生窗口打开。
- 窗口能正常移动、缩放、关闭。
- Retina 下文字无异常。
- 日志记录 `rustc -V`、`sw_vers`、commit SHA。
- Intel/Apple Silicon 至少覆盖目标发布架构。

### 8.4 失败分诊

| 失败 | 首先检查 |
|---|---|
| Rust toolchain 版本错误 | 仓库 `rust-toolchain.toml` |
| Cargo feature 冲突 | gpui/gpui_platform features |
| 链接失败 | Xcode Command Line Tools |
| 启动黑屏 | 平台 backend / GPU |
| 字体无法加载 | 示例字体资源 |
| 仅 Debug 失败 | 日志、符号和资源路径 |

P0 未通过时，不进入自研编辑器 Element 开发。

---

## 9. P1：自定义 Element

### 9.1 目标

验证一个真正自定义的 Editor Element，而非 `div().child(text)` 的包装。

### 9.2 探针功能

- 固定 400×240 逻辑像素编辑区域。
- 绘制背景、文本和一条 caret。
- 接收 MouseDown。
- 点击时输出局部坐标。
- Window resize 时重新布局。
- 无 Rope、无 Markdown，仅测 GPUI Element 生命周期。

### 9.3 生命周期

```text
request_layout
    ↓
prepaint (layout geometry / hit-test preparation)
    ↓
paint (draw and input registration)
```

准确的 trait 方法参数以锁定版本源码为准。

### 9.4 测试

- 元素 bounds 与 hit-test bounds 一致。
- 鼠标点击在视口边界内/外行为确定。
- 只改变 caret blink 不重新塑形。
- 资源释放后无 Entity 泄漏。

### 9.5 交付

`probes/p1-element` + `docs/api-audit.md` 的 E3/E4 证据。

---

## 10. P2：焦点、输入与 IME

### 10.1 官方示例的可借鉴部分

`examples/input.rs` 提供焦点、键盘 action、EntityInputHandler、UTF16Selection、marked text、字符位置查询的示例。

应借鉴其**接口连接方式**，不能复制其单字符串编辑模型作为正式 Document。

### 10.2 项目内适配

```rust
pub trait PlatformTextInputBridge {
    fn selected_utf16_range(&self) -> Utf16Range;
    fn text_for_utf16_range(&self, range: Utf16Range) -> Option<String>;
    fn replace_text(&mut self, replacement: PlatformTextReplacement);
    fn set_marked_text(&mut self, update: MarkedTextUpdate);
    fn unmark_text(&mut self);
    fn first_rect_for_range(&self, range: Utf16Range) -> Option<ScreenRect>;
    fn character_index_for_point(&self, point: ScreenPoint) -> Option<Utf16Offset>;
}
```

**全部为项目内部设计接口**，不是 GPUI 现成 trait。

### 10.3 坐标空间

```text
Platform UTF-16
  ⇄ composition-aware editable text
  ⇄ Source Anchor / Overlay
  ⇄ Projection Display
  ⇄ Layout geometry
  ⇄ Window/Screen
```

### 10.4 IME 状态机

```text
Idle
  → Marked(text, selection)
  → Marked(updated text, selection)
  → Committed(text)
  → Idle
```

组合文本只存在 overlay，commit 才形成 Document Transaction。必须验证真实 macOS 输入法是否要求在组合期间对平台报告一个“包含 marked text 的临时文本视图”。

### 10.5 必测

- 拼音候选选择与取消。
- 日文假名转汉字。
- 韩文组合。
- emoji 选择器。
- 组合中移动光标、滚动、失焦。
- 组合中 Undo、保存、外部文件变化。
- surrogate pair 的 UTF-16 长度。
- 候选框跟随软换行和窗口移动。

---

## 11. P3：Shaping 与单行 Caret

### 11.1 探针

展示：

```text
abc fi é 中文 👨‍👩‍👧‍👦 العربية 123
```

**注意**：`é` 包含组合音标；家庭 emoji 包含 ZWJ 序列。

### 11.2 需要记录的数据

- UTF-8 byte offsets。
- Grapheme boundaries。
- `x_for_index` 输出。
- `index_for_x` 输出。
- 行基线与 glyph bounds。
- 每个 visual caret stop。
- Bidi level 与 affinity（若底层可得）。

### 11.3 阻断点

如果 `ShapedLine` 仅支持有限的 `x_for_index` / `index_for_x`，但无法提供完整的 Bidi 双亲和性和 ligature 内 caret stops，应考虑：

1. 查阅 GPUI 内部文本布局结构。
2. 在 `gpui-compat` 暴露必要 geometry。
3. 若需修改私有实现，建立受控 GPUI fork。
4. 以 Unicode golden 验证扩展正确性。

**不能通过 `text.len()`、平均字符宽度或字符数近似。**

---

## 12. P4：Rope 与 Transaction 接入

### 12.1 最小 Document API

```rust
pub struct DocumentSnapshot {
    pub revision: DocumentRevision,
    pub text: Arc<RopeSnapshot>,
}

pub struct EditTransaction {
    pub changes: Vec<SourceEdit>,
    pub selection_before: SelectionSet,
    pub selection_after: SelectionSet,
}

pub struct AppliedEdit {
    pub before: DocumentRevision,
    pub after: DocumentRevision,
    pub change_map: ChangeMap,
}
```

项目设计 API；Rope 类型需在第三份和第七份约束下实现。

### 12.2 最小闭环

- 输入 `a`：Source Rope 增加 `a`。
- Undo：恢复源字节。
- Redo：重新应用变更。
- 点击：Caret Anchor 可转换到 SourceByte。
- 编辑不直接调用 GPUI 文字组件内部状态修改。

### 12.3 不变量

```text
RenderState = f(DocumentSnapshot, SyntaxSnapshot, ProjectionSnapshot, LayoutSnapshot)
```

**禁止**：

```text
GPUI TextElement owns String
AND
Document owns Rope
```

否则出现两个权威文本。

---

## 13. P5：Markdown Projection 最小闭环

### 13.1 先验证一个语法

输入：

```markdown
abc **hello** xyz
```

**离开 Strong**：

```text
abc hello xyz
```

**进入 Strong**：

```text
abc **hello** xyz
```

### 13.2 需要测量

- SourceByte 与 DisplayByte 映射。
- 隐藏 delimiter 的 zero-width 多对一边界。
- Source Anchor + Bias / Affinity。
- 进入/离开 Strong 的 Layout invalidation。
- 光标展开后原 Source Anchor 保持不变。
- Undo 只针对实际 Document Transaction。

### 13.3 可执行属性

```text
Given: source = "abc **hello** xyz"
When: selection moves outside Strong
Then: display = "abc hello xyz"
And: document source unchanged
And: document revision unchanged
And: undo depth unchanged
```

### 13.4 安全回退

如果 marker 范围无法被权威 Syntax Engine 精确验证，则显示完整源码。不能隐藏推测位置。

---

## 14. P6：多行、软换行与 HitTest

### 14.1 最小场景

```markdown
第一行有一些 **粗体** 和英文 words。
第二行包含 [link](https://example.com)。
第三行含有 👨‍👩‍👧‍👦 和 العربية。
```

在不同宽度下进行 wrap。

### 14.2 Geometry

```rust
pub struct VisualLine {
    pub display_range: DisplayRange,
    pub top: LayoutPx,
    pub baseline: LayoutPx,
    pub bottom: LayoutPx,
    pub caret_stops: Vec<CaretStop>,
}
```

### 14.3 命中测试闭环

```text
Source Anchor
  → Projection
  → Shaped CaretStop
  → WindowPoint
  → HitTest
  → equivalent Source Anchor + Affinity
```

### 14.4 核心 bug 回归

鼠标点击隐藏语法边界后：

1. 用旧布局计算 Source Anchor。
2. 更新 Selection。
3. 更新 Projection。
4. 重新布局。
5. 从同一 Source Anchor 画新光标。

**禁止在第 4 步后重新根据旧鼠标像素计算 Source Anchor。**

---

## 15. P7：虚拟化与滚动

### 15.1 最小目标

- 10 万个短 Block。
- 1 MB 单 Block。
- 可变窗口宽度。
- 快速滚动。
- 随机插入/删除 Block。
- Source-backed ScrollAnchor。

### 15.2 数据结构

- Block 高度索引：动态聚合 B-tree。
- BlockLayoutCache：按 Projection / Typography / Width 键缓存。
- VisibleRange：Viewport + overscan。
- 单 Block 内 VisualLine chunk index。

### 15.3 验收

- 首屏无需全文 shaping。
- 随机 `scroll_y` 能 O(log N) 找 Block。
- 上方高度变化后阅读锚点稳定。
- 当前光标 Block 优先布局。
- 过期异步结果不覆盖新布局。
- 内存增长有上界或明确缓存预算。

---

## 16. P8：Accessibility

### 16.1 需要确认的接口

- GPUI 对 macOS Accessibility tree 的构建方式。
- Role / label / value / selected text。
- Text range → screen bounds。
- Screen point → character index。
- VoiceOver 对自定义文本编辑器的读取与编辑。
- 虚拟化文档中离屏 range 的按需几何。
- 焦点、选区与 AX 通知时序。

### 16.2 文本模型冲突

Live Markdown 有两套文本：

```text
Source:      "abc **bold**"
Presentation:"abc bold"
```

平台文本输入与 Accessibility 可能使用 UTF-16 ranges；不能把一套 range 直接套到另一套字符串。

建议在原型中先做**双模型实验**：

- Experiment AX-S：以完整 Source 作为 editable text value。
- Experiment AX-P：以 Presentation 作为可读 text value，通过映射实现编辑。

通过真实 VoiceOver 和 IME 选区行为比较，决定对外文本模型；该决策应形成 ADR，不要凭直觉。

### 16.3 阻断标准

如果不能稳定完成选区读取、范围定位和 VoiceOver 逐字导航，则 Accessibility 仍未达标，即使视觉编辑器看起来正常。

---

## 17. `gpui-compat` 适配层

### 17.1 为什么单独隔离

GPUI 仍在快速迭代。项目核心算法不能被具体版本的 `Window`、`App`、`Element` 签名污染。

### 17.2 内部接口

```rust
pub trait NativeShaper {
    fn shape(&self, request: ShapeRequest) -> Result<ShapedParagraph, ShapeError>;
}

pub trait NativePaintBackend {
    fn paint(&mut self, commands: &[PaintCommand]) -> Result<(), PaintError>;
}

pub trait NativeInputBackend {
    fn install_input_handler(&mut self, handler: EditorInputHandle);
}

pub trait NativeAccessibilityBackend {
    fn publish(&mut self, snapshot: &AccessibleSnapshot);
}
```

以上为项目接口草案，不能直接当作 GPUI API。

### 17.3 适配层不得承担

- Markdown 语法解析。
- Source Anchor 变换。
- Undo/Redo。
- 文件保存。
- 全局 Projection 映射。

### 17.4 版本升级协议

```text
Upstream GPUI bump
  → update pinned SHA
  → cargo check all probes
  → run GPUI integration tests
  → run IME/AX macOS tests
  → compare rendering geometry
  → performance regression
  → update api-audit and ADR
```

---

## 18. 版本一致性与事件时序

### 18.1 统一快照键

```rust
pub struct EditorFrameKey {
    pub document_revision: DocumentRevision,
    pub syntax_revision: u64,
    pub projection_revision: u64,
    pub hybrid_presentation_revision: u64,
    pub composition_revision: u64,
    pub layout_revision: u64,
    pub typography_revision: u64,
    pub viewport_revision: u64,
}
```

### 18.2 输入时序

```text
macOS event
  → GPUI InputHandler
  → EditorCommand
  → Document Transaction
  → SyntaxSnapshot
  → Hybrid policies / FlowBlocks
  → ProjectionSnapshot
  → LayoutSnapshot
  → RenderSnapshot
  → GPUI paint
```

### 18.3 防止撕裂帧

不能：

```text
text from revision 15
caret from revision 14
selection from revision 13
```

必须：

```text
RenderSnapshot {
  document: revision 15,
  projection: derived-from-15,
  layout: derived-from-projection-15,
  caret: derived-from-layout-15,
}
```

### 18.4 任务调度

- 输入与 Selection 状态在 UI 线程序列化。
- 后台工作只处理不可变快照。
- 结果提交前校验版本链。
- IME composition 有独立 overlay generation。
- Blink 只修改 paint 状态，不修改文档版本。

---

## 19. 测试与 CI 门禁

### 19.1 单元测试

- SourceByte/DisplayByte 强类型。
- ChangeMap、Anchor Bias。
- Undo/Redo 与事务合并。
- Projection 区间与映射。
- UTF-8 ↔ UTF-16。
- Grapheme 与 Unicode word boundary。
- HeightIndex 前缀和。

### 19.2 属性测试

```text
random edit sequence
  → incremental result
  == full recomputation result
```

关键对象：

- AnchorStore。
- SyntaxEngine。
- Projection Map。
- CaretStop Graph。
- HeightIndex。
- Undo/Redo。
- File round-trip。

### 19.3 GPUI 探针 CI

| 探针 | Linux CI | macOS CI | macOS 实机人工 |
|---|---|---|---|
| P0 Window | 可选 | 必须 | 必须 |
| P1 Element | 部分 | 必须 | 必须 |
| P2 Input/IME | Mock | 编译 | 中日韩必须 |
| P3 Shaping | 可选 | 必须 | Unicode/Bidi |
| P4 Rope | 必须 | 必须 | 不需要 |
| P5 Projection | 必须 | 必须 | 必须 |
| P6 Layout | 必须 | 必须 | 必须 |
| P7 Virtualization | 必须 | 必须 | 压力 |
| P8 Accessibility | 不适用 | 部分自动 | VoiceOver 必须 |

### 19.4 构建检查

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check --workspace
```

真实工程可能因上游 features、平台条件需要分组命令。所有例外必须写入 CI 配置和 ADR。

### 19.5 用户行为验收

第八份的 CT-001～CT-084 应进入 conformance runner，标记：

```yaml
case_id: CT-001
layer: projection
platform: cross-platform
automation: property
status: pending
```

**不要把“测试用例已写”误报为“用例已通过”。**

---

## 20. 性能基准

### 20.1 负载

| 场景 | 输入 | 核心指标 |
|---|---|---|
| 小文件 | 10 KB | 键入到一致帧 |
| 中等文件 | 1 MB | 编辑/滚动 P95 |
| 大文件 | 10 MB | 首屏时间、内存 |
| 多 Block | 100k Block | HeightIndex / scroll |
| 长 Block | 1 MB 单段落 | 分段 shaping |
| 混合文本 | CJK/RTL/emoji | caret geometry |
| 语法频繁展开 | `**`/`*`/link | 局部重排 |
| IME 连续组合 | 中文/日文/韩文 | 候选框、输入延迟 |
| Resize | 窄→宽→窄 | wrap cache、scroll anchor |
| 外部文件修改 | 保存前冲突 | FileSession 正确性 |

### 20.2 观测事件

```rust
pub enum EditorTraceEvent {
    TransactionApplied,
    SyntaxParseStarted,
    SyntaxParseFinished,
    ProjectionUpdated,
    LayoutStarted,
    LayoutFinished,
    RenderSubmitted,
    HitTestCompleted,
    ImeCompositionUpdated,
    AccessibilityQuery,
}
```

每条记录携带 revision、BlockId、耗时、是否 cache hit。

### 20.3 目标原则

- 优先测 P95/P99 而非平均值。
- 用稳定 macOS 硬件建立基线。
- 不在没有实测时承诺具体毫秒值。
- 正确性失败是发布阻断，不因性能漂亮而放行。
- 仅 caret blink 不应触发全文 parse/shaping。

---

## 21. 风险与 Fork 决策

### 21.1 关键风险

| 风险 | 严重度 | 缓解 |
|---|---|---|
| GPUI API 快速变化 | 高 | Pin commit + compat |
| Bidi caret 信息不足 | 极高 | 扩展 shaper / Fork |
| IME marked range 错误 | 极高 | macOS 实机探针 |
| Accessibility 文本模型冲突 | 极高 | AX-S/AX-P 对比实验 |
| 长 Block 全量 shaping | 高 | VisualLine 虚拟化 |
| 异步布局撕裂帧 | 高 | 版本化快照 |
| GPUI 上游 fork 维护成本 | 中高 | 最小补丁 + upstream |
| 字体 fallback 行盒错位 | 高 | Unicode golden |
| macOS 文件原子保存 | 高 | FileSession 独立测试 |

### 21.2 Fork 判定

符合任一条件时可以 Fork GPUI：

1. 公开 API 无法获取精确 caret/cluster 几何。
2. IME 事件或 candidate rect 无法正确接入。
3. macOS Accessibility 缺失关键 text range 能力。
4. Element 生命周期无法保证项目要求的绘制一致性。
5. 必须访问 GPUI 内部文本布局状态才能实现正确性。

Fork 前必须：

- 编写最小复现。
- 定位具体源码缺口。
- 比较扩展公开 API 与 Fork 的维护成本。
- 记录 ADR。
- 尽量向上游贡献可复用接口。
- 保证补丁不侵入 Document/Projection 核心层。

### 21.3 ADR 清单

```text
ADR-001 GPUI upstream pin strategy
ADR-002 gpui-compat boundary
ADR-003 authoritative text shaper
ADR-004 source vs presentation accessibility model
ADR-005 IME overlay platform contract
ADR-006 bidi caret affinity model
ADR-007 long-block virtualization
ADR-008 GPUI fork policy
ADR-009 document newline preservation
ADR-010 renderer snapshot atomicity
```

---

## 22. 开发命令与诊断清单

### 22.1 环境收集

```bash
uname -a
sw_vers
xcode-select -p
rustc -Vv
cargo -V
git --version
```

### 22.2 上游固定

```bash
git clone https://github.com/zed-industries/zed.git
cd zed
git rev-parse HEAD
git checkout <PINNED_COMMIT>
git rev-parse HEAD
```

**重要**：`<PINNED_COMMIT>` 是占位符。尚未选择并验证实际 SHA，不能声称工程版本已经固定。

### 22.3 基线

```bash
cargo run -p gpui --example hello_world
cargo run -p gpui --example input
```

以上是官方仓库示例的候选执行命令，需以锁定 commit 的 examples 清单为准。

### 22.4 源码检索

```bash
rg "trait Element" crates/gpui/src
rg "trait EntityInputHandler" crates/gpui/src
rg "struct ShapedLine" crates/gpui/src
rg "shape_line" crates/gpui/src
rg "handle_input" crates/gpui/src
rg "accessibility" crates/gpui/src
```

检索后必须阅读**定义、实现和调用点**，不能只看命中行就认定接口语义。

### 22.5 诊断记录

每个探针提交：

```text
probe:
  name
  commit
  rustc
  target
  command
  compile_result
  runtime_result
  screenshot_or_video
  known_limitations
  owner
  next_action
```

---

## 23. 验收清单

### 23.1 P0 / P1

- [ ] 固定 GPUI commit。
- [ ] 官方 hello_world 在 macOS 运行。
- [ ] 自定义 Element 可编译。
- [ ] 文本绘制和鼠标事件通过。

### 23.2 P2 / P3

- [ ] 输入 Handler 与焦点正确。
- [ ] UTF-16/UTF-8 range 转换正确。
- [ ] 中文、日文、韩文 IME 实测通过。
- [ ] Bidi、emoji、ligature caret 几何通过。
- [ ] 候选框跟随 caret。

### 23.3 P4 / P5 / P6

- [ ] 单一 Rope，无第二份权威字符串。
- [ ] Transaction/Undo/Redo 正确。
- [ ] Strong marker 精确显隐。
- [ ] 光标显隐后 Source Anchor 不变。
- [ ] 多行 soft wrap / HitTest 正确。
- [ ] 同一帧 RenderSnapshot 版本一致。

### 23.4 P7 / P8

- [ ] 大文件首屏无需全文 shaping。
- [ ] 长 Block 分段布局。
- [ ] HeightIndex 更新与滚动锚定正确。
- [ ] VoiceOver 文本和选区正确。
- [ ] Accessibility range bounds 正确。

### 23.5 发布阻断

- [ ] 无已知 Source 数据丢失路径。
- [ ] 无错误语法隐藏导致不可编辑。
- [ ] 无 IME composition 破坏事务。
- [ ] 无过期 Layout 决定新 Source edit。
- [ ] 无严重 Accessibility 导航错误。
- [ ] 无无界缓存增长。

**所有方框默认未通过；只有附带测试证据才能勾选。**

---

## 24. 建议的实际执行顺序

**第一步：不要立即写完整编辑器。** 在目标 Mac 上锁定 GPUI commit，运行官方 hello_world 与 input 示例。

**第二步：做最危险的三个探针**：

1. Bidi / ligature / grapheme 的精确 caret geometry。
2. 中日韩 IME composition + candidate rect。
3. VoiceOver + AX selected text range / range bounds。

原因：这三项最可能迫使 GPUI 扩展或 Fork，应该在大量编辑器 UI 代码产生之前确认。

**第三步：完成 P1 自定义 Element + P4 Document 最小内核**，保持两个模块的边界清晰。

**第四步：接入最小 Markdown Strong Projection**，打通 Source → Display → Shaping → HitTest → Source。

**第五步：扩展多行与 Inline；接入 BlockPresentationPolicy、结构化标题/列表/引用/代码/任务、局部与全文源码模式；随后接入 H2 表格及虚拟化与滚动锚定。**

**第六步：把第八份 CT 验收集接入 CI**，在每次 GPUI 升级时运行完整回归。

---

## 25. 附录：源码证据与实现注意事项

### 25.1 官方源码核验摘要

| 来源 | 观察结果 | 证据等级 |
|---|---|---|
| `crates/gpui/README.md` | GPUI pre-1.0；Application、窗口与平台入口 | E1 |
| `crates/gpui/Cargo.toml` | 当前页面 gpui 包版本 `0.2.2` | E1 |
| `examples/README.md` | hello_world、input、uniform_list、testing | E1 |
| `examples/input.rs` | EntityInputHandler、ElementInputHandler、UTF16Selection | E1 |
| `examples/input.rs` | ShapedLine、shape_line、x_for_index、index_for_x | E1 |
| `src/element.rs` | Element 生命周期源码位置 | E1 |
| `src/_accessibility.rs` | Accessibility 相关模型和 ID 约束 | E1 |

### 25.2 示例代码的局限

官方 `input.rs` 是输入 API 示范，不是成熟 Markdown 编辑器内核。它使用单一字符串和单行 `ShapedLine`，不能直接替代：

- Rope 与增量 ChangeMap。
- Source/Display Projection。
- 多行 Bidi caret graph。
- Block/VisualLine virtualization。
- 文件安全保存。
- Accessibility 全文范围模型。

### 25.3 参考资料

1. GPUI 官方 README  
   https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md
2. GPUI 官方 Cargo.toml  
   https://github.com/zed-industries/zed/blob/main/crates/gpui/Cargo.toml
3. GPUI 官方示例索引  
   https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/README.md
4. GPUI 官方 Input 示例  
   https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/input.rs
5. GPUI Element 源码  
   https://github.com/zed-industries/zed/blob/main/crates/gpui/src/element.rs
6. GPUI Accessibility 源码  
   https://github.com/zed-industries/zed/blob/main/crates/gpui/src/_accessibility.rs

---

## 结语

第九份的结论不是“GPUI 已经具备全部编辑器能力”，而是：

**GPUI 官方示例已经提供了窗口、自定义 Element、文本输入、单行 shaping 与基础交互的可核查起点；真正决定项目能否达到 Typora 风格 Hybrid View 的原生体验的，是复杂文本几何、macOS IME 和 Accessibility 的实机验证。**

工程实施应以**锁定版本 → 源码核验 → 编译探针 → macOS 实机测试 → 正式模块接入**为不可跳过的链路。

只有这样，前八份设计中的正确性约束才不会在实际编码时被框架细节悄悄削弱。
