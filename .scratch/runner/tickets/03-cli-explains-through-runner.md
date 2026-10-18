# 03: CLI: explain 路径改走 Runner

**What to build:** CLI 的 explain 端到端改走 `Runner`，删掉 CLI 自己那份 paths / config / setup / 凭据 / store / cache / 运行时刻装配。Passage 的来源（stdin / 文件 / 剪贴板与敏感内容拒绝后的结果）、切分、批次规划与重复 Passage 去重、stdout 的 markdown / JSON、逐块 stderr 进度、退出码全部保持现状。

唯一行为变化：store 写失败不再中止整批——已付费的答案继续输出、stderr 说明未入库、结尾以非零退出码表示有未入库 / 失败块。`RunnerError` 按 spec 映射到既有退出码；运行级凭据 notice 在 stderr 出现一次；`TurnFailure::Run` 仍是 exit 1 且后续块继续。

**Blocked by:** 02。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §4、§6、§7；三处行为变化见 Further Notes

**Status:** ready-for-agent

- [ ] CLI explain 不再直接打开配置、store、缓存、secret store，也不再自带本地 `now()`；这些由 `Runner` 承担。
- [ ] `--regenerate`、`--factory-prompt`、`--format`、文件 / stdin / 剪贴板来源、批次进度、去重提示与 stdout 形状保持不变。
- [ ] 退出码 0 / 1 / 2 / 3 / 4 的既有含义不变；`RunnerError` 的映射符合 spec §2。
- [ ] store 写失败：artifact 仍在 stdout、stderr 说明未入库、后续块继续、结尾 exit 1；新增一条真实二进制的测试覆盖它。
- [ ] 运行级凭据 notice 只在 stderr 出现一次；stdout 仍然只有产物。
- [ ] `cargo test -p plainly-cli` 全绿；现有断言除新增用例外不改。
- [ ] `cargo clippy -p plainly-cli --all-targets` 通过；`scripts/check-headless-cli.sh` 仍通过。
