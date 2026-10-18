# 04: scripts: 守卫"改源码不失效依赖缓存"

**What to build:** 一条廉价的手动检查，把"改 `.rs` 不会让依赖缓存失效"变成可重复验证的事实，而不是只存在于这次改造的注释里。它守的是 crane 的依赖缓存结构：有人给 dummy source 加覆盖、换掉 fold 成固定输出的 vendor 来源、或者升级 nixpkgs / crane 改变行为时，这个脚本会说 FAIL。

**Blocked by:** [03](03-gui-package-crane-deps-cache.md)（要两个包都迁到 crane 之后，检查才有两个对象）

**Spec:** 无（这是构建性质的回归守卫，不是产品行为）。

**Status:** done (2026-10-17)

- [x] 新增 `scripts/check-incremental-build.sh`：从干净树 `git archive HEAD` 出两份拷贝（一份原样、一份在 `plainly-core` 的源文件末尾追加一行注释），分别 eval 两个公开包的依赖派生，**比较依赖派生的输出路径**；两个包都相同才 exit 0，并打印每个包的那条路径。
- [x] 关键测量对象是 **deps 的输出路径，不是 `.drv` 路径**：`fetchCargoVendor` 是 fixed-output derivation，源码改动会让它的 `.drv` 路径变，但输出路径被折叠成固定哈希；依赖缓存复用靠的是后者（原型已验证）。
- [x] 反向自检：把对照从"追加注释"换成"改 `Cargo.lock`"（或脚本里临时做等效改动）时脚本必须报 FAIL 并列出前后路径。
- [x] `docs/manual-checks.md` 加一行：升级 nixpkgs / fenix / crane 输入后跑一次；说明它只 eval、不构建（原型实测约 26 s）。
- [x] 不新增 flake 输出：依赖派生从 `nix derivation show` 的输入里按 `-deps-` 名字发现；脚本不依赖未提交的工作区改动（用 `git archive HEAD`，文档写明这一点）。
- [x] 不进 CI、不改任何 CI 文件（仓库 CI 刻意不装 Nix，`AGENTS.md` 有理由）。

## 为什么放在 scripts/ 而不是 CI

仓库的 CI 只证明两个无图形栈的 crate 能在裸 Rust 上构建，装 Nix 会把"便宜的事"变成"昂贵的事"。而这个守卫的价值只在**升级 flake 输入**时出现，天然是低频手动动作，所以脚本 + 一行文档是它的正确位置。

## 不在本票据里

- 性能阈值断言（机器不同，数字不稳定）。
- 把守卫做成 CI job。

**2026-10-17 — 真仓库落地与验证（提交前，含两轴 review 后的修正）**

- 脚本：`scripts/check-incremental-build.sh`。从 `git archive HEAD` 出 `base` / `edited` 两份树，`edited` 里给 `crates/plainly-core/src/lib.rs` 追加一行注释；对 `plainly` 与 `default` 各 eval 一次，顺着 `nix derivation show` 的输入找到 `*-deps-*` 派生，取它的 **`outputs.out.path`** 比较。`default` 是 `symlinkJoin`，所以先下一跳找 `plainly-desktop-*` 再找它的 deps。house style 跟 `check-flake-packages.sh`：`NIX_CACHE_HOME` 可写缓存、`usage()` / `fail()`、失败证据走 stderr 且带 `FAIL:` 前缀。
- 正向（应该通过）：首跑 **14.6 s**、热缓存 **8.1 s**，两个输出都不变：
  - `plainly` → `y3alyijwwmp3pds48jkc641wy8q0camd-plainly-deps-0.1.0`
  - `default` → `0mh60dczw52nwwwr14sik5msgvfdi9ff-plainly-desktop-deps-0.1.0`
- 反向（`--self-test`，应该让两处检查都判失效）：首跑 **11.2 s**、热缓存 **11.7 s**。Spec 轴纠正过我最初的一个错误说法——**改 lockfile 是会移动依赖输出路径的**（crane 的 dummified source 会带 repo 的 `Cargo.lock`），只是 CLI 用的是 flake 里手同步的 `cliLock` 字面量、对根 `Cargo.lock` 不敏感。所以自检现在是**两个包各用一个 lock 级控制**：根 `Cargo.lock`（GUI）与 `cliLock` 字面量（CLI），并要求**两个都失效**才算通过——否则某个包的检查即使变成恒真，自检也看不出来。两个控制下的实测：
  - `plainly`：`y3aly…` → `xrn3l1bz4qavbhp6s66agwnnxks5fqs5-plainly-deps-0.1.0`
  - `default`：`0mh60…` → `cxx90rkscijsqm6pm0m8qh5485b5l9d0-plainly-desktop-deps-0.1.0`
- 自检的语义边界写在脚本与文档里：它证明"两个包的检查都对各自的 lock 敏感"，正常模式的报错路径由正向/热缓存的实际运行覆盖；它不模拟一次真正的 guard `exit 1`（那需要一个会被误报的输入）。
- `docs/manual-checks.md` §0 加了说明：**先提交 `nix flake update` 的结果**再跑（脚本 archive 的是 `HEAD`，未提交的 bump 看不见——这是 Spec 轴抓到的、我最初写反的地方）；只 eval 不构建、约 10–15 s；`--self-test` 改两个包各自的 lock 来源。
- 没动 CI（仓库 CI 刻意不装 Nix）；没新增 flake 输出（deps 派生按 `-deps-` 名字从 `nix derivation show` 的输入里发现）；脚本依赖 `python3`（dev shell 已声明）与 `nix`，不需要 `jq`。
- Review：Standards 轴指出 house-style 差距（`NIX_CACHE_HOME`、失败走 stderr、不可达分支与 package 表、命名），已全部修正；Spec 轴指出自检对 `Cargo.lock` 的判断错误、`invalidated > 0` 不能证明两处都活着、以及文档的"先升级后跑"与 `git archive HEAD` 矛盾——均已按上面重写并重测。

## Comments

**2026-10-18 — 后记**：CLI 的合成 workspace 数据已不再是 flake 里的手抄字面量——manifest 与 lock 由 `scripts/generate-cli-workspace.sh` 从根 `Cargo.toml` / `Cargo.lock` 派生、提交在 `nix/cli-workspace/`。本票的自检随之改为「往 `nix/cli-workspace/Cargo.lock` 追加一行」（GUI 仍控制根 `Cargo.lock`），脚本里那段在 flake 源码里切片找 `cliLock` 的 Python 已删除；同步守卫是 `scripts/check-cli-workspace.sh`。决定与验收见 `.scratch/flake-workspace/spec.md`。
