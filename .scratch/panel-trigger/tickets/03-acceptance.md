# 03: 验收：grep、全量测试、spec 漂移与记录

**What to build:** 跑验收与记录，不改产品代码（除非验收暴露缺陷）：两条 grep 验收、`nix develop -c cargo test -p plainly-panel`、`nix develop -c cargo clippy --workspace --all-targets`；核对 spec 的每一条 claim（行号、映射表、测试位置）与实际代码一致，漂移就改 spec；确认 README / `docs/manual-checks.md` 不因本次改动而需要新条目；把验收输出写进本票与 01/02 的 Comments。可选：有 `wayland-1` 会话时做一次最小真机 smoke 并记录，不作为完成条件。

**Blocked by:** 01、02。

**Spec:** [spec.md](../spec.md) §Testing Decisions；§Out of Scope

**Status:** done (2026-10-18)

- [x] grep 验收（仓库根，输出为空即通过）：`grep -nE 'plan::|\.showing =|= Showing::|regenerate_recall|match press' crates/plainly-panel/src/panel.rs`；`Press::` 只剩按键 / ⟳ 接线处的实参（`grep -n 'Press::'` 只该命中那两行）。
- [x] `plan.rs` 与 `mod plan` 不存在；`git ls-files crates/plainly-panel/src/plan.rs` 为空。
- [x] `nix develop -c cargo test -p plainly-panel` 全绿，记录通过数；`nix develop -c cargo clippy --workspace --all-targets` 不是 exit 0 的话记录既有警告与新警告的差别。
- [x] spec 的 6 条 Implementation Decisions 与测试位置逐条对照代码，漂移已修正。
- [x] README / `docs/manual-checks.md` 未改，并在 Comments 里说明原因（无可感知变化）。
- [x] 01 / 02 的 Status 标 `done`，Comments 填本次实现 commit 与验证证据。

## Comments

验收（本 commit 的工作树，`nix develop`）：

- grep：`grep -nE 'plan::|\.showing =|= Showing::|regenerate_recall|match press' crates/plainly-panel/src/panel.rs` 无输出；`grep -n 'Press::'` 仅 `panel.rs:405,425` 两处按键 / ⟳ 接线实参。
- `plan.rs` 与 `mod plan` 不存在（`plan.rs` 在 commit 里删除）。
- `cargo test -p plainly-panel`：82 passed；`cargo test --workspace`：624 passed, 1 ignored（`keyring_live` 既有 ignored）。GTK 测试在本机有 display，实际执行。
- `cargo clippy --workspace --all-targets`：exit 0；4 条既有警告（`plainly-cli` 3 条 `collapsible_if`、`plainly-desktop` 1 条 `find_map`），`plainly-panel` 0 条。
- spec 漂移已修正：Testing Decisions 的 grep 说法按实际写成「`Press::` 只作为 `start` 实参」；Further Notes 改成「等待 focus 的 Trigger」（评审修正：`pending` 存整个 Trigger、`read` 才安装）；§Problem Statement 标明是改动前快照并去掉失效的 `plan.rs` 链接与漂移行锚。
- README / `docs/manual-checks.md` 未改：零可感知变化；行为等价的边界见 spec Further Notes 的两条（触发时 Prefs 快照、等待 focus 的 Trigger）。
- 评审第三轮的 nit 全部落地：focus 交接抽成可测 helper（`waited_trigger`，含一条无显示测试）、句子绘制用 `expect`、`recall` / `prefs` 改只读 getter、spec §4 的安装时点措辞修正。
- 真机 smoke：未做（可选，不列为完成条件）；本机有 `wayland-1` 会话，随时可补。
