# GPUI pre 升级可行性核验记录

日期：2026-11-15。基线 `bf8ebd5ef9271f599f56747e2b0c1c5ac626d9a7`，目标 `gpui-pre 0.3.12`。
完整方案见 [升级计划](../../50_GPUI_Pre_Upgrade_Plan.md)。

这是正式升级前的历史探针记录。2026-11-16 的生产适配和实际测试见
[正式升级记录](implementation/README.md)，下文“主工作区”“未执行”均指 11 月 15 日快照。

**结论：存在保留现有分层的可编译迁移路径，可以进入升级实现；尚未完成运行和发布验收。**
实际探针只修改两份 Cargo manifest 和 `ryne-ui` 下的 16 个 Rust 文件，另生成候选锁文件。
没有更换解析器、Markdown 事务、公式/Mermaid 服务或 PDF 架构。
本目录内的补丁是独立副本的编译探针，主工作区的生产源码和依赖仍为原版本。

## 1. 执行环境和结果

本机 macOS 15.8.1，Intel x86_64，Homebrew Rust/Cargo 1.99.1。
探针从固定基线 `git archive` 建立，不包含工作区的未跟踪文档。
使用独立 `CARGO_HOME`，共享原 Cargo registry 缓存，但不继承或修改全局镜像配置；
采用官方 crates.io 索引。编译产物使用独立 `CARGO_TARGET_DIR`。

| 操作 | 实际结果 | 证据和解释 |
| --- | --- | --- |
| 只改包名和版本，保留旧 runtime_shaders | 退出 101，预期的反例 | [直接替换日志](01-direct-replacement.log)：核心包不再提供该 feature |
| 分离 core/platform，保留原 Rust 调用 | 依赖解析成功，编译退出 101 | [完整首轮日志](02-cargo-check.log)：library 53 条错误、3 条弃用警告；部分为级联错误 |
| 最小接口适配后全目标检查 | 退出 0 | [最终检查日志](06-minimal-check.log)：`cargo check --locked --workspace --all-targets` |
| 同一最小探针全目标 Clippy | 退出 0，无警告 | [Clippy 日志](07-minimal-clippy.log)：`--locked --workspace --all-targets -- -D warnings` |
| 主工作区原生产代码格式检查 | 退出 0 | [基线格式检查](baseline-fmt.log)：`cargo fmt --check`，不是候选补丁格式验收 |
| 主工作区架构检查 | 退出 0 | [基线架构日志](baseline-architecture.log)：当前生产依赖边界通过；新包别名的漏检问题仍须修复 |
| 复现脚本准备阶段 | 退出 0 | 补丁成功应用；导出后的全部 Rust、Cargo manifest 和锁文件与最终编译候选逐字节相同，数量见 checks.json |

Clippy 实际通过 `/usr/local/opt/rust/bin/cargo-clippy clippy ...` 调用匹配 stable 的驱动。
最终补丁保留基线格式、避免附带不相关格式化；正式实现仍须执行候选分支的格式检查。
本轮的 `cargo check` 和 Clippy 编译了测试目标，但没有执行测试函数，也没有链接/启动新版应用。

未执行：`cargo test`、真实 GUI 冒烟、原生 IME/窗口控制、macOS ARM64、Windows 构建/运行、
release app bundle/ZIP/MSIX、性能与内存对照。不能由 macOS 类型检查推导这些项目已通过。

## 2. 来源和锁文件

发布包的 `package.metadata.gpui-pre.zed-rev` 指向
`d2a4fb6ded7c470be25d1541281da3882495b65b`。
核心、平台、macOS、Windows 四份 tarball 已通过官方索引的 checksum 校验，
版本、发布日期、撤回状态及 feature 快照见 [packages.json](packages.json)。
原始入口为 [核心索引](https://index.crates.io/gp/ui/gpui-pre) 和
[平台索引](https://index.crates.io/gp/ui/gpui-pre-platform)，
tarball URL 格式为 `https://static.crates.io/crates/{name}/{name}-0.3.12.crate`。

候选 [Cargo.lock](candidate.Cargo.lock) 的 SHA-256 记录在 [checks.json](checks.json)。
相对基线增加 179 个、移除 232 个 `(包名, 版本)` 组合；版本升级按一增一减计，
这些数量不是新增/删除的独立依赖根，也不是运行时加载库数量。
完整列表见 [lock-delta.json](lock-delta.json)，包含 Cargo 为跨平台解析保留的依赖。

| 依赖 | 原锁定 | 候选锁定 |
| --- | --- | --- |
| GPUI | gpui 0.2.2 | gpui-pre / gpui-pre-platform 0.3.12 |
| unicode-segmentation | 1.13.3 | 1.13.3 |
| pulldown-cmark | 0.13.6 | 0.13.6 |
| image | 0.25.10 | 0.25.10 |
| math-core / Formulary | 0.9.0 / 0.3.0 | 不变 |
| Merman / krilla | 0.10.0 / 0.8.2 | 不变 |
| usvg / resvg | 0.45.1、0.47.0 | 增加 GPUI 使用的 0.46.0，其余保留 |
| Taffy | 0.9.0 | 0.15.1 |
| windows | 0.57.0、0.61.3 | 增加 0.62.2，其余保留 |

## 3. 可复现探针

需要 Python 3.12+（tar 安全提取 filter）、Git、当前 stable Rust 和对应平台的构建环境。
在仓库中运行：

```sh
python3 docs/reviews/gpui-pre/probe.py
```

脚本校验候选 lock 的 SHA-256，从固定提交导出到新临时目录，检查并应用
[compile-probe.patch](compile-probe.patch)，写入候选锁文件，再执行全目标 `cargo check`。
终端输出临时源码、Cargo home 和日志位置；不会修改主工作区或覆盖现有应用包。
首次运行可能下载和编译大量依赖，临时产物保留供复查。

```sh
# 仅检查补丁能否应用并准备源码，不编译。
python3 docs/reviews/gpui-pre/probe.py --prepare-only

# 检查并追加 Clippy；先确保 PATH 中 cargo-clippy 与 rustc 匹配。
python3 docs/reviews/gpui-pre/probe.py --clippy
```

脚本默认使用当前宿主目标。本次成功证据只来自 macOS x86_64；在 Windows 运行产生的是新的核验结果。
正式适配完成后不要继续把固定基线探针当成当前分支测试，应按主计划直接验证候选工作区。

## 4. 源码核验与剩余风险

| 项目 | 检查的发布源码 | 核实的事实及后续要求 |
| --- | --- | --- |
| 平台拆分 | gpui-pre-platform 的 Cargo.toml、src/gpui_platform.rs | application → Application::with_platform；font-kit/runtime_shaders 传入后端 |
| macOS 字体与 shader | gpui-pre-macos/src/platform.rs；gpui-pre-apple/build.rs | 无字体 feature 时使用 NoopTextSystem；runtime shader 拼接依赖包内 vendor/gpui，当前宿主编译成功 |
| 自定义 Element | gpui-pre/src/element.rs | request_layout / prepaint / paint 可继续使用，现有 EditorElement 实现通过编译；辅助功能方法有默认值，不代表完整可访问编辑 |
| 文字和缓存 | src/text_system/line.rs、line_wrapper.rs | WrappedLine 无 Clone；采用 Arc 维持克隆布局目录；ShapedLine 绘制增加对齐参数；wrap_line 需要 IndentAdjustment |
| 图像与 SVG | src/window.rs、assets.rs | paint_svg 新增 data；paint_image 区分可见/完整图像区域；RenderImage 仍声明 BGRA；保留已有字节转换 |
| 输入 | src/input.rs、platform.rs、window.rs | 原必需 UTF-16 方法仍可实现；新增默认 paste/选区/文本配置及通知接口；平台通知默认空实现，未发现 macOS/Windows 覆盖 |
| 按键和焦点 | src/platform/keystroke.rs、interactive.rs、window.rs | focus/blur 需要 App；secondary 按运行平台判断；prefer_character_input 与 AltGr 有关，需真实行为测试 |
| 测试 | src/app/test_context.rs、platform/test/platform.rs | 模拟上下文可编译；新旧均有 NoopTextSystem 路径，不能把模拟结果当作真实字体或原生输入证明 |
| Windows | gpui-pre-windows/src/window.rs、build.rs | 保留 WindowControlArea；release 编译 HLSL 并寻找 fxc；必须在 Windows SDK 环境实际构建，ARM64/x86 也要分别核验 |
| 架构边界 | Ryne scripts/check-architecture.py | 原检查只识别真实包名 gpui；新版 alias 后可能漏检，需要专门修补和正反例 |

首轮错误类别及最小适配方式详见主计划 4.3。除 Cargo 文件外，最终探针的改动全部位于 `ryne-ui`。
补丁没有实现架构守卫修复、原生输入新语义审查或运行测试，不应直接作为发布补丁合入。

下一步顺序：U1 固定依赖并修守卫 → U2 正式适配与绘制检查 → U3 测试和原生输入 →
U4 双平台构建/分发 → U5 性能、文档与回退验收。各项放行标准见主计划的 G01–G16。
