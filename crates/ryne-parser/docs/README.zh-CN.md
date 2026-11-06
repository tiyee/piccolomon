# 包开发文档

[English](README.md)

本目录保存 `ryne-parser` 的实现说明及开发指南。面向使用者的安装方法及 API 示例放在包的[英语 README](../README.md) 和[中文 README](../README.zh-CN.md)。

当前运行时契约、API 迁移及验证范围见[编辑核心契约与验证](Editing_Core.zh-CN.md)。较早且带日期的方案和对比为历史基线。

## 阅读顺序

1. 阅读包 README，了解当前公共行为和能力限制。
2. 阅读 [`src/lib.rs`](../src/lib.rs) 和 [`tests/gfm_fixtures.rs`](../tests/gfm_fixtures.rs)，确认实际实现和回归覆盖。
3. 修改架构或语义时，阅读上游的 [v4 行为规范](https://github.com/iDigimon/markdown-engine/blob/main/docs/Editable_GFM_Engine_v4_Typora_Hybrid_Specification.md)、[技术方案](https://github.com/iDigimon/markdown-engine/blob/main/docs/Editable_GFM_Engine_v4_Complete_Technical_Plan.md)和[可行性报告](https://github.com/iDigimon/markdown-engine/blob/main/docs/Editable_GFM_Engine_v4_Feasibility_Report.md)。
4. 阅读 [writ 对比与吸收计划](Writ_Comparison_and_Adoption.zh-CN.md)，了解投影、标记、交互、输入、存储及增量方面的待实施工作。
5. 运行生成证据的脚本或辅助 cmark-gfm 比较前，阅读上游的[可行性工具指南](https://github.com/iDigimon/markdown-engine/blob/main/feasibility/v4/README.md)。

上游 v4 文档目前包含中文内容。设计描述目标能力；源码、测试和运行记录用于判断实际实现状态。

## 实现位置

| 位置 | 职责 |
| --- | --- |
| [`src/lib.rs`](../src/lib.rs) | 源码身份、原子事务、位置映射、会话、composition 及单元测试 |
| `src/syntax.rs` / `src/projection.rs` | 自有解析快照、保真 ownership、源码/预览映射及展开交互 |
| `src/planner.rs` / `src/render.rs` | 带版本的编辑意图、剪贴板数据策略及自有 HTML renderer |
| [`tests/editing.rs`](../tests/editing.rs) | CRLF 逆事务、随机多补丁、身份、映射、会话及 Planner 回归 |
| [`tests/gfm_fixtures.rs`](../tests/gfm_fixtures.rs) | 677 个固定示例的源码往返及原生 HTML 诊断基线 |
| `tests/fixtures/` | GFM 输入、hash 锁及独立规范许可声明 |
| [`examples/v4_probe.rs`](../examples/v4_probe.rs) | 复用库实现的可行性探针和基准 |
| 工作区 `Cargo.toml` / `Cargo.lock` | 共享元数据、依赖声明及锁定版本 |

## 开发约束

保留原文字节及 UTF-8 字节坐标；拒绝 CRLF 内部编辑端点。全部编辑基于同一 revision，先完整校验再修改；保持逆事务与 affinity，将 composition preedit 与源码隔离。不从语义文本或 HTML 重建源码。

后端有意只启用表格、删除线和任务列表。保持锁定版本及显式选项；`Options::all()` 或 `ENABLE_GFM` 会扩大方言。当前 629/677 原生 HTML 匹配数是诊断基线，不是完整 GFM 兼容成绩；每个原生差异现按 example 和精确输出锁定，修改前需逐例审查。自有 renderer 另有带明确章节配置的 677 例门禁。

运行时与验证专用依赖分开管理；共享依赖在工作区声明，本 crate 继承。代码、源码注释、诊断及默认文档使用英语。中文文档使用 `.zh-CN.md`，同步维护内容并提供顶部语言互链。

## 验证

在工作区根目录使用 stable Rust/Cargo 执行：

```sh
cargo fmt --all -- --check
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --locked --no-deps
cargo build --workspace --locked --examples
```

聚焦检查：

```sh
cargo test -p ryne-parser --locked
cargo test -p ryne-parser --locked --doc
```

纯文档修改检查示例、链接、双语一致性及行为描述。README 中修改的示例需显式编译运行；Cargo 不会自动把 README 代码块作为 doctest 执行。

仅当任务要求更新性能或可行性证据时执行：

```sh
cargo run -p ryne-parser --locked --release --example v4_probe -- bench
```

库代码使用 MIT；保留 fixtures 独立的 [CC-BY-SA-4.0 声明](../tests/fixtures/NOTICE.md)。
