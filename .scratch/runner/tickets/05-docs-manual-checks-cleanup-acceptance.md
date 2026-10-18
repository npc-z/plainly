# 05: 收尾: 文档、手测清单、删除残留与验收

**What to build:** 把这次的行为变化写进用户可见文档与手测清单；删除两个表面里残留的重复 helper；跑验收 grep 与全量验证。

文档要写清三处变化：store 写失败保留答案、批次继续、CLI 结尾非零；面板显示凭据 notice；面板对同一次触发里的重复 Passage 复用一次结果。README 的诚实限制 / 验证段落与两份 manual-check 清单按需更新，并新增对应的手测项（CLI 的 store 失败、面板的凭据 notice 与未入库警告）。

**Blocked by:** 03、04。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §7、§Testing Decisions；ADR：[0003](../../../docs/adr/0003-the-runner-owns-one-runs-machine-state.md)

**Status:** ready-for-agent

- [ ] README 与两份手测清单记录了三处行为变化；没有声称未实现的行为。
- [ ] 验收 grep：explain 路径里除 `runner` 外，`Paths::discover` / `ConfigFile::open` / `Store::open` / `Cache::new` / `Secrets::from_process_env` / 本地 `now()` 不再出现。
- [ ] CLI 的本地 `now()` 与不再使用的 helper 删除；`providers` / `history` 仍用得到的 helper 保留。
- [ ] `cargo test -p plainly-core -p plainly-cli`、`cargo clippy --workspace --all-targets`、flake 环境里的 `cargo test --workspace` 全绿；`nix build .` 与 `scripts/check-flake-packages.sh` 通过。
- [ ] 三处行为变化各有端到端证据：CLI store 失败测试、core 的凭据 / 未入库 / Bypass 测试、面板 notice 的纯决策测试或手测记录。
- [ ] 按流程跑 `/code-review`，遗留问题记录在本票的 Comments。
