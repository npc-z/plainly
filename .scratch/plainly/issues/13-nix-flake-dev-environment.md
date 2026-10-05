# 13 开发环境（nix flake）的范围与内容

Type: grilling
Status: open

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
