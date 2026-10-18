# 01: readiness: 让 core 拥有「这个 provider 现在能用吗」

**What to build:** core 新增 `readiness` module。它用一个值回答一个 **Provider Profile** 的可用性：`Readiness { requirement, resolution, key, local }`，其中 `resolution: Result<ProviderSetup, SetupError>`、`key: KeyPresence::{Found(KeySource), Missing, Unreadable(SecretError)}`、`local` 由 endpoint 判；派生 `usable() = resolution.is_ok() && (Optional || key is Found)`。入口为 `Readiness::of(name, &Config, &Secrets)` 与纯函数 `local_of(name, &Config)`。运行路径的 `resolve_credential` 与它共用同一个 crate 内部 classifier（输入 requirement + `Secrets::resolve` 的结果，两种投影），公开形状与可见行为不变；值里不带 secret；readiness 不联网、不碰 probe/capability。三个表面一行不动，CI 保持 green（expand 步）。

**Blocked by:** 无（可立即开始）。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §1、§2、§5

**Status:** done (2026-10-18)

- [x] `readiness` module 导出 `Readiness` / `KeyPresence`，`Readiness::of` / `usable` / `local_of` 可用，并由 core 的 lib 重新导出。
- [x] `requirement` 解析成功时等于 endpoint 过滤后的 `ProviderSetup.key`；解析失败时按 spec §2 的同一过滤规则回退（无 preset 或 endpoint 已指到别处 → Optional）。
- [x] `key` 来自恰好一次 `Secrets::resolve`（环境变量 → keyring → 会话内存；失败即停的既有语义），映射为 `Found` / `Missing` / `Unreadable`。
- [x] `usable()` 符合 spec §2 的矩阵：Required + 缺失/读不到 ⇒ 不可用；Optional + 读不到 ⇒ 可用且保留原因。
- [x] `local` 用 `endpoint_of` + `is_loopback`；本地 preset 还没选 model（`NoModel`）时 `local = true`、`usable = false`；未知名字为 false。
- [x] 值里没有 secret；`resolve_credential` 的公开签名不变，Runner 的凭据测试原样通过——它们是「运行路径没有变」的证据。
- [x] 等价性测试：每个 requirement × key 状态下，`Readiness` 的 key 状态与 `resolve_credential` 的结局同构。
- [x] 测试不需要显示、网络或真实 keyring；`Readiness` 不触发 probe。
- [x] `cargo test -p plainly-core` 与 `cargo clippy -p plainly-core --all-targets` 通过。
- [x] 核对 ADR-0004（已随 spec 写好）与本票实现没有出入。

**Comments**

- 实现：`386cbfc`（readiness: add the provider readiness value），合入 `1903c5e`；`Readiness` / `KeyPresence` / `local_of` / `usable` 与共享分类器落地，`Runner` 公开形状与行为不变。
- 测试：`tests/readiness.rs` 8 条矩阵 + 与 `resolve_credential` 的等价性；review 后 `e18a527` 追加 `setup::requirement_of` 的四个单元事实。最终 `cargo test -p plainly-core`：334 passed / 1 ignored（keyring_live）。
