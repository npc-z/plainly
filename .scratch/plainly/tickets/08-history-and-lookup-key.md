# 08: 历史库与 Lookup Key 幂等

**What to build:** 每次解释自动落进本地历史，同一段文本第二次查询零成本拿回同一条——而换了模型、换了等级、改了提示词之后，不会再命中旧的烂答案。

**Blocked by:** 07

**Spec:** [spec.md](../spec.md) §9 历史库、Lookup Key 与导出

**Status:** ready-for-agent

- [ ] SQLite 以 WAL 打开（多进程并发读 + 串行写），`records` + `glosses(record_id, ord, expression, gloss)` 关系化存储；原文逐字入库、不归一化
- [ ] Lookup Key 元组 = 归一化(Passage) + level + source_language + native_language + prompt_version + provider + model + thinking；用 SHA-256；`lookup_key` 上建 UNIQUE
- [ ] 归一化只做首尾空白与 CRLF→LF；内部空白变化产生新 key；**`artifact_version` 不进 key**
- [ ] 命中**复用同一行**（不插新记录）并更新 `last_seen`；历史按 `last_seen` 排序；不记 `lookup_count`
- [ ] `--regenerate` 绕开命中并**原地覆盖** Explanation / 更新 `generated_at`，保留 `created_at` 与 `last_seen`
- [ ] `source_language` 是常量 `"en"` 并照样入库，**不做检测**
- [ ] `plainly history list` 与 `history show <id>` 可用，每条带 provenance（provider / 模型 / thinking / 提示词版本）
