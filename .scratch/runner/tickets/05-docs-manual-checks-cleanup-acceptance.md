# 05: 收尾: 文档、手测清单、删除残留与验收

**What to build:** 把这次的行为变化写进用户可见文档与手测清单；删除两个表面里残留的重复 helper；跑验收 grep 与全量验证。

文档要写清三处变化：store 写失败保留答案、批次继续、CLI 结尾非零；面板显示凭据 notice；面板对同一次触发里的重复 Passage 复用一次结果。README 的诚实限制 / 验证段落与两份 manual-check 清单按需更新，并新增对应的手测项（CLI 的 store 失败、面板的凭据 notice 与未入库警告）。

**Blocked by:** 03、04。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §7、§Testing Decisions；ADR：[0003](../../../docs/adr/0003-the-runner-owns-one-runs-machine-state.md)

**Status:** done (2026-10-18)

- [x] README 与两份手测清单记录了三处行为变化；没有声称未实现的行为。
- [x] 验收 grep：explain 路径里除 `runner` 外，`Paths::discover` / `ConfigFile::open` / `Store::open` / `Cache::new` / `Secrets::from_process_env` / 本地 `now()` 不再出现。
- [x] CLI 的本地 `now()` 与不再使用的 helper 删除；`providers` / `history` 仍用得到的 helper 保留。
- [x] `cargo test -p plainly-core -p plainly-cli`、`cargo clippy --workspace --all-targets`、flake 环境里的 `cargo test --workspace` 全绿；`nix build .` 与 `scripts/check-flake-packages.sh` 通过（clippy 仅 3 条 pre-existing warning，exit 0）。
- [x] 三处行为变化各有端到端证据：CLI store 失败测试、core 的凭据 / 未入库 / Bypass 测试、面板 notice 的纯决策测试或手测记录。
- [x] 按流程跑 `/code-review`，遗留问题记录在本票的 Comments。

## Comments

本票的收尾 commit 在 `ticket/05` 上（`runner: close out the run refactor`），合并前尚未进入 `runner-integration`。

**验收 grep（在 worktree 根执行，输出为空即通过）：**

```sh
grep -nE 'Paths::discover|ConfigFile::open|Store::open|Cache::new|Secrets::from_process_env' \
  crates/plainly-cli/src/commands/explain.rs crates/plainly-panel/src/worker.rs
grep -rnE 'fn now\(|SystemTime::now' crates/plainly-cli/src crates/plainly-panel/src
```

两条都没有输出（exit 1）。口径是 spec §7 的 **explain 路径**：`commands/explain.rs` 与 `panel/src/worker.rs`（测试文件不算表面路径）。`Timestamp::now` 只被 `crates/plainly-cli/src/commands/explain.rs`、`crates/plainly-cli/src/commands/providers.rs`、`crates/plainly-panel/src/worker.rs` 调用；core 的转换在 `crates/plainly-core/src/artifact.rs`。

桌面主窗口不在这条路径上，仍保留它既有的注入时钟 seam：`crates/plainly-desktop/src/app.rs` 的 `Clock` / `SystemClock`（窗口时间戳）与 `single.rs` 里读文件时间戳的 `SystemTime::now` 都是 pre-existing，不参与一次 Passage 的 turn；本次没有动，也不在票 01–05 的范围内。把 grep 扩到 `crates/plainly-desktop` 会看到这两处，这是那条命令唯一会非空的来源。

**验证：** `cargo test --workspace` 580 passed / 0 failed / 1 ignored（`keyring_live`，需要真 keyring）；其中 `plainly-core` 的 `tests/runner.rs` 21 passed、`plainly-panel` 68 passed。`cargo clippy --workspace --all-targets` 通过（3 条 pre-existing `collapsible_if`：`commands/providers.rs:305`、`tests/support/provider.rs:268`、`desktop/src/copy.rs:122`，都不在本次改动行上）。`scripts/check-headless-cli.sh` 通过（依赖图 `none`，`env -i` 链接检查无图形库）。

**留给 Lead 的最终 flake 验收：** `nix build .`（checkPhase）与 `scripts/check-flake-packages.sh` 未在本票跑——按分工由 Lead 在合并后执行，所以上面第 4 个框保持未勾。

**review 遗留：** 代码评审 flagged 的 provenance 回归已修：面板不再从 `Recall` 推导 `local`，`Turn.local` 由 runner 按实际 setup 给出（core 测试 `a_turns_local_fact_is_the_provider_it_actually_asked`）。另补了同一触发内重复 Passage 的去重（worker 的 `first_occurrences`，纯决策测试）。除此之外没有未处理的 review 项。

**文档：** README 的「What to believe」新增第三条结构事实（Runner 是一个 Passage 运行状态的唯一 owner，面板策略因此进 CI）与两条诚实限制（未入库保留答案 + exit 1；凭据读不到 ≠ 没有 key）；`docs/manual-checks.md` 新增 1.23（面板未入库警告）、1.24（重复 Passage 只问一次）、3.10（CLI store 失败 + exit 1）及凭据 notice 无手测步骤的说明（由 core 测试覆盖）；`docs/desktop-manual-checks.md` 增加一条「未入库的记录不进历史」的跨表面项，并说明主窗口不运行 Passage。

**review 修复（2026-10-18，`runner-integration`）：** store 读失败不再被当成 cache miss——`Runner::cached` 返回 `Result<Option<Record>, StoreError>`，`TurnFailure::Store(StoreError)` 在调用 provider 之前返回；CLI 计划阶段的读错误走既有 store 错误映射中止整次运行、turn 中的读错误按失败块处理，面板把该 turn 映射为带 store 原因的块失败；core 新增 `a_history_that_cannot_be_read_fails_the_turn_without_asking_the_provider`（重命名 `records` 表制造真实读失败，断言 provider 零调用、`cached` 返回 `Err`）。凭据措辞移回表面（ADR-0003）：core 的 `CredentialError::Missing` 只陈述 provider 与变量，CLI 保留原 NotConfigured 句子，面板在 `strings.rs` 里给「缺 key / 读不到凭据存储」两句中文，不再对 `RunnerError` 直接 `to_string()`。README 的 CI 覆盖说法与 ADR-0003 的维护 grep 收窄到 explain 路径（并注明面板 `prefs.rs` 的每次触发配置读取不在检查内）。CLI 的本地计划结构 `Turn` 改名 `PlannedTurn`，避免与 `plainly_core::Turn` 混淆。验证：`cargo test --workspace` 581 passed / 1 ignored，clippy 仅 3 条 pre-existing warning，`scripts/check-headless-cli.sh` 通过；`nix build` 仍留给 Lead。

**最终 flake 验收（2026-10-18，Lead）：**

- `cargo test --workspace`（flake 工具链，display 可用）：**581 passed / 0 failed / 1 ignored**（`keyring_live` 需真 keyring）；core 321、CLI 127、panel 68、desktop 65。
- `cargo clippy --workspace --all-targets`：exit 0，仅 3 条 pre-existing `collapsible_if`（`commands/providers.rs:305`、`tests/support/provider.rs:268`、`desktop/src/copy.rs:122`），不在本次改动行。
- `scripts/check-headless-cli.sh`：通过（依赖图 `none`、`env -i` 无图形库）。
- `nix build .`：**通过**；default 输出 `/nix/store/5q6c8bq31mn8hsip8k78vrzvkvlp7pq8-plainly-0.1.0`，CLI 包输出 `/nix/store/8r4121vjijp5hx1b20f2j8hmrncbv4vl-plainly-0.1.0`；两个包的 checkPhase 都跑过。
- `scripts/check-flake-packages.sh --no-build`：**OK**——`nix run .` / `.#plainly` 到 `plainly 0.1.0`；default 三件套 + `.desktop` + 512×512 图标；CLI 运行时与 build inputs 无图形库（5 paths vs 187）；GUI `env -i` 启动到参数处理、`ldd` 无缺失（panel 67 / desktop 156 个库）；CLI 在空环境下给出「no API key」的可读拒绝。
- **flake 同步（`56afecc`）**：CLI 的 store-failure 测试引入 `rusqlite` dev-dependency 后，flake 里手写的 `cliLock` 需要补 `rusqlite` 到 `plainly-cli` 的依赖列表；根 `Cargo.lock` 变化使 GUI 的 `fetchCargoVendor` 固定哈希变化，`guiCargoDeps` 同步更新。这正是架构评审候选 4 记录的「第二份 workspace 手抄」成本，已在票 01–05 之外单独提交。
- 手测：面板的三处行为真机手测与票 04 的手测框仍未做——本环境没有可交互的合成器按键路径；`docs/manual-checks.md` 的 1.23/1.24/3.10 与凭据 notice 的说明已就位，供人工执行。

**第二轮评审修复（2026-10-18，`/caveman-review`）：**

- `Notice` 拆成 run-level 的 `RunNotice` 与 turn-level 的 `TurnNotice`，CLI/panel 不再为不可能的分支写匹配；
- 凭据策略收进 core 的 `resolve_credential`/`CredentialState`，CLI 的 `providers` 与 explain 共用一套三态判断，CLI 自己的 `resolve_key`/`credential_error` 只保留措辞与退出码；
- `Turn.record` 与 `Turn.local` 的文档修正（未入库不等于 miss；cache hit 没有 endpoint 真正回答，locality 来自当前解析出的 setup）；
- 面板的「未入库」文案不再承诺「下次触发会重新生成」（失败的 regenerate 会保留旧 Record），README 同步限定；
- worker 的重复 Passage 规划与 notice 游标移到已测的 `state` module，去掉 worker 线程里的 `expect`；未入库警告改用独立的 `plainly-warning` 样式；
- 面板 dedupe 的注释写明「Bypass 的 override 在单个 trigger 内恒定，所以用 configured Lookup Key 分组等价」。

**刻意未做：** `Runner` interface 继续变宽的架构提醒（下一次加访问器应拆 planning/turn）、`cliLock` 手抄第二份 workspace（架构评审候选 4）、CLI 计划阶段与 turn 的重复 store recall（一次便宜索引读，避免为此扩大 interface）。
