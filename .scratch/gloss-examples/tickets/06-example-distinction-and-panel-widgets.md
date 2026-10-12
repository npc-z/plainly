# 06: 例句与释义的视觉区分；面板不再显示 markdown

**What to build:** 真机看过之后的两处返工：桌面卡片里"释义"和"例句"区分不够；面板把 core 交来的 markdown 原样打了出
来（`-`、反引号、`_`），CLI 上 `glow` 又把两行缩进合成了一段。根因是渲染器把**文档**交给了不是文档的表面——所以 core 交出的改成**结构**（`Body::Prose` / `Body::Glosses`），每个表面自己排。

**Blocked by:** 01、02、03、04。

**Spec:** [spec.md](../spec.md) §6、§7、§8

**Status:** done (2026-10-12)

- [x] `core/src/render.rs`：`Section.body` 从 `String` 改成 `Body`；文档面把 `Glosses` 排成 bullet + 释义行 + **嵌套引文块**，条目之间空行；空例句只有前两行；`Body` 从 `lib.rs` 导出。
- [x] `panel/src/panel.rs`：`body_widget` 按 body 分派；`glosses_widget` / `gloss_widget` / `example_widget` 把一条 Gloss 画成 chip + 正文 + 规线引文，**没有任何 markdown 记号**。
- [x] `panel/src/css.rs`：`.plainly-expression-chip` / `.plainly-expression` / `.plainly-example-rule` / `.plainly-example`（衬线斜体、降低不透明度）。
- [x] 桌面 `ui/style.css`：释义改用 `var(--ink)`；例句衬线斜体 + 12.5px + `var(--dim)` + 左侧 `2px solid var(--quote)` 规线 + 再缩进；条间距 16px。**不借用「原文」的纸色块**。
- [x] `panel.rs` 新测试 `the_key_help_reaches_the_screen_without_markup`：CSS 能解析 + 控件树恰好五个 label（两条 Gloss，其中一条没有例句）。需要 display，没有时说明并跳过。
- [x] 全部钉住旧形状的测试跟着改（核心黄金文本、导出、CLI、面板复制）。
- [x] 真实预览：桌面卡片用 `firefox --headless` 渲染 `ui/style.css` + `app.js` 的 `detailView` 输出；面板用 `gtk::OffscreenWindow` 渲染同一棵控件树到 PNG。两张图与做法存 [`ui/`](../ui/README.md)（面板那次是临时 `#[ignore]` 测试，看完即删）。
