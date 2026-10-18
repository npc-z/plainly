# 04: 面板: worker 改走 Runner，并显示 notice 与未入库警告

**What to build:** 面板 worker 改走 `Runner`，只保留线程、channel、事件与映射；配置 / provider / 凭据 / store / cache / 运行时刻的装配删除。按键仍是 `Recall::Reuse`；⟳ 仍是绕过缓存，并在有可用云端 provider 时带 `Bypass { provider }`。

新增三条用户可感知的行为（这是这次统一的全部面板侧变化）：

- 运行级凭据 notice 在面板状态行显示一次，不重复到每个块；
- store 未入库的块仍展示答案，并附警告说明未保存；
- keyring 读不到不再被静默当作「没有 key」。

面板的正文、标题栏、复制、位置、自动消失与退出路径不变。

**Blocked by:** 02。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §4；三处行为变化见 Further Notes

**Status:** ready-for-agent

- [ ] worker 不再直接打开配置 / store / cache / secret store，也不再读系统时钟。
- [ ] 按键 → `Recall::Reuse`；⟳ → `Bypass { provider }`，本地模型有可用云端 provider 时优先换云端，行为与今天一致。
- [ ] 运行级凭据 notice 只显示一次；未入库块展示 artifact + 警告；正常块与失败块的状态不变。
- [ ] 面板的纯决策测试更新并通过；不新增 GTK 测试。
- [ ] `cargo test -p plainly-panel` 通过；`nix build .` 的 checkPhase 通过（`PLAINLY_TEST_NO_DISPLAY=1` 下如常）。
- [ ] 手测按 spec 的三处行为变化核对面板：notice 一次、未入库警告、重复 Passage 不再重复付费。
