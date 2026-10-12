# package: `nix build` 的 checkPhase 会跑整个 workspace 的测试

**What to build:** 让 `nix build .` 稳过。它失败在一个和打包无关的地方：`buildRustPackage` 的 checkPhase 用 release 配置跑**整个 workspace** 的测试（`cargoBuildFlags` 只管 build，不管 test），于是把 CLI 测试里一个竞态、以及面板那条需要 display 的测试一起带了进来。

**Blocked by:** 无（01 的包定义已经在了）。

**Spec:** 无（这是测试与打包的接缝，不是产品行为）。

**Status:** done (2026-10-12)

## 现象

```
$ nix build .
…
     Running tests/explain.rs (target/x86_64-unknown-linux-gnu/release/deps/explain-acde50f595bc0df8)
test a_missing_key_is_not_configured_and_nothing_is_sent ... FAILED
thread '…' panicked at crates/plainly-cli/tests/support/mod.rs:127:14:
stdin accepts the test's input: Os { code: 32, kind: BrokenPipe, message: "Broken pipe" }
```

## 两个原因

**一、测试脚手架把"子进程先退出"当成失败。** `plainly_with_session` 把整段 stdin 写进去并 `.expect(...)`。可 `a_missing_key_is_not_configured_and_nothing_is_sent` 测的正是"没有 key 就立刻退出、**不等 stdin**"——子进程先走，父进程的写就撞上 EPIPE。debug 下子进程慢一点，写赶得上，所以本地 `cargo test` 一直是绿的；release 下子进程快得多，`nix build` 必现。紧挨着的那条测试（`a_missing_key_is_reported_without_waiting_for_stdin_to_end`）说明这个行为是**故意**的，所以是脚手架错了，不是产品错了。

修：`write_all` 只容忍 `BrokenPipe`，其它写错误照旧失败；注释写明为什么（并指向那条同伴测试）。

**二、面板那条测试在无 display 的沙箱里会失败。** 它是本轮新加的（见 `.scratch/gloss-examples/`）：GTK 没有 display 就初始化不了，于是测试**断言**"要么有 display，要么 `PLAINLY_TEST_NO_DISPLAY=1`"，而不是静默跳过。checkPhase 正是在没有 display 的沙箱里跑它。

修：`gui` 派生的 `env` 里设 `PLAINLY_TEST_NO_DISPLAY=1` —— 一次 headless 运行**明说**自己 headless，正是这个变量的用途（名字带 `TEST`：应用里没有任何东西读它）。注意 `//` 是浅合并：直接写 `env = { … }` 会把 `common.env.LIBCLANG_PATH` 丢掉，所以写成 `env = common.env // { … };`。

**这条也意味着 `nix build` 绿不代表那条面板测试跑过**：它在 checkPhase 里是被明确跳过的，只有带 display 的 `cargo test -p plainly-panel` 才真的验它。flake 的注释里写了这一句，别把它当成"打包顺手验过了"。

## 验收

- [x] 本地 release 下 `cargo test --release -p plainly-cli` 连跑三次全绿。
- [x] `nix build .` 通过（checkPhase 里 CLI、core、panel、desktop 的测试都跑到）。
- [x] 无 display 且无 opt-in 时，面板那条测试**失败**并打印该设什么变量；带 `PLAINLY_TEST_NO_DISPLAY=1` 时通过（两种都实测过）。

## 不在本票据里

- 让 GUI 派生的 checkPhase 只跑 GUI 两个包的测试（`cargoTestFlags`）：CLI 与 core 的测试就不会在这里再跑一遍，但也就失去"打包时顺手把所有下游测试跑过"这条性质。现在保留全量，代价是这一类失败会先在 `nix build` 里冒出来。
- `nix flake check` 的 `checks` 输出：仍然故意为空（见 [01](01-flake-package-output.md) 第 2 条）。
