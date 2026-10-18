# 03: desktop: 让窗口只画 sections

**What to build:** `ui/app.js` 的 `detailView` 删掉五节硬编码，改为遍历 `record.sections`：`<h3>` 用 Rust 给的 `title`，body 按 `body.type` 画 `prose` / `note` / `glosses`；`note` 的材质按 kind 保留（Grammar → `.notneeded`，Key Help → `.note`）；例句只判 `gloss.example` 是否为 `null`，不再 `trim`。JS 不再使用 `t.detail_*` / `t.glosses_empty` / `t.grammar_not_needed` 画详情（这些字段仍由 Rust 用于生成 sections）。`ui/style.css` 把 `.orig` / `.comp` / `.zh` / `.grammar` 迁到 `[data-kind] .prose` 选择器，`.gl` / `.gl .gloss` / `.gl .eg` / `.note` / `.notneeded` 保持不变，达到像素级等价。同时 `view.rs` 删除 `DetailView` 的五个扁平字段（`passage` / `comprehensible` / `glosses` / `grammar` / `translation`），只剩 metadata + `markdown` + `sections`，并把 `window.rs` 的 detail 测试改写成只断言 sections 与 markdown（不再从扁平字段重建 `Explanation`）。其余 JS（h2 level、meta、provenance、local、actions、tags、未选中提示）不动；HTML 仍只在 JS 拼。

**Blocked by:** 02。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §4、§5、§6

**Status:** ready-for-agent

- [ ] `detailView` 里没有五节顺序、空节判断或例句 `trim`；`record.sections.map(...)` 顺序即渲染顺序。
- [ ] `prose` / `note` / `glosses` 三种 body 的 DOM 与今天等价；`[data-kind]` 选择器承载原 `.orig` / `.comp` / `.zh` / `.grammar` 材质；没有例句（`null`）时不画 `.eg`。
- [ ] `DetailView` 只剩 metadata + `markdown` + `sections`；`serde_json` 契约测试与 `window.rs` detail 测试改写后全绿。
- [ ] 验收 grep 成立：`crates/` 内 `example.trim` / `example.is_empty` 只在 `render.rs` 的 `GlossView::of`；`render::panel` 0 处；`app.js` 详情路径无 `t.detail_` / `t.glosses_empty` / `t.grammar_not_needed`。
- [ ] `nix develop -c cargo test -p plainly-desktop`、`cargo clippy -p plainly-desktop --all-targets`、`cargo build -p plainly-desktop`（资产内嵌）全绿。
- [ ] 视觉 parity 不在本票做自动化断言，留给 04 的手测清单（本票不引入 JS 测试）。**Note:** 未勾的视觉项在 04 完成。
