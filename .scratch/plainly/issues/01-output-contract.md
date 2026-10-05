# 01 输出契约 schema 与 markdown 渲染

Type: grilling
Status: resolved

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

## Answer

六个决策，2026-10-05 全部按推荐采纳：

1. **粒度**：一个 **Explanation 对应一个 Passage**（一段话）。超长输入在**段落边界**切分，每段各自成篇——不是把一个产物拆成多次调用。一次调用才能让改写、gloss、翻译**互相看得见**（改写要知道哪些表达已 gloss 过）。
2. **模型输出边界**：模型只产出内容四件套——`comprehensible`、`glosses[]`、`grammar|null`、`translation`。**原文与全部元数据**（Level、Native Language、provider、模型、时间、版本）由应用附加。理由：原文的权威副本在应用手里，**回显等于给模型一次静默篡改的机会**（原型第一条规则是 "The original is authoritative"）；Level 与 Native Language 是应用的设置，不是模型的观察。附带收益：输出越小，schema 越窄，**本地小模型的契约遵循率越高**。
3. **gloss 字段**：`glosses` 是**对象数组** `[{expression, gloss}]`——字典不可行（OpenAI strict 不支持 `patternProperties`），数组还能保序。**不带 `why`**：那是过程不是产物，加了只会撑大输出、拖慢本地模型。
4. **markdown 逐字复刻原型**：`### Original` / `### Comprehensible English` / `### Key Help`（`- \`expr\` → gloss`）/ `### Grammar`（需要时才出现，位于 Translation 之前）/ `### Translation`。CLI 与导出走这份，UI 用同一结构但标题可本地化。渲染器是**纯函数**，测试金标准就是原型文档里的两个例子。
5. **两个版本号**：`artifact_version`（解释数据模型，随记录入库）+ `prompt_version`（产出它的提示词）。**wire schema 不落地**——它与 artifact 版本同生共死。数据模型与提示词各自独立演化，绑在一起会让"只改了个措辞"被迫触发迁移检查。
6. **领域词**：**Explanation**（一次"把难英语变可理解"的成果）/ **Record**（历史库里的那一行）/ **Artifact**（仅指序列化形态，不作领域词）。已写入仓库根 `GLOSSARY.md`（该文件首次创建）。

**纸面 vs 经验**：以上是设计定稿。**schema 尚未在任何 provider 路径上真正编译通过**——验证需要跑起服务与模型，挂在 [03 默认本地模型质量门槛](03-local-model-quality-bar.md) 之后。

**给下游的约束**：

- 流式呈现的问题（此前是雾区）前提已清楚，已毕业进 [06 桌面 UI 信息架构](06-desktop-ui-ia.md)。
- [08 历史库 schema 与导出](08-history-schema-export.md)：Record 的字段集与两个版本号由此确定。
- [09 提示词参数化与版本迁移](09-prompt-parameterization.md)：`prompt_version`，以及"Level 作用域 / 旧记录是否重跑"（此前是雾区），合并为同一个迁移语义问题。
