# 08 历史库 schema 与导出

Type: grilling
Status: open
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
