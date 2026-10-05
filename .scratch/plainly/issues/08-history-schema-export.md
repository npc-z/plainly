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
