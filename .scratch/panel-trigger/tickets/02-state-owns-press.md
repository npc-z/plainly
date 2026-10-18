# 02: panel: state 拥有「按键意味着什么」，pending 换成 Trigger

**What to build:** `Press { Refresh, Regenerate }` 从 `panel.rs` 搬进 `state.rs`，新增 `Trigger::press(press, prefs) -> Trigger`（`Showing::Loading`；`Refresh → Recall::Reuse`；`Regenerate → prefs.regenerate_recall()`；`prefs` 快照随 trigger 走）。`Panel::start` 先 `reread_prefs()` / `reset_dismiss()`，再建 `Trigger::press`：能立即读的路径装进 `surface.trigger` 后 `read()`，GTK 等待路径把它存进 `pending`、`present()` 并等 focus；focus-in 取出并安装后才调无参的 `Panel::read()`。`Panel::read` 不再收 `Recall`：从 trigger 取 recall/prefs，UI 重置与 generation 逻辑照旧。`pending` 从 `Option<Recall>` 换成 `Option<Trigger>`，`waiting: Cell<bool>` 保留。

**Blocked by:** 01。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §2、§4

**Status:** done (2026-10-18)

- [x] `Press` 定义在 `state.rs`、文档随迁，`panel.rs` 不定义 Press；`Action::Regenerate` 仍然走 `Panel::start(Press::Regenerate)`，行为不变。
- [x] `Trigger::press` 三种 Regenerate 配置与 Refresh 的行为由无显示测试钉住（云端 → `Bypass { provider: None }`；本地 + 可用云端 → `Bypass { provider: Some(name) }`；本地 + 无云端 → `Bypass { provider: None }`）。
- [x] `Panel::start` 只在立即读的路径装 trigger；GTK 等待路径把整个 `Trigger` 存进 `pending` 且不安装（等待期间当前 trigger 仍属上一条运行）；focus-in 取出安装后调 `Panel::read()`。
- [x] `Panel::read` 无 `Recall` 参数，起 worker 用的 recall 从 trigger 取；READING/状态行/复制/chip/generation 的时序不变。
- [x] `nix develop -c cargo test -p plainly-panel` 全绿（GTK 测试在本机有 display 时实际执行）；`cargo clippy -p plainly-panel --all-targets` 无新警告。

## Comments

实现：本 commit。`Press` 搬进 `state.rs`（文档随迁），`Trigger::press` 解析 `Recall` 并保留 prefs 快照；`Panel::start` 先 `reread_prefs` / `reset_dismiss`、再建 `Trigger::press`；DataControl / 已激活的 Gtk 路径立即装并 `read()`，等待路径把它存进 `pending`（**不安装**，否则上一条运行的流式事件会画进新 `Loading`），focus-in 取出安装后才调无参 `Panel::read()`。`pending` 从 `Option<Recall>` 换成 `Option<Trigger>`，`waiting: Cell<bool>` 保留。

新测试 6 条：Refresh → `Recall::Reuse`；Regenerate 三种配置（hosted **且有可用云端** → `Bypass { provider: None }`；local + 可用云端 → `Bypass { Some("deepseek") }`；local 无云端 → `Bypass { provider: None }`）；press 保留 prefs 快照；press→read 是同一个 trigger。

偏差（评审后修正）：初版把 Trigger 在 `start` 就装进 cell，评审指出 GTK 降级路径下上一条运行的事件会画进新 `Loading`（chip 重置、复制变灰、`Failed` 落成句子而正文仍是旧 stack）。改为「等待路径存整个 Trigger、`read` 才安装」，与旧行为一致；见 spec Further Notes「等待 focus 的 Trigger」。

评审后收紧（第三轮）：focus 交接抽成 `waited_trigger(pending, prefs)` 并加一条无显示的单元测试（`debug_assert` 钉住「等待一定带着 press 建好的 Trigger」这个不变量）；句子的绘制改用 `expect`，`None` 不再被静默画成空正文。

验证：`nix develop -c cargo test -p plainly-panel` 82 passed；`cargo clippy -p plainly-panel --all-targets` 零警告；`grep -n 'Press::' crates/plainly-panel/src/panel.rs` 只剩按键 / ⟳ 接线处的两行实参。

偏差：无。
