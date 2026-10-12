# 01: `example` 进契约与存储

**What to build:** `Gloss` 增加必填的 `example`，wire schema 与 Rust 结构体同时改；`ARTIFACT_VERSION` 升到 2；`glosses` 表加列，并引入仓库第一个迁移步骤（`PRAGMA user_version` 0 → 1）。

**Blocked by:** 无。

**Spec:** [spec.md](../spec.md) §1、§2、§3

**Status:** done (2026-10-12)

- [x] `core/src/explanation.rs`：`Gloss { expression, gloss, example: String }`；`wire_schema()` 的 gloss item `required` 加 `example`、`properties` 加 `{"type":"string"}`；**不加任何长度 / 模式约束**（ADR-0001）。
- [x] `core/src/artifact.rs`：`ARTIFACT_VERSION = 2`，文档说明 v2 加了 Gloss 的 Example。
- [x] `core/src/store.rs`：`SCHEMA` 的 `glosses` 加 `example TEXT NOT NULL DEFAULT ''`；`remember` 的 insert 带 `example`；`READ` / `Raw` / `Raw::gloss` / `decode` 带 `example`。
- [x] `core/src/store.rs`：迁移步骤——`PRAGMA user_version` < 1 时，若 `PRAGMA table_info(glosses)` 里没有 `example` 就 `ALTER TABLE glosses ADD COLUMN example TEXT NOT NULL DEFAULT ''`，然后写 `user_version = 1`；两种库（全新 / v0）都只跑一次且可重复打开。
- [x] `gloss_fts` **不动**（例句不进索引，spec §9）。
- [x] 新增 store 测试：手工建 v0 形状的库 → 新版本打开不报错、`user_version` 变 1、旧行 `example == ""`；再打开一次（幂等）；新 Record 的 `example` 往返一字不差。
- [x] 全仓 fixture 更新（`core/tests/support/mod.rs` 等所有内联 glosses JSON）与 `core/tests/contract.rs` / `artifact.rs` / `explain.rs` 的版本断言。
- [x] `scripts/stub-provider.py` 补 `example`。
