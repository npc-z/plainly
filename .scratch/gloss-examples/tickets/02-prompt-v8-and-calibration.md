# 02: 提示词 v8 与 cloud 校准

**What to build:** 出厂提示词增加 `example` 的定义与规则，`PROMPT_LABEL` 升到 `v8-examples`；bench 提示词与 schema 同步，cloud 跑完并记进 FINDINGS。

**Blocked by:** 无（与 01 可并行；`bench/schema.json` 是它自己那份契约副本）。

**Spec:** [spec.md](../spec.md) §4、§5、§12、§13

**Status:** done (2026-10-12)

- [x] `core/src/prompt.rs`：`PROMPT_LABEL = "v8-examples"`。
- [x] `core/src/prompt.rs`：`"glosses"` 那条改成"释义 + 例句"，写进 JSON 形状；新增 **Rules for an "example"**（新句 / Level 用词 / 上下文能读出意思 / 允许屈折 / 不许为空）；「不要编造原文没有的细节」显式限定作用域（管改写、释义、翻译；例句可以有自己的主语与情境）。
- [x] 改动必须与 `.scratch/plainly/bench/prompts/v8-examples.txt` **逐字一致**（那边是先跑的版本）。
- [x] `core/tests/prompt.rs`：`the_prompt_asks_for_json_and_names_every_field_in_words` 加 `example`；toggles 测试的字段表跟着改。
- [x] `bench/schema.json` 加必填 `example`；`bench.py` 的 `validate` 要求三个键且非空。
- [x] 跑 `BENCH_PROVIDER=deepseek BENCH_MODEL=deepseek-flash BENCH_PROMPT=v8-examples BENCH_EXTRA='{"thinking":{"type":"disabled"}}`，12 段 ×（B2 默认 + A2）。
- [x] 结果与样本写进 `bench/FINDINGS.md` 的 "v8-examples" 一节，含"本地 4B 未测 = 已知风险"。
