# 03: 三行形状进每一个渲染器

**What to build:** 关键帮助一节在 core 的渲染器里从 `- \`expr\` → gloss` 一行变成三行（表达 / 释义 / 例句），去掉箭头；raw CSV 同形状但不带 `_`；空例句只出两行。

**Blocked by:** 01（渲染器要有 `example` 才谈得上形状）。

**Spec:** [spec.md](../spec.md) §6

**Status:** done (2026-10-12)

- [x] `core/src/render.rs` 的 `key_help`：

  ```
  - `organically`
    by itself, in a natural way, without being planned or forced
    _She picked up Spanish organically while working in Madrid._
  ```

  例句为空时只出前两行；`- ` 保留，箭头去掉。
- [x] `core/src/export.rs` 的 `glosses_text`：每条 Gloss 三行、无箭头、**无 `_`**（CSV 单元格不是文档）；引号 / 逗号 / 换行的转义规则不变。
- [x] `core/tests/render.rs` 的黄金文本、`core/tests/export.rs` 的 markdown 与 CSV 断言跟着改，并新增"空例句只出两行"。
- [x] 下游同步：`cli/tests/explain.rs`、`cli/tests/history.rs`、`panel/src/document.rs` 的断言。
