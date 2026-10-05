# 03 默认本地模型质量门槛

Type: task
Status: resolved

## Question

把**手上已有的这一个本地模型**的质量门槛量出来。**不再做多模型 A/B，也不再下载任何模型。**

**既定条件**（用户 2026-10-05 指定，覆盖本票据原来的写法）：

- 本地服务已在运行：`http://127.0.0.1:3060`。llama.cpp **router 模式**：`--models-max 1`、`--sleep-idle-seconds 900`，所以空闲会卸载模型、**首个请求有冷启动**。沙箱内可达（`/health` 与 `/v1/models` 均 200）。
- 唯一模型：`unsloth/Qwen3-4B-Instruct-2507-GGUF:Q4_K_M`，`--ctx-size 16384`，`--n-gpu-layers 99`（RTX 3060 Laptop / CUDA）。
- 服务定义在用户的 NixOS flake：`~/.config/nixos/modules/linux/base/llama-cpp.nix` 与 `~/.config/nixos/hosts/r9000p/configuration.nix`。
- **不要再下载任何模型。**

要回答的：

1. **契约遵循率**：这个 4B 模型能否守住票据 01 的契约（`comprehensible` / `glosses[]` / `grammar|null` / `translation`）？用**嵌套的** `response_format.json_schema.schema`（票据 10 的硬要求：扁平写法会被静默忽略）+ temperature 0，统计 JSON 解析率与 schema 校验通过率。
2. **内容质量**（需要人来判断，不是脚本）：gloss 是否只解释真正的 blocker、是否比被解释的表达更简单；改写是否保留有用的难度而不是把话说没了；翻译是否自然。
3. **延迟**：单段耗时，冷启动 vs 稳定态分开记。
4. **结论**：本地模式对外**怎么说才算诚实**（能承诺什么、不能承诺什么），以及"默认本地模型"这个说法还成不成立。

产出：`.scratch/plainly/bench/` 下的样本集、harness 与结果；结论写进本票据的 `## Answer`。

## Comments

- 这张票据顺带会**产出第一版具体的 JSON schema**（票据 01 只定了形状，没有可运行的副本），并把票据 01 那句"schema 尚未在任何 provider 路径上编译通过"补上经验证据。schema 的最终归属仍是票据 01 的决议。
- 本票据原写的"跑 Qwen3.5-9B / Gemma 4 12B / gpt-oss:20b 的 A/B"作废——用户已指定只用手上这一个模型。

## Answer

**结论：这个 4B 模型足以支撑"可用的本地模式"，但不足以被无条件信任——它会自信地把意思讲错，而 `grammar` 一次都没触发。**

### 契约遵循：满分

**12/12 通过**（JSON 解析 + schema 校验），`finish_reason` 全是 `stop`，无截断、无多余字段、无空 `content`。

这同时补上了票据 01 留下的空白：**解释 schema 首次真正跑通了一条 provider 路径**（llama.cpp，嵌套 `response_format.json_schema.schema` + temperature 0）。第一版具体 schema 落在 `.scratch/plainly/bench/schema.json`。

### 延迟：本地模式的实际手感

| | 秒 |
|---|---|
| 冷启动首个请求（含模型加载） | 7.2 |
| 热态 最小 / 最大 / 平均 | 1.21 / 2.75 / **1.87** |

12 段跑完不到 25 秒。**这推翻了我第一轮评估里"本地一次 30–60 秒、必须靠流式遮丑"的判断**——那是按"无 CUDA、纯 CPU"推的；实际是 RTX 3060 Laptop + CUDA（`--n-gpu-layers 99`）。流式仍值得做（1–3 秒的等待也要有交代），但不再是"本地可用性"的前提。

### 内容质量：五个具体缺陷（真正的门槛）

格式全对，内容有硬错。逐条：

1. **p02 把习语讲反了（最严重）**：`bottled it` 被释成 "kept something secret"，改写写成 "decided to keep the secret"，中文跟着错成"把这件事藏起来"。**正确意思是"临阵怯场、关键时刻没敢做"。** gloss / 改写 / 翻译三处**一致地错**——自信且自洽的错误，正是最难被用户察觉的一类。
2. **`grammar` 0/12**：包括 p03 那个教科书级倒装（`Not until … did the manager admit …`）——模型把它当词条塞进 gloss（"Not until → Only after"），而不是解释句子结构。原型文档明确把这类归到 `Grammar`。**这是提示词的锅**：系统提示写了 "use null when there is no such obstacle, which is the common case"，很可能把它压成了永远 null。
3. **p04 漏 blocker + 讲错语域**：只 gloss 了 `a state of genteel decay`，且释义偏——"polite or respectful appearance"；**genteel 是"有身份的、上流的"**，不是礼貌。**`repaired to`（前往）整个漏掉**，改写还把 `the Continent` 降成小写 "the continent"，丢掉"欧洲大陆"这个特定含义。
4. **p08 把反讽拉平**：`to put it mildly` 被改成 "in a mild way"，中文译成"说到底"。gloss 那半是对的（`not best pleased → very unhappy`），但**改写与翻译都错**——说明它认得这个词组，却没把这个结构用在句子上。
5. **p06 法律语域混淆**：`indemnify` 释成 "to pay for something when someone else is wrong or breaks a promise"（把"赔偿"和"过错方"混在一起）；改写里 `lessor`（出租人）凭空消失。中文翻译反而是对的。

另：p09 的改写凭空多出 "the missing part"（原句并未说明漏了什么）。

### 本地模式该怎么说才算诚实

- **能承诺**：契约形状稳定（12/12）、延迟 1–3 秒、完全离线、单段成本可忽略。
- **不能承诺**：**释义正确**。尤其**习语**与**语域**——恰好是学习者最需要帮助的地方，也是它最容易自信说错的地方。
- 所以本地模式 = "**够用，但必须让学习者看得见原文并自行核对**"。UI 不该把 gloss 呈现成权威（不能出现"记住这个"这类措辞却不给核对入口）——这条给票据 06。
- "默认本地模型"这个说法**成立**，但**默认 ≠ 可信**：默认指开箱可用，不指输出可依赖。

### 产出

`.scratch/plainly/bench/`：`schema.json`（第一版具体契约）、`passages.json`（12 段，覆盖 blocker 分类）、`bench.py`（仅 stdlib，一条命令）、`README.md`（怎么跑、为什么请求长这样）、`results/`（每段完整原始输出 + 汇总）。

缺陷 1/3/4/5 与 `grammar` 从不触发，是**提示词标定**的输入 → 已开票据 14。由于 harness 现成且平均 1.9 秒/段，改提示词后重跑全样本不到半分钟，迭代成本极低。
