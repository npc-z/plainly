# 02 面板形态原型

Type: prototype
Status: resolved

## Question

决定桌面面板的形态——方式是做一个**便宜的原型**，而不是继续调研。

1. 在 niri 上验证 Tauri 的 layer-shell reparenting hack 在**当前 Tauri 2.x** 上还能不能跑通：`.setup()` 里把 webview 的 `default_vbox()` 移进新建的 `gtk::ApplicationWindow`，在 `show_all()` **之前**调 `init_layer_shell()`。
2. 若跑不通，比较并选定退路：
   - (a) 独立的 GTK4 + `gtk4-layer-shell` 面板进程；
   - (b) `iced_layershell` 纯 Rust 面板（无 GTK/WebKit）；
   - (c) 普通 Tauri 窗口 + niri 窗口规则（`open-floating true`、`open-focused false`、`default-floating-position`、`on-xdg-activate "ignore"`）。

产出：一个能贴出锚定浮层的最小原型（或一份明确的失败记录 + 原因），以及一张取舍表（进程数、可维护性、每次 Tauri 升级的复验成本、外观）。

输入：`research/os-integration-feasibility.md` 第 5 节。这张票据的结论是票据 07 的前置。

## Answer

**面板形态定为：一个 Tauri 进程内的 layer-shell overlay 表面。reparenting hack 不需要，第二个进程也不需要——但内容渲染必须在宿主机复验（已转为票据 12）。**

三条已验证：

1. **niri 上 layer-shell 完全可用**（独立进程路已证）：C + gtk3 + gtk-layer-shell，overlay 层、锚定左上、`keyboard_mode=NONE`。面板显示期间 `niri msg --json focused-window` 的 id **不变** → 确实不抢键盘焦点。当时截了图（面板锚定左上、浮在真实窗口之上），但**按隐私决定不保留**——它拍到了周围桌面。留在证据链里的是客观事实：焦点窗口 id 不变，以及 niri 自己的 layer 列表。
2. **Tauri 2.12.1 能在同一进程里做出真正的 layer 表面。** tao#925 的失败原因是"窗口已经被 map"，**不是 API 没了**——`gtk_window()` 与 `default_vbox()` 在 2.12.1 里仍是公开 API（`tauri::Window` 与 `tauri::WebviewWindow` 上都有）。做法：以 `visible(false)` 创建 → 取 `gtk_window()`（实测 `is_mapped() == false`）→ `init_layer_shell()` 并设好锚点/层/键盘模式 → `show()`。合成器侧确认：`niri msg --json layers` 报 `{"namespace":"plainly-panel","layer":"Overlay","keyboard_interactivity":"None"}`，且**不在** `niri msg windows` 中、焦点不变。
3. **`Webview::reparent` 存在，但对 `WebviewWindow` 会返回 `CannotReparentWebviewWindow`**（`error.rs` 里写明）。既然 `visible(false)` 就够了，这条路不必走。

**两个不显然的必需项**（票据 07 与实现规格要用）：Tauri 的 `show()` **不递归**显示子控件，webview 需要补一次 `gtk_window().show_all()`；crate 需要 `gtk-layer-shell = { version = "0.8", features = ["v0_6"] }` 才有 `set_keyboard_mode`（默认 feature 为空，只剩废弃的 `set_keyboard_interactivity`）。

**未解决**：WebKit 在**这个沙箱里**画不出像素。表面存在、尺寸 470x340、`is_layer_window() == true`、WebKit 的 web/network 进程都在跑、子控件 `visible == true`，而屏幕上什么都没有。沙箱没有 `/dev/dri`，MESA 报 `ZINK: failed to choose pdev` / `egl: failed to create dri2 screen`；纯 GTK（Cairo/CPU）画得出来，WebKit 的合成路径不行。**证据指向环境而非 Tauri，但必须在宿主机复验**——见 [12 Tauri 面板的内容渲染验证（宿主机）](12-tauri-panel-render-check-host.md)。

### 取舍表

| 路线 | 进程数 | 状态 |
|---|---|---|
| **Tauri 进程内 layer-shell**（`visible(false)` → `init_layer_shell` → `show`） | **1** | 合成器侧已证实；内容渲染待宿主机复验（12） |
| 独立 GTK4/GTK3 + layer-shell 面板进程 | 2 | ✅ 已实证，含截图与焦点验证 |
| 普通 toplevel + niri 窗口规则 | 1 | 未实测；规则齐全（调研已确认） |

**决定**：主路线取第一条——一个进程、一套代码、一个前端。第二条作为**已经证实过的退路**写进规格（万一宿主机复验失败，代价是多一个进程而不是重新选型）。第三条不采用：layer-shell 既然可用，就没有理由要求用户去手写窗口规则。

**资产**：`prototype/panel-form` 分支——**只含 `prototype/panel-form/`，与 master 没有共同祖先**。这是故意的：master 是地图与票据的**唯一权威副本**，分支不得携带一份会分叉的竞争快照（它一度带过：那份 `.scratch/` 快照停在画图时刻，缺了 12、13 和本票据的 Answer）。

因此该分支**只读、不可合并、不要在其上工作**——检出它会把地图从工作区移走，合并它会删掉其余一切。要看文件用 `git show`，或 `git worktree add` 到一个临时目录。内含 C 原型 + Tauri 原型 + 运行说明（无截图）。

**遗留的环境知识**（票据 07 与 nix flake 要用）：在这台机器上跑 Tauri 需要 `rustc`（系统 profile 里没有）、`pkg-config`、`gtk3`、`gtk-layer-shell`、`webkitgtk_4_1`、`glib-networking`、`openssl`；`CARGO_HOME` 与 XDG 目录不能落在只读的 `~/.cargo`、`~/.cache`、`~/.local/share`；并且 `icons/icon.png` 必须存在，否则 `generate_context!` 直接 panic。
