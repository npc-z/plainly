# 13: 面板最小闭环

**What to build:** 我按一次键，剪贴板里的那段难英语在不抢焦点、不打断我打字的面板里变成解释；按 ✕、或它在 5 秒内没画出来，它都会消失。

**Blocked by:** 12

**Spec:** [spec.md](../spec.md) §12 面板（进程形态、触发、取词、退出路径）

**Status:** ready-for-agent

- [ ] `plainly-panel` 是独立 GTK 进程（GtkApplication，id `dev.plainly.panel`），以 layer-shell overlay 呈现，`keyboard_mode=NONE`：不抢键盘焦点、不进合成器的窗口列表
- [ ] 合成器键绑直接 spawn 面板（`spawn "plainly-panel" "--clipboard"`），桌面应用不必常驻；最小 niri 片段随交付
- [ ] 连按两次只出现一个面板：第二次 `activate` 复用已有窗口并**重读剪贴板刷新**
- [ ] 取词走 data-control（无焦点 layer 表面上 GTK 剪贴板 API 不工作）；没有该协议时退化为会取得焦点的普通窗口，此时 GTK 剪贴板可用
- [ ] 面板调 core 产出 Explanation 并按五节呈现，原文常显
- [ ] ✕ 可关，退出走 `g_application_quit()`（不是 `gtk_main_quit()`）
- [ ] **5 秒渲染看门狗**：没有成功绘制就自己退出（正常路径永不触发）
- [ ] 加载态里原文先可见；出错态给出人话原因与「剪贴板内容未变、什么都没存」
