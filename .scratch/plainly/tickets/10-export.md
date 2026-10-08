# 10: 导出三形态

**What to build:** 我能把一条复制成 markdown、把整库导出成单个 markdown 文件，或导成可直接进 Anki 的 CSV（每个 Gloss 一张卡）。

**Blocked by:** 08

**Spec:** [spec.md](../spec.md) §9 导出三形态

**Status:** done (2026-10-08)

- [x] Markdown：单文件、按 Record 分节，每节标题含日期 + 等级 + provider / 模型 + 提示词标签，正文用五节渲染
- [x] Anki CSV：**每个 Gloss 一行**，正面 = `expression`、背面 = `gloss`、上下文 = 该 Record 的 Passage，另附等级 / 母语 / provider / 模型 / 提示词标签列
- [x] 原始 CSV：每个 Record 一行，Gloss 以 `expression → gloss` 多行文本放进单个单元格
- [x] `plainly history export markdown|anki|raw` 把产物写到 stdout（stdout 只有产物）；空历史是合法输入
- [x] 单条导出（一条 Record）与全局导出走同一套渲染，不各写一份
- [x] **不做** `.apkg`

## 落地形态

- `plainly-core::export`（缝 A；与 `render` 一样是**纯函数**，不碰 IO、不碰库）：
  - `export::markdown(&[Record])`：一个 Record 一节，节标题
    `## <YYYY-MM-DD> · <Level> · <provider>/<model> · <prompt_label>`，正文**直接包
    `render::markdown(...)`**——五节只有一份实现，`show`、面板与导出都走它。日期取 `created_at`
    的前十个字符：`Timestamp::to_rfc3339` 的形状由类型保证，不为导出再写一份日历；它是 **UTC 的日期**
    （库里所有时间戳都是 UTC）。空历史 → 空字符串
    （不是报错，也不是半截文档）。
  - `export::anki_csv(&[Record])`：列
    `expression,gloss,passage,level,native_language,provider,model,prompt_label`；**每个 Gloss 一行**，
    所以**没有 Gloss 的 Record 不出卡**——空正面导进 Anki 会变成答不了的卡；这类 Record 在 markdown
    与 raw 里都在。**expression 是空白的 Gloss 同样不出卡**：schema 按 ADR-0001 不写长度下界，模型可以
    返回空 expression，所以"答不了的卡"这件事在导出这一层兜住，而不是指望契约拒绝它。列只放提示词**标签**不放哈希：筛选靠标签，哈希用来区分两个附录，那件事交给 markdown
    标题与 raw CSV。
  - `export::raw_csv(&[Record])`：一个 Record 一行，列
    `id,created_at,generated_at,last_seen,level,source_language,native_language,provider,model,thinking,prompt_label,prompt_version,artifact_version,tags,passage,comprehensible,glosses,grammar,translation`。
    **Glosses 在一个单元格里多行**（`expression → gloss` 一行一条），因为"一行一条 Record"才是表格能排序
    筛选的单位；`grammar` 为 null → 空单元格（不是 `null` 字样）；标签以 `, ` 连接，**这个单元格是散文不是
    列表**（标签自身含 `, ` 时读起来像两个——要问某个 Record 有哪些标签，问库，不要解析这一格）。
    `lookup_key` 不入表：那是库的内部身份，不是 Explanation 的事实。
  - **CSV 转义在 `csv_field` 一处**：含逗号 / 引号 / 换行 / 回车的字段加引号、内部引号翻倍（RFC 4180）
    ——Passage 有逗号、Gloss 可能引用原文，不转义就会悄悄改列。测试里既钉了逐字的转义样例，也用一个小型
    **引号感知读取器**把产物读回成行列（数行数在 Passage 有两个段落后就是错的）。
  - 三个函数都**保留调用者给的顺序**，自己不排序：CLI 传 `store.list()`（最近在前，与 `list`、主窗口
    一致），单条导出传 `&[record]`（tickets/14 的详情页）。
- `plainly` CLI：`history export markdown|anki|raw`，参数直接解析成 **core 的 `ExportFormat`**
  （`crates/plainly-core/src/config.rs`，与 `app.export_format` 同一个类型、同一份 `FromStr`）——同一个封闭
  词集只有一处拼写，`config set app.export_format anki` 与 `history export anki` 不可能各说各话（`as_str()`
  是配置里写的那个词）。产物写 stdout、**stderr 为空**（导出没有人话要说），空历史退出码 0；缺格式或未知
  格式 → 退出码 2 由 clap 给出。与 `list` 一样**不需要 provider，也不需要配置文件存在**。

## 给下游的交接

- **tickets/13 / 14（面板与主窗口）**：详情页的"复制 markdown / 导出这一条"用
  `export::markdown(&[record])` / `export::anki_csv(&[record])`；设置页的三条全局导出是同一个函数传整段
  历史。**不要另写渲染**：五节形状在 `render`，分节标题在 `export`。
- **tickets/16 / 17（加固）**：`export` 是纯函数、无状态、无 IO，加固时不需要为它做并发考虑（数据来自
  `Store::list()` 的一次读）。

## 已知取舍

- **Anki 导出会丢掉没有 Gloss 的 Record**：这是"每个 Gloss 一张卡"的直接推论，不是漏做；那些 Record 在
  markdown 与 raw 里都在。
- **原始 CSV 的 Glosses 在单元格里换行**：表格软件里要开"自动换行"才好看，换来的是"一行一条 Record"。
  真想要每 Gloss 一行的表格数据，Anki 那条导出就是它。
- **markdown 标题只写提示词标签，不写哈希**：规格的标题列表就是"日期 + 等级 + provider/模型 + 提示词
  标签"。同一段落的不同提示词版本在日期或内容上已经不同；要精确比对的人去读 raw CSV 的 `prompt_version`。
- **导出顺序 = 历史顺序（最近在前）**：导出是历史的忠实转储，不是重新编排；想按时间正序读，交给下游工具排。
- **`id` 进原始 CSV，`lookup_key` 不进**：`id` 能把一行对回 `history show <id>`；`lookup_key` 是库的
  缓存身份（Passage + 整套 provider profile 的哈希），对库有用、在表格里是噪音。代价是清库重导后 `id` 会变
  ——它不是稳定标识。
- **markdown 标题的日期是 UTC 的日期**：`created_at` 存的就是 UTC，v0 没有时区机器，不为一个页眉引依赖。
  界面里的今天/昨天（票据 14）会把同一个瞬间读成本地时区，所以**在读者偏移量的窗口内两者会差一天**——说的是
  同一个瞬间，别把导出的日期当成本地日历日。
- **没有 `.apkg`**：规格明确不做——写它就是实现半个 Anki 内核，而 Anki 本来就支持 CSV 字段映射。
