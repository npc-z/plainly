# 真机手测清单：面板、降级与 CLI

规格 §Testing Decisions 把面板与主窗口排除在自动化之外：GTK 的渲染、图层、看门狗与
退出路径需要**一块屏、一个合成器和一个真的剪贴板**，这些在测试进程里都不存在。自动化覆盖
停在两条缝上（`plainly-core` 的公共 API、`plainly` 二进制）加上各表面的纯决策函数；
`crates/plainly-panel/src/{state,prefs,position,provenance,document,watchdog}.rs`
与 `crates/plainly-desktop/src/watch.rs` 的判定都有单元测试，剩下的是「它到底上不上屏」。

本文件是**面板、会话降级与 CLI** 这份；主窗口与「复制自动弹」在
[desktop-manual-checks.md](desktop-manual-checks.md)。

## 0. 准备

**用装出来的二进制走这份清单，不要用 `target/debug` 的那份。** 两者的差别正是这份清单要验的东西：
`target/debug` 的二进制靠 dev shell 的 `LD_LIBRARY_PATH` 活着，而 niri 键绑 spawn 的进程继承的是
niri 的环境，不是那个 shell。装出来的那份把库路径写在自己的包装脚本里，所以它才是「别人装完能用」
的那个东西。

```sh
nix build .            # → result/bin/{plainly,plainly-panel,plainly-desktop}
export PATH="$PWD/result/bin:$PATH"   # 键绑按名字找 plainly-panel，就靠这一行
```

（`nix profile install .` 装进 profile 也一样，只是 `PATH` 由 profile 提供。开发中要改代码时才用
`nix develop` + `cargo build --workspace`；那时 `target/debug` 里的二进制是**另一个**环境。）

面板从裸会话起得来这件事本身有机器判据，不用靠肉眼：`scripts/check-flake-packages.sh` 会断言
CLI 的闭包里没有图形库、两个闭包大小不同、面板在 `env -i`（空环境）下不会报
`error while loading shared libraries`。§1.2 的焦点与 §2.1 的键绑仍然要人按一次键。

用 `nix flake update` 升级输入（nixpkgs / fenix / crane）并**提交**之后，再跑
`scripts/check-incremental-build.sh`：它只 eval、不构建（约 10–15 s），验证「改一行 `.rs` 不会让
crane 的依赖缓存失效」在升级后的输入上仍然成立；加 `--self-test` 会分别改两个包各自的 lock 来源
（repo 的 `Cargo.lock` 与 `nix/cli-workspace/Cargo.lock`），确认两处检查都还活着，否则任何一处变成
恒真都不会被发现。它比对的是两份 `git archive HEAD` 的拷贝，所以未提交的改动不参与——这也是
「先提交升级」的原因。

CLI 的合成 workspace（`plainly-core` + `plainly-cli`）的 manifest 与 lock 是**生成物**，提交在
`nix/cli-workspace/`。改完根 `Cargo.toml` / `Cargo.lock` 之后，在 dev shell 里跑
`bash scripts/generate-cli-workspace.sh` 重新生成并提交：它把根清单投影成 CLI 的 manifest、把根
lock 剪成种子后交给 cargo 规范化（首次运行需要网络或已填充的 cargo 缓存，之后是毫秒级）。
生成器能证明「这份 lock 是 cargo 写的」，证明不了「它还跟根 lock 同步」——那半个答案是
`bash scripts/check-cli-workspace.sh`：纯 `python3`、离线、**不装 Nix 也能跑**（约 1 s；投影规则在
`scripts/cli_workspace.py`，与生成器共用，且导入时不写字节码），检查
committed manifest 是否等于根清单的投影、committed lock 的每个版本是否都在根 lock 里、两个 crate
继承的依赖是否都被覆盖；漂移时它打印要重跑的命令。两者都是手动脚本，不进 CI。

每次检查前清一份临时状态，不要碰真实配置与历史：

```sh
export HOME=$(mktemp -d)                 # 关键的一行：其余每个没设的变量都回落到它
export XDG_CONFIG_HOME=$HOME/config      # 配置
export XDG_DATA_HOME=$HOME/data          # 历史库
export XDG_CACHE_HOME=$HOME/cache        # 能力探测的结论
export PLAINLY_DISABLE_KEYRING=1         # 不去碰会话 keyring
```

**`HOME` 那一行才是隔离的那一行**：`Paths` 对每个没设的 XDG 变量各自回落到 `$HOME`
（`crates/plainly-core/src/paths.rs`），所以把它指到临时目录，配置、历史库与缓存就都落在那里。
三个 XDG 变量（CONFIG / DATA / CACHE）仍然逐个写出来，是为了和集成测试的 `command()` 逐项一致
（`crates/plainly-cli/tests/support/mod.rs`）——照抄测试用的那条环境，清单与测试就不会各自漂移。

一个**真的 provider** 才能走完解释流水线。没有 key、也没有在跑的本地 runtime 时，面板会停在
「正在解释…」——那本身也是清单里的一项（加载态），但结果态、复制与 ⟳ 需要 §3 的假 provider。

## 1. niri 上的面板

启动前把要解释的一段英文放进剪贴板（`wl-copy "…"`），然后：

```sh
plainly-panel --clipboard        # 装出来的那个（PATH 上是 result/bin，见 §0）
```

| # | 检查 | 期望 |
| --- | --- | --- |
| 1.1 | 面板出现，宽度约 470px | 顶部标题栏 `Plainly` · provenance chip · 复制 · ⟳ · ⤢ · ✕ 五项齐全；正文里原文在上 |
| 1.2 | **焦点不变** | 面板出现前后 `niri msg focused-window` 是同一个窗口；面板期间继续在原来的窗口里打字，字照常落进去 |
| 1.3 | **不进窗口列表** | `niri msg windows` 里没有面板（layer surface，不是 window） |
| 1.4 | **Esc 无效** | 按 Esc 面板还在（`keyboard_mode=NONE`，面板收不到键盘事件） |
| 1.5 | **✕ 可关** | 点 ✕ 进程退出，退出码 0 |
| 1.6 | 再按一次（键绑或再跑一次 `plainly-panel --clipboard`） | 仍只有**一个**面板进程；内容重读剪贴板并刷新（换一段文本后按下，正文换成新的那段） |
| 1.7 | 位置设置生效 | `plainly config set app.panel_corner bottom-right` 之后再触发，面板落在右下；四个角各一次 |
| 1.8 | 自动消失 | `app.panel_autohide = true` + `app.panel_autohide_seconds = 12`：约 12 秒后进程自己退出（退出码 0）；把开关关回去，同一份配置下面板一直活着 |
| 1.9 | 关掉开关不等于时长 0 | `panel_autohide = false` 且 `panel_autohide_seconds = 5` 时**不**自动消失（开关与时长是两个独立键） |
| 1.10 | 看门狗：正常路径 | 面板出现后**仍然活着**超过 5 秒（看门狗只在「从没上屏」时开枪）。本机已验 |
| 1.11 | 看门狗：病态路径 | **要造出「永远画不出来」才行**，而正常的 layer 表面一旦 map 就立刻画（`connect_draw` 只在 `is_mapped()` 时解除看门狗），所以这一项在 niri 上无法可靠复现。可以试的构造：阻止合成器 map（例如把一个假的 `wayland-99` socket 配给 `GDK_BACKEND=wayland`，但那会在**连上之前**就退出，走不到绘制阶段）、或在嵌套/无渲染输出的合成器里跑。判定的边界本身有单元测试（`crates/plainly-panel/src/watchdog.rs`：`expired_at` 正好在 grace 到点时到期、`note_drawn` 之后永不到期）。**本机未复现** |

### 多块与两种拒绝

| # | 检查 | 期望 |
| --- | --- | --- |
| 1.12 | 多块堆叠 | 剪贴板里放 3 段（空行隔开、合计不超过 5 块）：面板按原文顺序堆叠在一个可滚动面板里，没有分页、没有隐藏状态 |
| 1.13 | 过长拒绝 | 放 6 块以上：面板明说「太长」并指出 `plainly` CLI 或先拆分；✕ 照样能关 |
| 1.14 | 敏感拒绝 | `wl-copy --sensitive "…"` 之后触发：面板明说这段带敏感标记、**未发送也未保存**；provider 请求 0 条、历史库不增加；✕ 能关 |
| 1.15 | 每种状态都有出路 | 加载 / 结果 / 出错 / 拒绝四种状态里 ✕ 都在同一个位置（标题栏），加载态里原文先出现 |

敏感那一条可以用历史库的行数对着看：

```sh
sqlite3 "$XDG_DATA_HOME/dev.plainly.app/history.db" 'select count(*) from records;'
```

`sqlite3` 不在默认 shell 里：要它就用 `nix develop .#inspect`（那个 shell 只多这一件工具）。

### 结果态、复制与 ⟳（需要 §3 的假 provider，或一个真 key）

| # | 检查 | 期望 |
| --- | --- | --- |
| 1.16 | 本地模型提示固定在正文下 | provider 是本地时，正文下方一条「本地模型……请对照上方原文，或 ⟳ 换成云端重新生成」；云端 provider 时没有 |
| 1.17 | 复制 | 点「复制」后粘贴到编辑器：是面板里那份文本（中文分节标题 + 原文），且**跟随分节开关**（关掉翻译再复制，粘贴里没有翻译） |
| 1.18 | ⟳ 绕开缓存 | 面板已缓存的那段再按 ⟳：provider 收到第二次请求（不是缓存命中）；本地 provider + 有云端 key 时换成云端，没有可换的云端时用同一个本地模型重跑，正文下那句提示跟着换 |
| 1.19 | ⤢ | 点 ⤢ 打开主窗口（或把已开的那个带到前面）；主窗口没开时落在历史页 |
| 1.20 | chip | 本地 provider 显示「本地」，云端显示「云端」；模型名与实际一致 |
| 1.21 | **高度跟着内容，到上限为止** | 一段短原文：面板正好容下五节加底部那句本地模型说明，最后一行不会被面板边缘切掉；换成很长的一段：正文最多到 `app.panel_height`（含标题栏与底部说明），其余在面板内滚动，标题栏与底部说明始终可见。面板只有一行高就是这条没做到——layer surface 的高度取自窗口的**最小**尺寸（gtk-layer-shell 这么要），所以得由 `Surface::state_body_height` 说出来；没有 XSettings 守护进程的会话还会先丢掉那把尺子（见 `measuring_screen`） |
| 1.22 | 宽高按配置 | `plainly config set app.panel_width 700` 与 `app.panel_height 300` 之后再触发：面板确实变宽、变矮，长内容在面板内滚动，标题栏与底部说明仍可见；手工把配置文件里的 `panel_width` 改成 2000（超出 320..900）面板按 900 收住——`plainly config set app.panel_width 2000` 会被拒绝（exit 2），不会写进文件。设置界面「行为」里的两个数字框写的是同样的两个键 |

### 未入库与重复 Passage（ADR-0003 的两处面板行为）

这两项用 §3 的假 provider：先按 §3 起好 stub、让 provider 指过去，并跑一次 §3.1 让历史库里有记录。
它们验的是 CLI 与面板共用的那一条运行路径——两边都由 `plainly_core::Runner` 决定一个 turn。

| # | 检查 | 期望 |
| --- | --- | --- |
| 1.23 | 历史库拒绝写入：答案仍在面板上 | 先按下面的命令装一个拒绝写入的 trigger，再用 `wl-copy` 放一段**历史里没有**的英文并触发面板：正文照常出答案（可复制），正文下方多一条「这个答案没有存进历史：…。」；`select count(*) from records;` **没有增加**；✕ 能关。验完把 trigger 删掉 |
| 1.24 | 同一次触发里的重复 Passage 只问一次 | 用 `wl-copy` 放**同一段、历史里没有**的英文两遍（空行隔开）再触发：两个位置显示同一份答案，stub 的 `scripts/stub-provider.requests.log` 只长 **1** 行。没有可用云端 provider 时再按 ⟳ 重来一次：日志同样只长 **1** 行（⟳ 只绕过缓存，不会为重复的那段再付一次） |

拒绝写入的 trigger（`sqlite3` 不在默认 shell 里，所以用 `nix shell` 取一份；先跑过 §3.1，库和表才在）：

```sh
history_db="$XDG_DATA_HOME/dev.plainly.app/history.db"
nix shell nixpkgs#sqlite -c sqlite3 "$history_db" \
  "CREATE TRIGGER refuse_write BEFORE INSERT ON records \
   BEGIN SELECT RAISE(ABORT, 'the history refused the write'); END;"
# …按 1.23 / 3.10 触发…
nix shell nixpkgs#sqlite -c sqlite3 "$history_db" 'select count(*) from records;'
nix shell nixpkgs#sqlite -c sqlite3 "$history_db" 'DROP TRIGGER refuse_write;'
```

用 trigger 而不是把目录设成只读：SQLite 在 WAL 模式下会沿已经打开的 `-wal` 文件描述符继续写，
只读目录拦不住它；trigger 让同一次真实写入在 SQLite 层被拒绝，正是测试里用的那个失败（`crates/plainly-cli/tests/batch.rs`、`crates/plainly-core/tests/runner.rs`）。

## 2. 键绑与常驻

| # | 检查 | 期望 |
| --- | --- | --- |
| 2.1 | niri 键绑 spawn | `Mod+Shift+E { spawn "plainly-panel" "--clipboard"; hotkey-overlay-title="Explain clipboard"; }` 能起面板，且快捷键浮层里显示 `Explain clipboard` |
| 2.2 | `spawn` 不走 shell、不展开 `~` | 把二进制只放到 `~/.local/bin/plainly-panel`：键绑**不**能找到它（`~` 没展开），写全路径或放进 `PATH` 才行 |
| 2.3 | 默认路径下没有常驻 | `copy_auto_popup` 保持默认 `false`、不跑 `--watch` 时：没有 `plainly-desktop`、也没有别的东西常驻；面板只在按键时存在 |
| 2.4 | 面板退出不留东西 | 关掉面板后 `$XDG_RUNTIME_DIR` 下没有面板留下的 socket / 锁（单实例是 GApplication 的，进程退出即释放） |

## 3. 端到端（CLI + 面板 + 主窗口，一个假 provider）

本机没有 key、也没有在跑的本地 runtime 时，这一节用**一个假的 OpenAI 兼容端点**把整条流水线
走完：它只是把「解释」换成一个固定答案，网络之外的每一段代码都是真的。仓库里带了那个端点。

**先看清哪个端口是空的**——默认 8080 只是 llama.cpp 自己的上游默认，不是本仓库的约定。真实的
llama.cpp 常被起在别的端口（本项目 bench 用的就是 **3060**），而一个端口只能有一个属主：撞上了
要么是 stub 起不来，要么更糟——你以为是 stub 在答，其实是本机那个真模型在答。

```sh
plainly providers discover                # stdout：答了的那些（这些端口不能复用）
                                          # stderr：它去问的地址（默认四个，
                                          # 加上你在配置里指过的 loopback 端点）
```

`discover` **stdout** 上列出的地址（例如 `http://127.0.0.1:3060/v1`）不能拿来做 stub；挑一个它去问、但没人答的端口，一路写进下面两处。下面是「你的 runtime 在 3060」时的那份：

```sh
python3 scripts/stub-provider.py --port 8080 &   # 每次请求追加一行到
                                                 # scripts/stub-provider.requests.log
plainly config set app.provider llamacpp
plainly config set providers.llamacpp.endpoint http://127.0.0.1:8080/v1
plainly config set providers.llamacpp.model stub-model
```

端口被占时 stub **报错退出**并让人去 `providers discover` 看看谁在那里（不会静默地假装在服务）。
`providers.llamacpp.endpoint` 与 `--port` 必须是同一个端口，否则 CLI 与面板问的是本机那个真 runtime。

请求日志就是缓存那一项的判据：同一段解释两次，日志只该长一行。

| # | 检查 | 期望 |
| --- | --- | --- |
| 3.1 | CLI 端到端 | `echo "The manager had already gone to ground." \| plainly` 退出 0，stdout 是原型那五节**形状**的 markdown；假 provider 的答案里 `grammar` 是 `null`，而空节会被整个省掉，所以**这次数出来是四节**（原文 / 改写 / 关键帮助 / 翻译）。数到四不是失败 |
| 3.2 | stdout 只有产物 | 上面那条的 stderr 说明用了哪个 provider，stdout 可以直接进管道（`\| tee` 再 `wc -l`，没有人类信息混进去） |
| 3.3 | 历史即缓存 | `plainly history` 列出刚才那条；**同一段再跑一次**：provider 不收到第二次请求（stub 的终端没有新增行），记录也不增加 |
| 3.4 | 面板端到端 | `wl-copy` 放一段英文 → `plainly-panel --clipboard`：面板从「正在解释…」走到结果态，正文与上面 CLI 的 markdown 同源；`sqlite3 … 'select count(*) from records;'` 增加 1 |
| 3.5 | 面板命中缓存 | 关掉面板，再对**同一段**触发：provider 不收到新请求，面板立刻出结果 |
| 3.6 | 主窗口 | `plainly-desktop` 打开后历史页有 3.x 的记录；搜索原文片段命中，搜译文里的中文命中 0 条，且搜索框下写着「中文翻译与例句暂不可搜」 |
| 3.7 | 主窗口的 ⤢ 交接 | 面板里点 ⤢：只有一个窗口，第二次启动把它带到前面（单实例）；窗口已经开着且停在设置页时它**不会**切回历史——这是已知缺口，见 §5 |
| 3.8 | 关键帮助是 bullet + 引文块 | 上面那条 stdout 里每条 Gloss 是 `- \`expression\`` + 缩进一行释义 + **空行 + `  > 例句`**；条目之间空行（松列表）。`glow` 那条命令下例句画成内缩的引用块，不和释义挤成一段。stub 的例句是 `The reporter went to ground until the trial was over.` |
| 3.9 | raw 导出无 markdown 记号 | `plainly history export raw`：`glosses` 单元格里每条三行——表达 / 缩进释义 / 缩进例句，**没有箭头、没有 `_`、没有 `>`**（电子表格单元格不是文档）；仍是一个单元格、仍按 CSV 规则转义 |
| 3.10 | 历史库拒绝写入：CLI 仍出答案、exit 1 | 先按 §1 的命令装好 `refuse_write` trigger。`echo "A sentence the history has never seen." \| plainly`：stdout 仍是完整的 markdown 产物；stderr 有 `plainly: chunk 1/1 was not stored: the history refused the write` 和结尾的 `1 of 1 chunks were not stored`；`echo $?` 是 **1**；库里没有这条。验完 `DROP TRIGGER`。这一条与面板的 1.23 是同一条运行路径的两个适配层 |

## 4. 降级与不支持

### GNOME（Mutter）：降级为取得焦点的普通窗口（**本机未验**）

GNOME 上既没有 data-control 也没有 layer-shell，所以面板退化成**一个会取得焦点的普通窗口**，
此时 GTK 自己的剪贴板 API 可用——两件事一起退化，是一次降级而不是两次。

| # | 检查 | 期望 |
| --- | --- | --- |
| 4.1 | 取词可用 | 在 GNOME 的 Wayland 会话里复制一段英文后触发：面板以普通窗口出现并**拿到焦点**，随后读出剪贴板并解释（不是空内容、不是停在「正在读取剪贴板…」） |
| 4.2 | 是窗口不是 layer 表面 | 有标题栏、可拖动、可最小化，出现在 GNOME 的窗口列表里 |
| 4.3 | 四角是尽力而为 | `app.panel_corner` 会被请求（`gtk_window_move`），但合成器可以忽略普通 toplevel 的移动请求：**不保证**落在角落。降级窗口的高度上限与底角定位在 resize 时重新应用 |
| 4.4 | 没有 ❌ 之外的保证 | 面板仍只有 ✕ 一条正常出路；自动消失与看门狗照常工作 |
| 4.5 | 复制自动弹灰掉 | 设置里「复制自动弹」**灰掉**并附一行「需要 wlroots 类合成器的 data-control 协议」（不是隐藏） |

**本机没有 GNOME 会话，4.1–4.5 未在真机走过**：它是这一份清单里唯一没有本地证据的一节。
降级由 `plainly_core::desktop::Desktop::presentation()` 决定，那条判定有单元测试
（`crates/plainly-core/tests/desktop.rs`），但「Mutter 上真的变成取得焦点的窗口、取词真的可用」
只能在 GNOME 机器上看。

### X11：明确不支持

| # | 检查 | 期望 |
| --- | --- | --- |
| 4.6 | 面板明确报不支持 | 在 `DISPLAY=:0` 且没有 `WAYLAND_DISPLAY` 的环境里跑 `plainly-panel --clipboard`：退出码 1，stderr 一行说 **X11 is not supported in v0**、v0 支持 Wayland（niri / sway 等）、以及 CLI 照样可用——不是 Wayland 客户端库的 socket 报错 |
| 4.7 | `--clipboard` 也明确报不支持 | 同样环境里 `plainly explain --clipboard`：退出码 1，stderr 说 X11 不支持并指向文件 / 管道 |
| 4.8 | **CLI 不受影响** | 同样环境里 `echo "…" \| plainly` 正常出四节（同上，`grammar` 为 `null`）；`plainly history`、`plainly config show`、`plainly providers probe` 都正常 |
| 4.9 | 无会话也有话说 | 既没有 `WAYLAND_DISPLAY` 也没有 `DISPLAY`（容器 / cron）时：面板退出 1，stderr 是 `no Wayland session: the panel needs one`；`--clipboard` 退出 1，stderr 是 `plainly: no Wayland session: reading the clipboard needs one`——**与 X11 那句不同**，因为修法不同；管道与文件照常工作 |

4.6–4.8 可以直接在 niri 的终端里用环境变量造出来（不用真的进 X11）：

```sh
DISPLAY=:0 env -u WAYLAND_DISPLAY plainly-panel --clipboard; echo "exit=$?"
DISPLAY=:0 env -u WAYLAND_DISPLAY plainly explain --clipboard; echo "exit=$?"
DISPLAY=:0 env -u WAYLAND_DISPLAY sh -c 'echo "The manager had already gone to ground." | plainly'
```

## 5. 已知没走到的部分

这些**不是**缺陷，是 v0 明确不做、或本机做不了的事；诚实记在这里，而不是让清单看起来全覆盖。

- **规格的 Out of Scope 整张表**都是没做的（下面照 `spec.md` §Out of Scope 逐条抄，不是摘要）：web API、MCP server、截图 OCR、多设备同步、
  SRS 复习循环、macOS / Windows 桌面、Tauri 发布链路（公证 / 签名 / 自动更新）、浏览器插件、
  X11 会话支持、面板自由拖动、敏感内容的启发式判定、历史库加密、中文翻译的全文搜索、
  自动打标、卡片 / 复习式导出（Anki CSV）、提示词片段注册表、**删除出厂提示词自己的规则**（附录只能追加）、
  项目级配置覆盖、流式输出、**对本地模型输出正确性的任何承诺**。
- **GNOME 那一节（§4.1–4.5）没有本地证据**：本机是 niri，清单里其余每一项都能在这里跑，
  这一节需要一台 GNOME 机器。
- **`--watch` 的进程间交接**：两个真实进程之间那句 `activate --watch|--nowatch` 的话没有自动化
  测试（`Watching` 这个纯值有），在 [desktop-manual-checks.md](desktop-manual-checks.md) 的
  「复制自动弹」一节里。
- **面板的四角只在 layer 路径上成立**：GNOME 的普通窗口上 `gtk_window_move` 是请求而非命令，
  见 §4.3。
- **`panel_display` 从某个 `output-N` 改回 `current` 不会当场解绑**：已经 map 的 layer surface
  会留在原输出，直到下一个面板进程。
- **⤢ 不切屏**：窗口已经开着并在设置页时，⤢ 不会把它切回历史页。
- **复制走 data-control 写**：`wl-clipboard-rs` 的 `copy` 在一条独立线程上 serve selection
  （owner 是进程），面板自己退出时这条线程一起消失——「已复制」到「粘贴」之间就是它要覆盖的
  窗口，而面板正好在那段时间里在屏幕上。
- **敏感判定的覆盖面**：只认 `x-kde-passwordManagerHint` = `secret`；文件与 stdin 没有类型标记；
  不遵守该约定的密码管理器不被保护。
- **`max_tokens` 与 150 词阈值成对**：改一个必须看另一个（规格 §Further Notes）。
- **看门狗的病态路径没有反向复现**：正常路径「从不触发」是清单 1.10（本机验过）；「渲染真的
  失败时 5 秒自退」是 1.11，本机造不出来（假 `WAYLAND_DISPLAY` 在连接前就退出，走不到绘制阶段），
  判定本身由 `crates/plainly-panel/src/watchdog.rs` 的单元测试覆盖。
- **面板的运行级凭据 notice 没有手测步骤**（ADR-0003）：它要的是「keyring 在、
  但读不到」——坏掉或锁住的 session bus / 没有 Secret Service。本清单的准备段用
  `PLAINLY_DISABLE_KEYRING=1` 恰好是「不去碰 keyring」，造不出这个状态；硬造它只会验到 keyring
  库自己的行为。这条改由 core 测试钉住：
  `an_optional_key_that_is_unreadable_opens_with_one_notice_and_explains_without_a_key`（notice
  恰好一次、turn 不带 key 继续）与 `a_required_key_that_is_unreadable_is_not_a_missing_key`；
  面板侧只是把 core 的 notice 列表逐条读一遍（`crates/plainly-panel/src/worker.rs` 的
  `report_new_notices`），文案由 `crates/plainly-panel/src/strings.rs` 的单元测试覆盖。这里不写
  一条按不出来的手测项。
- **键绑 spawn 的前提**：`nix build .` 装出来的面板自带库路径，但 `result/bin` 必须在 `PATH` 上；
  剩下的只是把那一行 `binds` 写进 niri 配置，然后按 §2.1 真按一次键。
