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
