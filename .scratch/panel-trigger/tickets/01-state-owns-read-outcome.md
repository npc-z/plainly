# 01: panel: state 拥有「读取结果意味着什么」，Plan 并入 Showing

**What to build:** `state.rs` 新增 `Trigger::read(&mut self, result: Result<Clipboard, clipboard::Error>) -> Option<Vec<String>>`：把读取结果落成 `Showing`（`Sensitive` / `Unread` / `Nothing` / `TooLong` / `Stack`；`Error::NoPassage` → `Nothing`、其它错误 → `Unreadable`），`Some` 时返回要跑的 Passage 列表。`plan.rs` 与 `Plan` 删除，6 条测试迁进 `state.rs` 新的 `mod trigger_tests` 并按新词汇改名，另加读取结果每个落点的测试。`Trigger.showing` 私有、加 `pub fn showing(&self) -> &Showing`；`Trigger::stack` 收为私有。`panel.rs` 的读取路径改调 `trigger.read(result)`，`Surface::show` / `Surface::stack` 改成只读绘制 `draw_message` / `draw_stack`（不再写 state）；`Press` 与 focus 等待本票不动。

**Blocked by:** 无（可立即开始）。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §1、§3（读取侧）、§5、§6

**Status:** ready-for-agent

- [ ] `Trigger::read` 的签名与返回：`Some(Vec<String>)` 仅当 `Showing::Stack`，列表顺序即剪贴板顺序；`None` 覆盖全部一句状态。
- [ ] 映射逐条不变：`NoPassage → Nothing`（reason 不显示）、其它错误 → `Unreadable`、Sensitive/Unread、TooLong、空列表 → Nothing。
- [ ] `plan.rs` 删除，`main.rs` 的 `mod plan;` 删除；`panel.rs` 里没有 `plan::`。
- [ ] `Trigger.showing` 私有；`pub fn showing(&self) -> &Showing`；`Trigger::stack` 私有；`recall` / `prefs` 仍 `pub`。
- [ ] `panel.rs` 只剩 `draw_message(&str)` / `draw_stack(&[String])` 两种只读绘制；没有 `.showing =` 或 `= Showing::`。
- [ ] `mod trigger_tests`：6 条 plan 测试已迁入并改名；新增读取结果的错误落点与 `Some` 顺序测试；现有 `mod tests` 与 `planning_tests` 未改。
- [ ] `nix develop -c cargo test -p plainly-panel` 全绿；`cargo clippy -p plainly-panel --all-targets` 无新警告。

## Comments

（实现后填：commit、测试数、偏差。）
