# 04 Provider 抽象与运行时探测

Type: grilling
Status: resolved

## Question

定下 provider 层的**接口与探测行为**。

- 一个 **OpenAI 兼容 adapter** 覆盖云端与本地：llama.cpp / Ollama / LM Studio / OpenAI / DeepSeek。
- 启动探测：base URL 可达性、`/v1/models`（或 `/api/version`）、模型是否加载、上下文长度是否够用。
- **BYO key 存 OS keyring**（本机 gnome-keyring portal 可用），不入 SQLite。
- 流式输出是硬需求（本地 CPU 推理一次 30–60 秒）。
- 错误分类：运行时缺失 / 端口冲突 / 超时 / 限流 / 模型偏离 schema——各自怎么向上汇报。

要决定的：用户如何配置（预设列表 vs 手工 base URL）、默认云端与默认本地各指向谁、探测失败时的降级文案。
输入：Q7 的决定、票据 10 的结论。

## Comments

- **来自票据 10 的调用面要求**：
  - **llama.cpp**：必须发**嵌套**的 `response_format: {type:"json_schema", json_schema:{schema:{…}}}`。扁平写法（它自己 README 记的那种）会被**静默忽略**并降级为"任意对象"，不报错。建议 sidecar 带 `--api-key` 跑在 loopback。
  - **LM Studio**：`{"type":"json_object"}` 会 400；读 `content`，为空时回退 `reasoning_content`（Qwen3.5 系列已知命中）。
  - **OpenAI**：解析前先查 `message.refusal` / `content[].type == "refusal"`，并处理 `status: "incomplete"`。
  - **Ollama**：优先走原生 `/api/chat` + `format`，`/v1` 的 `response_format` 作回退；关 thinking 时要求 ≥0.31.2；`num_ctx` 只能走 Modelfile 或 `OLLAMA_CONTEXT_LENGTH`。
  - **DeepSeek**：无 schema 强制，归入"提示词 + 校验 + 重试"档，且要处理空 content。
- 必读：`../research/structured-output-support.md`。
- 这条结论直接支撑"哪些 provider 敢承诺契约、哪些只是尽力而为"的对外说法。
- **来自票据 14 的三条实测约束**：
  - **重试只救随机畸形，救不了系统性漂移。** DeepSeek 上 v3 的错字段名（`paraphrase` 而非 `comprehensible`）**3 次全复现**，白跑 24 次调用——而且当时 `temperature` 在思考模式下被静默忽略，即那不是采样噪声。所以重试策略要能区分"输出畸形"（重试可能有用）与"契约系统性漂移"（重试无用，要么改提示词要么靠 schema 强制）。
  - **推理模型的 token 预算**：`deepseek-flash` 思考态下一个单句就能烧 1304 个 `reasoning_tokens`（completion 的 90%+）。`max_tokens` 必须按思考预算给足，否则长句会被截断成畸形输出——然后被上面的重试逻辑误判成"畸形"。
  - **思考开关与它的一串坑**：规范写法 `{"thinking":{"type":"disabled"}}`（或 `{"reasoning_effort":"none"}`）；`{"thinking":false}` 是 **422**；`{"enable_thinking":false}` **静默接受但无效**。另外**思考态下 `temperature`/`presence_penalty`/`frequency_penalty` 全部静默失效**（官方文档）——探测 provider 能力时不能只看 HTTP 200。
- **默认取"云端 + 关闭思考"**（票据 14）：1.3 秒、~175 输出 token，详尽度下降而正确性不降。

## Answer

六条决策，2026-10-05 全部按推荐采纳。

### 先更正一条过期前提

票据原文写"流式输出是硬需求（本地 CPU 推理一次 30–60 秒）"——那基于我早期**错误**的纯 CPU 假设。实测：本地热态 **1.7 秒**、云端关思考 **1.3 秒**。前提作废，见决策 6。

### 探测事实（实测，不是文档）

- **能力探测可以直接试**：DeepSeek + `json_schema` → **HTTP 400** `"This response_format type is unavailable now"`；同端点 + `json_object` → 200；llama.cpp + `json_schema` → 200。信号干净且廉价。
- **本地 router 的有用端点是 `/v1/models`，不是 `/props`**：`/props` 在 router 模式下只回 `role:"router"`、`n_ctx:0`（无用）；`/v1/models` 列出每个模型的 `status: loaded|unloaded` **与完整 argv（含 `--ctx-size 16384`）**——**不加载模型即可知道上下文长度**。用户机器上配了两个模型（4B 与 `Qwen3.5-2B-GGUF`）。
- **OS keyring 可用**：`org.freedesktop.secrets` 已在会话总线注册，`gnome-keyring-daemon` 在（无 `secret-tool` CLI，但应用走 secret-service 没问题）。

### 1. 配置面：预设 + 自定义

内置已知 provider 的预设（endpoint + 已知能力形态），同时允许自定义 base URL 与 model id。理由：provider 的差异**不止 base URL**（`response_format` 支持面、思考参数、模型列表能力都不同），而本地一个 base URL 下可能有多个模型——配置面必须能选**模型**，不只是选端点。

### 2. 能力探测：试一次 + 缓存 + 如实降级

首次配置某 provider 时发一个**极小探针请求**（`json_schema` 试探 + 读模型列表/上下文），结果**缓存**进配置。不被支持或探测失败 → 降级为"只能尽力而为"（`json_object` + 校验 + 重试）并**如实告知**用户，这正是"哪些 provider 敢承诺契约、哪些只是尽力而为"那句对外说法的机械依据。

**明确拒绝静态能力表**：同一家 API 会变，DeepSeek 就是活例——`json_object` 可用而 `json_schema` 不可用，光看 provider 名字判断不出来。

### 3. 默认端点

- **云端**：`deepseek-flash` + **默认关闭思考**（票据 14）。
- **本地**：**不猜端口**，探测一组常见端口（本机 3060 / Ollama 11434 / llama.cpp 默认 8080 / LM Studio 1234），把发现的运行时列进配置页让用户点选。

### 4. 错误分类与重试（四类，各走各的路）

| 类型 | 处理 |
|---|---|
| 输出畸形（JSON 解析失败 / 校验失败） | 重试，最多 2 次额外 |
| **同一错误重复**（第 2 次与第 1 次完全相同） | **立即停**——判定为系统性漂移，重试无用（票据 14 实测：v3 的错字段名 3/3 复现） |
| HTTP 4xx 参数不受支持 | 不重试；**降级能力并重新探测** |
| 429 / 5xx / 超时 | 退避重试 |

### 5. "思考"建模为可选能力

供应商层把它建模为 `thinking: on | off | unsupported`，**默认 off**。理由：它是**请求参数而不是模型选择**（DeepSeek 的三个候选参数里两个失败方式都不明显：`{"thinking":false}` 是 422、`{"enable_thinking":false}` 静默无效）；接口现在不表达它，以后想加"详尽档"就要动 provider 抽象层。**UI 是否暴露留给票据 06**。

### 6. 流式：v0 不做，接口留位置

理由是与一个已定决策**天生冲突**：**流式与"必须等完整 JSON 才能校验"矛盾**——要么边流边呈现未经验证的内容（等于把"自信但错"提前递给用户，正是票据 14 要防的），要么等完整产物再渲染（那就没有流式）。而等待只有 1.3–1.7 秒，这笔账不划算。接口留位，v1 若做"进度/思考"展示再谈。

### 留给下游

- **票据 06**：思考开关是否作为用户可见的"详尽档 / 快速档"（从雾区移入 06）。
- **新票据 15**：provider 配置与能力缓存放哪（配置文件 vs SQLite），密钥只在 keyring。
