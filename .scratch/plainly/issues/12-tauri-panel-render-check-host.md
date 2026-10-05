# 12 Tauri 面板的内容渲染验证（宿主机）

Type: task
Status: open

## Question

在**宿主机**（不经 agent 沙箱）跑一遍原型分支上的 Tauri 面板，确认 **WebKit 能否把内容画进 layer-shell 表面**。

这是 [02 面板形态原型](02-panel-form-prototype.md) 留下的唯一空白。已知（02 已证实）：

- 表面本身是**对的**：`niri msg --json layers` 报 `{"namespace":"plainly-panel","layer":"Overlay","keyboard_interactivity":"None"}`，且**不在** `niri msg windows` 里，焦点不变。
- 但屏幕上一个像素都没有：表面 470x340、`is_layer_window() == true`、WebKit 的 WebProcess/NetworkProcess 都在跑、webview 子控件 `visible == true`。
- 沙箱里的证据指向**环境**：没有 `/dev/dri`，MESA 报 `ZINK: failed to choose pdev` / `egl: failed to create dri2 screen`。纯 GTK（Cairo，CPU）画得出来，WebKit 的合成路径不行。

## Checklist

```sh
git checkout prototype/panel-form
cd prototype/panel-form/tauri
nix-shell --run 'cargo run'
```

首次要重新编译（`target/` 与 `.cargo-home/` 已被清掉）。`tauri/shell.nix` 把 `CARGO_HOME` 与 XDG 目录指到工作区内，是为了绕开 agent 沙箱的只读限制；在宿主 shell 里 `~/.cargo` 可写，可以不管这些 export。

依次观察并记录：

1. 面板是否出现在屏幕**左上**（锚定、浮层、不抢焦点——切到终端继续打字，看输入是否仍进终端）。
2. 若仍无像素，试 `LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe`。
3. 再试**去掉** `WEBKIT_DISABLE_COMPOSITING_MODE=1`（这个环境变量在部分 WebKitGTK 版本上会导致空渲染）。
4. 记录 `ls -l /dev/dri` 与 MESA 的输出，以及 `niri msg --json layers` 是否仍列出 `plainly-panel`。

## Answer 要给出的事实

一条结论就够：**"一个 Tauri 进程内做 layer-shell 面板"是可行的，还是必须退回独立面板进程**（C 原型已证明那条路可用，见分支上的 `panel.c`；截图按隐私决定未保留到仓库里）。

若宿主机上仍画不出，请记下排查到的最后一层原因（是 WebKit 与 layer 表面的兼容问题，还是本机 GL 栈的问题）——这决定票据 07 与实现规格里面板该怎么写。
