# Gloss 例句与多行版式规格

Status: ready-for-agent
Labels: ready-for-agent

> 由 2026-10-12 的 `/grill-with-docs` 会话坍缩而成（19 个决策，见文末溯源表）。
> 领域词一律按仓库根 `GLOSSARY.md`（本次新增 **Example** 词条、改写 **Gloss** 词条）。
> `docs/adr/0001` 是硬约束：本规格不向 wire schema 添加任何长度 / 模式 / 上下界。
> 与 `.scratch/plainly/spec.md` 的旧形状冲突之处以本规格为准：§3 的 `[{expression, gloss}]`、§4 的 `- \`expr\` → gloss`、§9 的 `glosses(record_id, ord, expression, gloss)`。

## Problem Statement

**关键帮助（Gloss）一节现在只解释意思，不展示用法。** 每条 Gloss 是一行 `` `expression` → 释义 ``。学习者读到 `organically` 的释义"by itself, in a natural way"，知道他大概是什么意思，但看不到这个表达**在另一个句子里长什么样**——而习语、搭配、语域的真正难点恰在用法：它跟什么主语走、接不接宾语、能不能拆开。对一个 A2 学习者，一行英文释义本身也可能不比原文容易多少。

**一条一行，四五条挤成一堵墙。** 表达与释义靠一个箭头分隔，条与条之间只有 5px。扫读时"哪里是一条 Gloss 的开头"只能靠箭头认，而箭头正是"一行要塞下两件事"的产物。

**更深一层：例句是本应用唯一一处"生成新句子"的产出。** 改写、释义、翻译都被"不要编造原文没有的细节"这条硬规则约束着，例句却必然引入原文没有的主语和情境——所以它必须被明确地定义、明确地限制在"不许改变这个表达的意思"上，否则它会和整篇 Explanation 的准确性规则打架。

## Solution

每个 **Gloss** 从两件东西变成三件：`expression`（原文里的表达）、`gloss`（plain-English 释义）、**`example`（一句 AI 生成的英文例句，与整篇 Explanation 同一个 Level）**。例句是**新句子**，不是原文那句，用的是这个表达**在原文里的那层意思**，框架词都在用户所选 Level 之内，让表达本身成为句子里唯一难的东西。

显示上每条 Gloss 是三块：**表达（等宽 chip）/ 释义 / 例句**，而每个表面把这三块画成自己的材质——桌面卡片是 chip + 正文 + 衬线斜体加一条左规线，文档面（CLI、复制、两种导出）是 bullet 加引文块，面板是 chip + 正文 + 规线引文的三个控件。**同一个结构，三种画法**：core 交出的不是文本而是结构（`Body::Prose` / `Body::Glosses`），各表面自己排；分段、顺序与"空节说什么"仍然只有 core 一份。

老 Record 照原样显示：它生成于 `example` 存在之前，没有例句就不显示那一行（不是空行，也不是假例句），也不做任何回溯。**存储引入本仓库第一个迁移步骤**——加列，旧行读回空例句。

## User Stories

### 读到一个 Gloss

1. 作为 A2 学习者，我想要每个 Gloss 附一句我读得懂的例句，以便看到这个表达在别的句子里怎么用，而不是只记住一句释义。
2. 作为 B2 / C1 学习者，我想要例句的用词跟着我选定的 Level 走，以便例句本身不变成新的障碍。
3. 作为学习者，我想要例句用的是这个表达在原文里的那层意思，以便它印证原文而不是引入第二种解释。
4. 作为学习者，我想要例句的意思能从它自己的上下文里读出来，以便例句真的帮我把释义坐实。
5. 作为学习者，我想要例句是另起的一句、不是原文那句的复制，以便它不是把原文再念一遍。

### 读到一张卡片

6. 作为扫读的人，我想要每条 Gloss 拆成"表达 / 释义 / 例句"三行、条与条之间留出间距，以便一眼分清哪句是释义、哪句是例句。
7. 作为使用者，我想要例句在视觉上退到背景里（斜体、变暗），以便它不抢释义的位置。
8. 作为使用者，我想要关键帮助一节在没有 Gloss 时的样子不变（"本段没有需要解释的表达"），以便空与非空仍能区分。
9. 作为在面板上读的人，我想要屏幕上就是排好的文字——表达、释义、例句各有各的材质——而不是 markdown 的记号（`-`、反引号、`_`），以便浮层里出现的是给人读的东西，不是源文件。
10. 作为用 CLI（`glow`）读的人，我想要例句是一段引文块而不是一行会被软换行吃掉的缩进，以便它看上去就是一句被引用的句子。

### 老记录、复制与导出

11. 作为翻历史的人，我想要生成于例句版本之前的 Record 照原样显示（没有例句就不显示那行），以便历史不出现空行或编造的例句。
12. 作为点"复制 markdown"或"导出这一条"的人，我想要复制出来的文本和我屏幕上看到的是同一个形状，以便复制件能直接读。
13. 作为在终端里用 CLI 的人，我想要 `plainly`、`history show` 与两种导出的关键帮助一节同样是那个形状，以便一个 Explanation 只有一个形状。
14. 作为搜索过历史的人，我想要搜索范围仍然只覆盖原文、改写、表达与释义（例句不进索引），以便我不把"搜不到例句里的词"当成丢数据。

### 维护者

15. 作为维护者，我想要 wire schema 里依然没有任何长度 / 模式 / 上下界约束，以便 OpenAI strict 模式不炸、llama.cpp 不被一次请求 OOM 杀掉。
16. 作为维护者，我想要旧库能被新版本直接打开（第一个迁移步骤），以便升级不要求用户删历史。
17. 作为维护者，我想要"例句在契约里必填、在旧记录里不存在"这条取舍写在 ADR 里，以便后来人不必从代码反推为什么有的 Gloss 没有例句。

## Implementation Decisions

### 1. 契约：`example` 必填，且不带上界

- `glosses` 的每一项从 `{expression, gloss}` 变成 `{expression, gloss, example}`，三项**都进 `required`**，都是 `{"type": "string"}`。
- 理由与 `grammar` 可空不同：`grammar: null` 是模型对"这句话没有结构难点"的**合法判断**，而"这条 Gloss 没有例句"不是判断，是缺失。契约靠 schema 约束解码，不靠提示词祈祷（`.scratch/plainly/spec.md` §5）。
- **ADR-0001 依旧成立**：`example` 不加 `minLength` / `maxLength` / `pattern` / `format`（OpenAI strict 不支持；llama.cpp 的下界会 OOM）。"一句、别太长、别为空"只能写在提示词里。
- `wire_schema()` 仍是形状唯一权威：`Explanation::parse` 先按它校验再反序列化，所以 Rust 结构体必须同时加字段，否则两条路会漂移（`core/src/explanation.rs` 模块文档的原话）。

### 2. 数据模型与存储：`artifact_version` 1 → 2，第一个迁移步骤

- `ARTIFACT_VERSION = 2`：加字段就是数据模型变了，Record 上那个版本号的意义就是"这一行按哪一代形状读"。
- `artifact_version` **不进 Lookup Key**（`store.rs` 明文），所以升版本本身不丢缓存；旧行继续按 1 读，新行写 2。
- `glosses` 表加 `example TEXT NOT NULL DEFAULT ''`，并引入 `PRAGMA user_version` 迁移：
  - 新建库：`CREATE TABLE` 直接是真形状，迁移步骤因列已存在而跳过；
  - 旧库：`CREATE TABLE IF NOT EXISTS` 不会加列，迁移步骤检测缺列后 `ALTER TABLE glosses ADD COLUMN example TEXT NOT NULL DEFAULT ''`。
  - 迁移以 `user_version` 为准，且加列前检测列是否存在——两种库都只跑一次、都可重复打开。
- 写路径（`remember` 的 insert）与读路径（`READ` / `Raw` / `Raw::gloss`）都要带 `example`。

### 3. 旧 Record：不回溯

- 生成于 v1 的 Record 永远没有例句。**不补生成、不在读取时重新生成、不提供"补例句"按钮。**
- 理由：重生成要么花用户的钱，要么在用户没要求的时候把一段已经读过的解释换掉。同一个 Passage 再被问到时，prompt 版本已经变了，那是一条**新的 Lookup**、新的 Record——例句会自然而然出现（ticket 09 记录的"换提示词会多出一条记录"是同一件事，不是新问题）。
- UI 表现：例句为空 = 这一条没有例句 = **不渲染那一行**；不显示空行、不显示"无例句"、不加"这条生成于例句之前"的提示。页脚的 provenance 已经有 `记录格式 v1/v2`，够用。

### 4. Example 的定义（提示词要按它写）

- **一句英文**，不配母语翻译（翻译永远是独立的 Translation 一节）。
- **新句子**，不是原文那句的复制。
- 用这个表达**在原文里的那层意思**，不是它别的义项。
- **框架词在用户所选 Level 之内**：`{{LEVEL}}` 管整篇，也管例句；表达本身可以是句子里唯一超纲的东西（否则 A2 学习者永远学不到一个 C1 的习语）。
- **意思能从上下文读出来**：读者应当能只看例句猜出释义。这是例句的教学 payoff，也是它区别于"用法展示"的地方。
- **允许屈折**：`go to ground` 可以以 `went to ground` 出现，只要仍认得出。（代价：UI 不做例句内高亮，见第 7 条。）
- **不配高亮、不配音频、不配第二条例句。**

### 5. 提示词与版本

- `PROMPT_LABEL`：`v7-descriptors` → **`v8-examples`**（人读的标签；真实版本是 `FACTORY_PROMPT` 的内容哈希，改文本自动变）。
- `FACTORY_PROMPT` 的三处改动：
  1. `"glosses"` 那条：每条变成"一个释义 + 一句例句"，并把 `example` 写进 JSON 形状说明；
  2. 新增一小段 **Rules for an "example"**：新句子 / Level 用词 / 上下文能读出意思 / 允许屈折 / 不许为空；
  3. 「不要编造原文没有的细节」那条**显式限定作用域**：它管改写、释义与翻译；例句是模型自己的句子，可以有自己的主语和情境，只是不许改变表达的意思。不加这句，模型会和自己的准确性规则打架。
- 有效提示词的构成（出厂 + 附录 + 等级描述表）与哈希规则不变。

### 6. 渲染：结构，不是文档

2026-10-12 第二轮（看真机截图之后）。第一版把三行写成 markdown 文本，两个后果都露出来了：CLI 里 `glow` 把两行缩进合并成一段（软换行把缩进吃掉了），面板这条 GTK label 则把 `-`、反引号、`_` **原样打了出来**。根因不是那三行怎么排，而是**渲染器把文档交给了不是文档的表面**。

所以 Key Help 的 body 不再是 `String`，而是 core 的一个结构：

```rust
pub struct Section { pub kind: SectionKind, pub body: Body }
pub enum Body { Prose(String), Glosses(Vec<Gloss>) }
```

- 文档面（CLI、复制、两种导出）把 `Glosses` 排成 markdown：每条 Gloss 一个 bullet，表达 + 释义占 bullet 的前两行，**例句是嵌套的 block quote**（`  > ...`），条目之间空行——松列表，因为带引文的条目贴在一起会读成一条：

  ```
  - `indemnify the lessor against all claims`
    promise to pay the owner back if other people make legal demands or ask for money

    > The contract says the company must indemnify the owner against all claims if a worker gets hurt.
  ```

  引文块而不是第二条缩进行：释义是解释在说话，例句是它**引用**的一段散文，每个 markdown 阅读器都把引文画成内缩、置灰的块；两条普通缩进行只会被合成一段（这正是 CLI 上发生的事）。
- **面板不再吃 markdown**。它拿到的是 `Body::Glosses`，每条 Gloss 画三个控件：表达的 chip（等宽 + 底色，不是反引号）、释义（正文）、例句（左侧一条 2px 规线 + 衬线斜体 + 降低不透明度）。`panel.rs` 的 `body_widget` 是那条缝：`Prose` → label，`Glosses` → 控件。**"一个渲染器"依然成立**——分段、顺序、空节措辞仍然只有 core 一份；变的只是核心交出的是结构而不是文本。
- **raw CSV 的 `glosses` 单元格**：三行、无箭头、无 markdown 记号（电子表格单元格不是文档）：表达 / 缩进释义 / 缩进例句。例句为空（旧 Record）时只有前两行。
- 例句为空（旧 Record，或模型留空）时：文档只有 bullet + 释义，面板只有两个控件。

### 7. 桌面卡片：三种声音

```html
<div class="gl">
  <div class="ex"><code>organically</code></div>
  <div class="gloss">by itself, in a natural way, without being planned or forced</div>
  <div class="eg">She picked up Spanish organically while working in Madrid.</div>
</div>
```

- 三段各有各的**材质**，而不是靠一条灰线区分：表达是等宽 chip（从句子里引用来的），释义是正文 sans、`var(--ink)`（答案，最深的一层），例句是**衬线斜体 + `var(--dim)` + 左侧 `2px solid var(--quote)` 规线 + 再缩进**（引文，不是更多定义；宽度与面板那条 2px 的规线对齐，颜色各自取自自己的调度——卡片有 token，layer surface 没有）。
- 位置：表达顶格；释义缩进 12px；例句在 12px 处起规线、文字再进 12px；**条间距 16px**（第一版 12px，三段挤在一条里）。
- **不高亮**例句里的表达：允许屈折之后字符串匹配不可靠（`taken for` 会以 `took … for` 出现），逐字匹配会让同一个表达时而高亮时而不亮，模糊匹配会标错。
- 例句**不借用「原文」那个纸色块**：那个材质在这个应用里是"你粘进来的那段原文"的签名，机器写的例句不该长得像它。区分靠字体、颜色与规线，不靠把两种内容混成一种。
- `GlossView`（`desktop/src/view.rs`）带 `example: String`；`app.js` 的模板字面量是唯一的 HTML 缝，没有 JS 测试——样式改动靠手测清单与真实渲染的预览（[`ui/desktop-detail.png`](ui/desktop-detail.png)，两张预览与做法见 [`ui/README.md`](ui/README.md)）。

### 8. 面板与 CLI

- 面板（GTK3 layer surface，470px）的关键帮助是**控件**，不是一条 label：`panel.rs` 的 `glosses_widget` / `gloss_widget` / `example_widget`，样式在 `css.rs` 的 `.plainly-expression-chip` / `.plainly-expression` / `.plainly-example-rule` / `.plainly-example`。
- 面板的 CSS 解析与控件树有测试兜底（`the_key_help_reaches_the_screen_without_markup`）：GTK 没有任何提示就忽略一条解析不了的规则，而"没有样式"和"设计如此"在屏幕上看不出区别。测试需要 display（GTK 不能无头初始化），没有 display 时打印一行说明并跳过。
- 面板那节仍然是可折叠的 expander，但每条 Gloss 更占纵向空间；手测清单里看一眼 470px 宽下能不能用。真实渲染：[`ui/panel-key-help.png`](ui/panel-key-help.png)。
- CLI 的 `explain` / `history show` / 两种导出走同一个 markdown 排法，因此例句是引文块（`glow` 会画成内缩的引用），不再是会被软换行吃掉的两行缩进。

### 9. 搜索范围不变

- `gloss_fts` 仍是 `(expression, gloss)`，**例句不进索引**。理由：搜索回答的是"哪条记录讲过这个表达 / 这个意思"；例句是生成出来的脚手架、全是常见词，进索引只会让搜索变吵。副作用是 **fts5 表结构不动**，旧库不需要重建索引。
- 代价：只出现在例句里的词搜不到——所以 `history.rs` 与 README 里"搜索覆盖原文、改写与 Gloss"的说明保持为真，不夸大。

### 10. 显示开关

- **不新增设置项**。例句属于关键帮助一节，跟着 `show_glosses` 走；`SectionKind` 也不新增成员。
- 已核实的事实：桌面详情卡目前根本不读那五个 section 开关（它们是面板的开关），所以桌面这边没有可加的开关。

### 11. 面板标题统一

- 面板的 `SectionKind::KeyHelp` 标题 `关键词` → **`关键帮助`**：同一节两个名字是现存不一致，本次既然在动这一节就顺手统一（桌面与设置页已经是"关键帮助"）。

### 12. 空例句

- schema 不许写 `minLength`（ADR-0001），所以"例句非空"和现行的"3 到 5 条 Gloss"一样是**提示词 + 事后发现**，不进校验器：不加 schema 之外的语义校验，`wire_schema` 仍是形状的唯一权威。
- 检出放在两处：校准（`bench` 的校验把空例句判为无效）与手测清单。
- 已知极限写进 `FINDINGS.md`。

### 13. 校准：只跑 cloud

- 新增 `.scratch/plainly/bench/prompts/v8-examples.txt`（新出厂提示词的逐字副本）与 `bench/schema.json` 的 `example`（必填）。
- 运行：`BENCH_PROVIDER=deepseek BENCH_MODEL=deepseek-flash BENCH_PROMPT=v8-examples BENCH_EXTRA='{"thinking":{"type":"disabled"}}'`，全部 12 段，`BENCH_MAX_TOKENS` 用默认 2048（与 app 的 `completion_budget(thinking=off)` 一致）。
- 验收：12/12 契约有效；每条 Gloss 都有非空例句；例句不是原文原句；例句意思与释义一致（人判）。B2 与 A2 各跑一遍，看的正是"例句跟着 Level 走"这条新轴。
- 结果落 `results/deepseek/v8-examples*.json`，`FINDINGS.md` 追加一节。
- **本地 4B 本次不测**，在 `FINDINGS.md` 里记为已知未验风险（ticket 14 的数据已经说明 4B 在更简单的任务上都会错）。

## Testing Decisions

### 缝 A：契约（`plainly-core/tests/contract.rs`）

- `example` 缺失 = `missing required field`；`example` 类型错 = 与 `gloss` 同样的问题文本。`the_wire_schema_is_the_contract_the_spec_records` 要钉住新的 `required` 与 `properties`。
- `the_number_of_glosses_is_advisory_not_a_bound` 继续证明条数无界（例句不影响它）。
- `core/tests/prompt.rs` 的"提示词逐字说出每个字段"要加入 `example`。

### 缝 B：存储迁移（`plainly-core/tests/store.rs`，新增）

- **关键新测试**：手工造一个 v0 形状的库（`glosses` 无 `example` 列、`user_version = 0`），写入一行，然后用新版本打开——必须不报错、`user_version` 变成 1、那一行读回来 `example == ""`。
- 再来一次打开（迁移幂等）：不报错、不重复加列。
- 新 Record 的往返：`example` 从 `remember` 到 `read` 一字不差，且不受 FTS 重建影响（`a_history_written_before_the_search_tables_existed_is_searchable` 要跟着带上例句）。

### 缝 C：渲染黄金文本（`core/tests/render.rs`、`core/tests/export.rs`）

- markdown 形状：bullet + 释义行 + **引文块**（`  > `）、条目之间空行（松列表）；空例句只有前两行。
- 结构本身：`Body::Prose` 与 `Body::Glosses` 分开，面板与文档拿到的是同一次分节。
- raw CSV：一个 `glosses` 单元格里每条 Gloss 三行、无 `_`、无箭头，引号/换行转义规则不变。
- 下游钉住旧形状的断言同步改：`cli/tests/explain.rs`、`cli/tests/history.rs`、`panel/src/document.rs`、`desktop/tests/window.rs`。

### 缝 D：桌面（没有 JS 测试，靠手测 + Rust 侧视图模型）

- `desktop/tests/window.rs` 断言 `DetailView.glosses[].example` 与 `markdown` 一致。
- 视觉部分进 `docs/desktop-manual-checks.md`：三行、缩进、斜体变暗、条间距、旧记录只出两行。

### 缝 E：面板的控件树（`panel.rs` 的新测试）

- `the_key_help_reaches_the_screen_without_markup`：GTK 初始化后断言两件事——`css::CSS` 能解析（GTK 会静默忽略解析不了的规则），以及一条 Gloss 的控件树里恰好是"表达 / 释义 / 例句"三个 label、没有任何记号；没有例句的 Gloss 是两个。测试需要 display，没有 display 时打印一行说明并跳过（面板本来就是需要 display 的 crate）。

### 先例

- fixture 更新是本次最大的一处机械改动：`core/tests/support/mod.rs`、各 crate 里内联的 `glosses` JSON、`scripts/stub-provider.py`。
- 迁移测试用 `rusqlite` 手工建旧表，不用 SQL 文件——仓库没有 migration 目录，测试就是迁移的唯一文档。

## Out of Scope

- 例句的母语翻译、例句朗读、每条 Gloss 多个例句。
- 例句里表达的高亮。
- 例句进搜索索引。
- 旧 Record 的回溯 / 补生成 / 对比视图。
- 本地 4B 上的例句质量（本次不测）。
- 面板正文的富文本排版（面板仍是一条 label，不做逐条控件）。
- 任何形式的 schema 长度 / 模式约束（ADR-0001）。

## Further Notes

- `GLOSSARY.md` 的 **Gloss** 词条已改为"释义 + 例句"，并新增 **Example** 词条；实现细节（列、迁移、重试）不进词表。
- ADR：[`docs/adr/0002-the-example-is-required-but-not-backfilled.md`](../../docs/adr/0002-the-example-is-required-but-not-backfilled.md)。
- 溯源：会话（2026-10-12 `/grill-with-docs`，19 问）→ 规格条目：

| 会话决策 | 落在这里 |
| --- | --- |
| Q1 契约必填 + `artifact_version` 2 + 迁移 | §1、§2 |
| Q2 不回溯 | §3 |
| Q3 例句不索引 | §9 |
| Q4 例句定义 | §4 |
| Q5 允许屈折 | §4、§7 |
| Q6 提示词与 `v8-examples` | §5 |
| Q7 cloud 校准 | §13 |
| Q8 桌面堆叠版式 | §7 |
| Q9 文本面镜像桌面 | §6、§8 |
| Q10 不加开关 | §10 |
| Q11 面板标题统一 | §11 |
| Q12 文档产物（词表 / 规格 / 票据 / ADR） | 本文件、`GLOSSARY.md`、ADR-0002 |
| Q13 先文档后实现 | 全部 |
| Q14 不高亮 | §7 |
| Q15 块内细节（缩进 / 斜体 / 间距 / 无左规线） | §7 |
| Q16 三行去箭头 + CSV 同形状 | §6 |
| Q17 空例句不进校验器 | §12 |
| Q18 校准运行方式 | §13 |
| Q19 词表措辞 | `GLOSSARY.md` |
| 第二轮：例句与释义区分不够 → 三种材质（chip / 正文 / 衬线斜体 + 规线） | §7 |
| 第二轮：面板不再吃 markdown（`Body` 结构 + GTK 控件） | §6、§8 |
| 第二轮：CLI 的例句改成引文块，缩进不再被软换行吃掉 | §6 |
| 第二轮：例句不借用「原文」的纸色材质 | §7 |
