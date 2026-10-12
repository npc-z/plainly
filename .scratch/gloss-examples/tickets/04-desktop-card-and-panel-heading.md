# 04: 桌面卡片与面板标题

> 版式部分（三行、间距、只靠斜体区分）已由 [06](06-example-distinction-and-panel-widgets.md) 取代；
> 这一张只剩"标题统一成 关键帮助"和 `GlossView` 带 `example` 两条还成立。

**What to build:** 桌面详情卡的关键帮助每条三行（chip / 缩进释义 / 缩进斜体变暗例句），条间距 12px；`GlossView` 带 `example`；面板那一节的标题从 `关键词` 统一成 `关键帮助`。

**Blocked by:** 01。

**Spec:** [spec.md](../spec.md) §7、§8、§10、§11

**Status:** done (2026-10-12)

- [x] `desktop/src/view.rs`：`GlossView` 加 `example: String` 并在 `DetailView::of` 里填。
- [x] `desktop/ui/app.js` 的 `detailView`：每条渲染 `div.gl > div.ex(code) + div.gloss + div.eg`；`example` 为空时不渲染 `div.eg`；表达式与例句都过 `esc()`。
- [x] `desktop/ui/style.css`：`.gl { margin: 0 0 12px }`；`.gl .gloss, .gl .eg { margin: 4px 0 0 14px }`；`.gl .eg { color: var(--dim); font-style: italic }`；chip 样式不变；**不加左规线**。
- [x] 不新增字符串、不新增设置开关（例句跟着 `show_glosses` 走，桌面详情卡本来也不读这些开关）。
- [x] `panel/src/strings.rs`：`SectionKind::KeyHelp => "关键帮助"`（`panel/src/document.rs` 的复制断言同步）。
- [x] `desktop/tests/window.rs`：断言 `DetailView.glosses[].example` 与 `markdown` 一致。
- [ ] 面板高度实看一眼：470px 宽下关键帮助展开后仍可用（记进 `docs/desktop-manual-checks.md`）。

**Note:** 上面留着一个未勾的步骤是给人做的真机目视项，已经写进 `docs/desktop-manual-checks.md` / `docs/manual-checks.md`，本次没有 GUI 会话去跑。
