# 04: 面板: worker 改走 Runner，并显示 notice 与未入库警告

**What to build:** 面板 worker 改走 `Runner`，只保留线程、channel、事件与映射；配置 / provider / 凭据 / store / cache / 运行时刻的装配删除。按键仍是 `Recall::Reuse`；⟳ 仍是绕过缓存，并在有可用云端 provider 时带 `Bypass { provider }`。

新增三条用户可感知的行为（这是这次统一的全部面板侧变化）：

- 运行级凭据 notice 在面板状态行显示一次，不重复到每个块；
- store 未入库的块仍展示答案，并附警告说明未保存；
- keyring 读不到不再被静默当作「没有 key」。

面板的正文、标题栏、复制、位置、自动消失与退出路径不变。

**Blocked by:** 02。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §4；三处行为变化见 Further Notes

**Status:** done (2026-10-18)

- [x] worker 不再直接打开配置 / store / cache / secret store，也不再读系统时钟。
- [x] 按键 → `Recall::Reuse`；⟳ → `Bypass { provider }`，本地模型有可用云端 provider 时优先换云端，行为与今天一致。
- [x] 运行级凭据 notice 只显示一次；未入库块展示 artifact + 警告；正常块与失败块的状态不变。
- [x] 面板的纯决策测试更新并通过；不新增 GTK 测试。
- [x] `cargo test -p plainly-panel` 通过；`nix build .` 的 checkPhase 通过（`PLAINLY_TEST_NO_DISPLAY=1` 下如常）。
- [ ] 手测按 spec 的三处行为变化核对面板：notice 一次、未入库警告、重复 Passage 不再重复付费。

## Comments

实现：`b83b3c2`（panel: run through the runner and show its notices）。收尾 commit（05）补了 `Turn.local` 与同一触发内重复 Passage 的去重。

- **`Turn.local` 修正**：面板的 provenance 不再从 `Recall` 推导。旧推导把 `Bypass { provider: Some(_) }` 一律当云端，理由是 `Prefs::regenerate_recall` 只从非 loopback 预置里选；但用户可以把那个预置的 endpoint 改到本机，于是「云端」徽章是错的。现在 `Turn` 带 `local`：`Reuse` / `Bypass { provider: None }` 取配置 setup，`Bypass { Some(_) }` 取覆盖 provider 的 setup，由 runner 回答实际问的是谁（core 测试 `a_turns_local_fact_is_the_provider_it_actually_asked`）。
- **重复 Passage 去重**：worker 用 `Runner::lookup` 的 Lookup Key（与 CLI 同一条规则）把重复段计划成前一次的重复，`first_occurrences` 是纯函数并有单元测试；⟳ 的 `Bypass` 同样适用，不再为同一段付两次。
- **未跑**：`nix build .` 的 checkPhase 与三处行为的真机手测留给 Lead / 人工（本环境没有合成器可按键）。`cargo test -p plainly-panel` 68 passed，core 测试与面板纯决策测试已过；手测项写在 `docs/manual-checks.md` 1.23–1.24、3.10 与 `docs/desktop-manual-checks.md`「历史与搜索」。

**最终 flake 验收（2026-10-18，Lead）：** `nix build .` 通过（含 GUI checkPhase 的 workspace 测试），`scripts/check-flake-packages.sh --no-build` 全过（default 三件套、CLI 闭包无图形库、GUI `env -i` 启动与 ldd、CLI 无环境读 Passage）。真机按键手测仍未做（没有可交互的合成器会话），保留上面的未勾项。
