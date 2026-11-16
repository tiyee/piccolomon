# GPUI pre 0.3.12 正式升级记录

日期：2026-11-16。分支：`codex/gpui-pre`。升级前基线：
`bf8ebd5ef9271f599f56747e2b0c1c5ac626d9a7`。

用户授权正式升级后，生产工作区已从 `gpui 0.2.2` 切换为精确锁定的
`gpui-pre` / `gpui-pre-platform 0.3.12`。本记录描述实际实现与本机验证，
[主计划](../../../50_GPUI_Pre_Upgrade_Plan.md) 保留完整的平台放行要求。
旧 [编译探针](../README.md) 仅作为升级前的可行性证据。

## 实现

- 根 manifest、UI manifest 和锁文件成套更新。启动采用 `gpui_platform::application()`；
  `font-kit`、`runtime_shaders` 由 platform 传到 macOS/Apple 后端，
  [实际 feature 树](platform-features.log) 确认没有落入无字形的默认路径。
- 适配 App 上下文相关的 focus/blur、关闭观察者、菜单 disabled 字段、滚动事件和列表滚动句柄。
  正文仍独立持有焦点，Windows 窗口控制继续使用原生控制区域。
- `WrappedLine` 使用 `Arc` 共享，保留原缓存及布局生命周期；混排折行显式使用 `SameIndent`。
  已定位的 ShapedLine 片段使用左对齐；表格自己的对齐不变。
- SVG 沿用资源路径与身份，新 data 参数为 `None`。图片明确传递完整放置区域，
  通过原有内容遮罩裁剪，避免用裁剪后的区域重新缩放图片。
- 新增平台 `EntityInputHandler::paste` 与快捷键共用原 `InputOrigin::Paste` 路径。
  先确认 IME，再执行粘贴事务，保留列表多行粘贴、源码换行和撤销规则。
  正文块菜单、侧栏导航与筛选框尊重 `prefer_character_input`，不抢占 AltGr 等字符输入。
- 复核新增原生文本状态通知：此固定版本的 macOS/Windows 后端未覆盖默认空实现。
  本次保留现有输入宿主注册及 UTF-16/预编辑/几何桥，不为了无操作通知把 Window 引入功能层。
- 架构守卫同时识别依赖真实包名与 alias，包括 `gpui`、`gpui-`、`gpui_` 包族，
  防止 core/platform/backend 进入 UI 之外的生产依赖。新增正反例覆盖普通、构建、目标条件和重命名。
- CI 增加 macOS ARM64/Intel 与 Windows 检查；原发布打包工作流继续负责 release、便携包及 MSIX。
  本地没有触发远程工作流，没有发布应用或提交商店。
- `rustfmt.toml` 显式保留仓库既有的 2021 排版风格，语言 edition 仍为 2024。
  本机默认 Cargo 插件找到旧 rustfmt 1.7.1，而独立 Cargo home 使用匹配 Rust 1.99 的
  rustfmt 1.10.0；后者默认采用 2024 排版并要求全仓库无关重排。固定 style edition 后，
  当前工具链的格式检查通过，不需要批量修改既有源码。

解析器、事务、投影、公式/Mermaid/PDF 的架构和关键版本保持不变。
锁文件的 21 个 GPUI 包统一为 0.3.12，旧 `gpui 0.2.2` 已移除。
锁文件与 [候选锁文件](../candidate.Cargo.lock) 一致；增加 179、移除 232 个 `(包名, 版本)` 组合，
完整差异见 [lock-delta.json](../lock-delta.json)。服务 SVG 0.45.1、GPUI SVG 0.46.0、PDF SVG 0.47.0 共存。

## 回归中发现并修复的既有问题

首次默认并行测试及随后串行复核均在五个语言识别/自动高亮测试失败。
[并行日志](test-parallel.log)、[串行日志](test-serial-before-fix.log) 保留失败证据。
最初怀疑 50 ms 预算，但 [逐语法诊断](language-diagnostic.log) 显示检测仅约 2–5 ms：
Java grammar 把 Rust 的 `fn main()` 当成返回类型为 `fn` 的 Java 方法，
同时命中 Rust/Java 后被保守规则拒绝。这不是 GPUI API 变更造成的预算回归；相关检测源码和 grammar 版本在升级前后相同。

修复只收紧 Java 的强识别证据：要求 class/interface/enum/package 声明，
孤立 method/constructor 不再独立构成证据。明确标注 Java 的高亮不受影响。
新增用例检查 Rust、Java 类/包、Go 及歧义方法片段，原 50 ms 预算和失败断言均未放宽。
不支持自动判定的片段仍保留普通代码，并可手动指定语言。

另发现完整冒烟驱动原有的结束边界错误：外层阶段为 `0..411`，
阶段 0 初始化、其余按 `stage - 1` 分发，而新增 Alerts 后具体用例已占满 `0..=409`，
导致汇总/退出分支永远不执行。首轮走完全部功能阶段后由外部 900 秒看门狗终止，
不能记为完整冒烟通过；证据见 [首轮日志](smoke-before-final-stage-fix.log) 和
[首轮状态](checks-before-final-stage-fix.json)。修复为 `0..=411`，包含最终汇总与退出；
保留全部原有断言、等待与失败路径，并从头重跑。本轮外部看门狗改为 1800 秒，
不改变程序中的 50 ms 识别预算、资源截止时间或 GUI 用例等待规则。

表格子菜单冒烟还暴露了真实指针造成的前置条件差异：打开弹窗后，原生悬停把
`selected` 从 0 改成了 2。诊断记录为 `selected=2, submenu=None, bounds=1`，
因此直接按 Right 没有子菜单可展开，不能把它归为绘制丢帧；见
[初次失败](smoke-menu-failure.log) 与 [状态诊断](smoke-menu-diagnostic.log)。
检查现在先使用原有 Up 动作导航至第一个子菜单入口，再按 Right；
依然断言两层菜单、Down/Enter 编辑结果及原子撤销，没有屏蔽悬停或修改生产菜单行为。

## 实际验证

宿主为 macOS 15.8.1 / Intel x86_64，Rust/Cargo 1.99.1。
使用 `CARGO_PROFILE_DEV_DEBUG=0`、`CARGO_PROFILE_TEST_DEBUG=0` 减少本地构建磁盘占用；
这是本次命令环境，没有修改仓库 profile。`CARGO_HOME=/tmp/ryne-upgrade-cargo-home`
共享 registry 缓存；位于用户目录内的 Cargo 命令仍会读取祖先目录配置并提示旧配置文件名，
未修改全局镜像配置。版本以锁文件和包 checksum 固定。

- `cargo test --locked`：466 成功、0 失败、2 原有 ignored；见 [完整日志](test.log) 与
  [测试/Clippy 状态](test-clippy.json)。
  两个 ignored 分别为外网图片集成和一万文档的测量样本，没有增加跳过项。
  包括新增三项 GPUI 输入回归、Java 识别回归，以及现有结构编辑、文件、导出、布局和窗口分发测试。
- 全 workspace/all-targets `cargo check --locked` 与匹配驱动的 Clippy `-D warnings` 通过，
  见 [check.log](check.log)、[clippy.log](clippy.log)。架构检查、守卫单元测试及当前 rustfmt 检查通过。
- 完整真实窗口冒烟通过并正常退出 0，包含阶段 0–411（最终汇总及退出已实际执行）；
  `--smoke-test-failure` 正常退出 1，无 panic 穿过原生事件循环。见
  [完整窗口日志](smoke.log)、[失败路径](smoke-failure.log)。
- 图片、块公式、Alerts、主题、代码语言菜单、导出菜单、区块树七组专项冒烟均退出 0；
  每次只启用对应一个 `RYNE_SMOKE_*_ONLY=1`。结果与完整命令见 [checks.json](checks.json)。
- 生成 `target/Ryne.app`（Intel / debug），Info.plist、严格签名校验、ICNS 及两套数学字体
  许可文件均通过；包内二进制与构建产物的 Mach-O UUID 一致。签名会改变可执行文件字节，
  因此用 UUID 核对构建身份，另保存签名产物 SHA-256。实际启动签名后的应用包，图片冒烟退出 0。
  见 [打包日志](bundle-debug.log)、[校验日志](bundle-verify.log)、[包信息](bundle.json) 和
  [签名包启动日志](bundle-launch.log)。
- 最后的测试/Clippy 已在两处冒烟驱动修正后重跑通过。
  真实窗口冒烟调用编辑回调、使用平台字体与 Metal；它不等同人工候选窗口、鼠标及多 DPI 验收。

## 与计划逐项对照

下表将生产适配与平台验收分开；“本机”只指上述 Intel macOS 环境。

| 项目 | 已落实的实现/证据 | 仍需补充 |
| --- | --- | --- |
| G01 依赖 | 同版精确锁定、候选 lock checksum 一致、feature 树 | 无自动追新 |
| G02 启动 | 平台 facade、真实字体和 Metal 窗口冒烟 | 其他宿主实际启动 |
| G03 Element | 全目标检查、模拟事件分发、旧帧及几何回归 | 平台人工交互 |
| G04 文字 | Arc 共享、SameIndent、真实字形/折行/命中及块树冒烟 | 双 DPI、跨机字体差异 |
| G05 SVG/公式 | 路径身份、两字体单元测试、公式/主题/Alerts 冒烟 | 多 DPI 目视复核 |
| G06 图片/图表 | 明确完整图像区域、像素协议、图片/Mermaid 回归 | 部分裁剪图像逐像素对照 |
| G07 输入 | 原生与 action 粘贴/IME 事务一致性测试、现有 UTF-16 回归 | 两系统真实候选窗口 |
| G08 焦点/事件 | API 适配、字符优先回归、侧栏/焦点测试 | Windows AltGr 与滚轮实机 |
| G09 标题栏/菜单 | 生产树分发测试、导出菜单冒烟、原关闭流程 | 双系统窗口按钮与全屏人工检查 |
| G10 异步 | 保存、取消、过期结果、恢复与资源生命周期测试 | 长时间交互运行 |
| G11 结构/保存 | 完整 workspace 测试与完整原生编辑冒烟 | 人工编辑抽检 |
| G12 导出 | PDF、冻结资源、分页测试，导出菜单冒烟 | 发布平台导出抽检 |
| G13 守卫/测试 | 包名/alias 边界正反例、466 项通过，无新增 ignored | 其他平台执行同套检查 |
| G14 macOS 包 | 本机 debug bundle 与本地校验见下文日志 | ARM64、release、最低 macOS 12 |
| G15 Windows 包 | SDK 路径说明更新、增加 Windows CI；原包工作流保留 | SDK/CRT/HLSL 实际构建、ZIP/MSIX 及 GUI |
| G16 性能/回退 | 依赖差异与基线固定、未改持久化格式 | 同机前后五轮测量、RSS/包体与回退演练 |

U1/U2 的生产适配已完成；U3 的本机自动化检查有日志。
U4 只记录实际完成的本机构建，U5 的跨版本性能比较尚未完成。
这些剩余项继续作为正式发布门槛，不能由 CI 配置或类型检查推断通过。

## 平台与发布边界

当前仍须在对应环境完成：macOS ARM64 和 Windows 构建/原生运行、两系统人工 IME、
Windows AltGr/标题栏/文件激活、Windows x86/ARM64 包与 MSIX、最低 macOS 12、不同 DPI、
同机升级前后性能/RSS/包体比较。新增 CI 配置不等于这些任务已执行。

本次不改变 Markdown、配置、草稿格式及产品发布版本号。回退应成组恢复 Cargo 文件、
平台启动和 UI 适配；不能仅把一个依赖版本改回。用户已有的未跟踪文档保持原样。
