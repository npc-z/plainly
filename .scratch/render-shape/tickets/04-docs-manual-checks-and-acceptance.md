# 04: 收尾：手测清单、预览与验收

**What to build:** 把这次结构决定写进面向维护者的文档，并证明「除例句 trim 外没有可见变化」。`docs/desktop-manual-checks.md` 的「详情与导出」逐条复核：五节顺序与标题、空 Grammar → 「本段不需要」、空 Key Help → 「本段没有需要解释的表达」、三行 Gloss 材质、旧记录两行、复制 markdown 的 bullet + 引文块；补一句维护者说明（详情由 core 的 sections 驱动，JS 不含顺序 / 空节 / 例句判断）。`docs/manual-checks.md` 的 CLI / 面板 / CSV 条目不因本次改动而变，复核措辞即可。可选：按 `.scratch/gloss-examples/ui/README.md` 的方法用新的 `detailView()` 重生成桌面预览图。跑 spec §7 的 grep 判据与全量验证，把结果、偏差与 `/code-review` 遗留问题记进本票 Comments。

**Blocked by:** 03。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §7、§Testing Decisions；ADR：[0005](../../../docs/adr/0005-the-rendered-shape-is-a-value-core-owns.md)

**Status:** ready-for-agent

- [ ] 两份手测清单真实：桌面五节 / 空节文案 / 三行 Gloss / 旧记录两行 / 复制 markdown 逐项可勾；没有声称未实现的行为。
- [ ] spec §7 grep 判据全部成立（例句判空唯一、`render::panel` 消失、`DetailView` 无扁平 Explanation 字段、app.js 详情路径无旧文案键）。
- [ ] `nix develop -c cargo test --workspace`（面板 GTK 测试在 display 下跑）与 `cargo clippy --workspace --all-targets` 全绿；`nix build .` 的 checkPhase 通过。
- [ ] 桌面手测在真实窗口跑一遍：五节顺序与空白、Grammar / Key Help 空节材质、三行 Gloss、旧记录（`记录格式 v1`）两行、复制 markdown 与 CLI 一致；任何额外视觉差异修回或记录为有意变化。
- [ ] 预览图若重生成，文件在 `.scratch/render-shape/ui/` 并注明生成方法；若不重生成，在 Comments 说明原因。
- [ ] 按流程跑 `/code-review`，findings 与修复（或拒绝理由）记在 Comments。
- [ ] `git status` 除本 feature 的 spec / ADR / tickets 与实现改动外干净；架构评审 HTML 仍不入库。
