# 05: 收尾：文档、grep 验收、全量检查

**What to build:** 把这次的结构决定写进面向维护者的文档，并用 spec §7 的 grep 判据与全量检查证明「只有四处边缘行为变化，其余不变」。README 的「One Passage's run has one owner」段落补上 readiness（可用性是一个 core 值，三个表面只翻译它）；替换 desktop / panel 里被 03、04 取代的旧注释（「复制常量是有意」改成「写严格、读容忍」的说明）；跑全 workspace 测试、clippy、`nix build` 的 checkPhase；逐条核对四处边缘行为变化，其余任何 stdout / stderr / 退出码 / 界面差异都记为 bug 并修回；确认工作树没有残留的临时文件。

**Blocked by:** 03、04。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §7、§Further Notes

**Status:** done (2026-10-18)

- [x] README 的结构性段落补上 readiness 与 `local_of`（不夸大 CI 覆盖面：只写 core 测试覆盖了判定）。
- [x] desktop / panel 里被取代的旧注释已改写成「写严格，读容忍」的说法；没有指向已删除常量的过时注释。
- [x] grep 判据全部成立：`desktop/settings` 无 `secrets.resolve` / `environment_provides`；`desktop/view` 无 `endpoint_of`；`panel/provider` 无 `preset_requirement` / `is_loopback` / `secrets.resolve`；`panel/prefs` 无自己的范围字面量；`desktop/app` 无范围比较；范围数字只在 core `config`；凭据判定只在 readiness 分类器与 `resolve_credential` 投影（例外：CLI `providers discover` 的 `candidate_key`，§3 冻结、仍直接匹配 `secrets.resolve`）。
- [x] `cargo test --workspace`、`cargo clippy`（相关 crate）、`nix build` 的 checkPhase 全绿；panel / desktop 测试在 checkPhase 下通过。
- [x] 行为审计：只有 spec Further Notes 列出的四处边缘变化，逐条由测试或手测证据支持；任何额外的输出差异已修回。
- [x] `git status` 干净（除本 feature 的 spec / ADR / tickets 与实现改动），没有遗留的临时恢复目录或未跟踪文件。
- [x] 在 `docs/adr/0004-provider-readiness-is-one-value.md` 与本票 Comments 里记录实现偏差（如有）。

**Comments**

**文档**

- README「One Passage's run has one owner」补上 readiness：能否现在使用（requirement、凭据状态与来源、endpoint 是否本机）是 core 的一个值 `Readiness`；桌面设置页与面板 ⟳ 读它，CLI 的凭据适配器与运行路径共用它背后的分类器。core 测试覆盖这个共享判定，各表面的测试钉住自己的边缘。
- desktop `settings.rs`、`app.rs` 与 panel `prefs.rs` 中被 03/04 取代的「复制常量是有意 / screen refuses, panel clamps」注释已经换成「写严格、读容忍」的说明。本票把 `prefs.rs` 里仍以「设置页的 range」指代 owner 的三处说法改为「core 的写侧范围」，并把 core `config.rs` 顶部同样归因给设置页的一句改为「range its key declares」。纯注释，无行为改动。

**§7 grep 验收（工作树根目录执行）**

| 判据 | 结果 |
| --- | --- |
| `desktop/settings` 无 `secrets.resolve` / `environment_provides` | 0 处 |
| `desktop/view` 无 `endpoint_of` | 0 处 |
| `panel/provider` 无 `preset_requirement` / `is_loopback` / `secrets.resolve` | 0 处 |
| `panel/prefs` 无自己的范围字面量 | 0 处；三个范围只经 core 常量引用 |
| `desktop/app` 无范围比较 | 0 处；三个常量只在 `write_refusal` 里用于把 core 的 zero 错误还原成原有中文句子，不是比较 |
| 范围数字只在 core `config` | `(5, 300)` / `(320, 900)` / `(240, 1200)` 只出现在 `crates/plainly-core/src/config.rs`；测试里的一次断言在 `core/tests/config_file.rs` |
| 凭据判定只在 readiness 分类器与 `resolve_credential` 投影 | 对 store 结果的唯一分类是 `readiness.rs` 的 `classify`；`runner.rs` 的 `resolve_credential` 只把 `Judgement` 投影成运行态；CLI / desktop / panel 不再自行判定。**已记录的例外**：CLI `providers discover` 的 `candidate_key` 仍直接匹配 `secrets.resolve` 的结果（`cli/src/commands/providers.rs:81`），§3 冻结该输出、本规格不改。 |

**行为审计（只有 spec Further Notes 的四处边缘变化）**

1. panel 云候选的 requirement 取 endpoint 过滤后的结果：`panel/src/provider.rs:255` 的 `a_shipped_preset_pointed_at_another_remote_host_is_offered_without_a_key`；core 侧同规则见 `core/tests/readiness.rs:220`。
2. panel 云候选的本机性按配置 endpoint 判断：`provider.rs:224` 的 `a_shipped_preset_configured_to_loopback_is_not_a_cloud_candidate`（既有的 `a_hosted_name_pointed_at_loopback_is_local` 仍通过）。
3. desktop 的非 UTF-8 env key 从「可用」变为 unknown：`desktop/tests/window.rs:759`（Required → first_run）与 `:800`（Optional → 仍可用）。
4. `plainly config set` 越界从 exit 0 变为 exit 2 且不写文件：`cli/tests/cli.rs:136` 与边界值 `:169`；desktop 的拒绝文案不变由 `window.rs:957` 钉住，拒绝后不残留半写入由 `window.rs:1004` 钉住。

其余无可见差异：panel / desktop / cli 的既有断言未改（只重构了 desktop 测试 helper），全部通过；`runner` 的凭据测试在 diff 中未被修改且通过，说明运行路径没有变；`setup` 的改动只是把 endpoint 过滤移进 `sources`，`resolve` / `endpoint_of` 的结果与错误不变（既有 `setup` 测试全过）。

**验证**

- `cargo clean -p ...`（共享 `CARGO_TARGET_DIR`）：exit 0，删除 3586 files / 4.8 GiB。
- `cargo test --workspace`：exit 0，**606 passed / 0 failed / 1 ignored**（`keyring_live` 需要真实 keyring）；正常显示环境下直接通过，无需 `PLAINLY_TEST_NO_DISPLAY=1`。分 crate：plainly-cli 129、plainly-core 334（+1 ignored，含新增的 `setup::tests` 1 条）、plainly-desktop 72、plainly-panel 71。
- `cargo clippy -p plainly-core -p plainly-cli -p plainly-desktop -p plainly-panel --all-targets`：exit 0；仍只有 4 条既有 warning（1 条 `unnecessary_find_map` 在 `cli/src/commands/explain.rs:243`，3 条 `collapsible_if` 在 `cli/tests/support/provider.rs:268`、`cli/src/commands/providers.rs:298`、`desktop/src/copy.rs:122`）；评审修复未新增 warning（`write_refusal` 的一次匹配版本曾引入 `type_complexity`，已改为局部 `Refusal` enum 消除）。
- `nix build .`：exit 0；GUI derivation 的 checkPhase 确实运行（`Running phase: checkPhase` → `cargo test --release --locked`，环境含 `PLAINLY_TEST_NO_DISPLAY=1`），36 个 test binary 全绿，**606 passed / 0 failed / 1 ignored**；生成的 `result` 符号链接已删除。
- `git status --short`：只有本 feature 的 spec / ADR / tickets 与实现改动；无未跟踪的日志、`result` / `gen` 或临时恢复目录。

**偏差**：行为实现与 spec / ADR 没有实质偏差；但评审发现文档把 CLI 凭据适配器写成了 `Readiness` 的读者，且 §7 的唯一性声明漏了 §3 冻结的 CLI `providers discover`。已改 README、ADR-0004、spec §6/§7 与本票对应段落；分类器的内部形状按评审收紧（不再携带 `required`），公开形状与行为不变。

**评审修复（review findings）**

- A1 文档真实性：README「One Passage's run has one owner」与 ADR-0004 改为「桌面设置页与面板 ⟳ 读 `Readiness`；CLI 凭据适配器与运行路径共用其背后的分类器」；测试覆盖改为「core 测试覆盖共享判定，各表面测试钉住自己的边缘」。
- A2 §7 例外：spec §7 与上表、验收清单都记录了 `providers discover` 的 `candidate_key` 仍直接匹配 `secrets.resolve`（§3 冻结），不再声称凭据判定 match 唯一。
- A3 §6 迁移：补记 `setup` 的 `Sources`（`shipped` / endpoint 过滤后的 preset）与 `requirement_of`，理由为失败路径必须复用同一 endpoint 过滤。
- B1：`setup.rs` 新增 `#[cfg(test)]` 单元测试，钉住 `requirement_of` 的四个事实：deepseek 默认 endpoint → Required；deepseek 指到别的 host → Optional；未知名字 → Optional；本地 preset 未选 model（`NoModel`）→ Optional。
- C1：`KeySpec::parse` 一次匹配同时得出 `(kind, range)`；desktop `write_refusal` 一次匹配得出 (fallback range, 文案)，再让错误自身的 min/max 优先。行为不变。
- C2：`Sources.preset` 改名为 `endpoint_matched`，字段文档同步。
- C3：`Judgement::{Missing, Unreadable}` 不再携带 `required`；新增 `KeyPresence::blocks(requirement)` 作为唯一的阻塞规则，`Readiness::usable` 与 `resolve_credential` 都消费它。公开 `Readiness` / `KeyPresence` 形状与全部行为不变。
- C4 **拒绝**：不抽取 `core/tests/readiness.rs` 与既有测试重复的 `SecretStore` fake。理由：spec §5 明确不新增 mock 层 / 第三条 seam；仓库的集成测试二进制各自保留局部 fake（runner / secrets）；抽取只会把互不相关的测试二进制耦合起来，没有行为收益。只修正 `tests/readiness.rs` 的模块文档——它们是本地副本，不是「runner 与 secrets 测试用的同一批 fake」。

- 实现：`33aa16b`；README + 注释 + §7 grep + 行为审计；`nix build .` 的 checkPhase 全绿。
- review 修复：`e18a527`（README/ADR/spec §5/§6/§7 的事实修正、`setup` 单元测试、key cascade 合并、`Judgement` 去重；C4 拒绝理由见上）。
- 最终验证：`cargo test --workspace` 606 passed / 0 failed / 1 ignored；clippy 无新 warning；`nix build .` checkPhase 606 passed / 0 failed / 1 ignored。
- caveman review pass 2：`Readiness::of` 在 `resolution` 失败时不再询问 secret stores（新增 `does_not_ask_the_stores` 测试）；desktop 的 settings patch 改为在 `ConfigFile` 副本上写完再换入，`set` 被拒不再需要重读、也不会留下半写状态；`Judgement::blocks` 收掉一次投影；`Readiness::of` 的 `local` 复用已解析 setup。行为不变。
