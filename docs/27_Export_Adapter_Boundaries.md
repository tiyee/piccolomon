# 导出适配器边界

本次增量保留九包结构及现有两种 PDF 导出行为，不改变依赖版本。

- `ryne-extensions::ExportRegistry` 启动时验证后端 ID 和默认文件名后缀；描述包含 PDF 编码端口，注册后只读。
- `src/main.rs` 装配 `pdf`、`pdf-v2` 及 `ArtifactStore`。新增编码后端不需要给 `DesktopServices` 增加字段。
- UI 的两种本地化菜单和快捷键仍保留为显式入口，通过 ID 查找描述。当前没有动态菜单注册，也没有动态插件加载；未来新增菜单仍需 UI 配置。
- UI 保留文件选择、覆盖确认、源文件别名保护、IME 确认与导出状态；独立快照生成和后台导出流程不变。
- `ArtifactStore` 是二进制产物写入端口；`AtomicArtifactStore` 使用同目录独占临时文件、同步、关闭句柄后重命名，失败清理临时文件。Markdown 文档保存仍沿用原 `FileStore` 的权限保留策略。
- `print_layout.rs` 私有模块独立持有 A4 分页、字体、资源限制和 SVG 中间页；`pdf.rs` 与 `pdf_v2.rs` 仅实现不同编码适配器。

验证重点：重复注册拒绝、未知 ID、后端路由和文件名保持；二进制替换与重命名失败清理；既有共享分页、双 PDF 后端、独立快照及编辑回归。没有增加独立可编辑文档或改变源码事务。

后续可以沿同一方式统一功能贡献注册、拆分块呈现和收拢 UI 状态；这些尚未在本次实现。

## 本轮验证结果

- 架构检查、`cargo fmt --check`、`git diff --check` 通过。
- `cargo test --locked`：214 项测试通过。
- `cargo test --locked -p ryne-app --example export-pdf`：2 项测试通过，覆盖两种编码器的混合内容和长文档分页。
- Rust 1.99 配套 Clippy 的 `--locked --all-targets -- -D warnings` 通过；默认 Cargo 子命令发现旧 Clippy，使用 `/usr/local/opt/rust/bin/cargo-clippy clippy` 执行，未修改全局配置。
- 完整真实窗口冒烟退出码 1：第 176 阶段表格菜单几何检查 `render_menu_bounds.len()` 未达到期望的 2；本轮未修改该检查及菜单实现，未建立改造前对照，不能确认其来源。不能宣称完整 GUI 验证通过。

后续增量：导出描述已包含标签键与回退文案，两种桌面菜单改为遍历注册描述，通过通用导出动作分发。见 [组件化记录](28_Static_Feature_Components.md)。本文件前述静态菜单限制记录的是第一轮状态。

## 2026-10-26 PDF 回归补充

保留 PDF v1 的 pdf-writer 0.12.1 / svg2pdf 0.13.0 与 PDF v2 的 Krilla 0.8.2
（内部使用 pdf-writer 0.15.0），不调整依赖版本。

- `crates/ryne-services/src/pdf_tests.rs` 对两套真实编码器执行相同契约：空文档 A4、
  多页完整编码、`startxref` 字节偏移、非法控制字符、4 MiB 文本限制及 500 页限制。
  原先仅针对 v2 的空文档和失败检查合并到此处。
- `crates/ryne-session/src/export.rs` 覆盖快照捕获后继续编辑、LF / CRLF / CR、
  未确认 IME、局部/全文源码范围、dirty、路径、选区及撤销/重做保持；
  无效、不完整、未闭合、超过字节或行数限制的 Mermaid 保留字面代码。
- `examples/export-pdf.rs` 已通过 `test = true` 纳入默认 `cargo test --locked`，
  混合中文、行内格式、代码、列表、表格、公式和 Mermaid 在三种换行下经两套编码器导出，
  编码后再次检查编辑状态；长段落、代码和表格单元格分页检查两套后端页数一致。

这些回归验证当前编码和会话契约，不替代 PDF 阅读器中的视觉对比、人工交互或 Windows 实机验收。
