# 01: runner: 打开机器状态并跑通一个 Passage

**What to build:** core 新增 `runner` module。它能用注入的路径、secret store 与运行时刻，按 config → provider setup → 凭据 → store → 能力缓存 → prompt 的顺序打开一次运行，并对一个 **Passage** 执行 `Reuse` 回合：命中缓存时从历史库取回 **Record** 并前移 `last_seen`；未命中时调用现有 `explain::run`，把结果写入历史库，返回 `Turn`。`Turn` 携带 artifact、`record: Option<Record>`、`from_cache`、`resolution`、`downgrade`；失败沿用 `TurnFailure::Run(RunFailure)`。新增 `Timestamp::now()` 作为系统时钟的唯一转换入口。CLI 与面板一行不动，CI 保持 green（expand 步）。

**Blocked by:** 无（可立即开始）。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §1、§2、§3

**Status:** ready-for-agent

- [ ] `runner` module 提供 `Runner::open(prompt, now)`（生产：内部取路径与进程 secret store）与 `Runner::open_at(paths, secrets, prompt, now)`（测试注入）。
- [ ] `PromptChoice::{Configured, Factory}` 可用；CLI 的 `--factory-prompt` 将来映射为 `Factory`。
- [ ] open 按 config → 配置中 provider 的 setup → 凭据 → store → 能力缓存 → prompt 的顺序执行；paths / config / store / setup 失败各返回结构化 `RunnerError`，不 panic。
- [ ] 一个 `Reuse` 回合命中缓存：provider 零调用、`from_cache = true`、`record = Some`、`last_seen` 前移，返回的 artifact 与库里的 Record 一致。
- [ ] 未命中：provider 恰好调用一次、结果经 `Store::remember` 写入、`record = Some`、`resolution` 与 `downgrade` 来自本次 run。
- [ ] 现有 `explain::run` 的 probe / retry / downgrade 语义经新 seam 透传为 `TurnFailure::Run`，字段不丢；`explain::run` 的公共 interface 不变。
- [ ] `Timestamp::now()` 是唯一读取系统时钟的 helper；core 其余路径仍只接收 instant 值。
- [ ] 新增 core 集成测试用 scripted provider、内存或临时 store、注入的路径与 secret store 驱动，不依赖网络、显示或真实 keyring。
- [ ] `cargo test -p plainly-core` 与 `cargo clippy -p plainly-core --all-targets` 通过；CLI 与面板无改动。
