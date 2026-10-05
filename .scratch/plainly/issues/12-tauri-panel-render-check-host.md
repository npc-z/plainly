# 12 Tauri 面板的内容渲染验证（宿主机）

Type: task
Status: resolved

## Question

在**宿主机**（不经 agent 沙箱）跑一遍原型分支上的 Tauri 面板，确认 **WebKit 能否把内容画进 layer-shell 表面**。

这是 [02 面板形态原型](02-panel-form-prototype.md) 留下的唯一空白。已知（02 已证实）：

- 表面本身是**对的**：`niri msg --json layers` 报 `{"namespace":"plainly-panel","layer":"Overlay","keyboard_interactivity":"None"}`，且**不在** `niri msg windows` 里，焦点不变。
- 但屏幕上一个像素都没有：表面 470x340、`is_layer_window() == true`、WebKit 的 WebProcess/NetworkProcess 都在跑、webview 子控件 `visible == true`。
- 沙箱里的证据指向**环境**：没有 `/dev/dri`，MESA 报 `ZINK: failed to choose pdev` / `egl: failed to create dri2 screen`。纯 GTK（Cairo，CPU）画得出来，WebKit 的合成路径不行。

## Checklist（2026-10-05 修正：**不要 checkout 那个分支**）

`prototype/panel-form` 是**孤立分支**——与 master 无共同祖先、只含 `prototype/panel-form/`。**检出它会把地图从工作区移走**（这是故意的，见票据 02）。所以用 worktree 取到仓库外的临时目录：

```sh
cd /home/npc/github/user/plainly
git worktree add /tmp/panel-proto prototype/panel-form
cd /tmp/panel-proto/prototype/panel-form/tauri

ls -l /dev/dri      # 沙箱里这项是 "No such file" —— 宿主上应该有，这是最可能的差异

# 第一次要编译（约 400 个 crate + Tauri，几分钟）。顺手复用你已有的 cargo 缓存：
nix-shell --run 'CARGO_HOME=$HOME/.cargo PANEL_SECONDS=25 cargo run'
```

**如果看到面板** → 就完事了：截图、告诉我路径。
**如果仍然全屏无像素** → 依次试两个变体，每次都要**等日志出现 `[proto][t+1s]` 再截图**（那是面板 map 之后 1 秒，早截会误判）：

```sh
# 变体 2：软件 GL
nix-shell --run 'CARGO_HOME=$HOME/.cargo LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe PANEL_SECONDS=25 cargo run' > v2.log 2>&1 &
until grep -q 't+1s' v2.log 2>/dev/null; do sleep 1; done
grim -g "0,0 700x640" v2.png
wait

# 变体 3：去掉 WEBKIT_DISABLE_COMPOSITING_MODE（它在部分 WebKitGTK 版本上导致空渲染）
nix-shell --run 'CARGO_HOME=$HOME/.cargo; unset WEBKIT_DISABLE_COMPOSITING_MODE; PANEL_SECONDS=25 cargo run' > v3.log 2>&1 &
until grep -q 't+1s' v3.log 2>/dev/null; do sleep 1; done
grim -g "0,0 700x640" v3.png
wait
```

**要带回来的**（把**路径**贴给我就行，图我自己看）：

1. 各次运行的 `*.png`
2. `ls -l /dev/dri` 的输出
3. 各次 `.log` 里的 MESA / EGL / GLR 行，以及全部 `[proto]` 行
4. 运行期间 `niri msg --json layers` 是否列出 `plainly-panel`

**收尾**（那个目录是一次性的）：

```sh
git worktree remove /tmp/panel-proto --force
```

**为什么原型现在是"不透明 + 实底"的样子**：`transparent(false)`，页面背景是纯色 `#2b3245`。那是沙箱里最后一次调试留下的状态，**正好适合这次测试**——只要 WebKit 画出任何东西，就是一个可见的矩形；透明底反而让"没画"和"画了透明内容"难以区分。

## Answer 要给出的事实

一条结论就够：**"一个 Tauri 进程内做 layer-shell 面板"是可行的，还是必须退回独立面板进程**（C 原型已证明那条路可用，见分支上的 `panel.c`；截图按隐私决定未保留到仓库里）。

若宿主机上仍画不出，请记下排查到的最后一层原因（是 WebKit 与 layer 表面的兼容问题，还是本机 GL 栈的问题）——这决定票据 07 与实现规格里面板该怎么写。

## Answer

**结论：不可以。一个 Tauri 进程内的 layer-shell 面板画不出内容——表面造得出来，却没有任何像素上屏。面板形态因此改为独立的 layer-shell 面板进程，即票据 02 当时列为退路、且已经实证过的那条。**

### 怎么测的（全部在宿主机上，GPU 与命名空间均可用）

先排除了环境：`/dev/dri` 可见（`card1`/`card2`/`renderD128`），EGL 正常（`AMD Radeon Graphics (radeonsi, renoir, DRM 3.64) Mesa 26.2.3`），`bwrap` 与 `unshare -Ur` 都 OK。（早先那条 `bwrap` 失败是**假阴性**——NixOS 没有 `/bin/true`。）

然后逐条验证原型状态，**全部正常**：

- `init_layer_shell` 之前：`mapped=false realized=false`（gtk-layer-shell 的前置条件满足）
- 之后：`is_layer_window=true`
- 合成器侧：`niri msg --json layers` 列出 `{"namespace":"plainly-panel","layer":"Overlay","keyboard_interactivity":"None"}`；焦点窗口 id 前后不变；不在 `niri msg windows` 里
- 控件树：`GtkApplicationWindow alloc=470x340 mapped=true` → `GtkBox 470x340` → **`WebKitWebView alloc=470x340 mapped=true`**

**而屏幕上一个像素都没有。** 左上抓过、屏幕中央抓过（排除"锚点没生效、表面被居中"）、`t+1s`/`t+5s`/`t+10s` 三个时刻抓过（排除"截早了"）。

**决定性实验**：把窗口**自己**涂成纯红（`window { background-color: #cc2222; }`，**完全绕过 WebKit**）→ 仍然什么都没有。**全屏 2560×1600 的颜色直方图里，匹配 `#cc2222` 的像素为 0。**

**对照组**：同一目录、同一会话里的独立 GTK C 原型（`panel.c`）**照常渲染**。

### 结论与排除清单

**表面能造，内容上不去。** 逐条排除：GPU、挂载命名空间、`/dev/dri`、窗口未 realize、控件未分配、锚点位置、截图时机——**以及 WebKit**（连 GTK 自己画在窗口上的纯色背景都不上屏）。

### 对地图的影响

- **票据 02 的结论更正**：它依据"合成器确认了表面"判定"一个 Tauri 进程就够"。表面确实造得出来，但**画不出内容**。面板形态改为**独立的面板进程**。
- **票据 05 的"两个二进制"不再确定**：面板成为独立进程后，二进制集合可能是三个（`plainly` CLI / `plainly-panel` 面板 / `plainly-desktop` 主窗口）。
- **票据 07 已解锁**，而它的核心问题变了：要定的是**谁 spawn 面板**——合成器键绑直接 spawn 面板进程（则桌面应用不必常驻），还是桌面应用常驻并由它 spawn。

### 资产

原型在 `prototype/panel-form` 分支；宿主机 worktree 跑完已清理（回收 2.2G），诊断用的红色已还原，结论写进分支 README。

## Comments

- **2026-10-05 追加：一条中间假设被测试推翻。** 我曾怀疑"`GtkApplication` + layer-shell 就不画"——依据是 C 原型用裸 `GtkWindow` 能画、而 Tauri 用的是 `GtkApplicationWindow`。**实测：`GtkApplication` + layer-shell 正常绘制**（有截图确认，`keyboard_mode=NONE`、无焦点）。
  所以本票据的失败是 **tao/wry 特有**的，与 GtkApplication 无关。**根因没有定位，但决策不依赖它**（面板已改为独立进程，见票据 02 与 07）。**后人若要追根因，别从 GtkApplication 这条线查。**
