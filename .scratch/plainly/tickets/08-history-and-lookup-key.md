# 08: 历史库与 Lookup Key 幂等

**What to build:** 每次解释自动落进本地历史，同一段文本第二次查询零成本拿回同一条——而换了模型、换了等级、改了提示词之后，不会再命中旧的烂答案。

**Blocked by:** 07

**Spec:** [spec.md](../spec.md) §9 历史库、Lookup Key 与导出

**Status:** done (2026-10-08)

- [x] SQLite 以 WAL 打开（多进程并发读 + 串行写），`records` + `glosses(record_id, ord, expression, gloss)` 关系化存储；原文逐字入库、不归一化
- [x] Lookup Key 元组 = 归一化(Passage) + level + source_language + native_language + prompt_version + provider + model + thinking；用 SHA-256；`lookup_key` 上建 UNIQUE
- [x] 归一化只做首尾空白与 CRLF→LF；内部空白变化产生新 key；**`artifact_version` 不进 key**
- [x] 命中**复用同一行**（不插新记录）并更新 `last_seen`；历史按 `last_seen` 排序；不记 `lookup_count`
- [x] `--regenerate` 绕开命中并**原地覆盖** Explanation / 更新 `generated_at`，保留 `created_at` 与 `last_seen`
- [x] `source_language` 是常量 `"en"` 并照样入库，**不做检测**
- [x] `plainly history list` 与 `history show <id>` 可用，每条带 provenance（provider / 模型 / thinking / 提示词版本）

## 落地形态

- `plainly-core::store`：历史库与 Lookup Key（缝 A）。
  - 依赖 `rusqlite` 且开 `bundled`：SQLite（含 FTS5，留给票据 09）编进二进制并静态链接，机器只需要
    一个 C 编译器——CLI 因此不新增运行时动态库，既有的
    `the_cli_binary_needs_no_graphics_library` 仍然通过。库文件在 `appDataDir/history.db`
    （`Paths::history_file`）；`Store::open` 建目录、开 **WAL**、`busy_timeout = 5000`、
    `foreign_keys = ON`。
  - 两张表：`records`（`lookup_key` **UNIQUE**、Passage 逐字、Explanation 四件套、provenance、
    `created_at` / `generated_at` / `last_seen` 以 RFC 3339 存）+ `glosses(record_id, ord,
    expression, gloss)`，`ord` 保序、`ON DELETE CASCADE` 直接为票据 09 的硬删准备好。时间戳存
    字符串而不是秒数，是因为 `Timestamp` 的文档早就定了「wire form 就是 SQLite 排序用的形式」。
  - `Lookup` 是那八个分量的值对象（`From<&ExplainRequest>`）：归一化(Passage) + level +
    source_language + native_language + prompt_version + provider + model + thinking，长度前缀
    串成一个 material 再取 SHA-256。**`artifact_version` 不在里面**：形状变了不代表旧答案是错的。
  - `normalize`＝CRLF→LF + 去首尾空白；内部空白是语义，不动。**原文逐字入库**——归一化只用于
    身份，读回来的是存进去的那一份。
  - `LookupKey` 是独立类型而不是 `String`：Passage 或 provider 名不可能被误当成 key 传进去。
  - 读 Record 是**一条语句**：`records LEFT JOIN glosses`，一条记录一行就收成一条（`ORDER BY …,
    g.ord` 保证同一记录的行相邻、Gloss 保序）。这样一条记录的 Gloss 与它的列属于同一个快照——
    `recall` 与一次重新生成赛跑时，拿到的要么是旧的 Explanation、要么是新的，不会是各一半；
    `list()` 也因此不再是 N+1 次查询。
  - `Store::recall`：先一条 `UPDATE … SET last_seen` 把时间推到现在，再由**读**决定是否命中。
    命中不交给 `UPDATE` 的 change count：同一秒里的第二次提问写回的就是同一个值，而「有没有命中」
    不该依赖写操作是否自报为一次改变。中间插进来的并发写只会让结果变成「写之后的那一份」或
    「没有了」，不会返回一行库里已经不存在的记录。`remember`：`ON CONFLICT(lookup_key) DO UPDATE`
    覆盖这条记录的内容（Explanation 四件套）、`generated_at`、`artifact_version`、提示词标签，
    以及**逐字 Passage**——同一个 key 允许另一种首尾空白/换行写法，所以重新生成会把原文更新成这一跑
    真正用的那一份；**保留 `created_at` 与 `last_seen`**；glosses 先删后插，所以重新生成后条数变化
    不会留下旧卡。`list` 按 `last_seen DESC, id DESC`（秒级时间戳需要 id 兜底），`show(id)` 单条。
    **没有 `lookup_count`**：这不是 SRS。
  - `busy_timeout` 在 `journal_mode = WAL` **之前**设置：切 journal mode 要写文件，是最可能撞上
    另一个进程锁的那一条语句，它应该是等的那个而不是立刻失败的那个。
  - 读回时等级 / 思考 / 时间戳解析失败 → `StoreError::Corrupt` 如实报错，不猜一个默认值，并**带上
    记录 id**（历史库在磁盘上、可以手编；把 `Z9` 悄悄当成 B2 等于篡改这条记录声称的东西，而 id 是
    一个人唯一能拿来定位这一行的把手）。修法是改那一行，或对同一段文本再跑一次 `--regenerate`：
    upsert 不读旧行，会把它覆盖掉。
- `plainly-core::hashing`（私有）：`prompt_version` 与 Lookup Key 共用「长度前缀 + SHA-256」。
  两处各写一遍的话，"为什么字段不能互相挤"就有两份说法；现在只有一份。
- `plainly` CLI：
  - `explain` 先算 `Lookup`，命中就把**复用对象的 provenance 一并说出来**
    （`reusing the stored Explanation — provider/model (thinking …), 等级, label@hash8; generated …;
    nothing was sent`）并打印那一行，退出码 0；否则照常跑，成功后先 `remember` 再打印（先存后印：
    答案没进库就等于下次还要再付一次钱，而历史兼任缓存的意义正是不发生这件事），且打印的是
    `remember` 返回的那条**已存记录**——重新生成保留原来的 `created_at`，印新鲜的会与 `history
    show` 自相矛盾。`--regenerate` 跳过命中——但仍然写入，所以是**原地覆盖**。
  - 命中时**不报契约档位**：Record 不带档位（spec §9 的字段表里没有），而这一跑根本没有请求；
    声称一个档位就是在描述一次没发生的调用。
  - 顺序是：配置与 key 检查 → **打开历史库** → 读输入 → 命中 / 生成。key 检查留在读输入之前，沿用
    既有的「不要为一个永远不结束的管道挂住」；代价是**缓存命中也需要已配置**（退出码 3 不因为历史里
    有答案而豁免）。**开库失败**（不是数据库、只读、schema 建不起来）是退出码 1 并**带上路径**，
    运行中途的读写失败只报原因——本地状态要人去修，不是 provider 问题。
  - `history`（无子命令＝`list`）与 `history list`：一行一条
    `id  last_seen  level  provider/model  thinking  label@hash8  Passage 摘要`——题面要求列表
    带 provenance，`hash8` 是因为同一个 label 下附录不同只有哈希能区分。`history show <id>`：stdout
    出与 `explain` 相同的五节 markdown，stderr 出一行 provenance（provider/model/thinking/等级/
    **完整的提示词哈希**/created/generated/last seen）——列表为了行宽缩写，这里是唯一能把版本读全
    的地方。找不到 id → 退出码 2（用法错误，理由同"文件读不到"）。
  - `history` 不需要 provider，也不需要配置文件存在：本地的就是本地的。

## 给下游的交接

- **tickets/09（搜索、标签、硬删）**：FTS5 加在同一个库上；`records` 的硬删只要
  `DELETE FROM records WHERE id = ?`，glosses 已经被外键 CASCADE 带走。标签表加在 `SCHEMA` 里即可
  （`CREATE TABLE IF NOT EXISTS`）。
- **tickets/10（导出）**：`Store::list()` / `show()` 给的 `Record.artifact` 已经够 markdown 与原始
  CSV；Anki CSV「每个 Gloss 一行」正对着 `glosses` 表，一条 SQL 出卡。
- **tickets/11（长输入）**：一块一条 Record；`Lookup` 按块算 key，所以只有变化的那块需要重新生成。
- **tickets/15（面板）**：复用同一套 `Lookup` + `Store::recall` / `remember`；面板有自己的
  `Secrets`，但历史库是同一个文件（WAL + `busy_timeout` 已经设好）。**不要**在面板里另写一份
  "命中就用、否则生成"的判据——`Lookup` 与 `Store` 就是那份判据。
- **tickets/17（加固）**：WAL 由自动化测试读回（测试自己开一条连接读 `PRAGMA journal_mode`）；真正的
  多进程并发读写在自动化测试之外。

## 已知取舍

- **`list()` 没有上限也没有分页**：本地单用户库，记录条数由用户自己决定。真到了需要分页的规模再
  加（09 的搜索与 17 的加固是自然的位置）。
- **schema 没有 `PRAGMA user_version`**：v0 只有一版形状，读旧行的判据是**每行自己的**
  `artifact_version`。真正需要迁移（加列、改列）时再引入版本号——现在加只是一个没人读的数字。
- **一行坏数据会让 `list()` 整体失败**，而不是跳过那一行：宁可如实报错，也不静默丢一条记录。
  错误里带记录 id，`show(id)` 也仍能读到没坏的那条，所以坏行不会带走整库，也不会找不到是哪一条。
- **`--regenerate` 时若 provider 失败，旧记录原样保留**：没有生成就没有可覆盖的东西。这也是
  "旧答案不会被偷偷改写"的另一面。
- **并发只保证到「一条记录是完整的」**：`recall` 的 bump 与随后的读是两条语句，中间插进来的写会让
  结果变成写之后的那一份（或没有了），但不会是一半旧一半新。真要在面板与 CLI 同时写同一行时更强，
  得把 recall 提到写事务里（`BEGIN IMMEDIATE`），代价是命中也要排队——v0 不值这个价。
