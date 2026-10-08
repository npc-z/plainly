# 11: 长文本切分与 CLI 批次

**What to build:** 粘一段长文（或把一个文件交给 CLI）时，它按段落切成若干块、一块一条 Record、逐块报进度，而且只重新生成真正变了的那块；CLI 不被面板的上限拦住。

**Blocked by:** 08

**Spec:** [spec.md](../spec.md) §6 输入切分与长文本、§10 CLI 批次行为

**Status:** done (2026-10-08)

- [x] 阈值 150 词（空白切分）；段落边界优先；**单个段落自身超限时降到句子边界**
- [x] 一块一条 Record，Lookup Key 按块生效——只有变化的那块重新生成
- [x] 串行按原文顺序执行；开始前向 stderr 报「将执行 N 次请求」；每块完成报进度
- [x] 某块失败其余继续，已产出的部分保留，退出码为 `1`（不整批丢弃）
- [x] `--format json` 多块时是 Artifact **数组**，顺序同原文
- [x] CLI **不继承**面板的 5 块上限；`max_tokens` 与 150 词阈值成对（改一个必须看另一个，有注释 / 测试守住）
- [x] 面板侧的上限判定（> 5 块拒绝）以可复用的形式提供，供面板票使用

## 落地形态

- `plainly-core::split`（缝 A）：`CHUNK_WORDS = 150`、`PANEL_CHUNK_LIMIT = 5`、
  `chunks(&str) -> Vec<String>`、`panel_chunks(&str) -> Result<Vec<String>, TooManyChunks>`。
  - 切分是**原文的字节区间**，不是重新拼出来的字符串。段落＝非空行的连续段（空行分隔，成段的行
    即使硬折行也算一段），先按 150 词贪心打包；单个段落自身超限时降到句子边界（`.!?` 之后跟空白
    或段尾，收尾的引号括号算进这一句；词内句点 `3.14` 不是边界，而缩写结尾的句点 `e.g.`、`Mr.`
    与句尾无法区分，按句尾处理）；单个句子仍超限时降到
    **词边界**。第三条规格没写，但它是阈值成为承诺的条件——一句话可以任意长，而 `max_tokens` 与
    150 词是一对；碎片好过被截断的答案，所以这条也写进了模块文档。
  - 输入本来就在阈值内时**逐字返回**（含首尾空白）：只有真正的切分才改写文本，所以 `explain
    file.md` 的单块路径与从前逐字一致。多块时每块是原文里的一段连续区间，段间空行照旧保留。
  - `chunks` 不理会面板上限；`panel_chunks` 就是那条判定，`TooManyChunks { chunks, limit }`
    把实际块数与上限带出去，给人看的措辞留给面板。
- `plainly-core::split` 另有 `OUTPUT_TOKENS_PER_WORD = 6`（规格实测的输出/输入比）：
  `plainly-core::chat` 新增私有 `MAX_TOKENS_FLOOR = 1200`，两条 `const _: () = assert!(…)` 在
  **编译期**绑住「预算 ≥ 1200」与「1200 ≥ `CHUNK_WORDS` × `OUTPUT_TOKENS_PER_WORD`」；这个倍率
  只有一份，测试也引用同一个常量而不是另写一个数字。注释里不再写死「150 词」，一律指向
  `split::CHUNK_WORDS`。`MAX_TOKENS`（2048）与 `MAX_TOKENS_THINKING`（8192）的值不变。
- `plainly-cli explain`：读入后 `split::chunks`，先对**每个不同的提问**算 Lookup 并 `recall`，再报
  `N chunks; will make M requests`——M 是这一跑真正会发出去的请求数，命中不算，所以整份都在
  历史里时是诚实的 `0`——然后**串行按原文顺序**执行。命中打 `chunk i/n: reusing the stored
  Explanation — …`，生成打 `chunk i/n done`，失败打 `chunk i/n failed: …` 并且**不中断后续块**。
  - **重复的段落只问一次**：预扫描时用 `first_seen: HashMap<LookupKey, usize>` 按 Lookup Key 去重
    （`LookupKey` 因此加了 `Hash`），重复的那一块记 `repeat_of: Some(earlier)`，直接复用前一次
    的 Artifact——否则同一段会付两次钱，而第二次 `remember` 还会覆盖第一次的 Record。第一次就
    失败时，重复块跟着报失败，但不会再发一次注定失败的请求。产物仍然每个出现位置各一份。
  - 产物按槽位收进 `Vec<Option<Artifact>>`（失败的那块留空 `None`，不挤动后面的），最后按原文顺序
    写 stdout：markdown 每块一篇、以空行分节；`--format json` 多块是数组（部分失败时是成功那些的
    数组，形状跟着**输入**走）。单块路径的输出形状不变：markdown 一篇，json 一个对象。
  - 全部成功 → `0`；任一块失败 → `1`，并且**先**把已产出的部分写出去再报
    `k of n chunks failed; what succeeded is on stdout`。单块生成失败仍是原来的消息与空 stdout。
  - **历史库写失败**（WAL 撞上并发写、磁盘满）是这一跑自己的本地状态，不是某一块的失败：已经付费
    的产物（含刚生成这一份）**先**写 stdout，stderr 说明是哪一块没能入库，然后整批停下并退出 `1`
    ——继续跑只会付出存不下的答案。
- 测试：`crates/plainly-core/tests/split.rs`（阈值 / 段落打包 / 句子边界 / 词边界 / 词内句点 /
  空白输入 / 面板上限 / 规格数字），`crates/plainly-cli/tests/batch.rs`（超 5 块不被拒且 JSON 数组
  同序、markdown 空行分节、部分失败保留产出、第二跑零请求且一块一条 Record、只有变了的那块重新
  生成、重复段落只问一次、第一次就失败的重复段落不再问）。

## 给下游的交接

- **tickets/13、15（面板）**：`split::panel_chunks` 就是「> 5 块」的判定，`Err(TooManyChunks)`
  的两个字段足够面板自己组一句「这段太长，请用 CLI 或先拆分」；不要在面板里另写一遍块数判断。
- **tickets/17（加固）**：批次的端到端（顺序、部分失败、JSON 形状）已在 `tests/batch.rs` 覆盖；
  真机上一整份长文本的冒烟仍归 17。

## 已知取舍

- **句子边界是启发式**：词内句点（`3.14`）不会断句，但**以缩写收尾的句点**（`e.g.`、`Mr.`）与
  句尾长得一样，会被当成句尾——除非带一本缩写词典，否则分不开。规格只要求「降到句子边界」，而
  误判的代价是一次不理想的断句，小于超预算被截断的代价。段落边界（绝大多数情况）没有这个问题。
- **空白输入没有 Passage**：`chunks("")` 与 `chunks(" \n\n ")` 返回空列表，`panel_chunks` 同样
  返回 `Ok([])`——「什么都不解释」的措辞属于调用方。CLI 侧 `read_passage` 已经在更前面把空输入
  判成用法错误，所以那条路径到不了这里。
- **一批里每一块都各自报一次 contract 档位**：`report` 是「一次 run 一次」，批次就是 N 次 run。
  重复的是同一行，但把它折叠起来就要在 CLI 里维护「和上一块比有没有变」的状态，而档位之外还有
  降级 / 重探这些只属于某一块的事实；宁可能啰嗦也不漏报。
- **面板上限在 CLI 侧完全不存在**：不是「提高上限」也不是「加个 flag 绕过」，而是 CLI 不引用那个
  常量。面板拒了就说改用 CLI。
