# 07: 提示词：附录、内容哈希与迁移

**What to build:** 出厂提示词随版本更新、我追加的规则不丢；版本号不再靠人自觉——改了任何一个字版本就变，旧答案不会被误命中，而旧记录也不会被偷偷改写。

**Blocked by:** 03

**Spec:** [spec.md](../spec.md) §5 提示词模型

**Status:** done (2026-10-08)

- [x] 有效提示词 = 出厂默认（不可变、随应用更新）+ 用户附录；附录**只能追加**，界面与文档说清原因
- [x] 出厂默认是**内联等级描述表**的版本：`{{LEVEL}}` 代入「标签 + 含义」；C1 的描述不与「必须是真改写」冲突
- [x] `prompt_version` = `SHA-256(出厂默认 + 用户附录 + 等级描述表)`，占位符不参与替换；改一句描述会改变哈希（有测试）
- [x] Record 同时携带哈希与人类可读标签；**不存提示词正文**，旧记录永远可渲染
- [x] 契约持续失败时给出「你的提示词没通过契约」与一键「改用出厂默认重试」，并记录真正产出它的那个提示词
- [x] 改 Level / 母语不触碰任何旧 Record：同一段落在新等级下是**新查询**（新 key → 新记录）
- [x] 输出小节开关是显示层开关：不改提示词、不改 schema、不进 Lookup Key，切换即时且不必重新生成

## 落地形态

- `plainly-core::prompt`：有效提示词与它的版本（缝 A）。
  - `FACTORY_PROMPT` 就是标定过的 v6 文本，只改一处：`{{LEVEL}}` 现在代入**「标签 + 含义」**
    （`A2 (very common words, short sentences, concrete)`）。`PROMPT_LABEL` 随之从
    `v6-synthesis` 升为 **`v7-descriptors`**——名字必须跟着内容走，否则标签在说谎。
  - `prompt::factory_descriptor(level)` 给出出厂的那一行（写成一个 `match`，所以新增 `Level` 变体
    是编译错误，而不是运行时 panic）；`Prompt::descriptor(level)` 是设置页要显示的那一份
    （tickets/16 用它，不另抄一份）。**四行里只有 C1 是改过的**，其余三行照抄源契约的措辞（A1 与
    A2 共用一句，因为源契约就是这么写的）。C1 丢掉了「keep most of the original structure」，把
    「仍是真改写」写进描述本身——实测里正是这句话与提示词自己的硬规则打架，而硬规则赢，很可能就是
    B2 与 C1 无法区分的原因。B2 不顺手改写成「English first; keep nuance…」：那是未实测的措辞改动，
    spec §5 只授权 C1 这一处。
  - `Prompt { appendix, descriptors }`：**出厂文本是常量而不是字段**，用户没有任何入口能改它；
    附录只被追加（`system_prompt` 末尾接 `\n\n` + 附录），没有任何代码路径能前置它或把它并进基础
    文本——「只能加不能删」是结构，而不是一句要人自觉的约定。想反制某条基础规则，只能写一句相反的
    规则排在它后面（spec §5 诚实记下的能力弱点）。**两个占位符一趟填完**：先替 `{{LEVEL}}` 再拿结果
    去找 `{{NATIVE}}`，会把用户描述里写的 `{{NATIVE}}` 当成模板；描述是数据，不是模板。
  - 纯空白的附录＝没有附录：`appendix = ""` 与写满换行的附录是同一个提示词，因此同一个版本。
  - 描述覆盖：`[prompts.level_descriptors].<Level>` 非空即替换该行，空或缺失用出厂行；把出厂原文
    写回去不算改动（哈希不变）。**值**在 `[app]`、**含义**在这里，边界照 tickets/01/15 的约定。
  - `Prompt::version()` = `SHA-256(出厂文本 + 附录 + 描述表)`，**占位符不替换**——等级与母语已经
    是 Lookup Key 的独立分量，替换进来只会给同一个问题两个身份。描述表**整表**入哈希（spec §5 的
    字面要求）；字段之间用**长度前缀**串联，附录里写什么都不会把两个字段挤成同一串字节。
  - `Prompt::fallback_after(kind, level)`：只有在「契约类失败」且**这一跑真要发的文本**与出厂不同
    时才给出厂提示词。判据是**这次运行**而不是整份提示词数据：为别的等级改过的描述行根本不会代入
    这一跑的文本，退回去发的是逐字节相同的东西，只可能同样失败（描述表整体入哈希是 spec §5 的身份
    语义，这里问的是「重试能不能改变什么」）。provider 失败不怪提示词；这一跑已经在发出厂文本就没有
    可退的。
- `plainly-core::retry`：`FailureKind::is_contract()` = **`Malformed` 一个**（JSON 没解析出来、
  或没通过 schema）。**`Empty` 不算**：它还盖住端点侧的问题（`choices` 为空、没有 `message`、
  200 的 body 根本不是 JSON——`chat.rs` 里那几处 `ProviderError::empty`）。把这些算成「你的提示词
  没通过契约」是错的，给出的重试也不可能不同。`Empty` **仍然留在「同一错误重复即停」的漂移判据
  里**（那是 tickets/04 的语义）：两个判据从此有意地不相等，各自写明为什么。
- `plainly` CLI：
  - `explain` 用 `Prompt::from_config(&config.prompts)` 构造请求，`prompt_version` 与
    `prompt_label` 都来自它。Artifact（也就是未来的 Record）**只带哈希与标签，不带提示词正文**，
    而渲染不经过提示词，所以旧记录永远可渲染。
  - **`--factory-prompt`**：本次运行只用出厂提示词（附录与描述覆盖都不算），配置一个字节都不动。
    这是面板那颗「改用出厂默认重试」按钮在 CLI 上的形状；spec §10 的 explain 表里没有它，是本票为
    「一键」补的入口（spec §5 要求给出一键，而 v0 只有 CLI 这一个面）。
  - 契约失败时，stderr **总会**明说是什么没通过契约，但退路的有无取决于**这一跑真发出去的文本**：
    `Prompt::fallback_after(kind, level)` 在「`Malformed` 且这一跑的文本与出厂文本不同」时才给出
    `--factory-prompt` 这条一键重试；否则说「出厂提示词没通过契约」且不给退路——给一条发同样字节的
    退路只是噪音。注意这不等于「`is_factory()` 为真」：只为 C1 改过描述、却跑在 B2 上时提示词数据
    不是出厂的，但这一跑发的就是出厂文本，所以没有退路可给。CLI 只负责措辞，判据不自己写一遍。
    **运行本身绝不静默回退**：悄悄换掉用户写的规则既隐瞒失败，也替用户做了主。
  - 重试那一跑产出的 Artifact 记的是**真正产出它的那个提示词**（出厂哈希 + `v7-descriptors`），
    因为它本来就是拿这份提示词发出的请求。
- 显示小节开关仍然只影响显示：`Prompt` 只读 `[prompts]`，`[app]` 里的 `show_*` 够不到它；提示词
  照旧要四个键，schema 照旧四项，切换不进 Lookup Key。这是结构，不是纪律。

## 给下游的交接

- **tickets/08（历史库）**：Lookup Key 里的 `prompt_version` 用 `Prompt::version()`；Record 存
  `prompt_version` + `prompt_label`（Artifact 已经带着这两个字段），**不存提示词正文**。同一段落
  可以有多个 prompt 版本的记录，那是特性（可比），但列表行必须能区分（spec §13）。
- **tickets/15、16（面板与设置页）**：那颗「改用出厂默认重试」的判据与去处就是
  `Prompt::fallback_after(FailureKind, Level)`（`None` 时不要显示按钮，且必须在**当前等级**下问，
  否则会给出一个发同一段文本的无效重试）；设置页显示等级含义用
  `Prompt::descriptor` 与 `prompt::factory_descriptor`；附录编辑器写 `[prompts].appendix`，界面要
  说清「只能加不能删」的原因。
- **tickets/11（长输入）**：`chat::PROMPT_TOKENS = 2048` 的工装没变；出厂提示词只多了每次两处描述
  （最长一行 C1 约 100 字符，两处合计几十个 token），仍在预算里。若将来重算阈值，把提示词长度一起看。
- **tickets/17（加固）**：C1 措辞是**未实测**的修正（见下），要复测 B2 与 C1 是否真被分开。

## 已知取舍

- **描述表整表入哈希**：改任一等级的描述，会让**所有**等级已存的记录在下次查询时各重新生成一次。
  这是 spec §5 记下的「应用更新改动出厂提示词 → 每段各重生成一次」的更宽版本。换来的是：改描述
  却不升版本这种陈旧命中在**结构上**不可能发生（issue 09 要的那条纪律变成保证）。
- **C1 的措辞修正没有实测背书**：实测只证明「标签 + 描述」比「只发标签」强，以及 B2/C1 无法区分
  （4.32 vs 4.29、0.463 vs 0.462）。新 C1 行是否真把它们分开，留给 tickets/17 复测；本票交付的是
  「不再自相矛盾」这个前提。
- **`--factory-prompt` 是 spec §10 表外的新增旗标**：它不改变退出码语义，也不写配置，只是把 spec §5
  要求的「一键」落到 v0 唯一存在的那张脸上。面板的等价物是
  `Prompt::fallback_after(FailureKind, Level)`。
- **不做保存期探针、不在保存时阻断**（issue 09 第 5 条）：契约失败只暴露在真正失败的那次查询上，
  而且只有走完 tickets/04 的重试之后才说「你的提示词没通过契约」。
- **手编文件里拼错的等级键被忽略，不报错**：`[prompts.level_descriptors] B1 = "…"` 会解析成功、
  什么也不改、版本也不变。这是 config.rs 那条既有规则的代价——未知键一律加载，好让新版本写的文件
  能被旧版本读（`plainly config set prompts.level_descriptors.B1 …` 则会当场报错）。要收紧，属于
  配置解析（tickets/01）的地盘，不在本票里顺手改。
- **本票顺带补了 `GLOSSARY.md` 的三个词**（spec §450 点名的缺口）：`Provider Profile`、
  `Effective Prompt`、`Level Descriptor`，另加 `Factory Prompt` 与 `Prompt Appendix`，因为本票的
  代码与测试已经在用这些词，而词表里没有它们。
