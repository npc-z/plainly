# 09: 搜索、标签、删除与清除

**What to build:** 我能按英文原文或某个表达搜到历史、自己打标签整理，也能删掉不想留的记录。

**Blocked by:** 08

**Spec:** [spec.md](../spec.md) §9 搜索与生命周期

**Status:** ready-for-agent

- [ ] 两张 FTS5 表：一张索引 Passage + Comprehensible English，一张索引 Gloss 的 `expression` + `gloss`；查询取并集去重后按 `last_seen` 排序
- [ ] `translation` 与 `grammar` 不进索引；「中文翻译 v0 不可搜」以**事实**形式出现在搜索位置，不被当成隐藏缺陷
- [ ] 标签只做手动：可加、可删、可在列表按标签筛（自动打标刻意不做）
- [ ] 单条**硬删**（不留软删坟场）；全量清除需要二次确认
- [ ] 库**不加密**；删除与清除的语义在测试里可断言
- [ ] `plainly history search <query>` 与 `history delete <id>` / `history clear` 可用
