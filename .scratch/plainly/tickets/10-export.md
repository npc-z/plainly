# 10: 导出三形态

**What to build:** 我能把一条复制成 markdown、把整库导出成单个 markdown 文件，或导成可直接进 Anki 的 CSV（每个 Gloss 一张卡）。

**Blocked by:** 08

**Spec:** [spec.md](../spec.md) §9 导出三形态

**Status:** ready-for-agent

- [ ] Markdown：单文件、按 Record 分节，每节标题含日期 + 等级 + provider / 模型 + 提示词标签，正文用五节渲染
- [ ] Anki CSV：**每个 Gloss 一行**，正面 = `expression`、背面 = `gloss`、上下文 = 该 Record 的 Passage，另附等级 / 母语 / provider / 模型 / 提示词标签列
- [ ] 原始 CSV：每个 Record 一行，Gloss 以 `expression → gloss` 多行文本放进单个单元格
- [ ] `plainly history export markdown|anki|raw` 把产物写到 stdout（stdout 只有产物）；空历史是合法输入
- [ ] 单条导出（一条 Record）与全局导出走同一套渲染，不各写一份
- [ ] **不做** `.apkg`
