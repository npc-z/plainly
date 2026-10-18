# 03: 验收：grep、全量测试、spec 漂移与记录

**What to build:** 跑验收与记录，不改产品代码（除非验收暴露缺陷）：两条 grep 验收、`nix develop -c cargo test -p plainly-panel`、`nix develop -c cargo clippy --workspace --all-targets`；核对 spec 的每一条 claim（行号、映射表、测试位置）与实际代码一致，漂移就改 spec；确认 README / `docs/manual-checks.md` 不因本次改动而需要新条目；把验收输出写进本票与 01/02 的 Comments。可选：有 `wayland-1` 会话时做一次最小真机 smoke 并记录，不作为完成条件。

**Blocked by:** 01、02。

**Spec:** [spec.md](../spec.md) §Testing Decisions；§Out of Scope

**Status:** ready-for-agent

- [ ] grep 验收（仓库根，输出为空即通过）：`grep -nE 'plan::|Press::|\.showing =|= Showing::' crates/plainly-panel/src/panel.rs`。
- [ ] `plan.rs` 与 `mod plan` 不存在；`git ls-files crates/plainly-panel/src/plan.rs` 为空。
- [ ] `nix develop -c cargo test -p plainly-panel` 全绿，记录通过数；`nix develop -c cargo clippy --workspace --all-targets` 不是 exit 0 的话记录既有警告与新警告的差别。
- [ ] spec 的 7 条 Implementation Decisions 与测试位置逐条对照代码，漂移已修正。
- [ ] README / `docs/manual-checks.md` 未改，并在 Comments 里说明原因（无可感知变化）。
- [ ] 01 / 02 的 Status 标 `done`，Comments 填本次实现 commit 与验证证据。

## Comments

（实现后填。）
