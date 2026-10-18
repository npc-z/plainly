# 02: desktop: 把 core 的 sections 发到窗口

**What to build:** `crates/plainly-desktop/src/view.rs` 新增 wire DTO：`SectionView { kind: &'static str, title: &'static str, body: SectionBodyView }`，`SectionBodyView::{Prose { text }, Note { text }, Glosses { glosses }}`（`#[serde(tag = "type", rename_all = "snake_case")]`），以及 `GlossView { expression, gloss, example: Option<String> }`（由 core 的 `render::GlossView` 转来）。`DetailView` **新增** `sections`，暂时保留 `passage` / `comprehensible` / `glosses` / `grammar` / `translation` 与 `markdown`（过渡：JS 在 03 才切过来，本票结束时界面仍然可用）。`DetailView::of` 调 `render::sections`，传入 `EmptyNotes { grammar: Some(STRINGS.grammar_not_needed), key_help: Some(STRINGS.glosses_empty) }`，映射成 `SectionView`；`title` 由 `kind` 从 `strings.rs` 的 `detail_*` 选；kind 为 kebab-case 字符串，`example` 显式为 `null`，字段名沿用 snake_case。

**Blocked by:** 01。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §4、§6

**Status:** ready-for-agent

- [ ] `SectionView` / `SectionBodyView` / `GlossView` 在 `view.rs`，core 的 render 类型仍无 serde；`DetailView` 有 `sections`。
- [ ] sections 由 `render::sections` + `EmptyNotes` 生成：五节顺序来自 core，空 Grammar / Key Help 的文案来自 `strings.rs`，kind 五值正确。
- [ ] `example` 是 `Option<String>`：空 / 纯空白 → `null`，前后空白 → trim 后的句子；JS 此刻仍读旧的扁平字段，所以两种表示暂时并存、不冲突。
- [ ] `DetailView.markdown` 仍等于 `render::markdown(record)`（文档政策不变，空节仍省略）。
- [ ] `window.rs` 的 detail 测试新增 sections 断言：kind 顺序（original → comprehensible-english → key-help → grammar → translation）、空节 `Note` 文本、`example` 的 `Some` / `None`。
- [ ] 新增 `serde_json::to_value(&detail)` 契约测试：`sections[].body.type` 为 `prose` / `note` / `glosses`，`example: null`，字段名 snake_case，kind 拼写固定。
- [ ] `nix develop -c cargo test -p plainly-desktop`、`cargo clippy -p plainly-desktop --all-targets` 全绿；`ui/app.js` 与 `style.css` 未改。
