# 04: panel: ⟳ 的云端候选与本机提示从 readiness 得到

**What to build:** 面板不再自己回答「这个 provider 能不能用、是不是本机」：配置 provider 的 `local` 走 core 的纯函数 `local_of`；⟳ 的云端候选条件改为「**配置里**解析出的 endpoint 不是本机，且该 name 的 `Readiness.usable()` 为真」，删除 `usable` / `preset_requirement` / `is_loopback` 的本地推理，候选的 `local` 由同一判断派生而不是硬编码。可观察的边缘变化有两条：云端 preset 被配置到本机时不再是云候选（会考虑下一个 shipped preset）；被配置到别的远程服务时按 endpoint 过滤后的 requirement 判断（Optional，无 key 也提供）。没有云端候选时仍静默（按钮不出现），运行级 notice 继续由 `Runner` 发。同时消费 02：`prefs` 的范围常量改从 core 取，宽高夹取与时长回退行为逐字不变。

**Blocked by:** 01（`Readiness` / `local_of`）、02（范围常量）。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §3（panel）、§4

**Status:** done (2026-10-18)

- [x] `ProviderChoice` 的 `local` 走 `local_of(config.app.provider, config)`；`provider` 里不再出现 `is_loopback`。
- [x] `cloud_alternative` = shipped presets 中第一个「`!local_of(name, config)` 且 `Readiness::of(name, config, secrets).usable()`」的名字；`usable` 与 `preset_requirement` 删除；候选的 `local` 由 `local_of` 派生，不硬编码。
- [x] 边缘：`deepseek` 被指到 loopback → 不是云候选（openai 可用则选 openai）；被指到别的远程服务且没有 key → 仍是云候选。
- [x] 没有云端候选时不新增文案/提示；`local_note`、`regenerate_recall`、hosted provider 上 ⟳ 的语义不变。
- [x] `prefs` 的宽/高/时长范围改为引用 core 常量；`size()` 夹取与 `autohide()` 回退到 20 秒的行为与文案不变。
- [x] panel 测试补齐：坏 keyring → 无云候选（不 panic、不新增提示）；preset 指回环；preset 指远程无 key；既有 provider / prefs / note / ⟳ 测试保持。
- [x] `cargo test -p plainly-panel` 与 clippy 通过。

**Comments**

- 实现：`697dec5`，合入 `0765329`；panel 云候选与本机性改走 `Readiness` + `local_of`，`preset_requirement` / `usable` / `is_loopback` 删除，prefs 常量改引 core。
- 测试：panel 71 passed（含 broken keyring、preset 指回环、preset 指远程无 key；边缘用例在旧代码上是红的）。
