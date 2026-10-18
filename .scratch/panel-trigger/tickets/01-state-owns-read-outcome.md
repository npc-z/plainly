# 01: panel: state 拥有「读取结果意味着什么」，Plan 并入 Showing

**What to build:** `state.rs` 新增 `Trigger::read(&mut self, result: Result<Clipboard, clipboard::Error>) -> Option<Vec<String>>`：把读取结果落成 `Showing`（`Sensitive` / `Unread` / `Nothing` / `TooLong` / `Stack`；`Error::NoPassage` → `Nothing`、其它错误 → `Unreadable`），`Some` 时返回要跑的 Passage 列表。`plan.rs` 与 `Plan` 删除，6 条测试迁进 `state.rs` 新的 `mod trigger_tests` 并按新词汇改名，另加读取结果每个落点的测试。`Trigger.showing` 私有、加 `pub fn showing(&self) -> &Showing`；`Trigger::stack` 收为私有。`panel.rs` 的读取路径改调 `trigger.read(result)`，`Surface::show` / `Surface::stack` 改成只读绘制 `draw_message` / `draw_stack`（不再写 state）；`Press` 与 focus 等待本票不动。

**Blocked by:** 无（可立即开始）。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §1、§3（读取侧）、§5、§6

**Status:** done (2026-10-18)

- [x] `Trigger::read` 的签名与返回：`Some(Vec<String>)` 仅当 `Showing::Stack`，列表顺序即剪贴板顺序；`None` 覆盖全部一句状态。
- [x] 映射逐条不变：`NoPassage → Nothing`（reason 不显示）、其它错误 → `Unreadable`、Sensitive/Unread、TooLong、空列表 → Nothing。
- [x] `plan.rs` 删除，`main.rs` 的 `mod plan;` 删除；`panel.rs` 里没有 `plan::`。
- [x] `Trigger.showing` 私有；`pub fn showing(&self) -> &Showing`；`Trigger::stack` 私有；`recall` / `prefs` 同样私有，配只读 getter。
- [x] `panel.rs` 只剩 `draw_message(&str)` / `draw_stack(&[String])` 两种只读绘制；没有 `.showing =` 或 `= Showing::`。
- [x] `mod trigger_tests`：6 条 plan 测试已迁入并改名；新增读取结果的错误落点与 `Some` 顺序测试；现有 `mod tests` 与 `planning_tests` 未改。
- [x] `nix develop -c cargo test -p plainly-panel` 全绿；`cargo clippy -p plainly-panel --all-targets` 无新警告。

## Comments

实现：本 commit（`panel: state owns the trigger transitions`）。`Trigger::read` 接手读取结果与 `Plan` 的五个落点（Sensitive / Unread / Nothing / TooLong / Stack，另加 `NoPassage → Nothing`、其它错误 → `Unreadable`）；`plan.rs` 与 `main.rs` 的 `mod plan;` 删除；6 条 plan 测试迁入 `state.rs` 的 `mod trigger_tests` 并按新词汇改名，另加 `NoPassage`、`ReadFailed`、recall/prefs 保真三条；`Trigger.showing` 私有 + `showing()`，`Trigger::stack` 私有；`Surface::show` / `stack` 换成只读的 `draw_message` / `draw_stack`。

验证：`nix develop -c cargo test -p plainly-panel`：81 passed（迁移前 71；plan 的 6 条迁入，trigger_tests 共 16 条，净增 10——评审又补了「在 Failed 上覆盖读」与「press→read 顺序」两条）；`cargo clippy -p plainly-panel --all-targets` 零警告；`grep -nE 'plan::|\.showing =|= Showing::' crates/plainly-panel/src/panel.rs` 无输出。

偏差：无。

评审后收紧（第二轮）：`Trigger::read` 的 Stack 分支改为只替换 `showing`（不再 `*self = Trigger::stack(...)`，以后给 `Trigger` 加字段不会被静默丢掉）；`stack` 变成 test-only 的便利构造；`commit` / `fail` 改私有，`state::apply` 因此是模块外唯一入口；`a_read_keeps_the_triggers_recall_and_prefs` 追加了句子态同样保留 recall/prefs 的断言。

评审后收紧（第三轮）：`recall` / `prefs` 也改成私有 + `recall()` / `prefs()` getter，panel 只读并 clone；三个字段的写者都只剩 state 的迁移。
