# 01: toolchain: dev shell 与两个包同用 fenix 1.99.1

**What to build:** 仓库里只有一个 Rust 版本事实：dev shell 的 cargo / rustc / clippy / rustfmt / rust-analyzer，和两个包（`.#plainly`、`.#default`）的构建平台，都来自同一个 pinned 的 fenix 工具链 1.99.1。`Cargo.toml` 与 flake 里手同步的 `cliManifest` 的 `rust-version` 一起从 1.85 抬到 1.99。做完之后，`nix develop` 里编出来的东西和 `nix build` 编出来的东西是同一个编译器，本地 `target/` 的增量结果才对 nix 那条路有意义。

**Blocked by:** 无（可立即开始）

**Spec:** [spec.md](../../plainly/spec.md) §14 开发环境（原文写"工具链取 `fenix.complete`"；本票据把这一句改成 pinned release，并把 build 侧也纳入同一个工具链）。没有产品行为变化。

**Status:** done (2026-10-17)

- [x] flake 里只有一个工具链定义：`fenix.packages.${system}.toolchainOf { channel = "1.99.1"; sha256 = "sha256-+xUr6Al1wZSqX8aEsq7DpuFOK4Ve/i+qTl+K5uWQ9GM="; }`。这个 sha256 是 1.99.1 的 channel manifest（`https://static.rust-lang.org/dist/channel-rust-1.99.1.toml`）的 SRI；写错时 Nix 会在报错里给出正确值。
- [x] dev shell 的四件工具 + 编辑器是同一 release：`nix develop -c rustc --version` = `1.99.1`、`cargo` = 1.99.1、`clippy` = 0.1.99、`rustfmt` = 1.10.0-stable（rustfmt 有自己的版本号，来自同一 1.99.1 release）、`rust-analyzer` = 1.99.1。
- [x] 编辑器不再是 fenix 的 nightly analyzer：用同一工具链的 `rust-analyzer` 组件（`toolchainOf` 的 `rust-analyzer` 属性），组件集合里按需带 `rust-src`。
- [x] 两个包的构建平台来自同一个工具链：判据是 `nix build` 的派生输入里出现 fenix 的 `rust-1.99.1-…`，而不是 nixpkgs 的 `rustc-1.98.1`。
- [x] `Cargo.toml` 的 `rust-version` = `"1.99"`；flake 内 `cliManifest` 的同一字段同步（那份是手写的第二份真源，不跟着改就会漂移）。
- [x] `nix build .#plainly` 与 `nix build .` 通过；checkPhase 的测试照跑（这一条是 02/03 的前置证据，不是本票据的目标）。
- [x] 换工具链后 GUI 的 checkPhase 仍能跑 test binary：GUI 包的 `env` 里补 `LD_LIBRARY_PATH`（见下面"换工具链带来的一个坑"），日志里 34 个 test target 全绿。
- [x] 裸 stable runner 的 CI 仍绿：`rust-version = "1.99"` 要求 runner 的 stable ≥ 1.99，今天 latest stable 就是 1.99.1；本地已用 `nix develop -c cargo test -p plainly-core -p plainly-cli` 预演 CI 跑的那一对 crate 并通过（真正的那次 CI 要等第一次 push）。
- [x] flake 的注释改成新理由：fenix 不是"为了 nightly"，而是"把 Rust 版本钉在 nixpkgs 之外、并让 dev 与 build 共用一套"；`pkgs` 上的 fenix overlay 不再需要。
- [x] spec §14 更新；AGENTS.md 里"开发环境是 flake"那句不受影响，不用动。

## 为什么不是"两边都用 nixpkgs"

已与用户确认（2026-10-17）：要的是 **Rust 版本与 nixpkgs 解耦**。nixpkgs 的 pin 也会钉住编译器，但更新粒度绑在一起；fenix 让升 nixpkgs 不换编译器、换编译器不升 nixpkgs。代价是发布产物的编译器变成上游二进制发行版（`static.rust-lang.org` 的 FOD），而不是 nixpkgs 从源码 tarball 构建的那份；这个代价被接受。

## 实现要点（来自 /tmp 原型，已验证）

- `toolchainOf` 在 `sha256 = null` 时走 impure 的 `builtins.fetchurl`，所以 pin 必须显式给哈希。
- 组件取法是 `rust.withComponents [ "cargo" "clippy" "rust-src" "rustc" "rustfmt" ]`（同一工具链的组件，不要用 `fenix.combine`——那是给混合不同工具链用的）。
- nixpkgs 接第三方工具链的正路是 `pkgs.makeRustPlatform { cargo = rust.cargo; rustc = rust.rustc; }`，行为与 `pkgs.rustPlatform` 一致（hook 仍是 nixpkgs 的），`buildRustPackage` 的 auditable 包装会跟到 1.99.1。
- 原型已验证 dev shell 打出 `rustc 1.99.1` / `cargo 1.99.1` / `clippy 0.1.99` / `rust-analyzer 1.99.1`，且 `nix build .#plainly` 在 1.99.1 下通过（161 s）。

## 换工具链带来的一个坑：test binary 找不到 GTK

nixpkgs 的 rustc 是用 `--enable-rpath` 构建的（`pkgs/development/compilers/rust/rustc.nix` 里那一条），所以它编出来的 test binary 在自己的 rpath 里带着 `buildInputs` 的库目录；fenix 的工具链是上游发行版二进制，没有这个配置。直接换过去之后：

```
/build/source/target/x86_64-unknown-linux-gnu/release/deps/plainly_panel-…:
error while loading shared libraries: libgobject-2.0.so.0: cannot open shared object file
… process didn't exit successfully (exit status: 127)
```

修法：GUI 包的 `env` 里加 `LD_LIBRARY_PATH = guiLibPath`（就是 dev shell 用的同一份 `libraries` 生成的路径），并在注释里写清它为什么存在——它是"沙箱里跑 test binary"的需要，不是给产物用的（产物的库路径仍由 `postFixup` 的 `wrapProgram` 负责）。CLI 不需要：它没有动态图形库依赖，`nix build .#plainly` 在纯 fenix 工具链下已经通过。

判据：`nix build .` 绿，且 GUI 派生日志里有 34 条 `test result: ok`、没有 `error while loading shared libraries`。

## 一次性代价

本地 `target/` 是 nightly 1.101 编的，换 1.99.1 后 cargo 判定全部 fingerprint 失效，**这一次**要全量重编。02/03 要消掉的是此后每次改一行的重复编译，不是这一次。

## 不在本票据里

- 构建缓存 / crane（[02](02-cli-package-crane-deps-cache.md)、[03](03-gui-package-crane-deps-cache.md)）。
- cargo-auditable 元数据（02 里随 crane 去掉）。
- CI 改用 `nix develop`：维持现状（CI 刻意不装 Nix）。

## Comments

**2026-10-17 — 原型验证（未改仓库，全部在 `/tmp` 副本里做）**

- dev shell 打出 `rustc 1.99.1 (58cf8f9c3 2026-10-13)` / `cargo 1.99.1` / `clippy 0.1.99` / `rust-analyzer 1.99.1 (58cf8f9 2026-10-13)`——编辑器与编译器同一 release，不再是 fenix nightly 的 analyzer。
- `nix build .#plainly`（纯 fenix 1.99.1）通过：161 s，checkPhase 的 core + cli 测试全绿。
- 第一次 `nix build .` 在 GUI checkPhase 失败：panel 的 test binary `libgobject-2.0.so.0: cannot open shared object file`（exit status 127），根因见上面「换工具链带来的一个坑」。
- 给 GUI 包的 `env` 加 `LD_LIBRARY_PATH = guiLibPath` 后重跑：`nix build .` **exit 0 / 209 s**，GUI 派生日志 **34 条 `test result: ok`**，没有 `error while loading shared libraries`。
- 注意：这里的 161 s / 209 s 是 1.99.1 下的数，与"现状 121 s / ~376 s"不是同一个编译器，只作量级参考；实现后要在真仓库上重测再更新。

**2026-10-17 — 真仓库落地与验证（提交前）**

- 改动面：`flake.nix`（工具链 pin 到 `toolchainOf`/`withComponents`、`makeRustPlatform`、两个包改走它、编辑器用同 release 的 `rust-analyzer`、去掉 fenix overlay、GUI `env` 加 `LD_LIBRARY_PATH`）、`Cargo.toml` 与 `cliManifest` 的 `rust-version`、spec §14、以及 CI 注释里过期的 `1.85` 引用。
- dev shell：`rustc 1.99.1 (58cf8f9c3 2026-10-13)` / `cargo 1.99.1` / `clippy 0.1.99` / `rustfmt 1.10.0-stable` / `rust-analyzer 1.99.1 (58cf8f9 2026-10-13)`。
- CI 等价：`nix develop -c cargo test -p plainly-core -p plainly-cli` **exit 0**，29 个 `test result: ok`。
- `nix build .#plainly`：**exit 0 / 114 s**（1.99.1 下全量）；派生输入里是 fenix 的 `rust-1.99.1-with-std` 与 `cargo-1.99.1`，不是 nixpkgs 的 `rustc-1.98.1`。
- `nix build .`：**exit 0 / 242 s**；GUI 派生日志 **34 条 `test result: ok`**，`libgobject` / `error while loading` 各 0 条。
- 注意：`.scratch/` 下的文档也是 flake source 的一部分，所以改票本身会让 source 哈希变化、触发一次它自己的重建；这是 source 过滤的性质，不是本票要解决的问题。
- Review：Standards 轴无硬违规，采纳了评论措辞修正（"same toolchain release"、`rustToolchain`/`shellComponents` 改名、overlay 的 why 注释）；Spec 轴确认代码项全部落地，只抓到 spec §14 把 `.#gui` 说成公开输出这一处措辞错误，已修。
- 定稿树（含本票与 spec §14 的文档改动）再跑一次确认：`nix build .` **exit 0 / 205 s**，输出 `/nix/store/wrgr6rj88256lk0vcd7nwm7x67l4nn9s-plainly-0.1.0`，`bin/` 三件齐全（`plainly`、`plainly-desktop`、`plainly-panel`）；GUI 日志仍是 34 条 `test result: ok`、0 条加载错误。

**2026-10-17 — 补充：运行时 RUNPATH 回归（由 02 的验收脚本发现）**

换到 fenix 工具链后，不只是 checkPhase 的 test binary 需要库路径——**装出来的** GUI 二进制也失去了 RUNPATH。旧产物（nixpkgs 的 rustc）的 RUNPATH 由链接环节产生（`glib`/`cairo`/`gdk-pixbuf`/`gtk+`/`pango`/`webkitgtk`/`libsoup`/`dbus`/glibc/gcc），fenix 的 rustc 不给；`env -i result/bin/plainly-desktop --help` 因此死于 `libgdk_pixbuf-2.0.so.0: cannot open shared object file`。

修复（独立 follow-up commit）：给 `gui` 加 `autoPatchelfHook`，按 DT_NEEDED 的实际解析结果给装出来的 ELF 打 rpath；checkPhase 的 `LD_LIBRARY_PATH` 保留（测试二进制不经过 `fixup`，`autoPatchelfHook` 看不到它们）。已验证 `scripts/check-flake-packages.sh --no-build` 全绿、GUI 34 个 test target 仍过。

试过但不成立的两种修法，记下来省得重走：`RUSTFLAGS = "-C rpath"` 在 1.99 上只加一条 `$ORIGIN/…/rust-1.99.1-with-std/lib` 的 sysroot rpath；`RUSTFLAGS = "-C linker=cc"` 也没有恢复 RUNPATH（cargo hook 本来就通过 `CARGO_TARGET_*_LINKER` 指向 cc）。
