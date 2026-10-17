# 04: scripts: 守卫"改源码不失效依赖缓存"

**What to build:** 一条廉价的手动检查，把"改 `.rs` 不会让依赖缓存失效"变成可重复验证的事实，而不是只存在于这次改造的注释里。它守的是 crane 的依赖缓存结构：有人给 dummy source 加覆盖、换掉 fold 成固定输出的 vendor 来源、或者升级 nixpkgs / crane 改变行为时，这个脚本会说 FAIL。

**Blocked by:** [03](03-gui-package-crane-deps-cache.md)（要两个包都迁到 crane 之后，检查才有两个对象）

**Spec:** 无（这是构建性质的回归守卫，不是产品行为）。

**Status:** ready-for-agent

- [ ] 新增 `scripts/check-incremental-build.sh`：从干净树 `git archive HEAD` 出两份拷贝（一份原样、一份在 `plainly-core` 的源文件末尾追加一行注释），分别 eval 两个公开包的依赖派生，**比较依赖派生的输出路径**；两个包都相同才 exit 0，并打印每个包的那条路径。
- [ ] 关键测量对象是 **deps 的输出路径，不是 `.drv` 路径**：`fetchCargoVendor` 是 fixed-output derivation，源码改动会让它的 `.drv` 路径变，但输出路径被折叠成固定哈希；依赖缓存复用靠的是后者（原型已验证）。
- [ ] 反向自检：把对照从"追加注释"换成"改 `Cargo.lock`"（或脚本里临时做等效改动）时脚本必须报 FAIL 并列出前后路径。
- [ ] `docs/manual-checks.md` 加一行：升级 nixpkgs / fenix / crane 输入后跑一次；说明它只 eval、不构建（原型实测约 26 s）。
- [ ] 不新增 flake 输出：依赖派生从 `nix derivation show` 的输入里按 `-deps-` 名字发现；脚本不依赖未提交的工作区改动（用 `git archive HEAD`，文档写明这一点）。
- [ ] 不进 CI、不改任何 CI 文件（仓库 CI 刻意不装 Nix，`AGENTS.md` 有理由）。

## 为什么放在 scripts/ 而不是 CI

仓库的 CI 只证明两个无图形栈的 crate 能在裸 Rust 上构建，装 Nix 会把"便宜的事"变成"昂贵的事"。而这个守卫的价值只在**升级 flake 输入**时出现，天然是低频手动动作，所以脚本 + 一行文档是它的正确位置。

## 不在本票据里

- 性能阈值断言（机器不同，数字不稳定）。
- 把守卫做成 CI job。
