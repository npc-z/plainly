# package: 一份装完就能用的 flake 输出

**What to build:** `nix build` / `nix profile install` / `nix run` 给出**装完直接能用**的三件程序——面板从裸 niri 键绑起得来（不靠 `nix develop` 的 `LD_LIBRARY_PATH`）、主窗口能启动并出现在应用列表里、CLI 可以单独装在一台没有图形栈的机器上。

**Blocked by:** 17（v0 的验收与手测清单是这一张的验收依据）

**Spec:** [spec.md](../../plainly/spec.md) §14 开发环境（只定 devShell）、§Out of Scope「**Tauri 发布链路**（公证、签名、自动更新）」——本票据**落在 v0 范围之外**，是让 v0 的验收材料变成可安装的东西，不是补 v0 的缺口

**Status:** ready-for-agent

- [x] flake 暴露 `packages.<system>.plainly`：**只有**瘦 CLI，单独可装，闭包里**没有** GTK / WebKit / Wayland
- [x] flake 暴露 `packages.<system>.default`：三个二进制 + `.desktop` + 图标（`symlinkJoin`），两个 GUI 二进制**wrap 过运行时库路径**
- [x] `nix run . -- "一段英文"`（或 `echo … | nix run .`）直接出解释；`nix run .#plainly -- --version` 也通
- [x] `nix build .#plainly` 的闭包与 `nix build` 的闭包**不同**：前者 `nix path-info -r` 里 grep 不到 gtk/webkit（拆包是否真成立，用这个判据，不靠包名）
- [ ] **裸 niri 键绑**：`spawn "…/bin/plainly-panel" "--clipboard"` 在**没有** `nix develop`、没有手写 `LD_LIBRARY_PATH` 的会话里出面板（这是本票据成不成立的那一条）——**等价环境已验，真按键未验**：2026-10-11 用 `env -i`（只剩 `WAYLAND_DISPLAY` / `XDG_RUNTIME_DIR` / `DBUS_SESSION_BUS_ADDRESS` / `HOME` / `PATH`）起了装出来的面板，niri 的 `layers` 里出现 `plainly-panel` 且焦点一字未动（docs/manual-checks.md §7），但**没有**把 `result/bin` 放进 niri 的 `PATH`、写进 `binds`、真按一次键。差的是那一下。
- [x] 主窗口同样能从裸会话启动，并且有 `.desktop` + `icons/icon.png`，在应用启动器里显示为 Plainly（`StartupWMClass` 见下面第 3 条：写的是窗口真实的 app_id `plainly-desktop`，不是 identifier）
- [x] `nix profile install .#plainly` 在一台没有 GTK 的机器上装出来的 `plainly` 能跑（`ldd` 里没有 `not found`，也没有 gtk/webkit）——2026-10-11 用 `nix profile install --profile /tmp/… .#plainly` 验过（隔离 profile，不动真实 profile）
- [ ] 一次真机验收：按 [docs/manual-checks.md](../../../docs/manual-checks.md) §0/§1 的路径，用**装出来的**面板而不是 `target/debug` 的那份走一遍（至少：键绑 spawn、焦点不变、✕ 可关、结果态）——焦点、结果态、退出都验了（§7），**键绑 spawn 与 ✕ 的鼠标点击没验**（退出用的是信号）。这条要等真按一次键、真点一次 ✕ 才算完

## 为什么现在做不到

`flake.nix` 现在只有三个输出：`devShells.default`、`devShells.inspect`、`formatter`。没有 `packages`、没有 `apps`，`checks` 是**故意**空的（原注释：让 `nix flake check` 不变成一次完整发布构建）。实测（2026-10-10，commit 59d12cf）：

```
$ ./target/debug/plainly --version                 # 能跑
$ ./target/debug/plainly-panel --clipboard         # 起不来
plainly-panel: error while loading shared libraries: libgtk-layer-shell.so.0: cannot open shared object file
$ ldd target/debug/plainly-panel | grep 'not found'
	libgtk-layer-shell.so.0 / libgobject-2.0.so.0 / libgtk-3.so.0
	libgdk-3.so.0 / libatk-1.0.so.0 / libgio-2.0.so.0
```

CLI 那条能跑是**顺带**的结果（`rusqlite` 的 `bundled` + rustls 让它基本自足），不是 flake 给的保证，而且它只在 `target/debug` 里，没有任何 output 引用它。面板与主窗口则必须由 `wrapProgram`/`makeWrapper` 把运行时库路径写进包装脚本——**不 wrap，装完照样是上面那行错**。这一步是「能构建」与「能用」的分界。

## 已经有的、可以直接用的

- 构建所需的 `buildInputs` 就是 devShell 现在的 `libraries`：`glib`、`at-spi2-core`、`gtk3`、`gtk-layer-shell`、`webkitgtk_4_1`、`libsoup_3`、`glib-networking`、`openssl`（`at-spi2-core` 那一对是票据 15 加了 `atk` 之后留下的，理由写在 flake 里：gtk3 闭包里的 `libatk-1.0.so.0` 不在 `LD_LIBRARY_PATH` 上）。
- `pkg-config`、`clang`（`clang` 是 `rusqlite` 的 bundled SQLite 要的 C 编译器）、`patchelf`。
- `icons/icon.png` 已在仓库里（Tauri 的 `generate_context!` 缺它会 panic，票据 01 就处理过）。
- `crates/plainly-desktop/ui/` 是 `frontendDist`，纯 HTML/CSS/JS，**不需要 pnpm/node 参与构建**（没有打包器）；`cargo build -p plainly-desktop` 单独就能过（2026-10-10 实测）。
- 版本 0.1.0、`rust-version = 1.85`、MIT——`meta` 直接照抄。

## 已定的三件事（2026-10-10，与用户确认过，不要重新讨论）

1. **CLI 与 GUI 拆成两个包**：`packages.plainly` 只含 `plainly`（给服务器 / CI / 没图形栈的机器），`packages.default` 是 `symlinkJoin` 出来的三件套（面板 + 主窗口 + CLI）+ `.desktop` + 图标。合成一个包会让「装个 CLI」把 WebKit 一起拖进闭包，而 CLI 不依赖图形库正是 workspace 拆 crate 的全部理由（规格 §2）。
2. **`checks` 保持空，但理由要重写**。本票据一旦有了 `packages`，`nix flake check` 就会去构建它们——现在 flake 里那句「checks 空着是为了不让 `nix flake check` 变成一次完整发布构建」就**默默失效**了：它挡不住了。保留空 `checks`，把注释改成说清「要验的是 `nix build` / `nix profile install`，不是 flake check」。
3. **`.desktop` 的 identifier 跟 `dev.plainly.app` 走**（票据 01 定的 identifier）：desktop entry 的文件名与 `StartupWMClass` 都与它一致，这样配置目录、D-Bus 名与启动器里的一条是同一个身份。**2026-10-11 修正**：文件名与 `Icon` 仍是 identifier，`StartupWMClass` 改成 `plainly-desktop`——实机测出来窗口的 Wayland app_id 是它，不是 identifier（`tao` 用没有 application ID 的 `gtk::Application`，GTK 便公布程序名）。让 app_id 说 identifier 得打开 Tauri 的 `enableGTKAppId`，而那会启用基于会话总线的单实例，抢在 `--watch` 的交接 socket 之前接管第二次启动。选的是「让 `.desktop` 说出窗口真实的 app_id」；完整理由在 `docs/manual-checks.md` §8。

## 不在本票据里

- 公证 / 签名 / 自动更新 / 各发行版原生包（`.deb`、AUR…）——规格明确出界。
- `aarch64-linux`、macOS、Windows：v0 的 flake 只列 `x86_64-linux`，加系统是另一张票据。
- 把面板做成 Cargo 的 `install` 目标或加 `cargo-dist`：本票据只要求 flake 这一条路。

## 验收材料

[docs/manual-checks.md](../../../docs/manual-checks.md) §0 现在写着「这些命令都在 `nix develop` 里跑」——本票据做完之后那句话要改：跑清单的人应该用**装出来的**二进制。改哪几句、以及要不要在 §6 那种「实际走查」里记一次安装版的走查，都算这张票据的一部分。
