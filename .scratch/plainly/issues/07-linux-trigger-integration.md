# 07 Linux 触发集成与能力探测

Type: grilling
Status: open
Blocked by: 02, 12

## Question

定下 Linux 上的**触发与取词路径**及能力矩阵。

- **niri 同类（wlroots/Smithay 系，有 data-control）**：合成器绑键（`binds { Mod+Shift+E { spawn "plainly" "--explain-clipboard"; } }`）→ CLI → 守护进程 → 用 `ext-data-control` 读剪贴板（**无需焦点**）→ 面板（形态由 02 定）。
- **GNOME**：Mutter 两个 data-control 协议都不实现 → 只能**快捷键按需读取**，无法监听复制事件。

要决定：

- **能力探测的具体判据**：协议存在性（`ext_data_control_manager_v1` / `zwlr_data_control_manager_v1`）、`wl-clipboard` 版本（≥2.3.0）、GlobalShortcuts portal 是否存在。探测结果怎么驱动 UI（"复制自动弹"开关灰掉还是隐藏）。
- **"复制自动弹"开关**的默认值与文案（默认关）。
- X11 会话路径是否在 v0 支持（还是仅记录为已知可用）。
- 交付给用户的**最小 niri 配置片段**（`binds` + 可选窗口规则），以及 `spawn` 不走 shell、不展开 `~` 这类坑的说明。
- niri 的已知 bug 应对：`spawn` 在按键按下时就触发，客户端可能收不到修饰键抬起。

输入：`research/os-integration-feasibility.md` 第 5 节。

## Comments

- **来自票据 05 的一条接口事实**：键绑应指向 **`plainly-desktop --explain-clipboard`**，**不是** `plainly`。理由：CLI 是瘦的、不依赖 GUI（票据 05 决策 1），它无法触发面板；而桌面二进制用 `tauri-plugin-single-instance` 的 D-Bus 交接**自己就能做到**"已有实例 → 转发 argv → 面板显示；无实例 → 冷启动并显示面板"。所以票据 05 已把"CLI → unix socket → 守护进程"这条备选整条删掉，这里的最小配置片段也不该出现 CLI。
- **⚠️ 2026-10-05 更正（票据 12 已 resolved，本票据随之解锁）**：上面那条**作废**。票据 12 证明 **Tauri 画不出 layer 面板**（表面能造，连纯色背景都不上屏），所以键绑**不可能**指向 `plainly-desktop`——它不参与画面板。
  本票据现在的**核心问题变了**：**谁 spawn 那个独立的面板进程？**
  - (a) **合成器键绑直接 spawn 面板进程** → 桌面应用（Tauri 主窗口）**不必常驻**；面板自己读剪贴板 → core → 显示。最小配置片段就一行 `spawn "plainly-panel" "--clipboard"`。
  - (b) **桌面应用常驻**，由它 spawn 面板（面板成为它的子进程）。
  - (c) 两者都支持——但那样面板自己也要处理"已有面板在显示"的重入问题（连按两次键）。
  这决定最小配置片段长什么样、以及要不要让主窗口常驻。**另需一并决定**：面板进程自己要不要用 single-instance 交接（避免连按出多个面板）。
