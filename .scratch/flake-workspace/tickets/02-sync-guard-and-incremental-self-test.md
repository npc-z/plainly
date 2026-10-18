# 02: 守卫提交的 CLI 子集与根清单/lock 同步，并改写增量守卫自检

**What to build:** 新增 `scripts/check-cli-workspace.sh`（bash + 内联 `python3`，离线、不调用 nix 或 cargo），按 spec §3 四项检查生成物是否仍同步于根清单 / lock：manifest 等于从根清单派生的值（TOML 语义比较）、committed CLI lock 的每个 `(name, version)` 都在根 `Cargo.lock` 里同名同版本、两个 crate 所有 `workspace = true` 的依赖都被 manifest 覆盖、manifest 不出现两个 crate 都不用的依赖。同时把 `scripts/check-incremental-build.sh` 的 `--self-test` 从「用 Python 切片改 flake 源码里的 `cliLock`」改为「改 `nix/cli-workspace/Cargo.lock`」，并把生成 / 守卫的说明写进 `docs/manual-checks.md`。做完后 `--self-test` 仍能证明两个包各自的依赖缓存对各自的 lock 敏感，且脚本里不再出现「在 flake 源码里找字符串」。

**Blocked by:** [01](01-flake-drop-cli-workspace-data.md)（需要提交的 `nix/cli-workspace/` 与 flake 收口后的状态）。

**Spec:** [spec.md](../spec.md) §3、§5、§6（manual-checks 部分）

**Status:** done (2026-10-18，工作区改动未提交)

- [x] `scripts/check-cli-workspace.sh` 落地：`set -euo pipefail`、`cd "$(dirname "$0")/.."`、`usage()` / `fail()`（house style 同 `check-flake-packages.sh` / `check-incremental-build.sh`）；失败信息走 stderr、带 `FAIL:` 前缀、并打印 `bash scripts/generate-cli-workspace.sh`。
- [x] 检查 ①：`nix/cli-workspace/Cargo.toml` 与「从根 `Cargo.toml` + 两个 crate manifest 派生出的 manifest」按 `tomllib` 解析后的结构相等（不比较格式）。
- [x] 检查 ②：`nix/cli-workspace/Cargo.lock` 与根 `Cargo.lock` 一致——每个 `(name, version)` 同名同版本、包集不超出根 lock 的 CLI 闭包、锁内每条依赖边都能解析；失败时列出漂移的包 / 超出闭包的包 / 悬空的边。
- [x] 检查 ③：两个 crate 的三个依赖节里所有 `{ workspace = true }` 的名字都出现在 committed manifest 的 `[workspace.dependencies]`。
- [x] 检查 ④：committed manifest 的 `[workspace.dependencies]` 不出现两个 crate 都不用的名字（抓手改 / 误加）。
- [x] 正向：在干净树上 `scripts/check-cli-workspace.sh` exit 0、无输出（或只一行 ok）。
- [x] 反向自检（手工控制，做完 `git checkout` 恢复）：(a) 改根 `Cargo.lock` 里某个共享包的 `version` → 检查 ② 报错；(b) 从 committed manifest 删一个依赖 → 检查 ③ 报错且 ① 也报错；(c) committed manifest 加一个没人用的依赖 → 检查 ④ 报错且 ① 也报错。记录三次的实际输出。
- [x] `scripts/check-incremental-build.sh`：头注释「CLI's synthetic workspace uses the hand-synced `cliLock` literal in flake.nix」改为「committed at `nix/cli-workspace/Cargo.lock`」；`--self-test` 的 CLI 控制项改为往 `nix/cli-workspace/Cargo.lock` 追加一行，删掉 67–75 行的 Python 切片；GUI 控制项（根 `Cargo.lock`）不变；两个 cache 都失效才通过。
- [x] `scripts/check-incremental-build.sh` 正向通过（改 `.rs` 后两个依赖输出路径不变）；`--self-test` 通过（两个控制各让对应 cache 失效）；记录耗时与前后路径。
- [x] `grep -n 'cliLock\|cliManifest' scripts/*.sh docs/manual-checks.md` 不再命中（`scripts/check-cli-workspace.sh` 除外——它是新脚本，不引用旧字面量）。
- [x] `docs/manual-checks.md`：33–36 行的「（repo 的 `Cargo.lock` 与 flake 里的 `cliLock` 字面量）」改为「（repo 的 `Cargo.lock` 与 `nix/cli-workspace/Cargo.lock`）」；§0 新增一段生成 / 守卫说明（依赖变更后、升级 flake 输入后跑；只读、约 1 s；不装 Nix 也能跑；生成器需要 `nix develop` 里的 cargo 与网络或缓存）。
- [x] 记录实际输出到本票 Comments；核对 spec §3/§5 与实现无出入。

## Comments

**2026-10-18 — 实现（按用户要求不提交代码，改动留在工作区）**

- **守卫**：`scripts/check-cli-workspace.sh`（bash + 内联 python3，离线、不调用 nix/cargo；投影规则与生成器共用 `scripts/cli_workspace.py`，导入时 `sys.dont_write_bytecode = True` 因此不留 `scripts/__pycache__/`）落地。正向 exit 0，四行 ok：manifest 形状与依赖声明是投影 / 每个 CLI lock 版本都在根 lock 里 / 继承依赖全被声明 / 无未使用的 workspace 依赖。四项各自能独立失败。
- **反向自检**（每次手工控制后 `git checkout` 恢复；四项检查都各自独立失败，失败信息只描述那一个事实）：(a) 把根 `Cargo.lock` 里 `serde` 改成 1.0.999 → 只有检查 2 失败：`FAIL: the CLI lock pins versions the root Cargo.lock does not have: serde 1.0.229`；(b) 从 committed manifest 删 `sha2` → 只有检查 3 失败：`FAIL: ... is missing [workspace.dependencies] for: sha2`；(c) 加 `gtk = "0.18"` → 只有检查 4 失败：`FAIL: ... declares workspace dependencies no crate uses: gtk`。三次都 exit 1，恢复后正向再次 ok。
- **增量守卫**：`scripts/check-incremental-build.sh` 头注释更新；`--self-test` 的 CLI 控制项改为往 `nix/cli-workspace/Cargo.lock` 追加一行、GUI 仍控制根 `Cargo.lock`；flake 源码切片与 `cliLock = pkgs.writeText` 的 Python 已删除（`grep -n 'flake.nix' scripts/check-incremental-build.sh` 为空）。因为按用户要求改动未提交，`git archive HEAD` 看不到新树，验证在一个一次性 git 副本里做（rsync 工作区 + 一次性 commit，只用于跑脚本，验证后删除；本仓库没有任何新 commit/ref）：正向 24.4 s，两个依赖输出不变（plainly `z49rhalidib569cks4a5w6gjxn1v2yhc-plainly-deps-0.1.0`，default `b0ycaqabqvhkf7qdshschxqq19m8p4yy-plainly-desktop-deps-0.1.0`）；`--self-test` 16.8 s，两个 cache 各自失效（plainly → `m3pw22bkp289vjd5j91maazph3zsjshj-plainly-deps-0.1.0`，default → `v4lj8bayj8zdn7jgnrxy7nxrsik1g35b-plainly-desktop-deps-0.1.0`），通过。
- **Review 修订（第二轮）**：检查 ② 增加「committed 包集 ⊆ 根 lock 的 CLI 闭包」与「锁内依赖边可解析」两条（挡住 committed lock 缺包 / 超包集）；生成器只在真·cache miss 时才回退联网；`--check` 过期不再冒充「生成失败」；manifest header 措辞改为「root workspace manifests」；`wholeSource` 排除 `__pycache__`；flake 与 `cli_workspace.py` 互相注明两个 crate 要同步改。
- **Review 修订**：四项检查改为各自独立失败（① 形状+声明值、② lock 版本、③ 缺失依赖、④ 多余依赖），一次错误只报一条 FAIL；导入共享模块时 `sys.dont_write_bytecode = True`，守卫不再留下 `scripts/__pycache__/`；`.gitignore` 增加 `__pycache__/`；生成器与守卫的投影规则统一到 `scripts/cli_workspace.py`（新增，已 staged）。重验：正向 4 ok；反向 (a) 只有 ② 失败、(b) 只有 ③ 失败、(c) 只有 ④ 失败，各自 exit 1；额外 (d) 把根清单 `serde` 改成 `"2"` → 只有 ① 失败并指出 declarations differ。
- **文档**：`docs/manual-checks.md` 33–36 行的 lock 来源改为 `nix/cli-workspace/Cargo.lock`；§0 新增一段生成器与守卫说明（依赖变更后 / 升级 flake 输入后跑；守卫约 1 s、不装 Nix 也能跑；生成器需要 dev shell 的 cargo 与网络或缓存）。CI 未动。

## 不在本票据里

- 生成器、生成物、flake 收口、`cliCargoDeps.hash`（票 01）。
- `.scratch/plainly/spec.md` §14、历史票 Comments、最终验收记录（票 03）。
- 把新守卫加进 CI（spec Out of Scope）。
