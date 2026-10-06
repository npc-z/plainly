# 01: 工作区、开发环境、配置与密钥

**What to build:** 一个能构建、能跑、能记住我的设置的项目骨架——`nix develop` 进入锁定的开发环境，workspace 分成 core / CLI / 面板 / 主窗口四个 crate，其中 CLI 单独构建时不带任何图形依赖；`plainly config path` 告诉我配置文件在哪、`config set` 改得动它，而 API key 从不落进磁盘（环境变量 > keyring > 仅本次会话内存）。

**Blocked by:** None (can start immediately)

**Spec:** [spec.md](../spec.md) §2 工作区与 crate 边界、§8 配置与密钥、§14 开发环境

**Status:** ready-for-agent

- [ ] `nix develop` 给出锁定环境（fenix 工具链、gtk3、gtk-layer-shell、webkitgtk_4_1、C 编译器），`GDK_BACKEND=wayland` 已设；`.envrc` 可用
- [ ] `cargo build -p plainly-cli` 在一个没有 GTK / webkit 的环境里也能通过（不是空话，能验证）
- [ ] 新克隆第一次构建不会失败（占位图标已提交进仓库）
- [ ] 配置是 TOML、三段（`[app]` / `[providers.*]` / `[prompts]`），位置由 config / data / cache 三个目录决定，identifier 为 `dev.plainly.app`
- [ ] `plainly config path|show|set` 与 `plainly providers key set|clear` 可用
- [ ] 配置写入是原子的（临时文件 + rename），外部改动（mtime 变化）被检测到而不是被覆盖
- [ ] 密钥按 环境变量 > OS keyring > 仅本次会话内存 解析，任何路径都不写明文文件；keyring 不可用时明确告知只在本次会话有效
- [ ] core 不依赖 GTK / webkit / Wayland（平台能力以数据注入），可在无图形环境构建与测试
