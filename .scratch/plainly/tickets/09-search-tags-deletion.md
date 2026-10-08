# 09: 搜索、标签、删除与清除

**What to build:** 我能按英文原文或某个表达搜到历史、自己打标签整理，也能删掉不想留的记录。

**Blocked by:** 08

**Spec:** [spec.md](../spec.md) §9 搜索与生命周期

**Status:** done (2026-10-08)

- [x] 两张 FTS5 表：一张索引 Passage + Comprehensible English，一张索引 Gloss 的 `expression` + `gloss`；查询取并集去重后按 `last_seen` 排序
- [x] `translation` 与 `grammar` 不进索引；「中文翻译 v0 不可搜」以**事实**形式出现在搜索位置，不被当成隐藏缺陷
- [x] 标签只做手动：可加、可删、可在列表按标签筛（自动打标刻意不做）
- [x] 单条**硬删**（不留软删坟场）；全量清除需要二次确认
- [x] 库**不加密**；删除与清除的语义在测试里可断言
- [x] `plainly history search <query>` 与 `history delete <id>` / `history clear` 可用

## 落地形态

- `plainly-core::store`（缝 A）：
  - **两张 FTS5 表**加在同一个库上：`record_fts(passage, comprehensible)`，一行一条 Record（`rowid` ＝
    `records.id`）；`gloss_fts(expression, gloss, record_id UNINDEXED)`，一行一条 Gloss。**故意不用
    external content**：它们自己存一份文本，于是重新生成就是 `DELETE` + `INSERT` 两条直白语句，而
    external content 要先把旧值喂回 FTS5 的 delete 命令、contentless 要 `contentless_delete`——
    两份副本换来的简单是划算的，因为库是本地单用户。
  - `translation` 与 `grammar` **不进任何一张表**。`translation` 是中文，而 tokenizer 是 per-table 的：
    要搜它得再来一张 trigram 表（升级路径是纯增量，v0 不做）。CLI 的搜索位置每次都明说这条事实。
  - `Store::search(query, tag)`：`record_fts MATCH ?` **UNION** `gloss_fts MATCH ?`，再
    `r.id IN (…)`，所以两表都命中的 Record 只出一行；`tag` 给定时再收窄到那条标签。排序与 `list()`
    相同（`last_seen DESC, id DESC, g.ord`）——`recall` 更新过 `last_seen` 的记录因此在搜索结果里也
    排到前面；`list()` 现在就是 `search("", None)`，两处各写一遍 ORDER BY 迟早会分叉。空查询＝**不加
    文本约束**（搜索框清空后的语义），`tag` 为空串/纯空白＝`StoreError::EmptyTag`（与存储标签同一条
    规则，`trim_tag()` 一处）。
  - 查询串由 `match_terms` 构造：**一个词一条 clause**，每个词加引号再挂 `*`，clause 之间 AND。**不能
    把整条查询拼成一个 `a AND b` 表达式**：`MATCH` 是**按表**求值的，那样等于要求 Passage 与 Gloss
    各自都含全部词，而"一个词在原文、一个词在 gloss"是最常见的查询（code review 抓到的真实缺陷，
    `the_terms_of_one_query_may_be_spread_across_the_fields` 就是它）。引号把 FTS5 的查询语言挡在
    搜索框之外（`-`、`*`、`NEAR(`、落单的 `"` 都是字面量；纯标点查询是"匹配不到"而不是报错，实测过），
    前缀是因为搜索框是边打边搜。词里的 `"` 用 FTS5 的写法翻倍转义。
  - 标签：`tags(record_id, tag)` 表，主键 `(record_id, tag)`，`ON DELETE CASCADE`。`Store::tag` /
    `untag` 返回 `StoreError::NoRecord`（id 不存在）或 `EmptyTag`（trim 之后什么都不剩），成功时**返回
    存下来的那一个标签**（`Result<&'a str, _>`，借用入参），所以 CLI 的确认说的是库里的事实而不是被敲
    进来的那串字符。标签在 **store 里 trim**（`trim_tag`），界面不必各写一遍。重复加同一条标签是成功
    （`INSERT OR IGNORE`），删一条本来就没有的标签也是成功（幂等），两者都是"事后的状态正是我要的"。
    领域词进 `GLOSSARY.md`（**Tag**，_Avoid_: label / category / keyword），代码里不再用 "label" 说它，
    免得和 `prompt_label` 撞车。
  - `Record.tags` 随每条记录读出来：`read_tags()` 单独一条语句（`read_by_id` 也走它，所以
    `show`/`remember` 的返回值带着标签）。**不做第三条 join**——两个一对多表同时 join 会乘行，
    一条三 Gloss 两标签的记录会回来六行。
  - 单条**硬删** `Store::delete(id)`：先删两张 FTS 表的行，再 `DELETE FROM records`（Glosses 与 tags
    被外键 CASCADE 带走，`foreign_keys` 已在 `prepare` 里打开）。**不留坟场**这件事是可断言的：SQLite
    会把删掉的 rowid 交给下一条 insert（没有 AUTOINCREMENT），所以残留的搜索行会让**下一段文字回答
    上一段文字的查询**——`a_deleted_record_leaves_nothing_for_the_next_record_to_inherit` 就是这么
    断言的，不靠读 SQL。
  - `Store::clear()` 返回删掉几条；两张 FTS 表整表清空（记录没了就没有 id 可瞄 `gloss_fts` 的行）。
  - `reindex()`：打开时若某张 FTS 表为空而它该有的东西非空，就从 `records` / `glosses` 重建。这是**给
    票据 08 写的库补索引**的升级路径，稳态只是两次 `count(*)`；两半各自判断，因为"一条 Gloss 也没有
    的历史"是正常的。测试用 `DROP TABLE record_fts` 模拟旧库，不复制 schema 文本。
  - `remember` 在同一条事务里重写两张 FTS 表，且 Gloss 的删+插在**同一个循环**里同时写 `glosses` 与
    `gloss_fts`：同一份数据只在一处维护。
- `plainly` CLI（缝 B）：
  - `history list [--tag <tag>]`（无子命令＝`list`）、`history search <query> [--tag <tag>]`、
    `history tag <id> <tag>` / `history untag <id> <tag>`、`history delete <id>`、
    `history clear --yes`。
  - 搜索位置（stderr）固定一行事实：`search covers the Passage, the Comprehensible English and the
    Glosses; the Translation is not searchable in v0`。空查询是用法错误（`history` 已经列全部，而它
    不假装搜过）。
  - 列表行在提示词版本之后多一列 `[tag …]`（行里藏标签会让 `--tag` 看起来什么都没匹配到），
    `show` 的 provenance 行末尾多一句 `, tagged a, b`。**带空格的标签要加引号**（`["to read"]`）：
    帮助文本说了"有空格就加引号"，那两行就得能把它如实说回去，否则一个标签看起来像两个。
  - 删除、清除、打标签都**不是产物**：stdout 什么都不出，人的信息走 stderr（§10 的 I/O 契约）。
    `delete`／`tag` 找不到 id → 退出码 2；`clear` 没有 `--yes` → 退出码 2 并说明怎么确认。**二次确认
    ＝命令 + `--yes` 两次动作**，拒绝发生在开库之前（拒绝的理由是关于这个请求的，不该取决于文件能不能
    打开）；`clear --yes` 报出删了几条。
  - `StoreError::NoRecord` / `EmptyTag` 映射到退出码 2（问错了，CLI 用自己的措辞说这两件事），其余
    store 失败仍是 1；`commands::no_record()` 的措辞与 `history show` 共用一处。

## 给下游的交接

- **tickets/10（导出）**：`Store::list()` / `show()` 的签名没变，只是每条多了 `Record.tags`；两张 FTS
  表与导出无关。
- **tickets/13/14/15（面板与主窗口）**：搜索框直接用 `Store::search("", tag)`（空查询＝全部，正是搜索框
  清空后的语义）与 `Store::search(query, tag)`；标签 chip 读 `Record.tags`、编辑走 `tag()` / `untag()`。
  **「中文翻译暂不可搜」那行字在桌面端要照抄**——v0 现在存在的搜索位置只有 CLI，UI 文案归票据 14 的
  strings 模块。`search` 是只读的、不需要 provider，面板可以随便调。
- **tickets/17（加固）**：`search` 与 `list` 一样没有分页也没有上限；多进程并发写在自动化测试之外，
  但 `delete`/`clear`/`tag` 都在事务里，与既有 `remember` 同级。

## 已知取舍

- **FTS 表是普通表，文本存两份**：换到的是重新生成时的删+插直白可读。代价是历史库里多一份英文文本的
  体积。
- **删除不擦除已释放的页**：SQLite 默认 `secure_delete=off`，`DELETE` 之后字节可能留在文件的空闲页里
  直到被复用或 vacuum。"硬删"在这里的确切含义是**表里没有墓碑行、列表与搜索里都没有、id 会被下一条
  复用**，不是"磁盘字节被抹掉"。库本来就不加密（§9），所以这不是新增的暴露面——写进这里是因为
  "不留坟场"这句话容易被读成后者。
- **空查询两种语义**：`Store::search("")` 是"匹配一切"（搜索框的语义），CLI 的 `history search ""`
  是用法错误（命令行的语义）。两处都写明了，不是漏掉的一处。
- **前缀匹配**：`comm` 命中 `committee`。这是边打边搜要的，代价是短词（`a`、`i`）几乎命中一切；排序按
  `last_seen` 而不是 FTS 的 rank，所以噪声由时间而不是相关度兜底。真需要相关度排序时，`ORDER BY rank`
  还要与"最近看过在前"的既有承诺商量。
- **没有"列出我用过的所有标签"**：v0 的 chip 是每条记录自己的标签，还没有需要全局清单的界面。真要做
  就是一条 `SELECT DISTINCT tag`。
- **`list --tag` / `search --tag` 只筛一条标签**：多条标签的与/或不是 v0 的问题，真需要时是 API 上
  从 `Option<&str>` 到 `&[&str]` 的改动。
