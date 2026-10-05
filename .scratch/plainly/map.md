# Plainly v0 规格

Labels: wayfinder:map

## Destination

地图上不再有待决问题——v0 的每条决策都有答案，可以直接交给 `/to-spec` 坍缩成可施工的规格：Linux 一等公民（niri/Wayland 优先，GNOME 有据可查的降级路径）、core 保持跨平台的桌面应用。用户按快捷键，应用解释剪贴板里的难英语，产出**分级英语讲解（改写 + 习语 gloss + 语法）+ 母语翻译**，历史存本地 SQLite 并可导出。

本 effort **只产出决策，不做实现，也不自己写规格**——规格由 `/to-spec` 从本图的决策坍缩而来。

## Notes

- 缘起：`idea.md`——把 `comprehensible-english` 那份提示词契约产品化。
- **开工前先读**：
  - `research/local-models-and-incumbents.md`——本地模型质量/硬件门槛、竞品全景、楔子证据。
  - `research/os-integration-feasibility.md`——三平台 OS 集成可行性（第 5 节为 niri 追加调研）。
  - `research/structured-output-support.md`——各 provider 能否强制 schema，以及那些会咬人的坑。
  - 仓库根的 `GLOSSARY.md`（领域词表——写票据时用规范词，别引入同义词）与 `docs/adr/`（已有一条：解释 schema 为何不写长度/模式约束）。
  - 仓库根的 `AGENTS.md` 与 `docs/agents/*.md`。
- **本 effort 的既定偏好（已定，不要再重新讨论）**：
  - 只做规划，规格是终点。
  - **v0 = Linux 一等公民 + core 跨平台**；macOS / Windows 是 v1。
  - **交互无法跨平台统一**——能力在**运行期探测**，不写死进文档。
  - **本地模型不是卖点**，只是 provider 之一。
  - **不做 SRS 复习循环**，导出即可。
  - 契约靠 **schema 约束解码**，不靠提示词祈祷。
  - 触发器：**快捷键是通用核心**，"复制自动弹"是运行期探测的可选增强（默认关）。
- 常用技能：`grilling`、`domain-modeling`、`prototype`、`research`、`codebase-design`。
- 一次 session 只解决一张票据（research 票据可并行）。认领 = 把 `Status:` 改成 `claimed`，改之前先改。

## Decisions so far

<!-- 关掉的票据一行一条：名字 + 一句话结论。细节留在票据里，这里只做索引。 -->

- [10 结构化输出在各 provider 的支持面](issues/10-structured-output-support.md)：能真正强制 schema 的是 OpenAI / llama.cpp / LM Studio / Ollama（≥0.31.2，thinking 开，非 cloud）；DeepSeek 只能提示词+校验+重试。硬约束：禁矛盾上下界、优先 `maxItems`-only、OpenAI strict 下无长度/模式约束。
- [01 输出契约 schema 与 markdown 渲染](issues/01-output-contract.md)：一个 Explanation 对应一个 Passage；模型只产 `comprehensible` / `glosses[]` / `grammar|null` / `translation`，原文与元数据由应用附加；markdown 逐字复刻原型五节；版本号为 `artifact_version` + `prompt_version`。
- [02 面板形态原型](issues/02-panel-form-prototype.md)：**⚠️ 已更正**——面板 = **独立的面板进程**（独立 GTK + gtk-layer-shell）。原结论"一个 Tauri 进程内的 layer-shell overlay 就够"**被票据 12 推翻**：Tauri 能**创建** layer 表面，却**画不出任何内容**（连直接画在窗口上的纯色背景都不上屏）。Tauri reparenting hack 确实不需要，但那是无关的好消息。
- [13 开发环境（nix flake）的范围与内容](issues/13-nix-flake-dev-environment.md)：`flake-utils` + `fenix` 的 flake（锁 `nixos-unstable`），v0 只列 `x86_64-linux`；环境含 `gtk-layer-shell` + `webkitgtk_4_1` + **`GDK_BACKEND=wayland`（与 readest 相反）**；不抄 readest 的 `nixConfig`/Cachix 块；sqlite 走 `rusqlite` 的 `bundled`，系统无需安装；**CI 推迟到实现阶段**；图标提交占位图。
- [03 默认本地模型质量门槛](issues/03-local-model-quality-bar.md)：手上那个 4B（Qwen3-4B-Instruct-2507 Q4_K_M，`--ctx-size 16384`，RTX 3060）**12/12 契约通过**、平均 **1.87 秒/段**（热态）、冷启动 7.2 秒——**本地模式可用，我早先"30–60 秒"的判断作废**。但内容有硬错：`bottled it` 讲反、`grammar` 0/12、语域与反讽出错。**本地 = 够用但不可无条件信任**。同时补上了票据 01 的空白（schema 首次跑通 provider 路径）。
- [14 第一版系统提示词的标定](issues/14-prompt-calibration.md)：出厂提示词 = **v6-synthesis**（v2 的 grammar 措辞 + v5 的准确规则 + "必须是真改写"）；**`grammar` 字段保留**（云端 10–11/12 触发，本地 0/12 是能力边界）；**默认 provider = 云端 + 关闭思考**（1.3 秒、15× 少 token，详尽度降但正确性不降），**本地为离线备选**；`expression` 子串校验做。**本地那些错是能力问题不是提示词问题**——同一批提示词云端全对。分步式 v3 剔除（本地照抄原文 5/12，云端字段名漂移 12/12 违约）。
- [04 Provider 抽象与运行时探测](issues/04-provider-abstraction.md)：**预设 + 自定义**配置面（必须能选模型）；**能力靠试一次探针**（DeepSeek `json_schema` → 400，llama.cpp → 200）并缓存，拒绝会腐烂的静态表；云端默认 `deepseek-flash` + 关思考，本地**探测常见端口**而非猜；错误分四类（畸形重试 / **同一错误重复即停** / 4xx 降级并重探 / 429-5xx 退避）；**thinking 建模为 `on|off|unsupported` 能力**（默认 off）；**v0 不做流式**（与"必须等完整 JSON 才校验"天生冲突，而等待只有 1.3–1.7 秒）。
- [15 配置与密钥的存放](issues/15-config-and-secrets-store.md)：**配置走 TOML 文件、记录走 SQLite**（config 目录 vs data 目录，Tauri 的 `appConfigDir`/`appDataDir`）；**能力探测缓存单独放 `appCacheDir`**（可重算，不与人的选择竞争）；**密钥三级：环境变量 > keyring > 仅会话内存，绝不落明文**；一个文件分 `[app]` / `[providers.*]` / `[prompts]` 三段；identifier 定为 `dev.plainly.app`；项目级覆盖 v0 不做。**边界：存储归 15 / 记录归 08 / 提示词内容归 09**——等级与母语的值存在 `[app]`，但每条 Record 盖上"当时用的值"。
- [08 历史库 schema 与导出](issues/08-history-schema-export.md)：**关系化**（`records` + `glosses(record_id, ord, …)`）；**Anki 每个 gloss 一张卡**、走 CSV 而非 `.apkg`；Markdown 单文件分节；**FTS5 只索引英文**（tokenizer 是 per-table，中文检索 v0 不支持，且刻意不写成缺陷）；删除=硬删、不加密、标签只做手动、原文必须总入库。**核心：历史库兼任缓存**——`lookup_key = SHA-256(passage 归一化 + level + 源/母语 + prompt_version + provider + model + thinking)` 上建 UNIQUE，命中**复用同一行**并更新 `last_seen`（不记 `lookup_count`，因为不做 SRS）；key **含 provider profile**（否则切模型后会被旧的坏答案命中）且**不含 `artifact_version`**；"重新生成"原地覆盖。
- [05 CLI 命令面与守护进程触发](issues/05-cli-surface.md)：**两个二进制**——瘦 `plainly`（CLI，只依赖 core）与 `plainly-desktop`（GUI）；**IPC 整条删掉**，因为没有场景需要 CLI 触发面板（键绑直接指向桌面二进制，它自带 single-instance 的 D-Bus 交接，名字 `<identifier>.SingleInstance`）。四个顶层子命令 `explain`（无子命令时默认）/ `history` / `providers` / `config`；**stdout 只出产物、stderr 出人类信息**；`--format markdown\|json`、`--regenerate`；退出码 0/1/2/3；**CLI crate 不得依赖 GTK/webkit**。
- [12 Tauri 面板的内容渲染验证（宿主机）](issues/12-tauri-panel-render-check-host.md)：**不可以**。宿主机上（GPU/EGL/命名空间全部正常、窗口未 realize、控件全部分配、合成器也列出了表面）**一个像素都不上屏**——连**绕过 WebKit、直接画在窗口上的纯色背景**都不上屏（全屏颜色直方图匹配 0 像素）。对照组独立 GTK C 原型照常渲染。**结论：Tauri 能创建 layer 表面，画不出内容** → 面板必须是独立进程（更正 02），二进制集合可能变三个（影响 05），解锁 07 且问题变为"谁 spawn 面板"。
- [07 Linux 触发集成与能力探测](issues/07-linux-trigger-integration.md)：**合成器键绑直接 spawn `plainly-panel`**（GtkApplication，id `dev.plainly.panel`）——桌面应用**不必常驻**；`GtkApplication` 白送单实例（实测连按两次只出一个面板，`activate` 里复用已有窗口）。**取词必须走 data-control**：实测无焦点 layer 表面上 `wl-paste` 拿到值而 `gtk_clipboard` **直接失败**。能力探测查 `ext_data_control_manager_v1`/`zwlr_data_control_manager_v1`，**降级路径自洽**（GNOME 既无 data-control 也无 wlr-layer-shell → 面板退化为取焦点的普通窗口，此时 GTK 剪贴板可用）。"复制自动弹"不具备能力时**灰掉+说明**（不是隐藏）。**X11 不在 v0 范围**（明确写进规格，不留白）。niri 的 `spawn` 修饰键 bug 天然规避（键盘模式 NONE）。**二进制三个**：`plainly` / `plainly-panel` / `plainly-desktop`。
- [06 桌面 UI 信息架构](issues/06-desktop-ui-ia.md)：**面板 = 固定标题栏（provenance chip + 复制 / ⟳ 重新生成 / ⤢ 历史 / ✕）+ A 的正文密度**（原文常显 → 改写 → gloss 紧凑列表 → 翻译；分节可折叠，`grammar` 为空时显示"本段不需要"而非消失）；**本地 provider 时正文下方固定"请对照原文 / 换云端重生成"提示**——把票据 14 的结论与 08 的核对要求落到界面固定位置。变体 C（渐进出卡，原文与翻译默认隐藏）**因与"原文可核对"冲突而不采纳**，只作代价证明留在原型里。退出路径：**自动消失默认关**（打开后默认 20 秒；开关与时长是两个独立键，**不用 `0` 表示"永不"**）+ 点外侧关闭 + ✕，且**任何状态（含加载/出错）都有出路**（`keyboard_mode=NONE` 下 Esc 无效——所以默认关时**指针是唯一出路**）。`activate` 复用 = **重读剪贴板并刷新**；思考开关**只在设置里、默认关**；**UI 中文为主 + 文案集中为可替换资源（不引 i18n 框架）**。原型 `.scratch/plainly/ui/panel-ia.html`。**主窗口那一半拆成 16。**

## Not yet specified

<!-- 在范围内、但还看不清的问题；前沿推进后毕业成票据。不要在这里预切片。 -->

- 超长输入的切分**阈值**：切在段落边界已定（01），**本地上下文预算也已定**（llama.cpp 服务 `--ctx-size 16384`，03）。剩下的是经验规则——"超过多少 token 就切"，而且现在**可测**（harness 现成，1.9 秒/段）。
- 剪贴板里的敏感内容（密码管理器 hint）如何处理。

<!-- 已毕业/已消化（离开 fog 的去处）：
     · 失败与降级 UX —— 分类与重试策略归 04/10，面板里的**形态**归 06；剩下的是每类失败的具体文案，属规格层面，不是决策。
     · 首次运行引导、搜索与标签的具体形态 —— 毕业成 [16](issues/16-main-window-ia.md)。
     · 应用 UI 的语言与本地化 —— 由 06 结清（中文为主 + 文案集中为可替换资源，不引 i18n 框架）。 -->


## Out of scope

<!-- 越过 v0 终点的工作。不在这里毕业；终点重画时才作为新 effort 回来。 -->

- **web api**——为"浏览器插件"一个场景引入鉴权、端口、CORS、守护进程生命周期整条战线。
- **MCP server**——v1 第一顺位候选，等 core API 稳定。
- **截图 OCR**——idea 未提，但与"复制取词"是天然一对，故显式记为不做。
- **多设备同步**。
- **SRS 复习循环**——让别人做复习，我们只做导出。
- **macOS / Windows 桌面支持**——v1；core 保持跨平台以便届时接入。
- **Tauri 发布链路**（公证、签名、自动更新）。
- **浏览器插件**。
- **汇总为 v0 实现规格**——与 `/to-spec` 是同一件事。目的地本身就是"交给 `/to-spec`"，地图再自己写一遍规格只会让两份文档分叉。见 [11 汇总为 v0 实现规格](issues/11-v0-spec-assembly.md)。
