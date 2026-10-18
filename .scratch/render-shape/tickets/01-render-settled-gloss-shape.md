# 01: render: 让 core 拥有 Gloss 的已定型形状与空节 body

**What to build:** `plainly-core::render` 新增已定型的 `GlossView { expression, gloss, example: Option<String> }` 与唯一构造 `GlossView::of(&Gloss)`（`trim` 后非空才 `Some`，`Some` 里是 trim 后的文本）；`Body::Glosses` 从 `Vec<Gloss>` 改为 `Vec<GlossView>`；新增 `Body::Note(String)`，`sections()` 用它在调用方给文案时装空节说明；把 public `render::panel(passage, explanation, grammar_note, key_help_note)` 换成 `render::sections(passage, explanation, &EmptyNotes)`（`EmptyNotes { grammar: Option<&str>, key_help: Option<&str> }`，`silent()`），`markdown` 与 `render_with` 的行为不变。markdown、raw CSV、面板 `document::sections` 与 `body_widget` / `glosses_widget` / `gloss_widget` 全部改为消费 `Option` / `Note`；`lib.rs` 与 `Body` / `Section` / `SectionKind` 一起导出 `GlossView`、`EmptyNotes`。`Gloss`、wire schema、store、`ARTIFACT_VERSION` 不动；桌面本票不碰。

**Blocked by:** 无（可立即开始）。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §1、§2、§3

**Status:** ready-for-agent

- [ ] `GlossView` 在 `render.rs`，`GlossView::of` 是全仓库唯一的例句 `trim`；`""`、`"   "`、`"\n"` → `None`，`"  He went to ground.  "` → `Some("He went to ground.")`。
- [ ] `Body::Glosses(Vec<GlossView>)`；markdown / raw CSV / 面板 widget 三处不再出现 `example.trim()` 判断，改为 `Option`，输出逐字不变（除 trim 外）。
- [ ] `Body::Note(String)`：`sections()` 用 Note 装空的 Grammar / Key Help 的调用方文案；`markdown`（`silent`）仍省掉空节、不产生 Note；`render_with` 把 Note 当文本排。
- [ ] `render::panel` 删除；面板 `document.rs` 改为 `render::sections` + `EmptyNotes`，`.filter(prefs.shows)` 与复制路径不变；`markdown` 改用 `sections(..., &EmptyNotes::silent())`。
- [ ] `lib.rs` 与 `Section` / `Body` / `SectionKind` 一起导出 `GlossView`、`EmptyNotes`；`Gloss` / wire schema / store 未改。
- [ ] 面板 widget 既有 GTK 测试仍通过（Note 与 Prose 同材质），新增纯空白例句 → 两个 label 的用例（无 display 时按既有方式跳过并打印说明）。
- [ ] core 新测试：纯空白、前后空白、多行例句三种输入；面板复制文本的 Note 行为；CSV 三行 / 两行形状。
- [ ] `cargo test -p plainly-core -p plainly-cli`、`cargo clippy -p plainly-core -p plainly-cli --all-targets`、`nix develop -c cargo test -p plainly-panel` 全绿；桌面 crate 未改。
