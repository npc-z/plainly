# 03: desktop: 设置页从 readiness 得到唯一答案

**What to build:** 桌面主窗口的设置页不再自己回答「这个 provider 能不能用」：`KeyView`、`ProvidersView::usable()`、`first_run` 与 provider 选项/历史/详情里的「本机」标记全部从 core 的 `Readiness` 与 `local_of` 派生，删除对 `Secrets` 与 `environment_provides` 的直接调用。可观察的边缘变化只有一条：环境变量里的 key 不是合法 UTF-8 时，设置页从「可用」变为 `unknown`（Required → 落在设置页；Optional → 仍可用）。同时消费 02 交付的边界事实：范围常量改从 core 取，删除写入前的三处范围比较，捕获 `ConfigError::OutOfRange` 并用现有中文字符串渲染，任何 `set` 失败后重新读配置；用户可见文案与拒绝行为不变。

**Blocked by:** 01（`Readiness` / `local_of`）、02（范围常量与 `OutOfRange`）。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §3（desktop）、§4

**Status:** done (2026-10-18)

- [x] `KeyView` 从 `Readiness` 构造：三种 `Found` 的文案、`Missing` 的 Required/Optional 两条、`Unreadable` 的 `unknown` 文案与 `needed` 均按 spec §3；不再调用 `secrets.resolve` 或 `environment_provides`。
- [x] `ProvidersView::usable` = `Readiness.usable()`；`first_run = !usable()`；`current_problem` 仍来自 resolution 错误。
- [x] provider 选项的 `local` 与历史/详情行的 `local` 走 `local_of`；`view` 里不再出现 `endpoint_of`。
- [x] 非 UTF-8 的 env key：Required 下 `first_run = true` 且文案为 `unknown`；Optional 下仍 `usable`。
- [x] 边界：`BehaviorView` 的 min/max 来自 core 常量；`app` 里不再有范围比较；`OutOfRange` 映射为现有 `auto_hide_out_of_range` / `panel_size_out_of_range` 中文字符串；任何 `set` 失败后重新读取配置，内存句柄不残留半写入的编辑。
- [x] desktop 测试补齐：Required + 读不到 → first_run、Optional + 读不到 → 仍 usable、非 UTF-8 env key；既有 first-run / env / session / bounds 测试保持（文案与 `px` 断言不变）。
- [x] `cargo test -p plainly-desktop` 与 clippy 通过；手测清单仍可照常走（文案不变）。

**Comments**

**实现偏差（已记录，行为不变）**：`write_refusal` 除 `ConfigError::OutOfRange` 外也翻译 `ConfigError::Value`。core 的 0 规则在范围校验之前拒绝写入，只映射 `OutOfRange` 会让设置页显示 core 的英文 zero 消息；为保留 0 既有的中文句子，这条值错误也按 key 的 fallback range 渲染。这是对「OutOfRange only」的良性例外，用户可见文案与拒绝行为不变。

- 实现：`d5100bf`，合入 `0765329`；设置页 `KeyView` / `usable` / `first_run` 与本机标记改走 `Readiness` + `local_of`，范围常量与 `OutOfRange` 映射进 desktop，失败 `set` 后重读配置。
- 测试：desktop 42 lib + 30 window（含 unreadable Required/Optional、非 UTF-8 env key、半写入回滚）。
- 偏差：`write_refusal` 也翻译 core 的零值 `Value`，见上；`e18a527` 已记录。
