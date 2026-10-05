# 07 Linux 触发集成与能力探测

Type: grilling
Status: resolved
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

## Answer

四条决策 + 两组探针数据，2026-10-05 全部按推荐采纳。

### 探针数据（实测，是这套设计的证据）

**① 无焦点 layer 表面读剪贴板**（面板 `keyboard_mode=NONE`、永不取得焦点）：

```
[clip] wl-paste      -> ok=1 status=0 out=TEST-SENTINEL-123   ✅ 拿到
[clip] gtk_clipboard -> (null / FAILED)                        ❌ 失败（且不阻塞，直接返回空）
```

→ **取词路径不是偏好问题**：有 data-control 就必须走它（`wl-paste` / `wl-clipboard-rs`）。GTK 的剪贴板 API 在无焦点 layer 表面上**直接失败**——这不是"不太好用"，是不工作。

**② GApplication 的单实例与重入**：

```
第二个实例退出码=0；它自己的日志为空（没建窗口）
主实例：[probe] ACTIVATE-AGAIN in pid ... -> reusing existing window (1 already)
niri 里 plainly-probe 表面数量：1
```

→ 面板做成 `GtkApplication` **白拿单实例**（application uniqueness + `activate` 转发，走会话总线）。唯一要写的代码：`activate` 里**复用已有窗口**而不是再建一个。

**③ 一条被推翻的中间假设**：我曾怀疑"`GtkApplication` + layer-shell 就不画"（因为 C 原型是裸 `GtkWindow`、Tauri 用的是 `GtkApplicationWindow`）。实测：**`GtkApplication` + layer-shell 正常绘制**（有截图确认）。所以票据 12 的失败是 **tao/wry 特有**的，与 GtkApplication 无关。

### 1. 谁 spawn 面板：合成器键绑直接 spawn

面板**自给自足**：自己读剪贴板（data-control）、调 core、显示；`GtkApplication` 又白送单实例。**所以没有任何东西需要常驻**——少一个常驻进程是实打实的收益（不占内存、不占端口、崩溃面更小）。

最小 niri 配置片段：

```kdl
binds {
    Mod+Shift+E { spawn "plainly-panel" "--clipboard"; hotkey-overlay-title="Explain clipboard"; }
}
```

`spawn` 不走 shell、不展开 `~`；`hotkey-overlay-title` 让它在 niri 的快捷键浮层里有名字。代价：首次按键要**冷启动**面板进程（GTK 起来约百毫秒 + provider 调用 1.3–1.7 秒）。

**面板的 GApplication id 定为 `dev.plainly.panel`**——它是单实例的唯一性键，必须与主窗口的 `dev.plainly.app`（票据 15）区分开。

**niri 的 `spawn` bug 不需要应对**：那个已知 bug 是"客户端收不到修饰键抬起、以为 Mod 还按着"，而面板键盘模式是 `NONE`，根本不接收键盘事件——天然规避。

### 2. 能力探测：查协议，且降级路径自洽

- **判据**：`ext_data_control_manager_v1` / `zwlr_data_control_manager_v1` 是否存在（绑定后的接口指针，或 `wayland-info`）。`wl-clipboard` 版本**只在走外部命令时才查**。
- **降级**：没有 data-control → 面板退化为**会取得焦点的普通窗口**，此时 GTK 的剪贴板 API 可用。
- **为什么自洽**：GNOME/Mutter **既不实现 data-control 也不实现 wlr-layer-shell**。所以在 GNOME 上，面板本来就只能是取得焦点的普通窗口——而取得焦点之后 GTK 剪贴板就能用。**两件事一起退化，不是两个独立的降级。**
- **探测结果必须驱动行为**，不能只写日志。
- **与票据 04 的区分**：04 探的是 **provider 的能力**（能否强制 schema），这里探的是 **桌面环境的能力**（能否无焦点取词、能否放 layer 表面）。两者不要混为一谈。

### 3. "复制自动弹"开关：灰掉 + 说明

默认关已由 charting 轮定下；这里只定**呈现**：能力不具备时**灰掉并附一行可操作的说明**（"需要 wlroots 类合成器的 data-control 协议"），而不是隐藏。隐藏会让用户以为"这软件没有这个功能"；灰掉加说明等于把**能力矩阵教给用户**——他知道换个桌面就有。

### 4. X11：不在 v0 范围

明确写进规格：**"X11 会话不支持"**。X11 上没有 layer-shell（面板只能是普通窗口），抓键要另写一套 X11 路径，而它是退场中的会话。**留白比写"不支持"更糟**——留白会让实现阶段拿不准要不要写。

### 给下游

- **票据 06**：面板的触发路径改为"合成器直接 spawn"；重入由 GApplication 处理（**不再是** Tauri 的 single-instance 回调）；"复制自动弹"的灰掉+说明要落进设置页。
- **票据 05**：二进制集合确认为**三个**：`plainly` / `plainly-panel` / `plainly-desktop`。
- **票据 12**：GtkApplication 假设已被测试推翻（见上）。

### ⚠️ 一个已经咬过人的实现陷阱（务必传给实现阶段）

**`GtkApplication` 的主循环是 `g_application_run`，`gtk_main_quit()` 管不了它。** 要退出必须用 `g_application_quit()`（或销毁窗口并让应用自行退出）。

这不是理论——**我自己的探针程序就踩了**：`t.c` 里用 `gtk_main_quit` 做 12 秒自动退出，结果变体 A 的进程**从来没退出**，在用户桌面上挂了 **23 分钟**的 `plainly-test` overlay 表面，直到被手工 kill（`pid=350374`，`niri msg layers` 一直列着它）。

**为什么这个坑严重**：面板的 `keyboard_mode=NONE`，所以

- **用户按 Esc 没用**（面板根本不接收键盘事件）；
- 于是**退出只有两条路**：定时器自动消失，或指针操作（点面板外、点关闭区）。

两者之一写错，用户就会得到一个**关不掉、永远盖在最上层**的面板。所以：自动消失必须走 `g_application_quit()`；并且**指针可关闭**不是可选装饰，是唯一的另一条出路。（自动消失的时长与"点击外侧关闭"的语义归票据 06。）
