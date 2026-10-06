# Plainly v0 规格

Labels: wayfinder:map

## Destination

地图上不再有待决问题——v0 的每条决策都有答案，可以直接交给 `/to-spec` 坍缩成可施工的规格：Linux 一等公民（niri/Wayland 优先，GNOME 有据可查的降级路径）、core 保持跨平台的桌面应用。用户按快捷键，应用解释剪贴板里的难英语，产出**分级英语讲解（改写 + 习语 gloss + 语法）+ 母语翻译**，历史存本地 SQLite 并可导出。

本 effort **只产出决策，不做实现，也不自己写规格**——规格由 `/to-spec` 从本图的决策坍缩而来。

**➡️ 2026-10-05：已交给 `/to-spec`——规格落在 [spec.md](spec.md)（`Status: ready-for-agent`）。**
**➡️ 2026-10-05：已由 `/to-tickets` 拆成 17 张实现票据，在 [tickets/](tickets/)（01–17，编号即依赖顺序）。** 前沿：02（01 已完成，见 [tickets/01](tickets/01-workspace-config-secrets.md)）。下一步 `/implement`。

**✅ 2026-10-05：目的地已到达。** 18 张票据全部结清（其中 [11](issues/11-v0-spec-assembly.md) 明确出界），**Not yet specified 为空**。本图可以整体交给 `/to-spec`。后续：`/to-spec` → `/to-tickets` → `/implement`。

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
- [09 提示词参数化与版本迁移](issues/09-prompt-parameterization.md)：**`prompt_version` = 有效提示词的内容哈希（派生，不是人声明的）**——票据 08 那条"内容改了必须升版本"由**纪律**变成**结构保证**，不可能再被违反；哈希覆盖 `出厂默认 + 用户附录 + 等级描述表`，**占位符不替换**（等级/母语已是 key 的独立分量）。**可调项只有两个结构化参数**：等级与母语；**等级必须连含义一起发**（实测：只发标签时 3/8 相邻等级对产出**逐字相同**的文本，内联描述后降到 1/8，A2 平均词长 4.29→4.05、与原文相似度 0.460→0.395；**B2 与 C1 仍无法区分**，机制是 C1 的描述"keep most of the original structure"与契约自己的"never return the passage unchanged"相冲突，后者赢）；**母语是强参数**（日语母语下四段翻译全切日语，而 gloss 与改写**全部保持英文**，4/4）。**输出小节开关是显示层开关**（不改提示词、不改 schema → 不进 `lookup_key`）。**合并 = 出厂默认（不可变、随版本更新）+ 用户附录（append-only）**：上游改进能到达用户，且**只能加不能删**（对契约而言增加比删除安全）。**迁移 = 旧记录原样渲染，重跑只能显式**（改 Level 不触碰旧记录）。**兜底绝不静默回退**。连带：出厂 v6 要变成内联等级描述表的 **v7**。
- [17 超长输入：切分门槛与多块记录的形态](issues/17-long-input-threshold.md)：**约束在输出侧，不在输入侧**（实测：输出 token ≈ **6 × 输入词数**、耗时 ≈ **0.03 秒/词**、gloss ≈ **每 10 词 1 条**；190 词 = 1143 token / 5.7 秒，而输入仅约 260 token，离本地 16384 的预算很远）。**阈值 150 词**（≈900 token / 4.5 秒）——**根据是"能忍几秒"而不是"能不能放下"**；段落优先，**单段超限降到句子**。**一块一条 Record**（幂等键按块生效，只重生成变化的那块）。面板**堆叠滚动**呈现多块（键盘模式 NONE，翻页只能靠指针；堆叠无隐藏状态）。**面板硬上限 5 块**，超过则拒绝并指向 CLI，**CLI 不继承该上限**（它是长文本的正路）。**`max_tokens` 必须 ≥ 约 1200，与阈值成对。**
- [18 剪贴板里的敏感内容](issues/18-sensitive-clipboard.md)：**判据实测量化**——类型列表含 `x-kde-passwordManagerHint` **且**取值为 `secret`（两步、纯本地、零网络）。判定后**不发送、不入库、面板明说，且不给"坚持解释"的出口**（代价不对称：误发难挽回，少解释几乎无损；且"不入库"是票据 08"原文总是入库"的**有意例外，规格要点明**）。**不做启发式兜底**——假阳性 + 假阴性 + 最糟的"有防护"错觉；**记为刻意不做**。**本地 provider 不放宽**（规则与 provider 无关）。**检查在 core 一处兜住**（CLI 与复制自动弹都经手剪贴板）。**局限写明**：不遵守该约定的密码管理器不会被保护。
- [16 主窗口（历史、搜索、设置）的信息架构](issues/16-main-window-ia.md)：**A 的分栏骨架 + 借 C 的日期分组**——顶部标签（历史/设置）+ 列表常驻（左约 330px，含搜索框）+ 详情在右；桌面宽度下**单栏是浪费**，而分栏天然容纳**同一段落多版本并排比较**。搜索**只搜英文**，并在搜索框下**明说"中文翻译暂不可搜"**（08 要求写成事实）；标签在详情编辑、在列表作筛选 chip；**列表行必须显示 prompt 版本**（否则多版本会被当成重复条目）。导出**两处分工**：详情内"复制 markdown / 导出这一条"，全局三条在**设置的数据节**。设置**五节**（语言与等级 / Provider / 提示词 / 行为 / 数据），与 15 的配置分段对齐，本票据只管放置。**首次运行 = 设置页的空状态**（不做独立引导流）。**面板位置可配置**（四角 + 跟随当前显示器；**不做自由拖动**——成本在多显示器重锚定、分辨率无关性与复位，角设置以约 10% 成本拿约 90% 收益）。**顺带修正 06**：「点外侧关闭」在 Wayland 上不成立（客户端看不到落在别处的点击），已去掉，并加**渲染看门狗**。原型 `.scratch/plainly/ui/main-window-ia.html`。

## Not yet specified

<!-- 在范围内、但还看不清的问题；前沿推进后毕业成票据。不要在这里预切片。 -->

<!-- 当前为空：地图上所有开放问题都已具体到可以施工。只剩 16 / 17 / 18 三张开放票据。 -->

<!-- 已毕业/已消化（离开 fog 的去处）：
     · 失败与降级 UX —— 分类与重试策略归 04/10，面板里的**形态**归 06；剩下的是每类失败的具体文案，属规格层面，不是决策。
     · 首次运行引导、搜索与标签的具体形态 —— 毕业成 [16](issues/16-main-window-ia.md)。
     · 应用 UI 的语言与本地化 —— 由 06 结清（中文为主 + 文案集中为可替换资源，不引 i18n 框架）。
     · 超长输入的切分阈值 —— 毕业成 [17](issues/17-long-input-threshold.md)。实测把重心从输入侧挪到输出侧：输出 token ≈ 6 × 输入词数，190 词已 1143 token / 5.7 秒，而输入只用掉约 260 token。
     · 剪贴板里的敏感内容 —— 毕业成 [18](issues/18-sensitive-clipboard.md)。判据已实测量化（`x-kde-passwordManagerHint` 取值为 `secret`）。 -->



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
