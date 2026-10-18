# Render shape 规格：让 core 拥有 Explanation 的渲染形态，桌面详情退成 dumb renderer

Status: ready-for-agent
Labels: ready-for-agent

> 由 2026-10-18 的两轮 grilling 坍缩而成（13 个决策，见文末溯源表；架构评审候选 3「渲染形态没有 owner：空 Example / 节规则有四个实现」）。结构取舍见 [ADR-0005](../../docs/adr/0005-the-rendered-shape-is-a-value-core-owns.md)。
> 领域词按仓库根 `GLOSSARY.md`；本规格不新增 domain 词条——`Section` / `Body` / `GlossView` / `EmptyNotes` 是代码名，不是学习者领域的词。
> 不改变 **Explanation** 的契约与存储形状：ADR-0001 仍是硬约束，ADR-0002 的可见规则（"no Example, no Example line"）不变；`ARTIFACT_VERSION`、store schema 与 FTS 都不动。
> 可感知的输出变化只有一处，且正常数据下不可见：例句首尾空白在渲染前被 trim（见 Implementation Decisions §1、Further Notes 的变化清单）。

## Problem Statement

**ADR-0002 把「no Example, no Example line」定为可见规则，但规则没有 owner。** 四个渲染器各自用 `!example.trim().is_empty()` 重判：core 的 markdown（`render.rs:239`）、raw CSV（`export.rs:154`）、面板 widget（`panel.rs:1450`）、桌面卡片（`app.js:284`）。规则只以散文形式存在于 ADR-0002 与 `store.rs` 的注释里；`Gloss` 上没有方法、`Body` 上没有类型、`Section` 上没有字段能表达「这条 Gloss 没有例句」。空白与纯空白的模型输出没有被任何测试钉住——四处判断今天碰巧一致，但没有任何机制保证它们继续一致。

**节规则有两个半实现。** core 的 `sections()` 已经拥有节集合、顺序与空节政策：文档（markdown）省掉空的 Grammar / Key Help，面板保留它们并用调用方给的句子说明。桌面详情却完全绕过 `render`：`app.js` 自己硬编码五节顺序与空节规则，`view.rs` 把 Explanation 拍平成五个字段（`passage` / `comprehensible` / `glosses` / `grammar` / `translation`），`GlossView.example` 还是 `String`，注释明说「让前端决定」。桌面是唯一一个渲染规则住在 core 与 CI 之外的表面。

**代价已经发生过一次。** `832efcb`（Gloss 增加 Example）改动了 core 15 个文件 +588/−85，外加 CLI、面板、桌面与 `app.js`；下一次字段或规则变化仍会沿四份实现散开。这不是「某个 module 太浅」，而是「一条规则没有 owner」——删掉任意一处判断，规则仍在另外三处且不会集中。

**更深一层：结构交给了表面，但结构里没有「已定型」的部件。** `Body::Glosses(Vec<Gloss>)` 把原始 `Gloss`（`example: String`）交给每个表面；面板的 widget 层连「空节说明」与「Explanation 自己的一段正文」都分不出（两者都是 `Body::Prose`），桌面则靠 JS 分。

## Solution

把「一条 Explanation 的渲染形态」收进 core 的 `render` module：

- **`GlossView`**：一个 Gloss 的已定型形态——`expression`、`gloss`，以及**已判定的** `example: Option<String>`（`trim` 后非空才 `Some`）。`GlossView::of(&Gloss)` 是「有没有例句、显示什么文本」的唯一实现；`Body::Glosses(Vec<GlossView>)` 让四个消费者都消费 `Option`，而不是重判字符串。
- **`Body::Note(String)`**：模型说某个节为空时，表面补的那一句话成为独立 body 种类，不再是 `Prose`；文档排成文本（与今天一致），桌面保留今天的两种材质。
- **`render::sections(passage, explanation, &EmptyNotes)`**：读一次、在原位看的表面（面板、桌面详情）唯一的分节入口；`EmptyNotes` 带着两句空节文案。`markdown` 用 `EmptyNotes::silent()`，文档政策（省掉空节）不变。
- **桌面详情拿数据，不拿判断**：`DetailView` 用 `sections` 取代五个扁平字段；`app.js` 遍历数组画 `h3` 标题与 `prose` / `note` / `glosses` 三种 body，`kind` 只用来挂 `data-kind` 材质；节顺序、空节政策、「有没有例句」三件事里没有一件留在 JS。

四个消费者（markdown、CSV、面板 widget、桌面卡片）的可见输出保持不变，唯一例外是例句首尾空白被 trim。契约、存储、`ARTIFACT_VERSION`、FTS、面板与 CLI 的行为都不动。

## User Stories

### 读到一条 Gloss

1. 作为学习者，我想要没有例句的旧 Record 仍然不画例句行（不是空行，也不是假例句），以便历史读起来和今天一样。
2. 作为学习者，我想要有例句的 Gloss 仍然画三块（表达 / 释义 / 例句），以便形态不变。
3. 作为学习者，我想要桌面卡片的五节顺序、标题、空节说明与今天一致，以便这次重构不需要重新学习界面。

### 维护者：一条规则一个地方

4. 作为维护者，我想要「有没有例句」只有一个实现，以便四个渲染器不可能再判出不同答案。
5. 作为维护者，我想要「空节说什么」由表面给文案、由 core 给结构，以便面板与桌面的措辞各自保留而判断只有一次。
6. 作为维护者，我想要空白与纯空白的例句各有一条 core 测试，以便模型留白的行为不是靠「碰巧一致」。
7. 作为维护者，我想要例句首尾空白在形态构造时被 trim，以便「有没有」与「画什么」是同一个决定。
8. 作为维护者，我想要节顺序与空节政策只由 core 的 `sections()` 决定，以便桌面不再有第二份。
9. 作为维护者，我想要桌面详情的 wire 有契约测试，以便改 `sections` 形状时测试会失败，而不是靠手测发现。

### 未来实现者 / 第五个表面

10. 作为未来实现者，我想要 core 里有一个小 interface（`Section` / `Body` / `GlossView` / `sections()`）可以读，以便不打开 GTK、WebKit 或窗口就能改动渲染语义。
11. 作为未来表面作者，我想要直接消费 `Section` 列表就能得到正确顺序与空节政策，以便不用第三次实现它们。
12. 作为未来实现者，我想要 `Body::Note` 区分「模型说这里空」与「Explanation 的正文」，以便新表面不会把两句说明画成内容。
13. 作为维护者，我想要 `render::panel` 这个名字被表面中性的 `render::sections` + `EmptyNotes` 取代，以便桌面调用它时不名不副实。

### CLI、面板、桌面不变

14. 作为 CLI 用户，我想要 `plainly` / `history show` / 两种导出的 markdown 形状逐字不变（除 trim 外），以便管道继续可用。
15. 作为面板用户，我想要关键帮助的控件树、复制文本、空节文案与节开关都不变，以便浮层读起来一样。
16. 作为导出用户，我想要 raw CSV 的 `glosses` 单元格仍是三行、无 markdown 记号，以便电子表格读法不变。
17. 作为桌面用户，我想要「复制 markdown」与「导出这一条」仍走 core 的唯一文档渲染器，以便副本与 CLI / 面板一致。
18. 作为本地 / 云端两种模型的用户，我想要这次改动不碰 provider、prompt、缓存或运行路径，以便解释结果不变。

## Implementation Decisions

### 1. core 的已定型 `GlossView`

- `render.rs` 新增：

  ```rust
  pub struct GlossView {
      pub expression: String,
      pub gloss: String,
      pub example: Option<String>,
  }

  impl GlossView {
      pub fn of(gloss: &Gloss) -> Self { /* trim 一次；空则 None */ }
  }
  ```

  derive `Debug` / `Clone` / `PartialEq` / `Eq`（与 `Body` 一致）；从 `lib.rs` 与 `Body` / `Section` / `SectionKind` 一起 re-export。
- 构造规则：`let example = gloss.example.trim();` → `example: (!example.is_empty()).then(|| example.to_owned())`。这是 ADR-0002 可见规则的唯一实现；`trim` 在这里只出现一次。
- `Body::Glosses(Vec<GlossView>)` 取代 `Vec<Gloss>`；下列消费者改为消费 `Option`：
  - markdown：`if let Some(example) = &gloss.example`，仍按行加 `  > `（多行例句的处理不变）；
  - raw CSV：`if let Some(example) = &gloss.example`，仍缩进三行、不带 `>`；
  - 面板 widget：`if let Some(example) = &gloss.example`，仍画 chip + 正文 + 规线引文；
  - 桌面：wire 上是 `Option<String>`，JS 用 `gloss.example ? … : ""`，不再 trim。
- `Gloss` 本身不动：wire schema 仍 `required: ["expression","gloss","example"]`；store 旧行的 `""`（ADR-0002 的迁移）在这里变成 `None`；不加语义校验、不加 schema 边界、`ARTIFACT_VERSION` 不变。
- `expression` 与 `gloss` 不做 trim / 归一；多行处理仍归各表面（Out of Scope）。

### 2. `Body::Note`

- `pub enum Body { Prose(String), Note(String), Glosses(Vec<GlossView>) }`。
- `sections()` 在调用方给了空节文案时，用 `Body::Note` 装 Grammar / Key Help 的说明句；`markdown` 传 `EmptyNotes::silent()`，文档仍然省掉空节，因此文档路径不会产生 `Note`。
- `as_document` / `render_with` 把 `Note` 当文本排（面板复制路径今天就是如此：`.scratch/gloss-examples` 的 `the_two_empty_sections_are_said_in_the_copied_text_too` 继续成立）。
- 面板 `body_widget`：`Note` 与 `Prose` 一样渲染为 label（本次不改面板外观）。
- `Body::Note` 是代码名，不进入 `GLOSSARY.md`。

### 3. `render::sections` 与 `EmptyNotes`

- public API：

  ```rust
  pub struct EmptyNotes<'a> {
      pub grammar: Option<&'a str>,
      pub key_help: Option<&'a str>,
  }
  impl EmptyNotes<'_> { pub fn silent() -> Self { /* 两个 None */ } }
  pub fn sections(passage: &str, explanation: &Explanation, notes: &EmptyNotes) -> Vec<Section>;
  ```

- `markdown` = `render_with(SectionKind::title, sections(passage, explanation, &EmptyNotes::silent()))`；`render_with` 原样保留（面板复制仍用它 + 面板自己的标题）。
- public `panel()` 删除；private `sections()` 并入 public 那个。面板 `document::sections` 改为调用 `render::sections` 并保留 `.filter(prefs.shows)`；桌面 `view.rs` 用同一个函数、传自己的两句文案。
- 这次只改这一处 core 公共 API；不新增 module，`Section` / `Body` 仍在 `render.rs`。

### 4. 桌面 wire：`sections` 取代扁平字段

- 序列化留在 `crates/plainly-desktop/src/view.rs`，core 的 render 类型保持 serde-free：

  ```rust
  #[derive(Serialize)] pub struct SectionView {
      pub kind: &'static str,   // "original" | "comprehensible-english" | "key-help" | "grammar" | "translation"
      pub title: &'static str,  // strings.rs 的 detail_*，按 kind 选
      pub body: SectionBodyView,
  }
  #[derive(Serialize)]
  #[serde(tag = "type", rename_all = "snake_case")]
  pub enum SectionBodyView {
      Prose { text: String },
      Note { text: String },
      Glosses { glosses: Vec<GlossView> },
  }
  #[derive(Serialize)] pub struct GlossView {
      pub expression: String,
      pub gloss: String,
      pub example: Option<String>, // 由 core::render::GlossView 转来；None = 没有例句
  }
  ```

- wire 形状（契约测试钉住）：

  ```json
  "sections": [
    { "kind": "original", "title": "原文", "body": { "type": "prose", "text": "…" } },
    { "kind": "key-help", "title": "关键帮助", "body": { "type": "glosses", "glosses": [ { "expression": "…", "gloss": "…", "example": null } ] } },
    { "kind": "grammar", "title": "语法", "body": { "type": "note", "text": "本段不需要" } }
  ]
  ```

  `example` 显式 `null`（serde `Option` 默认），不省略；字段名沿用现有 DTO 的 snake_case。
- `DetailView::of` 调 `render::sections(&artifact.passage, &explanation, &EmptyNotes { grammar: Some(STRINGS.grammar_not_needed), key_help: Some(STRINGS.glosses_empty) })`，再映射成 `SectionView`（title 由 kind 选）。
- `DetailView` 删除 `passage` / `comprehensible` / `glosses` / `grammar` / `translation`；保留 `id`、metadata、`markdown`、`sections`。`markdown` 仍由 `render::markdown` 生成（文档政策：空节省略；与卡片有意不同，`docs/desktop-manual-checks.md` 已有说明）。
- 空节文案仍各表面一份：桌面 `strings.rs` 的 `glosses_empty` / `grammar_not_needed` 传给 core；面板用自己 `strings.rs` 的常量。core 只决定「何时用」。
- 过渡：ticket 02 先把 `sections` 加上、扁平字段保留（每个提交都保持可运行与测试绿）；ticket 03 在 JS 切到 `sections` 的同时删除扁平字段。最终形状如上。

### 5. 桌面 JS 与 CSS

- `app.js` 的 `detailView`：删掉五节硬编码，改为

  ```js
  record.sections.map((section) => `
    <h3>${esc(section.title)}</h3>
    <div class="sec" data-kind="${esc(section.kind)}">${bodyView(section)}</div>
  `).join("")
  ```

  `bodyView` 按 `body.type`：
  - `prose` → `<p class="prose">${esc(body.text)}</p>`；
  - `note` → `<p class="${section.kind === "grammar" ? "notneeded" : "note"}">${esc(body.text)}</p>`（材质的一行映射，不参与「有没有」判断）；
  - `glosses` → 今天的 `.gl` 模板；`example` 为 `null` 时不画 `.eg`，有则 `esc(gloss.example)`，**不 trim**。
- JS 不再使用 `t.detail_*` / `t.glosses_empty` / `t.grammar_not_needed` 画详情；这些字段留在 `strings.rs`（Rust 现在用它们生成 sections），`app.strings` 的 payload 不变。
- `style.css`：`.orig` / `.comp` / `.zh` / `.grammar` 迁到 `[data-kind="original"] .prose` / `[data-kind="comprehensible-english"] .prose` / `[data-kind="translation"] .prose` / `[data-kind="grammar"] .prose`；`.gl` / `.gl .gloss` / `.gl .eg` / `.note` / `.notneeded` 保持不变。
- 其它 JS（h2 level、meta、provenance、local 标记、actions、tags、没有选中时的提示）不动；HTML 仍只在 JS 里拼，Rust 不发 HTML。

### 6. 测试 seam

- **A：core 渲染**（`crates/plainly-core/tests/render.rs`）——owner 规则与节政策的主 seam。
- **B：core 导出**（`crates/plainly-core/tests/export.rs`）——CSV 的三行形状与例句缺失。
- **C：面板控件树**（`panel.rs` 的 GTK 测试，无 display 时跳过）——三块 / 两块与 whitespace-only。
- **D：桌面 DTO 与 wire**（`desktop/tests/window.rs`，新增 `serde_json` 用例）——`DetailView.sections` 的 kind 顺序、Note 文案、`example: null`。
- **E：手测**（两份 manual-checks）——JS/CSS 的视觉 parity；没有 JS 测试，不新增测试基建。
- 复用既有假件（`support` 的 Explanation 解析、临时库），不新增 mock 层。

### 7. 验收判据

- 全仓库只剩一处例句判空：`grep -rn "example.trim\|example.is_empty" crates/` 只命中 `render.rs` 的 `GlossView::of`（及其测试字符串）；四个旧站点与桌面 detail 测试里的旧断言都消失。
- `grep -rn "render::panel" crates/` 0 处；详情路径 `app.js` 里不再有 `t.detail_` / `t.glosses_empty` / `t.grammar_not_needed`。
- `DetailView` 不再有 `passage` / `comprehensible` / `glosses` / `grammar` / `translation` 字段；`sections` 有序列化契约测试。
- `Body::Prose` 不再承载空节说明（core 测试断言 `Body::Note`）。
- `cargo test -p plainly-core`、`cargo test -p plainly-cli`、`cargo clippy -p plainly-core -p plainly-cli` 全绿（CI）；`nix develop -c cargo test -p plainly-panel -p plainly-desktop` 与 `cargo clippy --workspace --all-targets` 在 flake 里全绿。
- 桌面详情的五节顺序、标题、空节文案、三行 Gloss、旧记录两行、复制 markdown 逐项由手测清单确认，且没有任何预期之外的布局变化。

## Testing Decisions

- **什么算好测试**：只测外部行为——`Section` 列表的内容与顺序、`GlossView.example` 的 `Some` / `None`、markdown / CSV 的逐字输出、面板控件树的 label 序列、桌面 wire 的 JSON。不断言私有 helper，不测内部调用顺序。
- **owner 规则用例**（core `tests/render.rs`）：
  1. `example: ""` → 两行（既有测试，保持）；
  2. `example: "   "` / `"\n"` → 两行（新增，纯空白）；
  3. `example: "  He went to ground.  "` → 三行且引文里是 trim 后的句子（新增）；
  4. 多行例句仍逐行加 `  > `（既有，不变）。
- **CSV**（core `tests/export.rs`）：空 / 纯空白 → 两行；前后空白 → 缩进的 trim 后句子；三行形状与转义不变。
- **面板**（`panel.rs` 的 `the_key_help_reaches_the_screen_without_markup`）：补一条 `example: "   "` 的 Gloss → 只有两个 label；空节说明仍是 `Body::Note`（core 测试已覆盖 body 种类，面板只测画出来的控件数）。
- **桌面**（`desktop/tests/window.rs`）：重写 `the_detail_carries_the_five_sections_and_the_markdown_export_of_one_record`——从 fixture `Record` 断言 `sections` 的 kind 顺序（original / comprehensible-english / key-help / grammar / translation）、空节 `Note` 文案、`example` 的 `Some` / `None`（含 whitespace-only 与前后空白）、`markdown` 等于 `render::markdown(record)`；新增一条 `serde_json::to_value(&detail)` 断言 `body.type`、`example: null` 与字段名。
- **CLI**：既有 golden 测试不变（它们是「除 trim 外逐字不变」的证据）；若 `cli/tests/explain.rs` 的 fixture 含前后空白再调整，否则不动。
- **面板复制**：`document.rs` 的既有复制测试继续通过（`Note` 当文本）；新增一条空 Key Help 的 Note 断言如缺。
- **手测**：`docs/desktop-manual-checks.md` 的「详情与导出」逐项过一遍；`docs/manual-checks.md` 的 CLI / 面板 / CSV 条目不因本次改动而改变；预览图可选重生成。
- **不做**：不引入 JS 测试运行器或打包器；不为 GTK / WebKit 增加 CI；不新增 mock 层。

## Out of Scope

- 面板 loading / failed 状态下自建的 Original 节（不是 Explanation 渲染）。
- 例句 trim 之外的文本归一：`expression` / `gloss` 不 trim，多行例句在面板 / 卡片里的折行仍归各表面。
- 合并桌面与面板的空节文案（copy 仍各一份；core 只决定何时用）。
- 桌面详情读取面板的五个节开关（现状就不读）。
- 把「有没有例句」做成契约校验或 schema 边界（ADR-0001 仍然禁止）。
- 把 `Gloss.example` 改成 `Option`（领域模型、wire schema 与 store 都不动）。
- 搜索 / FTS、`ARTIFACT_VERSION`、store schema、prompt、provider、运行路径。
- 视觉重设计；JS 测试基建；任何新的框架或 bundler。
- 评审候选 5（面板触发策略进 state）与其它候选。

## Further Notes

- **一处可见变化**：例句首尾空白在渲染前被 trim。正常模型输出不带首尾空白，因此实际不可见；`GlossView::of` 是唯一实现，测试钉住 `"   "`、`"\n"` 与 `"  He went to ground.  "` 三种输入。除此之外，CLI / 面板 / CSV / 桌面卡片的输出逐字不变；任何差异都按 bug 处理。
- **为什么现在做**：候选 3 的证据（四份判断、桌面绕过 core、`832efcb` 的波及面）已经成立，而候选 1/2/4 落地后桌面是最后一个「渲染规则在 core 之外」的表面；先做它，第五个表面才有可继承的形状。
- **结构记录**：ADR-0005 记录「渲染形态是 core 的一个值」及被否掉的替代方案（每面自己判、core 发 HTML、改 `Gloss.example` 为 `Option`、用 `Prose` 承载空节说明）。本规格记录行为与接口细节。
- **词表**：`GLOSSARY.md` 不变；`Section` / `Body` / `GlossView` / `EmptyNotes` 是代码名。
- **wire 是内部 IPC**：`sections` 的 JSON 是 desktop 的两个 in-repo 半区之间的契约，不是外部 API；因此 DTO 放在 `view.rs`，core 不 derive serde。
- **接续**：实现按本目录 4 张 tickets，依赖序 01 → 02 → 03 → 04；`/code-review` 与验收记录写进 04 的 Comments。架构评审 HTML 只存在于临时目录，有意不入库。
- **溯源（两轮 13 个决策）**：

| 会话决策 | 落在这里 |
| --- | --- |
| Q1 owner = core::render 的已定型 `GlossView`（`Option` + trim） | §1 |
| Q2 桌面详情整块改为 core 的 `sections`，保留空节与文案，外观不变 | §4、§5 |
| Q3 owner 同时定型文本：`Some` 里是 trim 后的句子 | §1、Testing Decisions |
| Q4 新增 ADR-0005 | ADR-0005 |
| Q5 不引入 JS 测试基建；Rust 侧 + 手测 | §6、Testing Decisions |
| Q6 产出 spec + ADR + tickets，先文档后实现 | 本文件、`tickets/` |
| Q7 `GlossView` + `render::sections` / `EmptyNotes`；取消 `panel` 名字 | §1、§3 |
| Q8 空节说明升为 `Body::Note` | §2 |
| Q9 序列化留在 `view.rs`，core serde-free | §4 |
| Q10 wire 形状、kebab kind、title/Note 来自 Rust、JS 与 CSS 接线 | §4、§5 |
| Q11 `DetailView` 删五个扁平字段；重写 detail 测试 + JSON 契约测试 | §4、§6、Testing Decisions |
| Q12 4 张 tickets，01 → 02 → 03 → 04 | `tickets/` |
| Q13 ADR-0005 一条，记录 owner 与替代方案 | ADR-0005 |
