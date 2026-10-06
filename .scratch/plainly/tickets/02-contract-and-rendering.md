# 02: Explanation 契约与五节渲染

**What to build:** core 的数据契约——模型只返回内容四件套（Comprehensible English / Gloss 列表 / Grammar / Translation），原文与全部元数据由应用附加——以及把任何 Artifact 渲染成与原型逐字同形的五节 markdown。用注入式假 provider 在 core 的 explain 路径上跑通，不需要网络。

**Blocked by:** 01

**Spec:** [spec.md](../spec.md) §3 Explanation 契约、§4 渲染（含来自原型的 schema 形状）

**Status:** done (2026-10-06)

- [x] wire schema 不出现任何长度 / 模式 / 数值上下界（ADR-0001）；`glosses` 是保序的对象数组 `[{expression, gloss}]`；`grammar` 写成 `["string","null"]` 且仍列进 `required`
- [x] 校验器能区分「JSON 解析失败」与「schema 校验失败」；字段为 null 时按「无该节」处理
- [x] Artifact 形状含原文与全部元数据：Level、源 / 母语、provider、model、thinking、`artifact_version`（v0 = 1）、`prompt_version` 哈希与人类标签、时间戳
- [x] 五节渲染是纯函数，输出与原型文档里的两个例子逐字一致（`grammar` 为 null 时省略该节）
- [x] 另有「面板模式」渲染变体：`grammar` 为 null 时该节显示「本段不需要」而不是消失
- [x] 注入式假 provider 可编程返回畸形 JSON 与错字段名，explain 路径据此返回可区分的成功 / 失败结果
- [x] `--format json` 的文档形状与 Artifact 一致

## Comments

### 2026-10-06 实现记录

**建了什么。** core 新增五个模块：`explanation`（wire schema、`Explanation`/`Gloss`、`ContractError`、以 schema 驱动的校验器）、`artifact`（`Artifact` + 手写的 `Timestamp`）、`provider`（`Provider` seam、`ExplainRequest`、`ProviderError`）、`explain`（`explain()` + `ExplainError`）、`render`（五节的 `markdown()` 与面板变体 `panel()`）。测试侧新增 `tests/{contract,artifact,explain,render}.rs`，可编程假 provider 在 `tests/support/provider.rs`。

**证据（都跑过）。**

- `cargo test --workspace` **110 通过 / 1 ignored**（此前 64；新增 46 = contract 16、artifact 14、explain 7、render 9；ignored 那条是票据 01 的 keyring 真路径）。
- `cargo clippy --workspace --all-targets -- -D warnings` 干净；`cargo fmt --all --check` 干净；`RUSTDOCFLAGS=-D warnings cargo doc -p plainly-core --no-deps` 干净（没有断的 intra-doc link）。
- `scripts/check-headless-cli.sh` 仍通过：新增的 `serde_json` 没有把图形依赖带进来（`ldd` 只有 libc / libgcc）。
- **「逐字一致」是脚本验过的，不是肉眼**：把原型 `SKILL.md` 的两个 ```markdown 例子块与 `tests/render.rs` 里两个 `expected` 字面量程序化对比，`equal=True`。

**规格层补的 / 有意偏离的判断。**

1. **校验器由 schema 驱动，而不是把形状在 Rust 里重写一遍。** `Explanation::parse` 先 `serde_json` 解析（失败 = `MalformedJson`），再拿 `wire_schema()` 逐字段校验（失败 = `Schema { problems }`，每条带路径，如 `output.glosses[0].gloss: expected string, got number`）。这样 schema 与校验器是**同一份文档**，ADR-0001 的硬约束只有一处要守；校验器实现的关键字只有五个，契约测试里钉住 schema 只用这五个（一个校验器会忽略的关键字就是一条没人执行的规则）。**代价**：一个约 80 行的 JSON Schema 子集解释器。没有选 `jsonschema` / `schemars`：契约形状是被原型**钉死**的，通用实现买到的是这个契约不需要的普适性，而 derive 版本会多一个"生成的 schema 与规格记的那份是否一致"的漂移面。
2. **`config::Provider` → `config::ProviderProfile`**（3 行）。运行期新增的 seam 叫 `Provider`（trait），与 `[providers.<name>]` 那张表同名会让 crate 根同时出现两个 `Provider`。规格 §9 自己用 **provider profile** 指"provider + model + thinking"这个组合，所以把表改名是往规格的词上靠，不是另造词。**顺带记下**：`Provider Profile` / `Effective Prompt` / `Level Descriptor` 这三个词规格点名要补进 `GLOSSARY.md`，现在仍缺——那是 `/domain-modeling` 的范围，本票据不擅自改词表。
3. **`Timestamp` 手写**（Hinnant 的 civil/days 互转，约 20 行；UTC、RFC 3339 进、RFC 3339 出）。仓库先例是"一个小工具自己写比加依赖便宜"（票据 01 的 `TempDir`），core 又要保持依赖少且跨平台，所以没有引入 `time` / `chrono`。解析**只接受我们自己写出的那一种形式**（固定宽度、ASCII 数字）：带偏移、带小数秒、缺 `Z`、符号、窄字段一律 `None`；不存在的日期（`2026-02-30`、`2025-13-01`、`2025-04-31`）靠"算出来再算回去必须落回原字段"拒绝，不静默位移。**可表示区间是类型的一部分**：存储形态写的是四位年份，所以 `from_unix_seconds` 只接受 `0000-01-01T00:00:00Z`..`9999-12-31T23:59:59Z`（越界返回 `None`，与 `from_rfc3339` 同形），因此 `to_rfc3339` 写出的东西**一定能读回来**。唯一新增的依赖是 `serde_json`（JSON 契约绕不开）。
4. **时间以值注入，不造 Clock trait；`Timestamp::now()` 也不在 core 里。** `explain(provider, request, now)`：这条路径对"现在"的唯一用法是给 Artifact 盖时间戳，注入一个 `Timestamp` 就是完整的注入，测试与 CLI 都不必为此多背一个抽象。**系统时钟的读法推迟到票据 03**（第一个真正的调用者）：那里"时钟读不出来"要连同退出码一起决定，而不是在领域层里落一个 1970 的默认值（review 抓到过这一版）。
5. **面板文案由调用方传入。** `render::panel(passage, explanation, grammar_not_needed)`。规格 §4/§12 要求面板把缺失的 Grammar 节显示成「本段不需要」，但 §5 同时规定界面文案集中在 strings 模块（票据 15）。所以 core 只负责"这一节存在，并且要说一句"，**那句话由面板给**；本票据只交付这个结构，**文案本身的落点是票据 15**（有意的推迟，不是漏做）——测试照样把 `"本段不需要"` 钉死。
6. **Artifact 是扁平的**（`#[serde(flatten)]` 把四件套铺在与元数据同一层）：`--format json` 打的就是这个文档，票据 03 只负责打印。AC7「`--format json` 的文档形状与 Artifact 一致」因此在**形状层**完成（`tests/artifact.rs` 把 16 个键的集合钉死），**没有**提前实现 `--format json` 本身（那是票据 03 的 AC）。`created_at` 与 `generated_at` 都在：一次全新生成时两者相等，命中缓存 / 重新生成之后才会分开（票据 08）。**`last_seen` 不在 Artifact 上**：它是"命中缓存"这条存储事实，是 §9 的 Record 字段，归票据 08；`--format json` 命中缓存时打什么，由票据 03 与 08 一起定。

### 2026-10-06 两轴 review（`/code-review`：Standards 与 Spec 各一个子代理）与处理

**修正的（都是真的）。**

1. **`Timestamp::from_rfc3339` 会接受非规范输入**（Spec 轴抓到的实质 bug）：`"2025-10-09T-1:00:00Z"` 里 `-1` 被 `parse::<i64>` 吃掉、只挡了 `> 23`，于是静默变成前一天的 23 点；`2025-9-9T0:0:0Z` 这种窄字段也被接受——与它自己那句"只接受写出的那一种形式，别的都是 `None`"直接矛盾。已改成**先校验形状再取值**：`YYYY-MM-DDTHH:MM:SSZ`，固定宽度 + ASCII 数字（新 `digits` 辅助函数），再走日历回算检查；新增一条测试覆盖 `-1` / `+1` / 窄字段 / 小写 `z` / 前导 `+`。
2. **静默兜底**（Standards 轴）：`Timestamp::now()` 在时钟早于 1970 时 `unwrap_or(0)`，会把 1970 悄悄写进溯源时间戳。**直接删掉 `now()`**：它当时没有任何非测试调用者，"时钟坏了怎么办"由第一个真正的调用者（票据 03 的 CLI）连同退出码一起决定，不在领域层用默认值糊过去。
3. **没有调用者的公开 API**（Speculative Generality；票据 01 的先例就是删掉无调用者的 API）：除 `now()` 外，删掉 `impl Display for Timestamp`、`SectionKind::ALL`（顺序改由 `sections()` 与 render 测试里的显式数组各自钉住）、以及公开常量 `UNDERSTOOD_WIRE_KEYWORDS`（关键字清单移进 `tests/contract.rs`，成为契约测试的一部分）。`unix_seconds()` 保留：它是 `from_unix_seconds()` 的取值口，测试也确实用它断言注入的时刻。
4. **重复的样板**：`tests/contract.rs` 里 5 处 `let ContractError::Schema {..} = ... else { panic! }` 收成一个 `problems_with()` 辅助函数；`type_matches` / `describe_type` 两份"字符串或字符串数组"的处理合并成一个 `allowed_types()`。
5. **Mysterious Name**：参数 `when_no_grammar`（它装的是文案，不是条件）改名 `grammar_not_needed`。
6. **Middle Man**：`ProviderError` 的 `message` 字段由 `pub` 改为私有，只留 `new()` 一个构造口。
7. **GLOSSARY 措辞**：`created_at` 的文档原来写 "the Record was first created"——这个类型还不是 Record，改成 "this Explanation"。**`Artifact` 这个名字保留**：票据与规格 §10 都用它指"序列化形态"（`--format json` 打的那份文档，也就是词表里"编码本身当主语"的那种用法）；`Record` 是票据 08 入库时才出现的东西（带 id / lookup_key / last_seen）。

**想清楚后不动的（连同理由，供复核判断）。**

- **改名的 scope creep 质疑**：保留 `Provider`（trait）+ `ProviderProfile`（配置表）。运行期 seam 必须叫 `Provider`，同名的两个 `Provider` 在 crate 根上迟早咬人；规格 §9 自己把"provider + model + thinking"叫 provider profile。改动 3 行，没有任何 CLI / 测试引用旧名。
- **`serde_json` 是新增依赖**：JSON 契约绕不开，没有替代（手写 JSON 解析器显然更糟）。这是本票据唯一的依赖新增，也是 core 至今唯一一处"宁可加依赖"的地方。
- **Data Clumps（provider / model / thinking 三个字段一起走）**：规格 §9 说这组值的正式名字与归属是 **Lookup Key**，而 Lookup Key 是票据 08 的东西。此刻另造一个类型只会与 `config::ProviderProfile`（endpoint / model / thinking，另一回事）撞名或撞概念，所以**留给票据 08 定名**——那时它是 `LookupKey` 的分量。
- **`passage: String` 的 Primitive Obsession**：Passage 目前没有可挂的行为；等票据 11 的切分给它行为的可能，再决定要不要成为类型。
- **Divergent Change（artifact.rs = 契约 + 日历；explanation.rs = 契约 + 校验器）**：两处都在同一条"契约"责任下，且都有注释说明为什么这样切；等它们真的各自长大再拆。

### 2026-10-06 第二轮复核（用户逐条提出）

1. **`to_rfc3339` 能写出 `from_rfc3339` 读不回来的形式**：年份 ≥ 10000 与负年份都能被 i64 构造出来，于是 `Timestamp` 能序列化成自己反序列化失败的 JSON（数据一旦入库就是静默丢失）。修法与上面第 3 条一致：`from_unix_seconds(seconds) -> Option<Self>` 定住区间，端点各一条测试，并直接测 `serde_json` 的写出→读回。
2. **`glosses` 为空时文档出现空标题**（`### Key Help` 下面什么都没有）。规则统一为：**模型"没有内容可放"的节在两种表面都省掉**——`grammar` 为 null（文档省略）、无 Gloss 的 Key Help（省略）；面板保留缺失的 Grammar 并标注是规格 §4 的特例。空**散文**（空的 comprehensible / translation）不适用这条：那是坏答案，归票据 07。文档与测试都写明了这个区分。
3. **`SectionKind::title()` 的文档会误导面板实现者**：改了措辞——它明确是"**markdown 文档**的标题"，并写明面板拿到 `kind` 后自己映射本地化标题（文案在票据 15），Explanation 的内容仍固定英文。
4. **裸的「ticket NN」在 `issues/` 与 `tickets/` 之间是有歧义的**（两个目录的编号大量重叠）。新代码里的引用一律写成 `tickets/NN` / `issues/NN`；其中两处原本指错了：五节形状出自 `issues/01-output-contract.md`（不是 tickets/01，那是工作区/配置/密钥），`paraphrase` 的实测出自 `issues/14-prompt-calibration.md`（不是 tickets/14，那是主窗口）。
5. **AC7 的闭合点在票据 03**：本票据只把文档形状钉死，`--format json` 的打印路径在票据 03 —— 已作为交接写进下面的缺口清单。

### 2026-10-06 第三轮小修（用户提出）

1. **`panel()` 的文档说错了话**：写的是 "Two differences from markdown"，但"省掉空的 Key Help"在两种表面一样，不是 difference。改成 "One difference"（只有 Grammar 那条），Key Help 的规则归到 `markdown()` 的说明里，`panel()` 指向它。
2. **空 Key Help 的不对称有了挂点**：[tickets/15](15-panel-actions-and-states.md) 的 Comments 里留了一条——core 为什么省、这条理由（分不清"没有"与"没生成"）对 Key Help 一字不差地成立、以及若面板要给一句，`panel()` 要加第二个文案参数（并写明**不许**在面板里重建 section 列表，否则两处规则会分叉）。决定权留给面板那一票。
3. **`from_rfc3339` 里的 `expect` 拿掉**：区间检查在**两个入口**各做一次，解析路径直接 `Self::from_unix_seconds(seconds)`。理由：这个不变量其实由边界测试守着（区间一收窄，`the_representable_range_...` 立刻红），所以 `expect` 买不到"开发期报警"，只买到"读旧记录时 panic"；现在收窄区间的代价是旧记录读不出来（可诊断的错误），而不是崩在别人的历史上。

**记下的缺口（不在本票据里悄悄糊掉）。**

1. `ProviderError` 现在只有一种分类（一条 `message`）：429 / 5xx / 超时 / 4xx / 空 content 的区分是票据 04 的重试策略的事；本票据只保证「provider 失败」与「契约失败」可区分。
2. `artifact_version` 还没有迁移代码——v0 只有一个版本，第二个版本要等票据 08 入库时才会出现。
3. 模型返回 `"grammar": ""` 时面板会渲染一个空节。契约只管形状不管质量，"空节"属于票据 07 的提示词问题（契约测试里显式钉住：空字符串是合法字符串）。
4. **AC7 的端到端闭合在票据 03**：`Artifact` 的 JSON 形状在这里钉死（16 个键），但"`--format json` 的文档形状与 Artifact 一致"只有在票据 03 真的把 `serde_json::to_string(&artifact)` 打到 stdout 之后，才算在用户可见的意义上完成。
