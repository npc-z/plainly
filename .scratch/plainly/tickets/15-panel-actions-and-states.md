# 15: 面板的动作与完整状态

**What to build:** 面板长成一个能长期用的表面——看得到这条解释出自谁、能复制、能一键换云端重生成、能跳去历史；位置与自动消失按我的偏好；长输入和两种拒绝也都有交代与出路。

**Blocked by:** 11, 13, 14

**Spec:** [spec.md](../spec.md) §12 面板（外观、退出路径、多块与拒绝态）

**Status:** done (2026-10-10)

- [x] 标题栏：provenance chip（云 / 本地可区分）+ 复制 / **⟳ 重新生成**（绕开缓存）/ ⤢ 打开主窗口（single-instance 交接）/ ✕
- [x] 分节可折叠；Grammar 为空时显示「本段不需要」而不是消失；Gloss 保序
- [x] 本地 provider 时正文下方**固定**一条核对提示（对照上方原文，或 ⟳ 换成云端重新生成）
- [x] 自动消失是两个**独立**键（开关默认关；打开后默认 20 秒），不用时长 `0` 表示「永不」
- [x] 位置是四角 + 跟随当前显示器（分辨率无关，换显示器不会把面板丢到屏幕外）；不做自由拖动
- [x] 多块输入按原文顺序**堆叠**在一个可滚动面板里（不做分页，没有隐藏状态）
- [x] 「内容被标记为敏感」与「输入过长（> 5 块）」两种拒绝态明说原因，且**同样有出路**，不会被关不掉
- [x] 界面文案是中文且集中在一份可替换资源里（不引 i18n 框架）；加载 / 结果 / 出错 / 拒绝四种状态都能关掉
- [x] 面板宽约 470px、不抢焦点，`activate` 复用语义不变

## 落地形态

面板从「一个文件既决定状态又画窗口」拆成**一个值 + 一层绘制**，因为窗口本身测不了、决定可以测：

| 文件 | 它是什么 |
| --- | --- |
| `src/state.rs` | 面板在显示什么：四种状态（加载 / 结果 / 出错 / 拒绝）、一堆 Slot、chip、复制文本 |
| `src/panel.rs` | 只有控件与那次运行：标题栏、可折叠的节、看门狗、定时器、把 `state` 画出来 |
| `src/prefs.rs` | 一次触发读出的配置：位置、时长、分节开关、provider 事实——**配置只在这里开一次** |
| `src/provider.rs` | 现在用哪个 provider，以及 ⟳ 能换到哪个云端（`Provider::of`）|
| `src/position.rs` | 四角 → 锚点、`current` / `output-N` → 显示器，纯函数 |
| `src/provenance.rs` / `src/document.rs` | chip 的三个事实；`sections()`（节开关的唯一过滤器）与复制文本（走 core 的同一个文档格式）|
| `src/css.rs` / `src/open.rs` | 那点样式；⤢ 拉起 `plainly-desktop`，靠它的单实例交接 |
| `src/strings.rs` | 界面文案一处：动作、四种状态、两种拒绝、两条本地提示 |
| `src/support.rs` | `#[cfg(test)]` 的共享夹具（Artifact、本地 provider 配置），三个测试模块共用一份 |

**⟳ 真的会换云端**（`spec §12` 的「换成云端重新生成」）：本地 provider 触发时 `Recall::Bypass { provider }`
带上 `Provider::cloud_alternative` 选出的云端 provider——**第一个 endpoint 不在本机、且 key 满足要求**的
出厂 preset（确定，不猜）。没有可换的云端时，⟳ 就是用同一个本地模型再生成一次，**正文下方那句提示会跟着
换一句话**（`LOCAL_NOTE_NO_CLOUD`），所以「按钮的说明」和「按钮的行为」不会分叉。

**空 Key Help 的决定**（票据 02 留的那处不对称）：**说一句**——core 的 `panel()` 加第二个文案参数
（`key_help_not_needed`），文档（markdown）仍然两种空节都省。理由与 Grammar 一字不差：面板只读一次，
缺一节分不清「没有」和「没生成」。文案「本段没有需要解释的表达」与 Grammar 的「本段不需要」不同，
两句不同的事实不该共用一句话。

**「有出路」是可验证的性质**：标题栏在**每个**状态之上（加载 / 结果 / 出错 / 拒绝），✕ 只在那里一次；
两句敏感拒绝都写明「换个内容再按一次键即可」；过长拒绝写明 CLI 与手动拆分。

## 验证

- 单元测试：`state.rs`（四种状态、每句状态都有句子、失败落到每个 Passage、复制文本只取已解释的）、
  `provider.rs`（本地 / 云端 / 无 key 时没有云端可换 / 端点覆盖决定 locality）、`prefs.rs`（两个独立键、
  `0` 与越界回落到 20 秒、分节开关）、`position.rs`（四角锚点、`current`、找不到的 output、任何分辨率
  都在屏幕内）、`provenance.rs` / `document.rs` / `strings.rs`。全部 `cargo test --workspace`：500 通过，
  clippy 零警告。
- **真机 smoke（本机 niri，wayland-1）**，用临时 `HOME` / `XDG_CONFIG_HOME`（不碰真实配置）：
  - 四个角各截一张：`top-left` / `top-right` / `bottom-left` / `bottom-right` 都落在对应角落，
    面板宽 470px、标题栏五项齐全（`Plainly` · chip · 复制 · ⟳ · ⤢ · ✕）、原文在上、本地提示固定在正文下方。
  - `panel_autohide = true` + `panel_autohide_seconds = 12`：14 秒时进程已退出、退出码 0；
    关掉开关时同一配置下进程一直活着（`✕` 与看门狗之外没有别的出口）。
  - `panel_corner` 生效这件事必须在**窗口 map 之后**再设一次锚点：GTK3 的 toplevel 是 realize 时才成为
    layer surface 的，`show_all()` 之前设的锚点设在还没有 layer surface 的 widget 上。这是这台机器上
    量出来的，代码里写明了理由。
  - `panel_display = "current"` 与四角同时用（默认设置那条路径）：面板落在右下、自动消失在设定秒数整。

**「每个触发都重读配置」是这条承诺的机制，不是文案**：面板是复用的（规格 §12 的 `activate` 语义），所以
`Panel::start(press)` 每次触发先 `reread_prefs()`——读文件，**再**按 `Press`（刷新 / ⟳）决定用什么 recall。
⟳ 换云端是关于那份配置的判断，在读取之前决定就会拿上一次触发的 provider；同一处也重新贴正文下面那句提示、
重新应用位置与高度上限，否则换了 provider / 换了角之后它们会留在上一次的状态。

**放置与测量是两件事，不能共用一个 `Option<Monitor>`**：`Display::Current` 要让合成器自己挑输出
（所以 layer surface 不 `set_monitor`），但高度上限、降级窗口的角、以及降级窗口量的高度都需要真实的东西——
早期版本把 `None` 同时喂给两者，结果是默认设置下 `set_max_content_height(0)`（相当于没有上限），而降级
窗口干脆不定位。`geometry()` 现在返回 `(anchors, pin, measure)`，`apply_place` 按形态各取所需；降级窗口
的底角用 `allocated_height()` 量（首次分配前用 `HEADER_HEIGHT`），并在 `size-allocate` 时重新定位——面板是
先短后长的（先只有「正在读取…」，随后才堆上原文）。**那条 `size-allocate` 里只能摆几何，不能碰文案**：
GTK3 的 `gtk_label_set_text` 无条件重算标签并排队一次 resize，在分配回调里改标签就是让回调自己喂自己
（只在降级窗口 + 本地提示可见时才走到）。`apply_layout()` = `apply_geometry()` + `show_note()`，前者是几何、后者是文案。

**复制的是「屏幕上那份」**：`copy_text` 走 `document::sections(artifact, prefs)`——和面板正文**同一个**过滤器，
所以 `app.show_*` 对两边同时生效。早期版本面板隐藏了某节、复制却把它带上，那正是「复制你看到的东西」这句
承诺反过来的样子；`sections()` 放在 `document`（叶子）而不是 `state`，就是为了不让「复制跟随正文」变成
`state ↔ document` 的循环。全部节都关掉时 `copy_text` 返回 `None`（不是空串），复制按钮因此变灰而不是
报一句「已复制」却只放了空白。

**`Recall` 住在 `worker.rs`**：它是「这次运行怎么跑」的指令（是否绕开缓存、用哪个 provider），不是面板在
显示什么。放在 `state.rs` 会让 `prefs ↔ state` 成环（`Prefs::regenerate_recall` 要读它）；放进 worker 之后
依赖是单向的：`prefs → worker`、`state → worker`。

**降级窗口的四角是尽力而为**：Wayland 上 `gtk_window_move` 对普通 toplevel 是请求而非命令（合成器可以
忽略），而 flake 把 `GDK_BACKEND` 钉在 wayland、v0 不支持 X11。所以 GNOME 那条路径上「四角」不保证做到；
代码与 README 都在说这件事，而 layer 路径（niri / wlroots 家族）才是四角真正成立的地方。仍然照常请求，
是因为它在被尊重的地方就是对的答案，而不做等于把一个「做不到」的设置藏成「没实现」。**这条限制决定了
`position::window_origin`、`height_of` 与 `size-allocate` 重新定位这套代码的价值**：它们在 Wayland 上可能
不生效，但它们是「在被尊重的会话上做对」的那份实现，删掉就等于承认这个设置永远不做。

## 给下游的交接

- **tickets/17（加固）**：真机 smoke 只覆盖了「四角 + 自动消失 + 文案齐全」。**没有走完的**：真实 provider
  的完整流水线（本机没有 key、也没有在跑的 llama.cpp，所以 smoke 停在「正在解释…」）、✕ 之外的
  `activate` 复用/看门狗自退复验、GNOME 降级窗口、X11 报错。这些仍归 17 的手测清单。
- **⤢ 复用已有窗口时不切屏**：`open.rs` 用不带参数的方式 spawn `plainly-desktop`，交接靠它自己的 socket
  说一句 `activate`——所以「窗口没开」时打开的就是历史页（规格 §12 要的那件事成立），但**窗口已经开着、
  而且停在设置页时，它不会切回历史**。让交接带上「去哪一屏」是票据 14 的激活语义（要改它 socket 上的那句
  话），不是面板能单方面决定的；本票据照规格 spawn，把这个缺口记在这里而不是假装做到。17 若要保证落在
  历史页，就把那一句话扩成 `activate <screen>`。
- **`panel_display` 从某个 `output-N` 改回 `current` 不会当场解绑**：`gtk_layer_set_monitor` 在绑定里没有
  `None` 重载，gtk-layer-shell 也没有「清掉输出」这个操作，所以已经 map 的 layer surface 会留在原输出，
  直到下一个进程（面板本来就不常驻，下一个进程就是下一次按键）。要真正做到得拆掉表面再重建——为一个人
  只会改一次的设置闪一下，不值。代码里在 `geometry()` 那条分支上写明了这个限制。
- **复制走 data-control 写**：`wl-clipboard-rs` 的 `copy` 在一条**独立线程**上 serve（selection 的 owner
  是进程，不是合成器）。面板自己退出时这条线程一起消失——「已复制」到「粘贴」之间就是它要覆盖的窗口，
  而面板正好在那段时间里在屏幕上。若将来面板要在复制后立刻自毁，这条承诺要重新看。
- **chip 的 `本地` 是缓存命中时按"现在"判断的**：Lookup Key 里没有 endpoint，所以召回一条旧记录时
  locality 由当前配置推出。这是有意的——提示说的是"现在跑的是谁"——但改了 `[providers.x].endpoint` 之后
  第一条召回会按新端点标注。要么接受，要么把 endpoint 收进 Artifact（契约变更），本票据选前者。
- **`panel_settings_honoured` 与「节全显示」的提示删掉了**：面板现在真的读这四族键，那条"改了不会立刻
  看出变化"的说明已经不成立；删字段而不是留一个恒为 `true` 的值，是因为前端也读不到它了。
- **`flake.nix` 补了 `atk`**：`LD_LIBRARY_PATH` 只由 `libraries` 拼出、不传递，gtk3 闭包里的
  `libatk-1.0.so.0` 不在路径上——面板能编能过测试（测试不开显示器），却在 shell 里起不来。这是
  「跑一遍才知道」的那类缺口，按 AGENTS.md 修在 flake 里。

## 已知取舍

- **只做标题栏的 ⟳，不做每节自己的 ⟳**：票据要的是"一键换云端重生成"，标题栏那一个就是它；每节再放一个
  ⟳ 会多一套 slot 级别的重跑状态，而那段代码在本票据里除测试外无人可达，所以删掉而不是留着当"以后有用"。
- **`复制` 复制的是面板自己的文本**（中文分节标题 + 原文），不是导出的 markdown（英文 `### Original`）：
  复制的是**眼前这份**东西，走 core 同一个 `render_with` 文档格式，所以两种表面的形状不会分叉。
- **拒绝态没有额外的按钮**：全部拒绝/出错/加载状态共用标题栏的 ✕；⟳ 也一直在，它的语义是"重新读剪贴板"，
  所以「改了敏感内容再拍一次」与「关掉它」都是确定可达的。不为拒绝态另造一套按钮，是因为那会让"永远关得掉"
  变成每个状态各自要记得的事。
