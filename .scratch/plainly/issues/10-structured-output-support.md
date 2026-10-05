# 10 结构化输出在各 provider 的支持面

Type: research
Status: resolved

## Question

查清 **schema 约束解码在各 provider 上的确切支持面与限制**，供票据 01 与 04 使用。

- **llama.cpp**：`json_schema` / GBNF grammar 的用法与版本要求；已知的 grammar 重复计数不设上限导致 OOM 的坑（`ggml-org/llama.cpp#29462`）；`--api-key`、`response_format` 的支持程度。
- **Ollama**：`format` 字段（JSON Schema）与 OpenAI `response_format` 的关系；哪些字段不支持；Ollama Cloud **不支持**结构化输出的影响；`num_ctx` 必须走 Modelfile。
- **OpenAI**：结构化输出 strict 模式的 schema 子集限制（必填字段、`additionalProperties: false`、不支持的关键字）。
- **LM Studio**、**DeepSeek**：各自的支持面与已知差异。

产出：一张 **"provider × 能否强制 schema × 已知坑 × 建议调用参数"** 表，外加一句结论：哪些 provider 可以真正保证契约、哪些只能"祈祷 + 校验 + 重试"。

资产：`research/structured-output-support.md`（由研究子代理产出）。
注明无法验证的项，不要用推断填空。

## Answer

**能真正强制 schema 的只有 4 家**，其余只能"提示词 + 校验 + 重试"。

| Provider | 能否强制 schema | 最要命的坑 |
|---|---|---|
| OpenAI | 能（参考实现） | strict 模式**禁止** `minItems`/`maxItems`/`minLength`/`pattern`/`minimum`；可选的语法小节必须写成 `type:["string","null"]` **且仍列进 `required`**；refusal 不遵循 schema |
| llama.cpp | 能（已核源码） | 只认**嵌套**的 `response_format.json_schema.schema`；**它自己 README 记的扁平写法被静默忽略**，降级成"任意 JSON 对象"且不报错。`#29462` 未修：`minItems > maxItems` 一次未认证请求即可 OOM 杀掉服务 |
| LM Studio | 能，但两个硬坑 | `{"type":"json_object"}` 直接 **HTTP 400**；**未修**的 bug 把 schema 输出塞进 `reasoning_content`、`content` 留空——**正好命中 Qwen3.5 这个我们推荐的本地模型家族** |
| Ollama | 能，但**必须 ≥0.31.2**、thinking 开、非 cloud | 低于该版本时关掉 thinking 会**静默且间歇性**地让 `format` 失效；`num_ctx` 在 OpenAI 兼容 API 上设不了 |
| DeepSeek | **不能**，只有 JSON mode | `response_format.type` 只接受 `text`/`json_object`；要求提示词里出现 "json" 并给示例，且承认偶尔返回空内容 |

**对我们 schema 的硬约束**（票据 01 必须遵守）：不出现矛盾的上下界；优先只用 `maxItems`；下界保持小；浮点类型上的 `minimum`/`maximum` 在 llama.cpp 里被静默忽略；OpenAI strict 下任何长度/模式约束都无法表达——"3–5 个 gloss"只能靠提示词 + 校验。

**对 adapter 的硬要求**（票据 04）：llama.cpp 发嵌套形状；LM Studio 读 `content` 并在空时回退 `reasoning_content`；OpenAI 先查 refusal 再解析；Ollama 走原生 `/api/chat` + `format`（`/v1` 作回退）；DeepSeek 归入"提示词 + 校验 + 重试"档。

资产：[structured-output-support.md](../research/structured-output-support.md)（579 行，含逐条来源与"未能验证"清单）。

**未做**：没有真的起过任何服务，所有"能"都是文档/源码级结论。Plainly 的 artifact schema 尚未跑通任何 provider 路径——那是票据 01 的下一步。
