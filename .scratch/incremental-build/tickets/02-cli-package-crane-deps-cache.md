# 02: package: CLI 的依赖编译产物进 store（crane）

**What to build:** `packages.plainly` 不再每次从空 `target/` 编译全部依赖。用 crane 的 `buildDepsOnly` 把"只编第三方依赖"的产物做成一个 store 输出，最终包用 `cargoArtifacts` 复用；改一行 `.rs` 之后，`nix build .#plainly` 只重建 workspace 自己的 crate。对外行为不变：同一个二进制、同一个无 GTK/WebKit 的闭包、checkPhase 照跑测试、`nix profile install` 照用。

**Blocked by:** [01](01-pin-toolchain-fenix-1.99.1.md)

**Spec:** [spec.md](../../plainly/spec.md) §2 工作区与 crate 边界（"CLI 不得依赖 GTK/WebKit"的承诺要照旧成立）。没有产品行为变化。

**Status:** ready-for-agent

- [ ] `.#plainly` 由 `craneLib.buildDepsOnly` + `craneLib.buildPackage` 产生，`craneLib = (crane.mkLib pkgs).overrideToolchain rust.toolchain`（接 01 的工具链）。
- [ ] 依赖派生（`plainly-deps-…`）的输出是 store 里的 `target.tar.zst`；改一行 `plainly-core` 或 `plainly-cli` 的 `.rs` 后重建，`nix build -L` 的待构建列表里**没有**依赖派生，只有最终包。
- [ ] 记录前后耗时并填进本票 Comments：原型（1.98.1 上测的，仅作量级参考）`buildRustPackage` 121 s、crane 首次 159 s、改一行 25 s；最终数字在实现后的仓库上重测。
- [ ] 产物能跑：`result/bin/plainly --version`；`nix profile install --profile /tmp/… .#plainly` 可装可跑。
- [ ] 闭包承诺不破：`nix path-info -r` 里 grep 不到 gtk/webkit；`scripts/check-flake-packages.sh` 通过。
- [ ] checkPhase 仍跑 core + cli 的测试（crane 默认 `doCheck = true`，跑 `cargo test --locked`），日志里有 `test result: ok`。
- [ ] 产物不含 `.dep-v0`（cargo-auditable）。这是**有意的语义变化**：仓库里没有任何东西消费它，保留它需要在 crane 上再套一层 cargo wrapper。决定记录在案；将来要 SBOM 再从这层加回。
- [ ] **不**新增 fast / debug 旁路输出：`.#plainly` 本身就是要变快的那个。

## 实现要点（来自 /tmp 原型，已验证）

- nixpkgs 的 `fetchCargoVendor` 输出不是裸 vendor 目录，而是 `Cargo.lock` + `.cargo/config.toml` + `source-registry-0/`。crane 的 `cargoVendorDir` 只认裸目录，所以要传 `${cliCargoDeps}/source-registry-0`；直接传顶层会 `error: no matching package named 'keyring' found`（原型踩过）。
- crane 的 `buildPackage` 默认 `doCheck = true` 且走 `cargo test --locked`；不要为了快关掉（Q10 已定：官方构建保留测试）。
- 首次构建比 `buildRustPackage` 慢：依赖阶段跑 `check --all-targets` + `build` + `test --no-run` 三遍，为的是把 dev-dependencies 也缓存。这个代价只在 `Cargo.toml` / `Cargo.lock` 变化时付一次，改 `.rs` 不用付。
- 依赖产物大小：CLI 138 MB。实现时 store 可能已接近上限（实测一度 85%），必要时先 `nix-collect-garbage -d`。

## 不在本票据里

- GUI 与 `default`（[03](03-gui-package-crane-deps-cache.md)）。
- 守卫脚本（[04](04-incremental-build-guard.md)）。
- 把 CLI 与 GUI 的依赖缓存合成一份：Q5 已定按两个 workspace 分开（两份 lock 的 vendor 不同，硬合会破 CLI 的闭包承诺）。
