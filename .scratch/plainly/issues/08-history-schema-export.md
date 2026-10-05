# 08 历史库 schema 与导出

Type: grilling
Status: resolved
Blocked by: 01

## Question

定下 **SQLite schema 与导出**。

- 存：原文、产物各字段、等级、母语、provider/模型、时间戳、标签、`schema_version`。
- 搜索：FTS5 全文索引（原文 + 改写 + gloss + 翻译都进索引？）。
- 导出目标：**Markdown / Anki（字段映射）/ CSV**——这是"不做 SRS"的兑现方式，字段映射要能直接进 Anki。
- 决定：一键清除与全量导出的语义；是否加密（倾向**不加密**，单用户本地库，收益低于忘记密码的代价）；库文件位置（XDG data dir）；原文是否总是保存（涉及隐私，倾向前端可见的清除开关）。

输入：票据 01 的 schema。

## Comments

- **来自票据 01 的字段约束**：一张 Record = Passage（原文）+ Explanation 四件套（`comprehensible` / `glosses[]` / `grammar`（可空）/ `translation`）+ 元数据（Level、Native Language、provider、模型、时间戳）+ 两个版本号 `artifact_version` 与 `prompt_version`。**wire schema 不落地**。
- 字段命名必须与仓库根 `GLOSSARY.md` 对齐（**Record** / **Explanation** / **Passage** / **Gloss** / **Blocker**），别再引入同义词。
- 注意：`glosses` 是对象数组 `[{expression, gloss}]`，不是字典——FTS5 索引要把 `expression` 与 `gloss` 都吃进去。
- 票据 01 已 resolved，**本票据的阻塞已解除**。
- **来自票据 15 的一条存储约束**：等级与母语**存的是"当时用的值"**（字符串），不是指向当前设置的外键。配置（`[app]`，15 的地盘）里存的是"现在的设置"；Record 里存的是"生成它时的设置"。这正是"改设置不会改变旧记录"能成立的机制——不要为了"省一列"改成外键，那会让改一次设置就悄悄改写全部历史。

## Answer

**五个决策 + 幂等键设计**，2026-10-05 全部按推荐采纳；幂等键由用户补充（"同一段文本优先取已落库数据"）并经三处收紧。

### 探测到的事实

- **FTS5 是现成的**：`libsqlite3-sys` 的 bundled 构建默认就带 `SQLITE_ENABLE_FTS5`（还带 JSON1 / RTREE / STAT4）——`rusqlite = { features = ["bundled"] }` 不需要额外开关。bundled 版本 3.53.2，`trigram`（3.34+）可用。
- **FTS5 的 tokenizer 是 per-table，不是 per-column**——这条直接决定了中文检索的设计（决策 3）。

### 1. Record 的存储形态：关系化

`records` + `glosses(record_id, ord, expression, gloss)`。理由：**Anki 导出的天然单位就是单个 gloss**（一条 SQL 出卡）；FTS5 可直接索引 `expression` / `gloss` 两列；`ord` 明确保序（票据 01：顺序有意义，从最挡路的开始解释）。JSON1 虽然可用，但会把"按表达检索"变成解析问题。

### 2. Anki 导出：每个 gloss 一张卡

正面 = `expression`，背面 = `gloss`，原句作上下文。理由：Anki 是用来**反复认搭配**的，最该反复看的正是 gloss 这一层；"整段重读"已由历史视图提供。按整条记录出卡以后要加只是换一个查询，不冲突。

### 3. FTS5 只索引英文

原文 / 改写 / `expression` / `gloss` 进索引；**中文翻译不进**。理由：tokenizer 是 per-table 的，所以"中文用 trigram、英文用 unicode61"**必须拆成两张表**，那不是一个搜索框能自然表达的东西（要按输入自动切表）。升级路径是**纯增量**的：以后加一张 trigram 表只索引 `translation`，不动主数据、无需迁移。**规格要写明"中文搜索 v0 不支持"这个事实**，而不是让它看起来像一个隐藏缺陷。

### 4. 导出形态

- **Markdown：单文件、按记录分节**（可 grep、可整段贴进笔记）。
- **Anki：CSV/TSV**（原生导入 + 字段映射）。**不做 `.apkg`**——那是带 schema 的 SQLite 包加 media 目录，等于实现半个 Anki 内核，而 Anki 本来就支持 CSV 字段映射。
- **原始 CSV**：给表格软件。

### 5. 生命周期

- **删除**：历史列表里可单条**硬删**；全量清除要二次确认。不留软删坟场。
- **加密**：**不加密**（单用户本地库，加密收益低于忘记密码的代价）。
- **标签**：v0 只做**手动**。自动打标要再加一次模型调用和一个新的失败面，而票据 14 刚证明**模型会自信地错**；标签只有被人 curation 才有价值。
- **原文必须总是入库**——已被票据 14 逼定（本地模式的输出必须能与原文核对，UI 才有意义）。

### 6. 幂等键：历史库兼任缓存

**不新建缓存层。**"优先取已落库的数据"与"查历史"是同一件事：一次命中就是一次查询，一次未命中就是一次生成 + 入库。这与票据 15 的原则一致——`appCacheDir` 只放**可重算**的（能力探测结果），而 Explanation 是**用户内容且生成有成本**，属于 data 库。

```sql
lookup_key TEXT UNIQUE   -- SHA-256 over the canonical tuple below
passage TEXT             -- 逐字存储，不归一化（原型第一条规则：原文权威）
level, source_language, native_language
provider, model, thinking
prompt_version, artifact_version
created_at, generated_at, last_seen
```

- **key 的元组** = 归一化后的 passage + level + source_language + native_language + prompt_version + provider + model + thinking。
- **归一化只做首尾空白与换行风格（CRLF→LF）**；内部空白保持有意义，原文逐字入库。
- **SHA-256**，不用 64 位哈希——碰撞意味着悄悄返回错的解释，这里的成本不对称。
- **命中复用同一行**（不插新记录）：更新 `last_seen`，**历史按 `last_seen` 排序**。
- **不记 `lookup_count`**（用户 2026-10-05）：它只服务于复习类功能，而 Out of scope 已明确不做 SRS；`last_seen` 就是"命中过"的唯一痕迹。
- **主键仍是整数 id**，`lookup_key` 上建 UNIQUE 索引——便于 `glosses` 外键。

**三处收紧（每处都有明确理由）：**

1. **key 必须含 provider profile（provider + model + thinking）。** 否则用户拿到烂答案后切到云端，**会从缓存里拿回同一个烂答案**——票据 14 刚证明本地 4B 会自信地把习语讲错。连带两条不可省：**UI 显示 provenance**（这条解释出自哪个模型）+ **"重新生成"绕开缓存**。有缓存就必然需要这个出口，它不是可选功能。
2. **命中复用而非新增**（见上）——否则同一句话查三次，历史里会出现三条一模一样的记录。
3. **key 含 `prompt_version`，不含 `artifact_version`。** 前者变了，答案该变（质量维度）；后者只是**形状**变了，放进 key 会让每次应用更新都碎片化整个缓存。代价：旧版本的 Explanation 可能缺新字段——所以 `artifact_version` 入库让渲染器知道形状，并提供"用当前版本重新生成"。

**源语言 v0 不检测**：plainly 的前提就是"难英语"，`source_language = "en"` 是配置常量；字段照样入库（溯源 + 未来多语言源），但**不做检测**——那要么加一次模型调用，要么分叉缓存。

**"重新生成"语义**：**原地覆盖** Explanation，更新 `generated_at`；`created_at` 与 `last_seen` 保留。

### 给下游的三条

- **票据 05**：CLI 要能**绕开缓存重新生成**（否则脚本里没有修正手段）。
- **票据 06**：面板要显示 **provenance**，并给"重新生成"入口；最现实的场景是"本地模型讲错了 → 一键换云端重生成"。
- **票据 09**：`prompt_version` 的纪律必须是**内容改了就必须升版本**——幂等键的正确性直接依赖它。

## Comments

- **✅ 2026-10-05 票据 09 结清时回填**：那条"升版本"的纪律**不再依赖人的自觉**——票据 09 把 `prompt_version` 定义成**有效提示词的内容哈希**（派生，录入时计算），于是"内容改了就必须升版本"变成了**结构保证**。本票据的 `lookup_key` 照旧包含它，无需改动。
  两点随之明确：
  1. 记录里**另存一个人类可读标签**（如 `v6-synthesis`）用于显示；哈希是"真的版本"，标签是"给人的名字"。
  2. **要接受一个后果**：提示词一改（**包括应用更新带来的出厂提示词变化**），hash 就变 → 同一段落会产生**新的一行**（旧行保留）。所以历史里同一段落可能有多条不同 prompt 版本的记录——这是**特性**（可以比较新旧提示词的效果），不是重复条目。UI 要能区分（归票据 16）。
