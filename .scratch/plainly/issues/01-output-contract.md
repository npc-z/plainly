# 01 输出契约 schema 与 markdown 渲染

Type: grilling
Status: open

## Question

定下解释产物的 **JSON schema** 以及它与 markdown 渲染的映射。

- 字段集：原文、分级改写、gloss 列表（表达式 → 释义）、语法说明（可选）、母语翻译、等级、母语语言、provider/模型元数据、schema 版本。
- 与原型 `comprehensible-english/SKILL.md` 的输出小节一一对应（`### Original` / `### Comprehensible English` / `### Key Help` / `### Grammar` / `### Translation`）。决定渲染出的 markdown 是否**逐字复刻**那个形状——用户看不出来是 JSON 渲染的。
- 哪些字段必填、哪些可选；`Grammar` 缺省时怎么表示（缺失 vs 空）。
- **长段落的粒度**：一次调用吃掉整段，还是按 blockers 分段多次调用再拼装。
- `schema_version` 字段的形态（供 09 的提示词迁移与 08 的入库使用）。

输入：原型 `SKILL.md`、Q6 的决定（结构化 JSON + 渲染）、票据 10 的结论。

## Comments

- **来自票据 10 的硬约束（设计 schema 时必须满足）**：不出现矛盾的上下界（llama.cpp `#29462` 未修，一次请求即可 OOM 杀掉 sidecar）；优先只用 `maxItems`；下界保持小；浮点上的 `minimum`/`maximum` 在 llama.cpp 里被**静默忽略**；OpenAI strict 模式**不支持** `minItems`/`maxItems`/`minLength`/`maxLength`/`pattern`/`format`/`minimum`——"3–5 个 gloss"这类约束不可表达，只能靠提示词 + 校验 + 重试；可选字段必须写成 `type:["string","null"]` 并**仍列进 `required`**；根节点不能是 `anyOf`。
- 必读：`../research/structured-output-support.md`。
- **该票据的自然起点**：把 schema 真的编译过一遍各 provider 路径——票据 10 没有起过任何服务，所有结论都是文档/源码级。
