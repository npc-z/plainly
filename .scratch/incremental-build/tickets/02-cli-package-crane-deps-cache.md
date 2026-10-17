# 02: package: CLI 的依赖编译产物进 store（crane）

**What to build:** `packages.plainly` 不再每次从空 `target/` 编译全部依赖。用 crane 的 `buildDepsOnly` 把"只编第三方依赖"的产物做成一个 store 输出，最终包用 `cargoArtifacts` 复用；改一行 `.rs` 之后，`nix build .#plainly` 只重建 workspace 自己的 crate。对外行为不变：同一个二进制、同一个无 GTK/WebKit 的闭包、checkPhase 照跑测试、`nix profile install` 照用。

**Blocked by:** [01](01-pin-toolchain-fenix-1.99.1.md)

**Spec:** [spec.md](../../plainly/spec.md) §2 工作区与 crate 边界（"CLI 不得依赖 GTK/WebKit"的承诺要照旧成立）。没有产品行为变化。

**Status:** done (2026-10-17)

- [x] `.#plainly` 由 `craneLib.buildDepsOnly` + `craneLib.buildPackage` 产生，`craneLib = (crane.mkLib pkgs).overrideToolchain (_: rustToolchain.minimalToolchain)`（01 那套工具链里的 cargo/rustc；与 `rustPlatform` 取的是同一组组件派生）。
- [x] 依赖派生（`plainly-deps-…`）的输出是 store 里的 `target.tar.zst`；改一行 `plainly-core` 或 `plainly-cli` 的 `.rs` 后重建，`nix build -L` 的待构建列表里**没有**依赖派生，只有最终包。
- [x] 记录前后耗时并填进本票 Comments：原型（1.98.1 上测的，仅作量级参考）`buildRustPackage` 121 s、crane 首次 159 s、改一行 25 s；最终数字在实现后的仓库上重测。
- [x] 产物能跑：`result/bin/plainly --version`；`nix profile install --profile /tmp/… .#plainly` 可装可跑。
- [x] 闭包承诺不破：`nix path-info -r` 里 grep 不到 gtk/webkit；`scripts/check-flake-packages.sh` 通过。
- [x] checkPhase 仍跑 core + cli 的测试（crane 默认 `doCheck = true`，跑 `cargo test --locked`），日志里有 `test result: ok`。
- [x] 产物不含 `.dep-v0`（cargo-auditable）。这是**有意的语义变化**：仓库里没有任何东西消费它，保留它需要在 crane 上再套一层 cargo wrapper。决定记录在案；将来要 SBOM 再从这层加回。
- [x] **不**新增 fast / debug 旁路输出：`.#plainly` 本身就是要变快的那个。

## 实现要点（来自 /tmp 原型，已验证）

- nixpkgs 的 `fetchCargoVendor` 输出不是裸 vendor 目录，而是 `Cargo.lock` + `.cargo/config.toml` + `source-registry-0/`。crane 的 `cargoVendorDir` 只认裸目录，所以要传 `${cliCargoDeps}/source-registry-0`；直接传顶层会 `error: no matching package named 'keyring' found`（原型踩过）。
- crane 的 `buildPackage` 默认 `doCheck = true` 且走 `cargo test --locked`；不要为了快关掉（Q10 已定：官方构建保留测试）。
- 首次构建比 `buildRustPackage` 慢：依赖阶段跑 `check --all-targets` + `build` + `test --no-run` 三遍，为的是把 dev-dependencies 也缓存。这个代价只在 `Cargo.toml` / `Cargo.lock` 变化时付一次，改 `.rs` 不用付。
- 依赖产物大小：CLI 138 MB。实现时 store 可能已接近上限（实测一度 85%），必要时先 `nix-collect-garbage -d`。

## 不在本票据里

- GUI 与 `default`（[03](03-gui-package-crane-deps-cache.md)）。
- 守卫脚本（[04](04-incremental-build-guard.md)）。
- 把 CLI 与 GUI 的依赖缓存合成一份：Q5 已定按两个 workspace 分开（两份 lock 的 vendor 不同，硬合会破 CLI 的闭包承诺）。

**2026-10-17 — 真仓库落地与验证（提交前）**

- 实现：`crane` input + lock（`ipetkov/crane@bd63bf1`，该 rev 的无 input 节点，lock 未手改）；`craneLib` 用 `rustToolchain.minimalToolchain`；CLI 拆成 `cliVendorDir` / `plainlyArgs` / `plainlyDeps` / `plainly`，`cargoVendorDir = ${cliCargoDeps}/source-registry-0`；`buildPackage` 默认 `--locked`，`cargoExtraArgs` 那行按 review 意见去掉了（它只是复述默认值），`cargoBuildFlags` 的消失写进了注释。
- 首次全量（1.99.1 + crane，冷 store 下的一次实测）：`nix build .#plainly` **109 s**（旧 `buildRustPackage` 路径 114 s，基本持平）。
- 改一行 `crates/plainly-cli/src/main.rs` 后重建：**16–21 s**（三次测量），只 build 最终派生；依赖输出路径改动前后都是 `lvj9ai…` / `y3aly…`（配置微调前后各测一次），始终不变。
- 依赖产物：`plainly-deps-0.1.0` 磁盘 **136 MB**（nar 372 MiB），输出里是 `target.tar.zst`。
- checkPhase：**29 条 `test result: ok`**；产物 `plainly 0.1.0` 可跑；`nix profile install --profile /tmp/plainly-profile .#plainly` 可装可跑。
- `.dep-v0`：0 段（有意去掉 cargo-auditable，已在注释里写清；`default` 仍从 `gui` 拿到它，直到 03 迁完，故只是过渡期的混合）。
- 闭包：CLI 运行时 5 个路径、0 个图形库命中；派生的直接引用同样 0。
- `scripts/check-flake-packages.sh --no-build`：**exit 0**（`nix run` 两条、default 三件套与图标、CLI 闭包与 build inputs 无图形库、两个闭包大小差异、GUI 在 `env -i` 下启动与 `ldd`、CLI 无环境读 Passage 全过）。
- 额外修复（**票 01 的运行时回归**，由本票的验收脚本暴露）：fenix 的 rustc 不给产物 RUNPATH（nixpkgs 的 rustc 曾白给），装出来的 `plainly-desktop` 在 `env -i` 下缺 `libgdk_pixbuf-2.0.so.0`。尝试过 `-C rpath` 与 `-C linker=cc` 都不复现该行为（证据留在此条），最终给 `gui` 加 `autoPatchelfHook`（按 DT_NEEDED 实际解析结果给装出来的 ELF 打 rpath）并恢复 checkPhase 的 `LD_LIBRARY_PATH`。这落在 GUI 包上、理论上是 01 的补丁，因此放在独立的 follow-up commit。
- Review：Standards 轴无硬违规（均已采纳：auditable 说明、flag swap 说明、"cargo+rustc" 重复改为 `minimalToolchain`、GUI 后续指针、vendor 子目录命名化）；Spec 轴确认核心验收全部落地，指出 GUI 改动属 scope creep（同上，按 01 的 follow-up 处理）与一处 toolchain 注释过时（已改）。
