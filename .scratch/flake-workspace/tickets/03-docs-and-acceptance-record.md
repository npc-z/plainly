# 03: 文档收口与验收记录

**What to build:** 把本规格之前散落在活文档里的「两处同步」说法收掉，并在本票 Comments 里留下完整验收记录。具体：`.scratch/plainly/spec.md` §14 里「`rust-version` 下限同步在 `Cargo.toml` 与 flake 内的 `cliManifest` 两处」改成一处（根 `Cargo.toml`；CLI 的合成 manifest 由脚本派生）；`flake.nix` 顶部注释若票 01 漏改一并修正；`.scratch/incremental-build/tickets/04-incremental-build-guard.md` 末尾追加一条 Comments，指向 `nix/cli-workspace/` 与新守卫（正文不动，历史保持原样）；重跑 spec 的 Acceptance 全项并把实际数字写进本票。

**Blocked by:** [01](01-flake-drop-cli-workspace-data.md)、[02](02-sync-guard-and-incremental-self-test.md)（要等生成物、flake 收口与守卫都落地，验收数字才完整）。

**Spec:** [spec.md](../spec.md) §6、§Testing Decisions、§Acceptance

**Status:** done (2026-10-18，工作区改动未提交)

- [x] `.scratch/plainly/spec.md` §14 的 flake bullet：删掉「与 flake 内的 `cliManifest`」这半句，改为「`rust-version` 只写在根 `Cargo.toml`；CLI 的合成 workspace manifest 由 `scripts/generate-cli-workspace.sh` 派生」。改后全文 `grep -n 'cliManifest' .scratch/plainly/spec.md` 为空。
- [x] `flake.nix` 顶部工具链注释（40 行附近）不再复述「第二处」；`grep -n 'cliManifest\|cliLock' flake.nix` 为空。
- [x] `.scratch/incremental-build/tickets/04-incremental-build-guard.md` 末尾追加 `**2026-10-18 — 后记**` 一条：CLI 的 lock 已移到提交的 `nix/cli-workspace/Cargo.lock`，自检的 CLI 控制项同步改掉；指向 `scripts/generate-cli-workspace.sh` / `scripts/check-cli-workspace.sh` 与 `.scratch/flake-workspace/spec.md`。不改正文、不改已有 Comments。
- [x] 按 spec §Testing Decisions 重跑并记录：生成器可复现（`git diff` 空）、`nix build .#plainly`（checkPhase 测试数与 `--locked` 通过）、`nix build .#default`、`nix develop -c cargo --version`、`nix run .#plainly -- --version`、`scripts/check-flake-packages.sh --no-build`、`scripts/check-incremental-build.sh` 与 `--self-test`、`scripts/check-cli-workspace.sh` 正向与三个反向控制。
- [x] 记录 Acceptance 1–8 的实际值到本票 Comments：flake 行数前后、`grep` 结果、`nix/cli-workspace/` 两文件的包数 / 版本差、`cliCargoDeps` 新旧哈希、`plainly-deps` 新旧输出路径、生成器耗时、`nix build` 耗时、测试数、守卫正反向输出；并确认根 `Cargo.lock` 未改、`guiCargoDeps.hash` 未变。
- [x] 检查是否还有活的文档在陈述旧事实：`grep -rn 'cliLock\|cliManifest' --include='*.md' --include='*.sh' --include='*.nix' . | grep -v '^./.scratch/incremental-build/' | grep -v '^./.scratch/runner/'` 只剩 spec / 新票 / 新脚本里对新状态的描述，或为空。
- [x] 核对 spec §6 与实现无出入；本规格的 Out of Scope 未被顺手做掉（尤其 CI、`checks`、GUI、其他摩擦点）。

## Comments

**2026-10-18 — 实现与验收记录（按用户要求不提交代码，改动留在工作区）**

- **文档收口**：`.scratch/plainly/spec.md` §14 改为「`rust-version` 只写在根 `Cargo.toml`；CLI 的合成 workspace manifest 由 `scripts/generate-cli-workspace.sh` 派生」；flake 顶部工具链注释同样改成一处；`grep -n 'cliManifest' .scratch/plainly/spec.md flake.nix` 为空。`.scratch/incremental-build/tickets/04-incremental-build-guard.md` 末尾追加「后记」（正文与已有 Comments 未动）。
- **Acceptance 1–8 的实际值**：
  1. flake 2754 → **472 行**（≤ 500）；`grep -n 'cliManifest\|cliLock\|\[\[package\]\]' flake.nix` 为空。
  2. `nix/cli-workspace/` 两文件存在并被 flake 引用：manifest 28 行（2 members / 11 workspace deps / 无 GUI 名字），lock 2246 行 / **236 包**；生成器重跑 `--check` 通过（无 diff）。
  3. 生成器与守卫落地；守卫正向 + 三个反向控制见票 02 Comments；`docs/manual-checks.md` 已写。
  4. 增量守卫正向与 `--self-test` 通过（在临时 git 副本里跑，因为工作区按用户要求未提交）；`grep -n 'flake.nix' scripts/check-incremental-build.sh` 为空。
  5. `nix build .#plainly` 2m17s；`nix build .#default` 首次 3m33s（并发下 GUI 的 `watch::tests::a_window_asked_for_without_a_watcher_ends_the_listener` flaky 一次，重跑通过，单独重跑 5/5）；checkPhase 测试：plainly 31 个 `test result` 行 / 464 passed，GUI 36 / 607 passed，0 failed；`scripts/check-flake-packages.sh --no-build` 通过，CLI 闭包仍无图形库。
  6. 根 `Cargo.lock` / `Cargo.toml` 未改（`git diff HEAD --` 为空）；`guiCargoDeps.hash` 未变。
  7. 文档：见上；`grep -rn 'cliLock\|cliManifest'` 在 `flake.nix` / `scripts/` / `docs/` / `.github/` / `README.md` 中无命中（历史票与本次新增的「后记」除外——那是在描述新状态）。
  8. 数字记录在票 01 / 02 的 Comments：`cliCargoDeps` 哈希 `sha256-PL9gVEXFHjTzIVXu1VjEK/lDC80/FGeU4md1QnSLk6U=` → `sha256-OVl064aNTeMQsK/SGdS5W/WI2ofO0zYlb6zWTbJrhqM=`；依赖输出 `y3alyijw…` → `z49rhalidib569cks4a5w6gjxn1v2yhc-plainly-deps-0.1.0`（plainly），`0mh60dcz…` → `b0ycaqabqvhkf7qdshschxqq19m8p4yy-plainly-desktop-deps-0.1.0`（GUI 依赖；该值来自临时副本 eval，取法同票 04）。
- **Review 修订后重验**：第二轮 caveman-review 又补了：检查 ② 覆盖包集/依赖边、生成器只在真 cache miss 时联网、`--check` 过期与生成失败分开报、`wholeSource` 排除 `__pycache__`、manifest header 与 flake 交叉注释、票里的临时副本路径改成「一次性副本」。投影规则抽到 `scripts/cli_workspace.py` 并由生成器 / 守卫共用（模块已 staged，不再是被 import 却未跟踪）；`__pycache__` 不再产生（`sys.dont_write_bytecode` + `.gitignore`）；守卫四项检查各自独立失败；工具链版本从 `flake.nix` 的 `channel` 读取；spec §1 的字段数（4）与 §2 的失败条件已按实际修正。`nix flake metadata` 证实未跟踪的 `architecture-review-*.html` **不在** flake source 里（Nix 的 git source 只含 tracked/staged 文件；它只在 `path:` / 非 git 打包方式下会进 `wholeSource`，那是 spec Out of Scope 的既有摩擦点）。
- **说明**：按用户要求没有 commit，所以「integration branch / draft PR」不适用。验证在 staged 工作树（`nix build .#plainly` / `.#default`、`check-flake-packages.sh --no-build`、守卫）与一个一次性 git 副本（仅供 `git archive HEAD` 的增量守卫使用，验证后删除）上完成；临时副本只是验证脚手架，本仓库没有产生任何新 commit 或 ref。工作区改动保持 staged 方便一次性提交；`architecture-review-20261018-131312.html` 保持未跟踪（本就有意不入库）。

## 不在本票据里

- 生成器、生成物、flake 收口（票 01）。
- 新守卫与增量守卫自检改写（票 02）。
- 历史上其他已完成的票的正文。
