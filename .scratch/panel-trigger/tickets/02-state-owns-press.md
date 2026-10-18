# 02: panel: state 拥有「按键意味着什么」，pending 消失

**What to build:** `Press { Refresh, Regenerate }` 从 `panel.rs` 搬进 `state.rs`，新增 `Trigger::press(press, prefs) -> Trigger`（`Showing::Loading`；`Refresh → Recall::Reuse`；`Regenerate → prefs.regenerate_recall()`；`prefs` 快照随 trigger 走）。`Panel::start` 先 `reread_prefs()` / `reset_dismiss()`，再把 `Trigger::press` 装进 `surface.trigger`，然后 `present()` 并等 focus；focus-in 只调无参的 `Panel::read()`。`Panel::read` 不再收 `Recall`：从 trigger 取 recall/prefs，UI 重置与 generation 逻辑照旧。`pending: RefCell<Option<Recall>>` 字段删除，`waiting: Cell<bool>` 保留。

**Blocked by:** 01。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §2、§4

**Status:** ready-for-agent

- [ ] `Press` 定义在 `state.rs`、文档随迁，`panel.rs` 不定义 Press；`Action::Regenerate` 仍然走 `Panel::start(Press::Regenerate)`，行为不变。
- [ ] `Trigger::press` 三种 Regenerate 配置与 Refresh 的行为由无显示测试钉住（云端 → `Bypass { provider: None }`；本地 + 可用云端 → `Bypass { provider: Some(name) }`；本地 + 无云端 → `Bypass { provider: None }`）。
- [ ] `Panel::start` 在 present 前装好 trigger；focus-in 调 `Panel::read()`；`pending` 字段与两处赋值删除。
- [ ] `Panel::read` 无 `Recall` 参数，起 worker 用的 recall 从 trigger 取；READING/状态行/复制/chip/generation 的时序不变。
- [ ] `nix develop -c cargo test -p plainly-panel` 全绿（GTK 测试在本机有 display 时实际执行）；`cargo clippy -p plainly-panel --all-targets` 无新警告。

## Comments

（实现后填：commit、测试数、偏差。）
