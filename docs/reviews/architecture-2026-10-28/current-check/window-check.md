# 本轮原生窗口验证

生产代码基线 `a5de8e7`，debug 构建，2026-10-28。本轮没有修改窗口检查或生产实现。

第一次执行：

```sh
RYNE_PROFILE=1 cargo run --locked -- --smoke-test
```

退出码 1，报告：`GUI smoke failed at stage 49: check failed: geometry.display.diagrams.len() != 1`。
该阶段要求列表/引用容器中的 Mermaid 已生成一个图表。检查位置为
[smoke.rs:1039](../../../../crates/ryne-ui/src/smoke.rs#L1039)。这是已有检查正常报告失败，
不是 panic 穿越原生事件循环。

当时并行进行本轮新启动的 release 性能示例依赖编译。为减少干扰，停止这批编译及其
遗留编译子进程，确认没有继续运行的本轮 rustc，再执行同一个已构建二进制：

```sh
RYNE_PROFILE=1 target/debug/ryne --smoke-test
```

第二次退出码 0，执行至 stage 374 并报告 `GUI smoke passed`。嵌套 Mermaid 的 stage 49
已通过，无需改变测试或延长等待。完整检查包括后续 P1 边界、低风险布局、控件及
IME 回调相关检查。阶段输出摘录保存于 [window-output.txt](window-output.txt)。

两次结果只能证明首次失败在第二次独立复跑中未复现，不能证明并行编译就是唯一根因。
可能涉及负载、渲染截止时间或异步检查时序，尚未得到渲染诊断和任务时间线证据，
因此不新增确定的产品缺陷，也不把首次运行算作通过。后续若复现，建议在该检查失败时
记录当前代码块诊断、JobHost pending/取消/截止状态，以及投影和帧身份，再做有界负载矩阵。

`RYNE_PROFILE` 的数据覆盖多种不同文档和检查动作，第一次还存在编译干扰；本轮不以这些
混合分位数宣称交互性能达标。本次未交付 release 大文件耗时或 RSS 基准。

真实窗口冒烟通过不等于 Windows 实机、人工鼠标键盘或真实输入法候选窗口验收。
