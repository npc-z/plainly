# 04: 收尾：手测清单、预览与验收

**What to build:** 把这次结构决定写进面向维护者的文档，并证明「除例句 trim 外没有可见变化」。`docs/desktop-manual-checks.md` 的「详情与导出」逐条复核：五节顺序与标题、空 Grammar → 「本段不需要」、空 Key Help → 「本段没有需要解释的表达」、三行 Gloss 材质、旧记录两行、复制 markdown 的 bullet + 引文块；补一句维护者说明（详情由 core 的 sections 驱动，JS 不含顺序 / 空节 / 例句判断）。`docs/manual-checks.md` 的 CLI / 面板 / CSV 条目不因本次改动而变，复核措辞即可。可选：按 `.scratch/gloss-examples/ui/README.md` 的方法用新的 `detailView()` 重生成桌面预览图。跑 spec §7 的 grep 判据与全量验证，把结果、偏差与 `/code-review` 遗留问题记进本票 Comments。

**Blocked by:** 03。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §7、§Testing Decisions；ADR：[0005](../../../docs/adr/0005-the-rendered-shape-is-a-value-core-owns.md)

**Status:** done (2026-10-18)

- [x] 两份手测清单真实：桌面五节 / 空节文案 / 三行 Gloss / 旧记录两行 / 复制 markdown 逐项可勾；没有声称未实现的行为。
- [x] spec §7 grep 判据全部成立（例句判空唯一、`render::panel` 消失、`DetailView` 无扁平 Explanation 字段、app.js 详情路径无旧文案键）。
- [x] `nix develop -c cargo test --workspace`（面板 GTK 测试在 display 下跑）与 `cargo clippy --workspace --all-targets` 全绿；`nix build .` 的 checkPhase 通过。
- [ ] 桌面手测在真实窗口跑一遍：五节顺序与空白、Grammar / Key Help 空节材质、三行 Gloss、旧记录（`记录格式 v1`）两行、复制 markdown 与 CLI 一致；任何额外视觉差异修回或记录为有意变化。
      **Note:** 这一步需要人工在真实窗口里看；本轮用真实 `detailView()` + 真实 `style.css` 的 headless 渲染对照旧预览，视觉差异已修回（见 Comments），但真机路径未由实现者代替人勾。
- [ ] 预览图若重生成，文件在 `.scratch/render-shape/ui/` 并注明生成方法；若不重生成，在 Comments 说明原因。
- [ ] 按流程跑 `/code-review`，findings 与修复（或拒绝理由）记在 Comments。
- [x] `git status` 除本 feature 的 spec / ADR / tickets / 预览与实现改动外干净；架构评审 HTML 仍不入库。

## Comments

**文档**

- `docs/desktop-manual-checks.md`「详情与导出」补了维护者说明：五节顺序、空节文案与例句有无由 core 的 `sections` 决定，前端只按 `kind` / `body.type` 选材质（ADR-0005）；并修正空节文案那句（Grammar 与 Key Help 各一句，不是三句）。
- `docs/manual-checks.md` 的 CLI / 面板 / CSV 条目本轮无需改动：CLI 的 markdown、raw CSV 的三行形状、面板的控件树都有测试与既有手测条目覆盖，唯一可见变化（例句 trim）不改变那些 fixture 的输出。

**spec §7 grep 验收（工作树根）**

| 判据 | 结果 |
| --- | --- |
| `grep -rn "example.trim\|example.is_empty" crates/` | 只剩 `crates/plainly-core/src/render.rs:100,104`（`GlossView::of`），四个旧站点与桌面测试旧断言都消失 |
| `grep -rn "render::panel" crates/` | 0 处；`render::sections` + `EmptyNotes` 是唯一入口 |
| `app.js` 详情路径的 `t.detail_*` / `t.glosses_empty` / `t.grammar_not_needed` | 0 处（这些字段仍由 Rust 生成 sections 时使用） |
| `DetailView` 的扁平 Explanation 字段 | 已删；`serde_json` 契约测试断言 `wire.get("passage" / "comprehensible" / "glosses" / "grammar" / "translation")` 全为 `None` |
| `Body::Prose` 承载空节说明 | 不再发生；core 测试断言 `Body::Note`，文档路径（`silent`）不产生 Note |

**验证**

- `cargo test --workspace`（flake）：**613 passed / 0 failed / 1 ignored**（`keyring_live` 需要真实 keyring）；面板 GTK 测试在本机 display 下实际执行（71 passed，含新增 whitespace-only 控件树用例）。
- `cargo clippy --workspace --all-targets`：仅 4 条既有 warning（`cli` 的 1 条 `unnecessary_find_map`、3 条 `collapsible_if`），未新增。
- `nix build .`：exit 0；GUI derivation 的 checkPhase 在改动后重跑通过（`cargo test --release --locked`）。
- 视觉证据：[`ui/desktop-detail.png`](../ui/desktop-detail.png) 由真实 `bodyView` / `detailView` + 真实 `style.css` + `firefox --headless` 渲染，方法见 [`ui/README.md`](../ui/README.md)；与 `.scratch/gloss-examples/ui/desktop-detail.png` 对照，节顺序、标题材质、Original 纸色块、三行 Gloss、Grammar 空节斜体、Translation 颜色与间距一致。

**code review（两轴并跑）与修复**

- Standards（无硬违规，5 条 judgement）：`view.rs` 的两个 `SectionKind` match 合成一个 `section_identity` 表；core 测试里重复的 Explanation fixture 收进 `tests/support::shaped_examples`（document / CSV / panel 共用）；`spoken` 改名 `sections_with_notes`；`view.rs` 的 “Adjacently tagged” 改为 “Internally tagged”；ticket 03/04 的状态与勾选补齐；手测清单的“三句”改为“两句”。
- Spec（4 条）：补 `"\n"`-only 的例句用例（现在与 `"   "` 一起由 `shaped_examples` 覆盖）与桌面 wire 的前后空白 trim 用例；修 `.sec > .prose { margin: 0 }` 的 scope creep —— margin reset 收窄到 `[data-kind="original"] / [data-kind="grammar"]`，Translation 的 UA 下边距恢复，间距 parity 回到旧预览；预览脚本改为从自身目录写 `detail.html`，README 命令可复现。
- 拒绝：无；所有 findings 都改了，改完重跑 workspace 测试、clippy 与 `nix build`。

**caveman review（第二轮，6 条全部修复）**

- `app.js:L284`：`bodyView` 对未知 `body.type` 显式 `throw new Error(...)`，不再落进 Glosses 分支抛 `undefined.map`。
- `window.rs` 的 wire 契约测试补了 `include_str!("../ui/style.css")` 的 kind→材质配对断言（original / comprehensible-english / grammar / translation；Key Help 的材质在 `.gl` 上），CSS 与 wire 的 kind 名不能再单边漂移。
- 去掉 `wire["markdown"] == detail.markdown` 的自反断言，改为断言 `markdown` 字段仍在且以文档标题开头。
- `render.rs` 模块文档的「said on the panel」改为「said on a surface read in place」；`tests/support` 的 fixture 注释不再声称 panel 使用它。
- `window.rs` 抽出 `secrets_with(keys)`，`window_with_env` 与 `test_secrets` 共用。
- 修后：workspace **613 passed / 0 failed / 1 ignored**；clippy 仍只有 4 条既有 warning；`nix build .` 重跑 exit 0。

**偏差**

- ticket 02 按计划先加 `sections`、保留扁平字段，ticket 03 切 JS 的同时删字段；`app.js` 在 02 有一行 `example.trim()` → `example` 的过渡修改（已在 02 Comments 记录），避免 02 与 03 之间界面坏掉。
- 本 feature 的实现与文档**未提交**：用户要求完成后不提交代码；工作树即交付物。
