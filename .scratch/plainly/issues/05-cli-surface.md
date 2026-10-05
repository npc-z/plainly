# 05 CLI 命令面与守护进程触发

Type: grilling
Status: resolved

## Question

定下 CLI 的**子命令与 I/O 契约**，以及它作为守护进程触发器的角色。

- 输入：stdin / 文件路径 / 系统剪贴板。
- 输出：markdown / json（原始结构）/ 仅退出码（脚本用）。
- 子命令与选项（`explain`、`history`、`config` 之类）以及**退出码语义**。
- `plainly --explain-clipboard` 如何把请求投递给**已在运行的桌面进程**：倾向 `tauri-plugin-single-instance` 的 argv 传递（回调能拿到第二个进程的 argv，无需自建 IPC 服务）；备选是 `$XDG_RUNTIME_DIR` 下的 unix socket。决定用哪个、以及桌面进程没在跑时的行为（自己起、还是纯 CLI 输出）。
- CLI 是 v0 的一等公民（core 的验证器），不是附属品——但界面能力以文本为准。

## Comments

- **来自票据 08 的一条要求**：CLI 要能**绕开幂等键缓存重新生成**（例如 `--regenerate`）。理由：同一段文本默认会命中已落库的 Explanation（票据 08 的 `lookup_key`），而"重新生成"是唯一的修正手段——在有缓存的系统里它不是可选功能。脚本里没有它就等于没有修正途径。
- 顺带注意：CLI 与 GUI 会**同时写同一份配置**（票据 15 已认下这个代价），所以 CLI 的写操作也要走原子写 + 外部修改检测。

## Answer

四条决策，2026-10-05 全部按推荐采纳。

### 读源码得到的事实（不是文档）

读的是 `tauri-plugin-single-instance` **2.5.2 的源码**：

- **Linux 上交接走 D-Bus**（不是 socket）：主实例占用名字 `<identifier>.SingleInstance`——本项目即 **`dev.plainly.app.SingleInstance`**（identifier 由票据 15 定）——并暴露 `org.SingleInstance.DBus.ExecuteCallback`。
- **回调跑在主实例里**，签名 `FnMut(&AppHandle, argv, cwd)`：拿到的是**第二个进程的 argv 与 cwd**。
- **第二个进程自动退出**。文档原话："the second instance never reaches `tauri::Builder::run`: it hands its arguments and working directory off to the first instance and **exits immediately**"。**所以在第二个进程里"改成打印"是做不到的——它会直接消失。**
- **由此推出的硬约束**：一个二进制兼作 CLI 与 GUI 时，CLI 的输出会跑到 **GUI 进程的 stdout**，调用者的管道里什么都没有。要避免，只能在 `main()` 最顶端、**构造 Tauri 之前**把 CLI 子命令分流掉（那也正是我们没选这条路的原因之一，见决策 1）。
- 小细节：开了 `semver` feature 时 D-Bus 名字会带上版本号 → 升级后新旧实例互不接管。**决定：不开该 feature**，名字只随 identifier 走。

### 1. 两个二进制

- `plainly`：**瘦 CLI，只依赖 core**
- `plainly-desktop`：Tauri GUI，带 `--explain-clipboard`

**IPC 整条删掉。** 没有任何场景需要 CLI 去触发面板——触发面板的是**合成器键绑**，而键绑可以**直接指向桌面二进制**，后者自带 single-instance 交接。我们一行 socket 代码都不用写。票据原文里"备选：`$XDG_RUNTIME_DIR` 下的 unix socket"因此作废。

**附带硬约束**：CLI crate **不得依赖 GTK/webkit**。否则没有图形库的环境（CI、headless）里 CLI 直接跑不起来——而这正是选两个二进制的主要收益。

### 2. 桌面端没在跑时：自己起

`plainly-desktop --explain-clipboard` 在无实例时正常启动并显示面板。理由：**按键的人不在看终端**——键绑是按"立刻看到解释"的期望按下的，退回打印等于什么都没发生。代价是首次冷启动延迟（诚实记下：票据 14 只量过热态 1.7 秒）。
**GUI 确实起不来时（没有 display）退回打印**，作为错误路径的兜底，不是主路径。

### 3. 四个顶层子命令

| 子命令 | 覆盖什么 |
|---|---|
| `explain` | **无子命令时默认走它**（`echo … \| plainly` 直接可用）；输入 stdin / 文件 / `--clipboard` |
| `history` | 列表、单条查看、**三条导出**（Markdown / Anki CSV / 原始 CSV）、单条删除与全量清除 |
| `providers` | 探测本机常见端口并列出运行时（票据 04 的落点）、显示当前 provider 与**能力探测结果** |
| `config` | 读写配置；**`config path`** 打印配置文件路径（dotfiles 与排障都用得上） |

### 4. I/O 契约与退出码

- **stdout 只出产物，stderr 出人类信息**——管道与脚本保持干净。
- `--format markdown|json`（默认 markdown）；`--regenerate` **绕开幂等键缓存**（票据 08 的硬要求）。
- 退出码：**0** 成功 · **1** provider/生成失败 · **2** 用法错误 · **3** 未配置（没有 key，或没有可用 provider）。
- **`explain` 在无输入且 stdin 是 TTY 时报用法错误**，不默默阻塞等输入。

### 给下游

- **票据 13**：workspace 必须拆到能让 CLI 单独构建（`cargo build -p plainly-cli`）且不带 GUI 依赖；**devShell 本身不变**（桌面 crate 仍需要那套 GTK 依赖）。
- **票据 07**：键绑指向 **`plainly-desktop --explain-clipboard`**，不是 `plainly`。
- **票据 06**：面板可以由**运行中实例收到的 argv** 触发（single-instance 回调），UI 要给这条路径一个明确的状态。

## Comments

- **⚠️ 2026-10-05 更正（来自票据 12）**：上面"两个二进制"里的第二个——`plainly-desktop`（Tauri GUI）——**画不出 layer 面板**（票据 12 实测：表面造得出来，连纯色背景都不上屏）。所以**二进制集合很可能是三个**：`plainly`（CLI）/ **`plainly-panel`（独立 layer-shell 面板进程）** / `plainly-desktop`（Tauri 主窗口：历史、设置、导出）。
  这不推翻本票据的其他决策（CLI 依旧瘦、依旧不依赖 GUI；stdout/stderr 约定、退出码、`--regenerate` 都不变），只是"两个"这个数字不再确定。**谁 spawn 面板由票据 07 决定。**
- **✅ 2026-10-05 由票据 07 定案：三个二进制。**
  - `plainly` —— 瘦 CLI，只依赖 core
  - `plainly-panel` —— **独立的 layer-shell 面板进程**，`GtkApplication`（id `dev.plainly.panel`），由合成器键绑 `spawn "plainly-panel" "--clipboard"` 直接拉起；自己用 data-control 读剪贴板，**桌面应用不必常驻**
  - `plainly-desktop` —— Tauri 主窗口（历史、设置、导出）
  **`--regenerate` 的归属要注意**：面板走的是幂等键缓存（票据 08），所以"重新生成"这个出口在**面板上**也要有（面板是用户唯一会看到解释的地方），不能只在 CLI 里。
