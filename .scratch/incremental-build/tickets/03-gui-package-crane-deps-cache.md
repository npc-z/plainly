# 03: package: GUI 与 default 也用 crane

**What to build:** `gui`（`plainly-panel` + `plainly-desktop`）与 `default`（`symlinkJoin`）也换成 crane 的依赖缓存。改一行 `.rs` 后 `nix build .` 只重建三个 workspace 派生（CLI、GUI、symlinkJoin），不重编第三方依赖。装完能用的性质一条不丢：两个 GUI 二进制仍被 wrap 过、图标与 `.desktop` 仍在、checkPhase 仍跑整个 workspace 的测试。

**Blocked by:** [02](02-cli-package-crane-deps-cache.md)（同一个 `flake.nix` 的 craneLib / 包定义段，02 先建立骨架；写作用域重叠，不是 GUI 技术上做不了）

**Spec:** [spec.md](../../plainly/spec.md) §14 开发环境（flake 输出）与 `.scratch/packaging/tickets/01-flake-package-output.md` 定下的"装完就能用"契约。没有产品行为变化。

**Status:** ready-for-agent

- [ ] `gui` 与 `default` 由 crane 产生，`gui` 复用 02 建立的 `craneLib`。
- [ ] **deps 参数与最终包参数显式拆开**：`buildDepsOnly` 会继承调用方的所有钩子（包括 `postFixup`），而依赖产物里没有二进制；不拆会 `Builder called die: Cannot wrap '…/bin/plainly-panel'`（原型实测失败点）。做法是依赖阶段只吃一份不含 `postFixup` / install 钩子的 args，最终包用 `cargoArtifacts = guiDeps` 再接上 `postFixup`。
- [ ] `nix build .` 产出 `result/bin/{plainly,plainly-panel,plainly-desktop}`；`result/bin/plainly-panel` 是 wrapper（含 `LD_LIBRARY_PATH` 与 `XDG_DATA_DIRS`），`result/share/{applications,icons}` 都在。
- [ ] checkPhase 照跑整个 workspace 的测试（34 个 test target）；`PLAINLY_TEST_NO_DISPLAY=1` 仍生效（面板那条测试仍按它的约定被明说跳过）。
- [ ] 改一行 `.rs` 后 `nix build .` 只重建 workspace 派生 + symlinkJoin；记录耗时并填进本票 Comments（原型：首次 351 s、改一行 105 s；现状对照约 376 s；最终数字在实现后的仓库上重测）。
- [ ] `scripts/check-flake-packages.sh` 通过；`nix run .` 仍跑 CLI。
- [ ] 依赖产物大小与 disk：GUI 424 MB（target 1.57 GB 压缩）、CLI 138 MB；实现时确认 store 余量，按需 `nix-collect-garbage`。

## 实现要点（来自 /tmp 原型，已验证）

- `cargoBuildExtraArgs` 只作用于 build 阶段（GUI 只产 `plainly-panel` + `plainly-desktop`）；`cargoExtraArgs` 同时作用于 deps / check，保持 `--locked`。
- GUI 的 `env` 仍是 `common.env // { PLAINLY_TEST_NO_DISPLAY = "1"; LD_LIBRARY_PATH = guiLibPath; }`（`//` 是浅合并，直接写 `env = { … }` 会丢 `LIBCLANG_PATH`）。`LD_LIBRARY_PATH` 是 [01](01-pin-toolchain-fenix-1.99.1.md) 换 fenix 工具链带来的：上游 rustc 没有 nixpkgs 的 `--enable-rpath`，GUI 的 test binary 要靠它才能在沙箱里加载 GTK；crane 的 checkPhase 同样跑这些 test binary，所以这条必须跟着迁过去。
- 两个包的 deps 参数里都要有 `env = common.env`：依赖阶段也会跑 build script 与 `pkg-config` 探测（原型里 `libsqlite3-sys` 的 bundled SQLite 就是在依赖阶段编的）。`LD_LIBRARY_PATH` 可以只出现在最终包（deps 阶段只编 `test --no-run`、不运行），但放进 `guiArgs` 更省心、也不会改变缓存键之外的语义。

## 不在本票据里

- 守卫脚本（[04](04-incremental-build-guard.md)）。
- 改测试策略 / 去掉 checkPhase（Q10 已定保留）。
- GUI 的手工验收：`docs/manual-checks.md` 那套仍按原样走一遍即可，本票据不新增清单项。
