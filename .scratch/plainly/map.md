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
- [02 面板形态原型](issues/02-panel-form-prototype.md)：面板 = **一个 Tauri 进程内的 layer-shell overlay**（`visible(false)` → 在 map 之前 `init_layer_shell` → `show`），reparenting hack 与第二个进程都不需要；独立 GTK 面板已验证可用，作为退路保留。WebKit 的内容渲染待宿主机复验（[12](issues/12-tauri-panel-render-check-host.md)）。
- [13 开发环境（nix flake）的范围与内容](issues/13-nix-flake-dev-environment.md)：`flake-utils` + `fenix` 的 flake（锁 `nixos-unstable`），v0 只列 `x86_64-linux`；环境含 `gtk-layer-shell` + `webkitgtk_4_1` + **`GDK_BACKEND=wayland`（与 readest 相反）**；不抄 readest 的 `nixConfig`/Cachix 块；sqlite 走 `rusqlite` 的 `bundled`，系统无需安装；**CI 推迟到实现阶段**；图标提交占位图。
- [03 默认本地模型质量门槛](issues/03-local-model-quality-bar.md)：手上那个 4B（Qwen3-4B-Instruct-2507 Q4_K_M，`--ctx-size 16384`，RTX 3060）**12/12 契约通过**、平均 **1.87 秒/段**（热态）、冷启动 7.2 秒——**本地模式可用，我早先"30–60 秒"的判断作废**。但内容有硬错：`bottled it` 讲反、`grammar` 0/12、语域与反讽出错。**本地 = 够用但不可无条件信任**。同时补上了票据 01 的空白（schema 首次跑通 provider 路径）。
- [14 第一版系统提示词的标定](issues/14-prompt-calibration.md)：出厂提示词 = **v6-synthesis**（v2 的 grammar 措辞 + v5 的准确规则 + "必须是真改写"）；**`grammar` 字段保留**（云端 10–11/12 触发，本地 0/12 是能力边界）；**默认 provider = 云端 + 关闭思考**（1.3 秒、15× 少 token，详尽度降但正确性不降），**本地为离线备选**；`expression` 子串校验做。**本地那些错是能力问题不是提示词问题**——同一批提示词云端全对。分步式 v3 剔除（本地照抄原文 5/12，云端字段名漂移 12/12 违约）。

## Not yet specified

<!-- 在范围内、但还看不清的问题；前沿推进后毕业成票据。不要在这里预切片。 -->

- 超长输入的切分**阈值**：切在段落边界已定（01），**本地上下文预算也已定**（llama.cpp 服务 `--ctx-size 16384`，03），云端仍待 04。剩下的是经验规则——"超过多少 token 就切"，而且现在**可测**（harness 现成，1.9 秒/段）。
- 失败与降级 UX：**失败分类已由票据 10 查清**（OpenAI refusal、LM Studio 的 `reasoning_content` 空洞、DeepSeek 的 JSON mode 漂移、超时/限流）；剩下的是它们在面板里各自怎么呈现。
- 首次运行引导：provider、key、模型下载。
- 搜索与标签的具体形态（等 08）。
- 应用自身 UI 的语言与本地化策略：01 已把 markdown 与 UI 的标题拆开（markdown 用固定英文，UI 标题可本地化），**UI 本身中/英/双语仍待定**。
- 剪贴板里的敏感内容（密码管理器 hint）如何处理。
- 思考开关的归属：**写死关闭**（票据 14 的默认就是它），还是作为"详尽档 / 快速档"暴露给用户？答案大概取决于票据 06 怎样呈现设置页——所以现在只是可疑问题，还不是票据。

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
