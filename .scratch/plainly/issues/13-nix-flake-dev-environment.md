# 13 开发环境（nix flake）的范围与内容

Type: grilling
Status: resolved

## Question

定下 v0 的开发环境长什么样。`idea.md` 写了"使用 nix flake 创建开发环境"，但内容一直没定——而票据 02 刚刚把**确切需要什么**撞了出来。

要决定的：

- **flake 还是 `shell.nix`**：原型用的是带 `<nixpkgs>` 的 `shell.nix`（**不锁版本**，跟着 channel 漂）。flake 能锁 nixpkgs 并给 `flake.lock` 复现性，代价是多一个文件、开发时改用 `nix develop`。既然这个项目的目标是跨平台发布（v1），锁版本的价值比省一个文件大。
- **必须包含什么**：rust 工具链、`pkg-config`、`gtk3`、**`gtk-layer-shell`**、`webkitgtk_4_1`、`glib-networking`、`openssl`。是否还要 `cargo-tauri` CLI、`sqlite`（票据 08 会用到）。
- **devshell 钩子放什么**：`GDK_BACKEND=wayland` 是**项目需要**的（`DISPLAY` 在时 GTK 会挑 X11，而 XWayland 没有 layer-shell）；而 `CARGO_HOME` / XDG 目录的重定向是 **agent 沙箱的限制，不要写进项目**——宿主机上 `~/.cargo` 是可写的。
- **是否同时当 CI 环境**：同一份 flake 给 CI 复用（v0 只跑 Linux）。这决定它是"开发者私有"还是"项目契约"。
- **v1 的跨平台构建**：现在就在 flake 里留结构（macOS/Windows 的 target），还是等 v1 再说。

输入：`prototype/panel-form` 分支上的 `tauri/shell.nix`（**可直接用的种子**）与 `shell.nix`；票据 02 的 Answer 里"遗留的环境知识"一节。

**这张票据不做什么**：不写 flake 本身。本图只产出决策——flake 是实现，归 `/to-spec` 之后的实现阶段（或作为它的一个直接产物）。

## Comments

- 另一个已撞出的硬事实：Tauri 的 `generate_context!` 在 `icons/icon.png` 不存在时**直接 panic**。这不是 flake 能解决的，但"新克隆的仓库第一次 `cargo build` 会不会直接失败"取决于它——需要考虑图标是提交进仓库还是构建时生成。

## Answer

五条决策，2026-10-05 全部按推荐采纳；**Q4 按"暂无 Cachix"这一现实修订为"推迟"**。

1. **骨架**：`flake.nix`，与 readest 同构——`flake-utils` + `fenix`（`inputs.nixpkgs.follows = "nixpkgs"`），nixpkgs 锁 `nixos-unstable` + `flake.lock`；工具链取 `fenix.complete`（cargo / clippy / rustc / rustfmt）。**不抄 readest 的 `nixConfig` 块**——它指向 `readest.cachix.org`，我们没有那样的缓存，声明白白指一个不存在的替代源只是噪音。**保留它的教训**：`checks` 不指向应用构建，否则 `nix flake check` 会变成 45 分钟的发布构建。`.envrc` 写 `use flake`（`direnv` 已装）。
2. **系统列表**：v0 只列 `x86_64-linux`。但**结构照 readest 写**（参数化的 mkShell 函数 + `eachSystem` 列表），v1 加 `aarch64-darwin` 是两行的事。理由：v0 的平台承诺是 Linux 一等公民（Q3=b）；没人验证的 darwin shell 会腐烂，还会给人"已经支持 macOS"的错觉。
3. **环境内容**：fenix 工具链、`pnpm` + `nodejs_24`、`pkg-config`、`gtk3`、**`gtk-layer-shell`**、`webkitgtk_4_1`、`libsoup_3`、`glib-networking`、`openssl`、`patchelf`、`xdg-utils`、`clang`；环境变量 `GDK_BACKEND=wayland` + `LD_LIBRARY_PATH`（由 `lib.makeLibraryPath` 生成）。
   - **一处必须与 readest 相反**：readest 设 `GDK_BACKEND = "x11"`；plainly **必须是 `wayland`**——layer-shell 在 XWayland 上不存在（票据 02 实测：不设就直接 FALSE）。照抄 readest 会坏在这里。
   - **去掉** readest 的 gstreamer 全家桶（plainly 不是阅读器、不碰音视频）、Android/iOS 工具、playwright。
4. **CI：推迟**。v0 不设 CI，本票据只定 devShell；规格里写一句"CI 在实现阶段定"。**触发修订的事实**：Cachix 缓存的是"**用 nix 发布**"（`nix build` 自家 derivation），不是"用 nix 开发"——`nix develop` 的依赖由上游二进制缓存供货。本机 `substituters` 已含 USTC/SJTU 镜像 + `nix-community.cachix.org` + `cache.nixos.org`，本 session 实测 gtk3 / gtk-layer-shell / webkitgtk_4_1 / libsoup_3 全是秒级拉取。**所以没有 Cachix 也能开发**；缓存只在想走 `nix build` 出发布物时才有意义，而发布链路本就在地图的 Out of scope 里。若哪天要：Cachix 对开源项目**免费**（只是注册一步）。
5. **图标**：提交一个占位 `icons/icon.png` 进仓库。理由是硬事实——缺失时 `generate_context!` 直接 panic，所以"新克隆能不能构建"取决于它。这也是 Tauri 的惯例（readest 存 `data/icons/readest-book.png`，再用 `tauri icon` 生成整套）；真实图标之后用 `cargo tauri icon` 替换。

### sqlite：不进 flake 的是"库"，不是"工具"

- **库（libsqlite3）不进 flake**：`rusqlite` 的 **`bundled`** feature 用 `cc` crate 把 SQLite 的 C 源码（内嵌在 `libsqlite3-sys`，当前 3.53.2；`rusqlite` 0.40.2）在**编译期**编入并静态链接。官方 README 的措辞是它"避免依赖系统上的 SQLite 版本（或你系统上的）"。
- **因此系统层面不需要安装 sqlite**——需要的是一个 **C 编译器**，而 `clang` 已在第 3 条清单里。
- **`sqlite3` 命令行工具**是另一回事：应用不需要它，但开发时想手动翻历史库会用得上。可**可选地**放进 devShell（几 MB，与发布物无关），或临时 `nix-shell -p sqlite`。这不改变"sqlite 不进 flake"的决定——那条说的是**库**。

**这张票据没有做、也不做**：不写 flake。flake 本身是实现，归 `/to-spec` 之后的实现阶段。
