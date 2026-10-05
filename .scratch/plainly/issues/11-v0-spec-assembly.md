# 11 汇总为 v0 实现规格

Type: task
Status: open
Blocked by: 01, 02, 03, 04, 05, 06, 07, 08, 09, 10

## Question

把上面所有结论组装成 `.scratch/plainly/spec.md`，**可直接交给 `/to-spec`**：

- 范围与能力矩阵（Linux 一等公民；niri 同类完整、GNOME 降级；core 跨平台）。
- core 的对外接口（契约 schema、provider trait）。
- 数据模型（SQLite schema、导出格式）。
- CLI 契约（子命令、I/O、退出码）与守护进程触发链路。
- UI 结构（v0 屏幕清单）。
- provider 支持面与本地模式的质量门槛／诚实描述。
- 非目标（Out of scope 一节原样带入）与 v1 候补。

要求：**逐条引用来源票据**，不留悬空假设。任何还没定的东西，要么补一张票据，要么明确写进"未决"。
