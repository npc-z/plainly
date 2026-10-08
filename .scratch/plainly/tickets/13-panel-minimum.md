# 13: 面板最小闭环

**What to build:** 我按一次键，剪贴板里的那段难英语在不抢焦点、不打断我打字的面板里变成解释；按 ✕、或它在 5 秒内没画出来，它都会消失。

**Blocked by:** 12

**Spec:** [spec.md](../spec.md) §12 面板（进程形态、触发、取词、退出路径）

**Status:** done (2026-10-08)

- [x] `plainly-panel` 是独立 GTK 进程（GtkApplication，id `dev.plainly.panel`），以 layer-shell overlay 呈现，`keyboard_mode=NONE`：不抢键盘焦点、不进合成器的窗口列表
- [x] 合成器键绑直接 spawn 面板（`spawn "plainly-panel" "--clipboard"`），桌面应用不必常驻；最小 niri 片段随交付
- [x] 连按两次只出现一个面板：第二次 `activate` 复用已有窗口并**重读剪贴板刷新**
- [x] 取词走 data-control（无焦点 layer 表面上 GTK 剪贴板 API 不工作）；没有该协议时退化为会取得焦点的普通窗口，此时 GTK 剪贴板可用
- [x] 面板调 core 产出 Explanation 并按五节呈现，原文常显
- [x] ✕ 可关，退出走 `g_application_quit()`（不是 `gtk_main_quit()`）
- [x] **5 秒渲染看门狗**：没有成功绘制就自己退出（正常路径永不触发）
- [x] 加载态里原文先可见；出错态给出人话原因与「剪贴板内容未变、什么都没存」

## 落地形态

- `crates/plainly-panel`：从占位换成真进程。`main` 只认 `--clipboard` 一个参数（规格里唯一的启动方式；
  其它一律用法错误退出 `2`），其余按职责分模块：
  - `app.rs`：一个 `GtkApplication`（id `dev.plainly.panel`）。**单实例是白拿的**——第二个进程把
    `activate` 交给第一个；`activate` 就是触发器与重触发：已有窗口就复用并**重读剪贴板**（规格 §12），
    没有才建。
  - `capability.rs`：连上 Wayland、列一遍 compositor 的 registry，把协议名交给
    `plainly_core::desktop::Desktop`，拿回 `Presentation`。**探测结果驱动形态**，不是日志。
  - `panel.rs`：窗口与状态。`Presentation::Layer`（且 `gtk_layer_shell::is_supported()`）→
    `init_layer_shell()` + `Overlay` + `keyboard_mode=NONE` + 左上锚点与 12px 边距 + namespace
    `plainly-panel`，宽度 470px；否则退化成一个会取得焦点的普通窗口（GNOME：data-control 与
    layer-shell 一起缺席）。正文按 `render::panel()` 的分节渲染，标题用面板自己的中文；原文永远在。
  - `clipboard.rs`：两条读取路径，交给 core 的都是同一种数据。layer 路径走 `wl-clipboard-rs` 的
    data-control（`get_mime_types_ordered` + `MimeType::Text` + 只在发布了 hint 时读它的值）；
    窗口路径走 GTK 自己的剪贴板（`wait_for_targets` / `wait_for_contents` / `wait_for_text`），
    那条路径的窗口本来就取得焦点，所以它才是可用的。
  - `plan.rs`：把 core 的判定（`Clipboard::reading`、`split::panel_chunks`）翻译成面板的四种结果——
    `Sensitive` / `Unread` / `Nothing` / `TooLong` / `Passages`。**敏感与超长都在这条纯函数上**，
    所以它们有单元测试，不需要显示器。
  - `watchdog.rs`：一个带截止时间的 `RenderWatchdog`；5 秒超时走 `app.quit()`（`g_application_quit`，
    不是 `gtk_main_quit`——那正是上次 23 分钟关不掉的原因）。**解除条件是「窗口已 mapped 之后的第一帧
    draw」**：`draw` 会在还没上屏的表面上发生，而这次要防的恰好是「从来没上屏」，所以未 mapped 的帧
    不算数。
  - 取词与解释都在**主循环之外**：data-control 的读取在一条线程上（它在等剪贴板 owner 写；owner 卡住
    不能把 ✕ 和看门狗一起拖死），解释在 worker 线程上；两者都通过 channel 回到主循环的一个 future，
    刷新时用 generation 丢弃上一轮的答案。
  - `worker.rs`：解释在**另一个线程**上跑（只有一个线程能碰 GTK）。它自己读配置 / key / 历史库，
    对每个 Passage 走 `Lookup` → `Store::recall`（命中即复用）→ `explain::run` → `Store::remember`，
    再把结果发回主线程——这就是票据 08 交给面板的那套判据，没有第二份。
  - `strings.rs`：本票据需要的那几行中文文案（加载 / 出错 / 敏感 / 超长 / ✕ / 节标题），集中一处；
    票据 15 在这个模块上补全，不引 i18n 框架。
- `plainly-core::explain::request(passage, config, setup, prompt)`（新增）：**两个表面现在用同一个
  函数造请求**。这些字段正是 Lookup Key 的分量（规格 §9），一边少设一个就会把缓存劈成两半；CLI 因此
  删掉了自己那份 `request_for`。
- `flake.nix`：`libraries` 补 `glib`。`LD_LIBRARY_PATH` 是从这张表直接拼的、**不传递**，所以 gtk3 的
  闭包里的 `libglib-2.0` / `libgobject-2.0` / `libgio-2.0` 不在路径上——面板能编译却在 devShell 里
  起不来。这是「跑一遍才知道」的那类缺口，按 AGENTS.md 修在 flake 里。
- `crates/plainly-panel/README.md`：最小 niri 片段（`spawn` 不走 shell、不展开 `~`）、GNOME 的降级、
  X11 不支持，以及「标记为敏感的内容不发不存」。票据 17 再把片段带进根 README。

## 验证

- 单元测试：`plan.rs`（敏感 / 未读 / 空 / 超长 / 多块保序）、`watchdog.rs`（未绘制会到期、绘制后永不
  到期）。全部 `cargo test -p plainly-core -p plainly-cli -p plainly-panel`：413 通过，clippy 零警告。
- **真机路径的端到端冒烟**（本机没有真 niri，用 headless wlroots 合成器 sway 顶替，协议面与 niri 同类：
  `zwlr_layer_shell_v1` + `zwlr_data_control_manager_v1` + `ext_data_control_manager_v1`）：
  - 剪贴板放一段普通英文 → 面板进程起来、**没有出现在合成器窗口列表里**（是 layer 表面，不是窗口）、
    过了 5 秒仍活着（看门狗正常路径没触发）、向假 provider 发了请求、历史库里多了 1 条 Record——
    说明无焦点下真的用 data-control 读到了剪贴板，并且走完了 core 的整条流水线。
  - 剪贴板换成 `wl-copy --sensitive` → 面板同样起来、同样的表面，但 **provider 请求 0 条、记录数不变**：
    拒绝发生在任何网络与任何写入之前。
  - **连按两次**（同一个会话里跑第二次 `plainly-panel --clipboard`）→ 第二个进程**立刻退出 `0`**、仍然只有
    一个面板进程、窗口列表里仍然 0 个窗口；换一段新文本后第二次按下**多出 1 次 provider 请求、记录 1 → 2**：
    第二次 `activate` 确实被复用并在重读剪贴板，而不是开出第二个面板。
- 无会话时 `plainly-panel --clipboard` 退出 `1` 并说明「cannot connect to the Wayland session」，不挂住；
  参数不对退出 `2`。

## 给下游的交接

- **tickets/15（动作与完整状态）**：面板现在有多少就多少——没有 provenance chip、复制 / ⟳ / ⤢，没有折叠，
  没有自动消失与位置设置；多块是「顺序解释 + 分隔线」的最小堆叠，没有滚动之外的组织；「过长」的文案
  与出路也只到「请用 CLI 或先拆分」。这些全在 15，且 `strings.rs` 就是它们的落点。
- **tickets/17（加固）**：本票据只在 sway headless 上验过；**真 niri**（键绑 spawn、焦点不变、连按两次
  复用、✕ 可关、看门狗自退）与 **GNOME 的降级窗口**（取词可用）仍要在真机上走一遍，那就是 17 的手测
  清单。X11 会话的明确报错也归 17。

## 已知取舍

- **降级路径的焦点时机**：窗口路径先 `present()`、等 `focus-in-event` 再读剪贴板（GTK 的剪贴板 API 需要
  焦点），而不是在 `activate` 里直接读。若合成器始终不给焦点，面板会停在「正在读取剪贴板…」——真机上
  的验证归 17；这也比在无焦点时读到空内容更诚实。
- **`Presentation` 与 GTK 的能力可能不一致**：理论上存在「有 data-control 但没有 layer-shell」的合成器，
  那时按 `gtk_layer_shell::is_supported()` 退化成窗口并走 GTK 读取；这条分支没有真机例子，写成防御。
- **一个多块输入就是 N 次串行请求**：面板没有 CLI 的进度文案，只有一个「正在解释…」；5 块以内是这样，
  超过 5 块直接拒绝（`panel_chunks`）。真正的堆叠体验与逐块进度感留给 15。
- **面板宽度与位置**：本票据只定死 470px 与左上角 + 12px 边距；四角 + 跟随显示器是 15 的键与算法。
- **GTK 回退路径的读取仍会阻塞主循环**：`gtk_clipboard_wait_for_text` / `wait_for_contents` 没有超时
  变体，一个卡住不放的 owner 能把回退路径的主循环挂住（✕ 与看门狗都在那个线程上）。data-control 那条
  主路径已经搬到线程上；回退路径只在 GNOME 这类没有 data-control 的会话里生效，且那句话的传输是本地的。
  记在这里而不是假装已经解决。
- **面板里的两处小重复是刻意的**：`now()` 与「keyring 失败但在 Optional provider 上容忍」这两条，CLI 与
  面板各有一份。core 明确不读时钟（`artifact.rs` 写了理由：读时钟属于能对超范围时钟说点什么的调用方），
  而容忍失败之后那句说明是表面文案（CLI 会打印一行，面板无处可打），所以它们留在表面。
