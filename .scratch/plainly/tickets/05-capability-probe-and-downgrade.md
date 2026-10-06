# 05: 能力探测、缓存与降级

**What to build:** 应用自己试一次就知道这家 provider 能不能强制 schema，把结论缓存起来并如实告诉我「哪些 provider 敢承诺契约、哪些只是尽力而为」。

**Blocked by:** 04

**Spec:** [spec.md](../spec.md) §7 Provider 层（探测、缓存、各 provider 怪癖）

**Status:** ready-for-agent

- [ ] 探针发一个极小请求（`json_schema` 试探 + 读模型列表 / 上下文），结论写进 cache 目录，**不**写进配置文件
- [ ] DeepSeek 的 `json_schema` → HTTP 400 会落到「提示词 + 校验 + 重试」档并如实告知；llama.cpp → 200 走强制档
- [ ] 不支持或探测失败时降级为 `json_object` + 校验 + 重试，且界面上与强制档可区分
- [ ] 4xx 参数不受支持、手动重探、endpoint / model 改动都会让缓存失效；清空缓存不丢用户的 provider 选择
- [ ] 各 provider 调用怪癖各有一条测试：LM Studio 的 `content` 为空时回退 `reasoning_content`；OpenAI 解析前先查 refusal 与 `status: incomplete`；Ollama 优先原生 `/api/chat` + `format`（`/v1` 作回退）；DeepSeek 只发 `json_object`
- [ ] 思考被建模为 `on | off | unsupported`，默认 off，接口能表达它（以后才加得上「详尽档」）
- [ ] `plainly providers` 显示当前 provider、模型、thinking 与探测结论；`providers probe [name]` 强制重探
