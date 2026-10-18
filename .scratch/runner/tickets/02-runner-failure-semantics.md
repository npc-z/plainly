# 02: runner: 统一三处失败语义

**What to build:** 在 01 的 interface 上补齐这次真正要统一的三条行为，并让它们由 core 测试钉住。

1. **凭据三态**：`CredentialError::{Missing, Unreadable}`；Required + 缺失、Required + 读不到分别失败；Optional + 读不到 → 打开成功并附一条运行级 notice，turn 继续且不带 key；Optional + 缺失 → 静默继续。绝不把「读不到」当作「缺失」。
2. **未入库**：store 写失败时 `Turn` 仍携带 artifact、`record = None`、附 `NotStored` notice；turn 不因此失败，答案不被丢掉。
3. **Bypass**：`Bypass { provider: Some(name) }` 按 turn 解析覆盖 provider 的 setup 与凭据；成功时绕过缓存、原地覆盖并保留 `created_at` 与 `last_seen`；失败返回 `TurnFailure::Unavailable { provider, error }`，原因可分辨 setup 与凭据。

notice 是结构化枚举，不是预拼好的句子；运行级 notice（凭据）与 turn 级 notice（未入库）分开。

**Blocked by:** 01。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §3、§4

**Status:** done (2026-10-18)

- [x] 凭据三态各有一条 core 测试；Required 缺失与 Required 读到的错误可区分。
- [x] 注入一个读不到的 keyring + Optional provider：open 成功，运行级 notice 只产生一次；turn 正常发出（无 key）。
- [x] store 写失败：artifact 返回、`record = None`、`NotStored` notice；测试用「打开后把目录设成只读」制造，不新增 `Store` trait。
- [x] `Bypass`（不带 provider）：不读缓存、写回覆盖、`created_at` 与 `last_seen` 保留。
- [x] `Bypass`（带 provider）：使用覆盖后的 setup 与凭据；不可用时 `TurnFailure::Unavailable`，setup 与凭据两种原因可分辨。
- [x] 运行级 notice 与 turn 级 notice 的归属明确；两者都不会被拼成面向用户的句子。
- [x] 新测试全部在 core 集成测试里，无网络、显示或真实 keyring。
- [x] `cargo test -p plainly-core` 与 `cargo clippy -p plainly-core --all-targets` 通过；CLI 与面板仍无改动。

## Comments

实现：`df2b1a0`（runner: unify the failure semantics）。

偏差：store 写失败测试没有用「打开后把目录设成只读」——SQLite 在 WAL 模式下会沿已经打开的 `-wal` 文件描述符继续写，只读目录造不出失败。测试改为在 `records` 上装 `BEFORE INSERT ... RAISE(ABORT)` trigger（`crates/plainly-core/tests/runner.rs::refuse_writes`）：失败仍发生在真实的 SQLite 写入处，也没有新增 `Store` trait。`Turn.local` 是 05 补的，见 04 的 Comments。
