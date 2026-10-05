# 02 面板形态原型

Type: prototype
Status: open

## Question

决定桌面面板的形态——方式是做一个**便宜的原型**，而不是继续调研。

1. 在 niri 上验证 Tauri 的 layer-shell reparenting hack 在**当前 Tauri 2.x** 上还能不能跑通：`.setup()` 里把 webview 的 `default_vbox()` 移进新建的 `gtk::ApplicationWindow`，在 `show_all()` **之前**调 `init_layer_shell()`。
2. 若跑不通，比较并选定退路：
   - (a) 独立的 GTK4 + `gtk4-layer-shell` 面板进程；
   - (b) `iced_layershell` 纯 Rust 面板（无 GTK/WebKit）；
   - (c) 普通 Tauri 窗口 + niri 窗口规则（`open-floating true`、`open-focused false`、`default-floating-position`、`on-xdg-activate "ignore"`）。

产出：一个能贴出锚定浮层的最小原型（或一份明确的失败记录 + 原因），以及一张取舍表（进程数、可维护性、每次 Tauri 升级的复验成本、外观）。

输入：`research/os-integration-feasibility.md` 第 5 节。这张票据的结论是票据 07 的前置。
