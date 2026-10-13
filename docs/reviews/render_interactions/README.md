# 渲染模式交互原型

架构设计：[20 · 渲染模式交互扩展](../../20_Render_Mode_Interaction_Extensibility.md)。

这是通过 `ryne-session` example 编译的独立无窗口实验。生产编辑器未接入新控件。
没有独立 Cargo workspace、新依赖或依赖版本调整，使用仓库现有锁文件。

| 文件 | 内容 |
|---|---|
| `protocol.rs` | 扩展契约、可执行注册、语义目标、命名区域、fixture 几何命中、令牌和分派 |
| `features.rs` | 表格/列表示例，真实 Markdown 功能与会话装配，候选计划 |
| `tests.rs` | 架构及一致性回归，第三个自定义角色扩展 |
| `main.rs` | 删除表格行并撤销的可执行演示 |

```sh
cargo run --locked -p ryne-session --example render-interactions
cargo test --locked -p ryne-session --example render-interactions
```

原型刻意简化：宿主上下文由可信调用方提供，布局使用 fixture；所有区域全文生成，
碰撞报错，只有按钮/菜单，示例没有参数 schema 或宿主效果。示例新条目正文 `new` 仅为
测试占位，未作为产品本地化模板交付。正式实现需要复用统一的选区/检查点构建器，
并在 GPUI 宿主真实维护投影、布局及视口版本，详见设计文档。
原型以一个目标对应一个区域，尚未实现同一目标的多个命名子区域、注册 Widget 或真实事件循环。

## 验证记录（2026-10-13）

| 检查 | 结果 |
|---|---|
| `python3 scripts/check-architecture.py` | 9 个私有包和生产依赖边界通过 |
| `cargo fmt --check` | 通过 |
| `cargo test --locked -p ryne-session --example render-interactions` | 原型 20 项通过 |
| `cargo run --locked -p ryne-session --example render-interactions` | 4 个 cell 控件；删除指定行；撤销恢复原始字节 |
| `cargo test --locked` | 现有 workspace 113 项通过；不包含上面单独运行的 example 测试 |
| `/usr/local/bin/cargo-clippy clippy --locked --workspace --all-targets -- -D warnings` | 通过，包含 example 及其测试 |

共验证 133 项测试。Cargo 提示已有第三方 `block 0.1.6`、`proc-macro-error2 2.0.1`
存在未来 Rust 兼容性警告，检查退出码均为 0；没有为此调整依赖或全局 Cargo 配置。

本次没有修改生产 GUI 或渲染通路，未运行真实窗口冒烟、重新打包或关闭已有窗口。
这份验证说明无窗口操作协议能够复用现有解析、投影与事务系统，不代替真实原生控件验收。

后续正式表格交互已接入 GUI；本目录继续保留初始独立原型，实际代码边界及窗口验证见
[21 · 表格交互实施记录](../../21_Render_Mode_Table_Interactions.md)。
