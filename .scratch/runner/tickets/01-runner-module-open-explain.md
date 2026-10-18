# 01: runner: 打开机器状态并跑通一个 Passage

**What to build:** core 新增 `runner` module。它能用注入的路径、secret store 与运行时刻，按 config → provider setup → 凭据 → store → 能力缓存 → prompt 的顺序打开一次运行，并对一个 **Passage** 执行 `Reuse` 回合：命中缓存时从历史库取回 **Record** 并前移 `last_seen`；未命中时调用现有 `explain::run`，把结果写入历史库，返回 `Turn`。`Turn` 携带 artifact、`record: Option<Record>`、`from_cache`、`resolution`、`downgrade`；失败沿用 `TurnFailure::Run(RunFailure)`。新增 `Timestamp::now()` 作为系统时钟的唯一转换入口。CLI 与面板一行不动，CI 保持 green（expand 步）。

**Blocked by:** 无（可立即开始）。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §1、§2、§3

**Status:** done (2026-10-18)

- [x] `runner` module 提供 `Runner::open(prompt, now)`（生产：内部取路径与进程 secret store）与 `Runner::open_at(paths, secrets, prompt, now)`（测试注入）。
- [x] `PromptChoice::{Configured, Factory}` 可用；CLI 的 `--factory-prompt` 将来映射为 `Factory`。
- [x] open 按 config → 配置中 provider 的 setup → 凭据 → store → 能力缓存 → prompt 的顺序执行；paths / config / store / setup 失败各返回结构化 `RunnerError`，不 panic。
- [x] 一个 `Reuse` 回合命中缓存：provider 零调用、`from_cache = true`、`record = Some`、`last_seen` 前移，返回的 artifact 与库里的 Record 一致。
- [x] 未命中：provider 恰好调用一次、结果经 `Store::remember` 写入、`record = Some`、`resolution` 与 `downgrade` 来自本次 run。
- [x] 现有 `explain::run` 的 probe / retry / downgrade 语义经新 seam 透传为 `TurnFailure::Run`，字段不丢；`explain::run` 的公共 interface 不变。
- [x] `Timestamp::now()` 是唯一读取系统时钟的 helper；core 其余路径仍只接收 instant 值。
- [x] 新增 core 集成测试用 scripted provider、内存或临时 store、注入的路径与 secret store 驱动，不依赖网络、显示或真实 keyring。
- [x] `cargo test -p plainly-core` 与 `cargo clippy -p plainly-core --all-targets` 通过；CLI 与面板无改动。

## Comments

实现：`dcf5697`（runner: add the core run module）。`cb2a955`（03）后来补了 `Runner::lookup` / `cached` / `level`，01 的 open 顺序与 `Turn` 字段由 `dcf5697` 钉住。

偏差：测试 seam 用 `#[doc(hidden)] with_transport` 注入 scripted endpoint —— 形状就是 `explain::run` 已有的 connector，返回现有 `Endpoint` 缝，没有新增 trait，也没有起本地 HTTP server。`with_pause` 同在 03 加入，让 CLI 用同一个 seam 打印退避等待。
