# 17: 收尾与验收

**What to build:** 一套别人能照着验的验收材料——niri 上的手测清单、GNOME 的降级行为、X11 的明确不支持、CI，以及新贡献者的上手说明。

**Blocked by:** 15, 16

**Spec:** [spec.md](../spec.md) §1 范围与平台承诺、§14 开发环境、Further Notes 的诚实记录

**Status:** ready-for-human (2026-10-10) — 只剩两项需要别人的机器：GNOME 降级、主窗口

- [x] niri 手测清单可执行且写下来：键绑 spawn 面板、焦点不变、Esc 无效、✕ 可关、看门狗自退、多块堆叠、两种拒绝态、位置设置生效
      （**清单写全了**；本机实跑的只有焦点不变、不进窗口列表、结果态、自动消失、看门狗正常路径——其余每条都在 §6 的「这次没走到的」里点名）
- [ ] GNOME 上验证降级为取得焦点的普通窗口且取词可用；X11 会话下面板明确报不支持（CLI 不受影响）
      （**后一半已实现并实测**：面板与 `--clipboard` 在 X11 上退出 1 并给出那句话，CLI 的管道 / 文件不受影响。
      **前一半没有本地证据**：本机是 niri，`docs/manual-checks.md` §4.1–4.5 标着「本机未验」，需要一台 GNOME 机器）
- [x] CI 跑 `-p plainly-core` 与 `-p plainly-cli` 的构建与测试（不整 workspace、不需要图形依赖；`scripts/check-headless-cli.sh` 仍然按规格 §14 只构建 `-p plainly-cli`）
- [x] README 给出最小键绑片段（`spawn` 不走 shell、不展开 `~`、`hotkey-overlay-title`）与首次配置步骤
- [x] 诚实记录进入面向用户的文档：等级是提示不是契约（B2 与 C1 实测分不开）、本地模型会自信地错、中文翻译不可搜、面板默认只有 ✕ 一条出路、敏感判定覆盖面有限
- [x] 全库跑一遍 `plainly` 的端到端冒烟（CLI + 面板 + 主窗口），并记录已知未做的部分（规格的 Out of Scope）
      （**CLI 与面板跑了**，见 §6；**主窗口没有**——它要 Tauri 的 webview，`docs/desktop-manual-checks.md` 那份清单整份未走，§6 也点名了）

## 落地形态

| 文件 | 它是什么 |
| --- | --- |
| `README.md` | 面向使用者与新贡献者：构建、首次配置（key / 本地 runtime / 验证 / 键绑）、诚实记录、验收材料在哪、四个 crate 各是什么 |
| `docs/manual-checks.md` | **面板、会话降级与 CLI** 的真机清单：准备、§1 niri 面板（焦点 / 窗口列表 / Esc / ✕ / 看门狗 / 多块 / 两种拒绝 / 位置 / 自动消失）、§2 键绑与常驻、§3 端到端（CLI + 面板 + 主窗口，用仓库自带的假 provider）、§4 GNOME 降级与 X11 不支持、§5 已知没走到、§6 2026-10-10 实际走查记录 |
| `docs/desktop-manual-checks.md` | 原有那份：主窗口与「复制自动弹」；本票据把已经过期的「这些设置面板还没读」那条删掉，并在行为一节指向面板自己的清单 |
| `scripts/stub-provider.py` | 一个假的 OpenAI 兼容端点（loopback、无 key、每次请求追加一行到 `scripts/stub-provider.requests.log`），让面板与主窗口在没有真 provider 的机器上也能走完流水线；日志就是缓存那一项的判据。**端口是一个参数，默认 8080 只是 llama.cpp 的上游默认，不是本仓库的约定**：真实 runtime 常起在别的端口（本项目 bench 用 3060），一个端口只能有一个属主，所以清单 §3 让人先 `providers discover` 看清哪个地址答了、再挑一个空的；占着别人的端口时 stub 报错退出并指回 `discover`，不静默假装在服务 |
| `.github/workflows/ci.yml` | 一个 job：`cargo test -p plainly-core`、`cargo test -p plainly-cli`、`scripts/check-headless-cli.sh`、`cargo clippy -p plainly-core -p plainly-cli --all-targets`。**不跑 workspace**（会因为缺 GTK 失败）、**不装 Nix**（flake 是给 GUI crate 的） |
| `scripts/check-headless-cli.sh` | 原来用 `node -e` 读 cargo 的 JSON。改成 `python3 -c`：CI 的 ubuntu runner 基础镜像有 python3，不保证有 node（这个 devShell 里两者都有，所以这个缺口只在 CI 才露）——脚本是 CI 那一步跑的，所以按 CI 的事实选工具 |
| `crates/plainly-core/src/display.rs` | `DisplaySession::{Wayland, X11, None}` + `SessionRefusal`：从环境变量判会话（`WAYLAND_DISPLAY` 优先，其次 `DISPLAY`），环境作为参数传入所以可测 |
| `crates/plainly-cli/src/clipboard.rs` | `read()` 先判会话：被拒的会话直接返回 `Error::Refused(SessionRefusal)`，不再去问 `wl-paste`；另外 `wl-paste` 自己说「连不上 Wayland / 没有 data-control」不再被误报成「剪贴板里没有文本」 |
| `crates/plainly-panel/src/capability.rs` | `presentation()` 连 Wayland 之前先判会话，X11 直接给出那句明文拒绝 |
| `flake.nix` | 两处：`checks` 的注释改掉「CI 推迟到 v0 之后」，指向 `.github/workflows/ci.yml`；`devTools` 补 `python3`——`scripts/check-headless-cli.sh` 与 `scripts/stub-provider.py` 都按名字要它，而清单要求这些命令在 `nix develop` 里跑（按 AGENTS.md，缺程序就补 flake，不能靠宿主 PATH 恰好有） |
| `AGENTS.md` | Environment 一节原本写「CI runs the same shell」，与本票据落地的 CI 不符；改成「flake 是开发环境，CI 是一个窄例外：对 `-p plainly-core` / `-p plainly-cli`（两个不需要图形依赖的 crate）用普通 toolchain，GUI crate 归手测清单」。**这是这次唯一动到仓库级 agent 说明的地方**，改它是为了让标准与实现一致（另一条路是在 CI 里装 Nix，规格 §14 的 `-p plainly-cli` 说明它不需要）。README 与 `flake.nix` 里描述 CI 的那两句也跟着改成两个 crate |

**X11 的「明确不支持」是这次补的代码，不只写在文档里。** 票据 13/15 交接时说「X11 报错归 17」。
实测那条路径此前是：面板连不上 Wayland → 一行英文的库错误；`--clipboard` → `wl-paste` 抱怨
`XDG_RUNTIME_DIR`。两者都非零退出（不是静默失败），但都**没有说出「X11 不支持」这件事**，
而人下一步该做什么（换 Wayland，或用管道/文件）正需要那句话。现在两条路径都在**尝试连接之前**
给出 `DisplaySession::refusal(…)` 产出的 `SessionRefusal`（「没有显示会话」也走这条，句子不同），CLI 的其余部分完全不受影响——那也是规格要求写明
的：X11 上 CLI 照常可用。

**会话判定为什么只看 `WAYLAND_DISPLAY` / `DISPLAY`：** `XDG_SESSION_TYPE` 是**登录会话**的名字，
而客户端连的是 socket；从 X11 登录起一个 Wayland 会话时两者不一致，名字说 wayland、socket 是空的。
所以有 `WAYLAND_DISPLAY` 就是 Wayland（哪怕 `DISPLAY` 也在），只有 `DISPLAY` 才是 X11，
两个都没有是「没有会话」——那不是排除，是另一回事（CLI 在容器 / cron 里要照常工作）。

## 验证

- `cargo test --workspace`：**543 通过，0 失败，1 ignored（live keyring，需要真会话）**，34 个测试目标；
  CI 跑的是其中不需要图形依赖的那半（`-p plainly-core` 293 + `-p plainly-cli` 126 = 419），
  新增的是 `crates/plainly-core/tests/display.rs`（7 条：Wayland 优先于 DISPLAY、只有 DISPLAY 是 X11、
  都没有是 None、空值不算、Wayland 不被拒、X11 那句拒绝文案、无会话是另一句话）与 `crates/plainly-cli/tests/clipboard.rs`
  的三条（X11 明文拒绝；管道在 X11 会话上照常解释；无显示会话也有自己的句子）。`cargo clippy --workspace --all-targets` 零警告。
- `scripts/check-headless-cli.sh`：依赖图里没有 gtk/webkit/gdk/wayland/soup/glib，构建出来的 CLI 的
  `ldd` 只有 libgcc/libm/libc；`plainly --version` 正常。
- **真机走查（本机 niri 26.04 + 仓库自带的假 provider，临时 `HOME`/`XDG_*`）**，逐条记在
  `docs/manual-checks.md` §6：CLI 按原型五节形状输出（假 provider 的 `grammar` 为 null，所以这次是四节）与 stdout/stderr 分流；第二次同一段命中缓存
  （假 provider 的请求日志一行没长）；面板从剪贴板走完 core 流水线并到结果态；焦点始终是原来那个窗口；
  `niri msg windows` 始终 5 个（面板是 `Overlay` layer，`keyboard_interactivity=None`——这同时是
  `keyboard_mode=NONE` 的机器证据）；标题栏五项与本地提示都在截图上；`panel_autohide` 8 秒自退退出码 0；
  X11 下面板与 `--clipboard` 各退出 1 并给出那句话；无会话时两边也都退出 1 且有话说。
- **没有本地证据、明确记下来的**：GNOME 整节（本机是 niri）、键绑 spawn 与快捷键浮层、Esc 用真键盘按一次、
  多块堆叠与两种拒绝态、复制 / ⟳ / ⤢ 的互动、看门狗的病态反向复现、主窗口与 `--watch`
  （后者在 `docs/desktop-manual-checks.md`）。这些写在 §5 与 §6 的「这次没走到的」，没有假装走过。

## 已知取舍

- **CI 跑 core 与 cli 两个 crate，两个 GUI crate 一行 CI 都没有**：规格 §2 要求 CLI 能单独构建、§14 要求 CI 用
  `-p plainly-cli`；`plainly-core` 是规格的**主缝**（缝 A），而它与 CLI 同属「不需要任何图形依赖」的那一半，
  所以两个都跑（`cargo test -p plainly-cli` 只跑 CLI 自己的 target，core 的测试不会跟着跑）。
  代价是**面板与主窗口**的回归只能由手测清单发现——所以清单写得比「点一遍看看」细。
- **CI 不装 Nix**：flake 是给 GUI crate 的；为了跑一个不需要任何系统库的 crate 而先装 Nix，是证明一件便宜的
  事的昂贵方式。代价是 CI 用的 stable 与 flake 钉的 toolchain 可能不同（`rust-version = "1.85"` 是地板）。
- **CI 不做 `cargo fmt --check`**：这份代码在 flake 钉的 rustfmt 与 runner 的 rustfmt 下输出不同
  （实测 `crates/plainly-cli/src/commands/history.rs` 一行），格式门会挂在没人碰过的行上。格式暂时归 review。
- **被拒的两种会话各说各的话**：X11 是 v0 的排除，句子说「不支持 X11、换 Wayland、CLI 照常」；
  「一个显示会话都没有」不是排除，句子只说 `no Wayland session: … needs one`。
  两者都在**尝试连接或问 `wl-paste` 之前**给出（`DisplaySession::refusal`），所以人得到的不是一句 socket 抱怨。
- **`is_session_failure` 认的是 `wl-clipboard` 的措辞**（wayland / compositor / XDG_RUNTIME_DIR /
  data-control）：措辞变了只会让「连不上会话」退回成「没有文本」这个次优说法，不会给错答案——
  两种情况都是读取失败、都不发请求。
- **假 provider 是脚本不是测试**：它不进 CI（CI 里没有面板可跑），也不带断言；它的价值是让手测清单里的
  端到端那几项在**没有 key 的机器**上也能做，并留下请求日志当缓存判据。
- **清单里的命令都在 `nix develop` 里跑**：v0 没有发布物，面板要 dev shell 的 `LD_LIBRARY_PATH`；
  键绑 spawn 的进程继承的是 niri 的环境，所以那一项在 v0 需要一个包装脚本（留给打包，不在 v0）。
