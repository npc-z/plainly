# 04 Provider 抽象与运行时探测

Type: grilling
Status: open

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
