# Plainly v0 规格

Labels: wayfinder:map

## Destination

一份可直接交给 `/to-spec` 的 **v0 实现规格**：Linux 一等公民（niri/Wayland 优先，GNOME 有据可查的降级路径）、core 保持跨平台的桌面应用。用户按快捷键，应用解释剪贴板里的难英语，产出**分级英语讲解（改写 + 习语 gloss + 语法）+ 母语翻译**，历史存本地 SQLite 并可导出。

本 effort **只产出决策与规格，不做实现**。

## Notes

- 缘起：`idea.md`——把 `comprehensible-english` 那份提示词契约产品化。
- **开工前先读**：
  - `research/local-models-and-incumbents.md`——本地模型质量/硬件门槛、竞品全景、楔子证据。
  - `research/os-integration-feasibility.md`——三平台 OS 集成可行性（第 5 节为 niri 追加调研）。
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

## Not yet specified

<!-- 在范围内、但还看不清的问题；前沿推进后毕业成票据。不要在这里预切片。 -->

- 流式输出与部分结果在面板里怎么呈现（等 01 定下契约才知道要渲染什么）。
- 超长段落的分块与截断策略（等 03、04 给出上下文与运行时探测的结论）。
- 失败与降级 UX：**失败分类已由票据 10 查清**（OpenAI refusal、LM Studio 的 `reasoning_content` 空洞、DeepSeek 的 JSON mode 漂移、超时/限流）；剩下的是它们在面板里各自怎么呈现。
- 首次运行引导：provider、key、模型下载。
- 等级（A1–C1+）由谁设置、记录到什么粒度（全局设置 / 每条记录）。
- 搜索与标签的具体形态（等 08）。
- 应用自身 UI 的语言与本地化策略。
- 剪贴板里的敏感内容（密码管理器 hint）如何处理。

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
