# 11 汇总为 v0 实现规格

Type: task
Status: closed (out of scope)
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

## Closure (out of scope)

**关掉，不做**——这张票据与 `/to-spec` 是同一件事，重复了。

画图时把"产出规格"既写成了地图的终点，又做成了票据，这是画图时的疏忽。Wayfinder 的规矩是**交接而非施工**：地图清空后由 `/to-spec` 把图上互相链接的决策坍缩成可施工的规格，再 `/to-tickets` 拆票、`/implement` 施工。地图自己再写一遍规格，只会得到两份会慢慢分叉的文档。

按 Wayfinder 的 Out of scope 规矩：这张票据**不进 Decisions so far**（范围边界不是路上的一步），只在 [map.md](../map.md) 的 Out of scope 留一行指针。它的阻塞边无需清理——它是被阻塞的一方，没有别的票据依赖它。
