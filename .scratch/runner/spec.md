# Runner 规格：把「一个 Passage 的回合」收进 core

Status: ready-for-agent
Labels: ready-for-agent

> 由 2026-10-18 的三轮 grilling 坍缩而成（架构评审候选 1「一次 Passage 的完整运行没有主人」）。结构取舍见 [ADR-0003](../../docs/adr/0003-the-runner-owns-one-runs-machine-state.md)。
> 领域词按仓库根 `GLOSSARY.md`；本规格不新增 domain 词条——`Runner` / `Turn` 是代码名，不是学习者领域的词（ADR-0003）。
> 不改变 **Explanation** 的契约与存储形状：ADR-0001 仍是硬约束，ADR-0002 的 `example` 规则不受影响；`ARTIFACT_VERSION`、store schema 与 FTS 都不动。
> 可感知的行为变化只有三处：store 写失败、凭据三态、面板内的重复 Passage（见 Implementation Decisions §3/§4，以及 Further Notes 的变化清单）。

## Problem Statement

同一个 **Passage** 在 CLI 与面板上会得到不同的结果，而用户只能看到一个入口：

- **store 写失败时，两个入口给相反的答案。** CLI 先打印已经付过钱的 **Explanation**，然后中止整批；面板则把这一块记为失败、丢掉刚拿到的 Explanation，继续下一块。用户为一个答案付了钱，却可能在任何一边都看不到它——或者看到了，却不知道它没有入库、下次还要再付一次。
- **keyring 读不到时，面板静默把它当作「没有 key」。** 用户看到的是「这个 provider 不能用」，无法区分「我还没设 key」和「keyring 坏了」；CLI 会说明，面板不会。同一台机器、同一个 **Provider Profile**，两个入口对「能不能跑」的判断不一致。
- **同一次触发里重复的 Passage，面板会付两次钱。** CLI 按 **Lookup Key** 去重，面板没有做；同一段话在剪贴板里出现两次，面板就生成两次、覆盖同一个 **Record**。
- **更深一层：这些分叉不是三个 bug，是一个结构问题。** 「这台机器上有什么」和「一个 Passage 的回合怎么走」在 CLI 与面板各装配一遍：配置、provider、凭据、prompt、**store**（同时是缓存）、能力缓存、时钟。两份已经漂移，而面板那一份在 CI 之外——改一次运行语义，维护者必须记得改两处，且没有任何测试能证明两处一致。

## Solution

把「一个 Passage 的回合」收进 core 的一个 deep module：`Runner`。它一次性拥有这台机器上的运行状态——路径、配置快照、解析后的 provider、凭据、prompt、history store、能力缓存、运行时刻——并对一个 Passage 执行 recall → generate → remember，把结果作为值返回。CLI 与面板退回适配层：Passage 从哪里来、长输入怎么切、哪些块是重复的、进度与错误怎么措辞、界面画什么，仍归它们。

三处已经漂移的行为按同一套规则统一；panel 的运行路径第一次进入 CI 覆盖的 core；两个表面里各一份的装配与 helper 删除。除这三处外，CLI 与面板的输出、文案、顺序、退出码保持不变。

## User Stories

### 两个入口给同一套答案

1. 作为用 CLI 的人，我想要 store 写失败时仍然看到已经付过钱的 Explanation，以便我的钱不白花。
2. 作为脚本作者，我想要 store 写失败仍然以非零退出码结束，以便我能检测到「答案没有入库」。
3. 作为面板用户，我想要答案未入库时面板明说，以便我知道下次还要重新生成、而不是以为它已经在历史里。
4. 作为面板用户，我想要同一次触发里重复的 Passage 只生成一次，以便我不为同一段文本付两次钱。
5. 作为 CLI 用户，我想要 keyring 读不到与「没有 key」给出不同的提示，以便我知道该修 keyring 还是去设置 key。
6. 作为面板用户，我想要 keyring 读不到时面板说出来，而不是静默把 provider 判成不可用，以便我不会去排查一个并不存在的问题。
7. 作为学习者，我想要同一个 Passage 从 CLI 与从面板得到相同的成功/失败判定，以便两个入口可以互相信任。
8. 作为本地模型的用户，我想要「没有 key 也可以继续」的行为保持不变，以便离线 runtime 仍能直接工作。

### CLI 行为保持不变

9. 作为 CLI 用户，我想要 `--regenerate` 仍然绕过缓存、原地覆盖同一条 Record 并保留 `created_at`，以便行为不变。
10. 作为 CLI 用户，我想要 `--factory-prompt` 仍然只影响本次运行的 prompt 版本，以便行为不变。
11. 作为 CLI 用户，我想要缓存命中的文案仍然是「reusing the stored Explanation」，以便我的脚本与眼睛都不受影响。
12. 作为 CLI 用户，我想要批次的 stdout 顺序、markdown/JSON 形状与逐块 stderr 进度保持不变，以便输出可以继续被管道消费。
13. 作为脚本作者，我想要退出码 0/1/2/3/4 的既有含义不变，以便我不用改脚本。
14. 作为 CLI 用户，我想要 keyring 的警告仍然只出现在 stderr、stdout 仍然只有产物，以便它继续可以接管道。
15. 作为长文本用户，我想要 CLI 继续不继承面板的 5 块上限，以便整篇文件仍能一次跑完。

### 面板行为保持不变（除三处统一之外）

16. 作为面板用户，我想要 ⟳ 仍然优先切到可用的云端 provider，以便本地模型出错时能一键换。
17. 作为面板用户，我想要 ⟳ 的云端 provider 解析失败时看到一块清楚的错误，而不是整块消失或整次触发失败，以便我知道这一块没跑成。
18. 作为面板用户，我想要运行级的凭据 notice 只显示一次，而不是每块重复，以便它不吵。
19. 作为面板用户，我想要面板的正文、标题栏、复制、位置、自动消失与退出路径都不变，以便这次改动不需要重新学习界面。
20. 作为面板用户，我想要面板仍然每次触发重读配置，以便设置页里改的东西下一次按键就生效。

### 维护者与结构

21. 作为维护者，我想要「这台机器上有什么」只在一个 module 里装配，以便改一次两个表面都生效。
22. 作为维护者，我想要 CLI 的 explain 路径不再自己打开配置、store、缓存或 secret store，以便「CLI 是薄的」这句话可以被 grep 验证，而不是靠阅读。
23. 作为维护者，我想要面板的运行策略进入 CI 覆盖的 core，以便它不再只靠手测清单。
24. 作为维护者，我想要三处统一后的行为由 core 测试钉住，以便未来改动不会悄悄回退。
25. 作为维护者，我想要 `explain::run`（probe → retry → downgrade）的 interface 与测试原样保留，以便这次改动不触碰已经测好的下层。
26. 作为维护者，我想要 `Runner` 可以用注入的路径、secret store 与运行时刻驱动，以便测试不需要真实 keyring、真实网络或系统时钟。
27. 作为维护者，我想要凭据的三种状态（拿到 / 缺失 / 读不到）是值而不是预拼好的句子，以便各表面措辞不同但判断相同。
28. 作为维护者，我想要「未入库」是 turn 的结果而不是异常，以便答案不会被一起丢掉。
29. 作为维护者，我想要时钟仍然以值传入、`Timestamp::now()` 是唯一的系统时钟转换入口，以便 core 不直接读系统时钟。
30. 作为维护者，我想要重复 Passage 的去重逻辑留在表面、但两个表面都按同一个 Lookup Key 规则执行，以便不为此新增一个批量 module。
31. 作为维护者，我想要这次结构决定记录在 ADR 里，以便后来人不会把它改回「每个表面各装配一遍」。
32. 作为未来实现 agent，我想要 core 里有一个小 interface（open、explain + 少量结果类型）可以读，以便不打开两个二进制就能改动运行语义。
33. 作为未来表面作者，我想要在 core 里找到唯一的「打开运行、跑一个 Passage」入口，以便不用第三次拼装 machine state。
34. 作为 CI 维护者，我想要新的 core 测试不需要显示器、compositor 或网络，以便 CI 与 `nix build` 的 checkPhase 都能跑。

## Implementation Decisions

### 1. Module 与职责边界

- core 新增 module `runner`，类型 `Runner`；它是一次运行中 surface 唯一需要打开的 module。
- `Runner` 拥有：路径（`Paths`）、配置快照（`Config`）、配置中 provider 的 `ProviderSetup`、凭据解析（`Secrets`）、`Prompt`、history store、能力 `Cache`、运行时刻。
- surface 保留：Passage 的来源（stdin / 文件 / 剪贴板，以及剪贴板拒绝后的结果）、切分与面板 5 块上限、批次循环与重复 Passage 的规划、进度与错误措辞、stdout 格式、退出码、UI 状态与 widget。
- `Runner` 组合现有 core 的 `explain::request`、`explain::run`、`Store::recall` / `Store::remember`、`Cache`、`Secrets`、`Prompt`；不重排 `explain` / `probe` / `retry` 的分层，`explain::run` 的公共 interface 保持不变。
- `Runner` / `Turn` 是代码名，**不**进入 `GLOSSARY.md`。

### 2. 构造、环境发现与失败分类

- 生产构造：`Runner::open(prompt, now)`——内部通过 `Paths::discover()` 与 `Secrets::from_process_env()` 取得环境；顺序为 config → 配置中 provider 的 setup → 凭据 → store → 能力缓存 → prompt。
- 可注入构造：`Runner::open_at(paths, secrets, prompt, now)`——测试用（将来 desktop 若需要也可用）。
- `PromptChoice::{Configured, Factory}`：CLI 的 `--factory-prompt` 映射为 `Factory`；面板永远 `Configured`。
- 失败分类 `RunnerError`：`Paths` / `Config` / `Store` / `Setup` / `Credential`。`Credential` 再分 `Missing` 与 `Unreadable`。表面映射：CLI 的 `Missing` → exit 3，`Unreadable` 与其他 → exit 1，配置解析失败 → exit 3，配置值错误 → exit 2；面板一律进入失败状态。
- **配置中 provider 的凭据在 open 时立即解析**，以保住 CLI「没有 key 时不必等 stdin 结束」的现有行为；`Bypass` 指定的另一个 provider 的凭据在 turn 时解析。
- 运行时刻由表面用新增的 `Timestamp::now()`（返回带原因的 `Result`）取得并传入；core 仍不直接读系统时钟。CLI 中 `providers` 的探针路径也改用这个 helper，删除 CLI 本地重复的 `now()` 实现。

### 3. 一次 turn 的语义

- `Recall` 进入 core：`Reuse`（按键、复用缓存）与 `Bypass { provider: Option<String> }`（⟳、绕过缓存，可选换 provider）。它是 turn 级选项；`--factory-prompt` 属于构造期选择。
- `Turn` 携带：artifact、`record: Option<Record>`、`from_cache`、`resolution: Option<Resolution>`、`downgrade: Option<Downgrade>`、`notices`。
- `TurnFailure` 有两种：`Run(RunFailure)`——沿用现有 probe/retry/downgrade 的字段与语义；`Unavailable { provider, error }`——只用于 `Bypass` 指定 provider 的 setup / 凭据解析失败。
- `Reuse` 命中：`Store::recall` 更新 `last_seen`，不调用 provider，`from_cache = true`，`record = Some`，`resolution = None`。
- `Reuse` 未命中：走 `explain::run`，成功后 `Store::remember`，`record = Some`，`resolution` / `downgrade` 来自本次 run。
- `Bypass`：不读缓存；`remember` 原地覆盖并保留 `created_at` 与 `last_seen`（与今天 `--regenerate` 一致）。
- **store 写失败不是 turn 失败**：`Turn` 仍携带 artifact，`record = None`，附 `NotStored` notice；批次是否继续由表面决定——CLI 继续并在结尾以非零退出码说明有未入库/失败块，面板继续并显示答案 + 警告。不新增 `Store` trait；测试用「打开后把目录设成只读」制造失败。
- 重复 Passage：表面按 Lookup Key 去重，每个 occurrence 仍得到 artifact。CLI 保留现有 stderr 提示；panel 静默复用同一结果（不加新文案）。去重逻辑不进入 `Runner`。
- 凭据三态：`Missing` 与 `Unreadable` 分开。Required + 读不到导致 open 失败；Required + 缺失导致 open 失败（CLI 映射 exit 3）；Optional + 读不到 → open 成功，运行级 notice，turn 继续不带 key；Optional + 缺失 → 静默继续（现状）。绝不把「读不到」静默当作「缺失」。
- 运行级 notice（凭据）由 `Runner` 持有并暴露给表面，只显示一次；turn 级 notice 只放 `NotStored`。
- CLI 运行前需要的事实（provider label / model / thinking / 是否本机）通过 `Runner` 暴露已解析的 `ProviderSetup` 读取，不新造一个事实类型。

### 4. 表面映射

- **CLI explain**：用 `PromptChoice` 与 `Timestamp::now()` 打开 Runner；运行级 notice 打印到 stderr 一次；缓存命中沿用现有文案；新生成沿用现有 report 行（tier / downgrade / local）；store 失败时打印 artifact、stderr 说明未入库、继续后续块、结尾 exit 1；`TurnFailure::Run` → exit 1、后续块继续；`RunnerError` 按 §2 映射。output / format / 进度 / 来源选择 / 分块 / 去重全部不变。
- **Panel worker**：构造 Runner；运行级 notice 通过 worker 事件交给 UI，在状态行显示一次；`Turn` 映射为「有答案 / 失败」两种块结果，`NotStored` 时仍展示 artifact 并附警告；⟳ 映射为 `Bypass { provider: 面板选出的云端 provider }`；worker 内的配置 / setup / 凭据 / store / cache / 时钟装配全部删除，只留线程、channel、事件与映射。
- 两个表面的 explain 路径都不再直接打开配置、store、缓存或 secret store。

### 5. 测试 seam

- **新增且只新增一个 seam：`Runner` 的 interface**（core 集成测试）。它位于 CLI 与面板共同调用点，是这次改动能取到的最高 seam；core 的公共 API 本来就是 spec 的第一条测试缝，Runner 是它的新入口。
- **保留既有 seam、不替换**：`explain::run` / probe / retry 的 core 测试原样；CLI 二进制的端到端测试（spec 的 seam B）原样；面板的纯决策测试原样。不新增 GTK 测试，也不新增 `Store` trait seam。
- 这个 seam 已在 grilling 中确认（第一轮 Q6、第三轮 Q4），此处按同一结论落地。

### 6. 迁移与删除

- CLI explain：删除 paths / config / setup / 凭据 / store / cache / 运行时刻的装配；保留 source、分块、批次规划、去重、打印、报告、退出码。
- Panel worker：删除配置 / provider / 凭据 / store / cache / 运行时刻；保留线程、channel、事件、Recall 映射。
- CLI `commands` 中的 store / cache 打开 helper 仍服务 `history` / `providers` 子命令，保留；本地 `now()` 在改用 `Timestamp::now()` 后删除。
- 不改 Explanation schema、`ARTIFACT_VERSION`、store schema / FTS、prompt、split、provider 层；不新增迁移。
- 面板 `Prefs` 与 worker 各读一次配置的窗口保持现状（见 Out of Scope）。

### 7. 验收判据

- 在 explain 路径里，除 `runner` 外不再出现 `Paths::discover` / `ConfigFile::open` / `Store::open` / `Cache::new` / `Secrets::from_process_env` / 本地 `now()` 的调用；这条判据可持续用 grep 检查。
- CLI 现有二进制测试保持 green（它们同时是「除三处行为变化外，其余输出逐字不变」的证据）；新增 store 写失败的 CLI 测试。
- 新增的 Runner 测试通过；panel 的既有测试与 `nix build` checkPhase 通过。
- 三处统一后的行为由 core 测试钉住，不依赖手测清单。

## Testing Decisions

- **什么算好测试**：只测外部行为——provider 被调用几次、`Turn` 的字段、store 里最后留下什么、CLI 的 stdout/stderr/exit code。不断言内部调用顺序、不测私有 helper。假件只用既有缝：scripted provider / endpoint、内存 store、注入的路径与 secret store、临时目录。新的 seam 不引入新的 mock 层。
- **被测 module**：`runner`（新增，主 seam）；CLI 二进制（既有 seam + 一条新的 store 失败用例）；面板纯决策（不动）；现有 core 测试（explain / probe / retry / store / render / export 等）不动。
- **prior art**：`explain::run` 的测试用 fake endpoint 驱动 probe / retry / downgrade；store 测试用临时数据库；CLI 的集成测试用真实二进制 + 临时 XDG 目录 + loopback 上的 HTTP stub；面板的判定测试不依赖显示。新的 Runner 测试沿用同一批假件。
- **用例清单**：
  1. open 的五类失败：paths / config / store / setup / credential（Required 缺失、Required 读不到、Optional 读不到 → 成功 + notice）。
  2. 缓存命中：`from_cache = true`、provider 零调用、`last_seen` 移动、`record = Some`。
  3. 新生成：`record = Some`、provider 恰好一次、artifact 元数据正确。
  4. `Bypass`：绕过缓存、原地覆盖、`created_at` 保留。
  5. `Bypass` 指定 provider：使用覆盖后的 setup 与凭据；不可用 → `TurnFailure::Unavailable`。
  6. store 写失败：artifact 仍返回、`record = None`、`NotStored` notice；用只读目录制造，不引入 trait。
  7. retry 失败透传：`TurnFailure::Run` 携带 resolution / reprobe / downgrade。
  8. 凭据三态：用注入的 secret store（含一个读不到的 fake）各测一条。
- **CLI**：新增一条 store 写失败用例——stdout 仍有 artifact、stderr 说明未入库、退出码非零；其余现有测试不改。
- **Panel**：worker 缩成适配层后不再需要单元测试；纯决策测试与手测清单不变。
- **时钟**：不为系统时间本身写测试；`Timestamp` 的范围行为由既有测试覆盖，`Timestamp::now()` 只是同一转换的入口。
- **CI**：`plainly-core` 与 `plainly-cli` 在 CI；Runner 测试不依赖显示、compositor 或网络，因此同时可在 `nix build` 的 checkPhase 运行。

## Out of Scope

- desktop 主窗口的状态与它自己的配置 / store 打开。
- `providers` / `history` / `config` 子命令的装配。
- 面板 `Prefs` 与 worker「各读一次配置」的窗口（评审候选 5 的触发流程）。
- 面板触发策略搬进 `state`（候选 5）。
- 渲染形态的 owner（候选 3）。
- flake 中 CLI 第二份 workspace（候选 4）。
- 把批次 / 去重搬进 core 的新 module。
- 新增 `Store` trait seam 或内存 store 的替身层。
- Explanation schema、`ARTIFACT_VERSION`、store schema / FTS、prompt、split、provider 层的任何改动。
- notice 的具体文案与 UI 排版（行为在范围内，措辞与样式不在）。
- 任何新的产品功能（本地模型质量、思考模式、导出格式等）。

## Further Notes

- **三处行为变化，其余不变**：(1) CLI 在 store 写失败后继续批次并保留答案；(2) 面板在 keyring 读不到时显示 notice，不再静默；(3) 面板对同一次触发内的重复 Passage 复用同一结果、不再重复付费。除此之外，现有测试是「没有别的变化」的证据；任何额外的 stdout/stderr/退出码差异都应被视为 bug。
- **为什么现在做**：CLI 与面板的装配已经漂移，而面板的运行策略在 CI 之外；这是评审里唯一一个「两个真实生产 adapter + 行为已经分叉 + 约束不冲突」的候选。
- **结构记录**：ADR-0003 记录「core 拥有 Runner」这个决定及其被否掉的替代方案（各表面装配、trait-based session、凭据留在表面、把时钟读进 core、继续叫 `Session`、把批次放进 Runner）。本规格记录行为细节，ADR 记录结构理由。
- **词表**：`GLOSSARY.md` 不变。`Runner` / `Turn` / `Recall` / `Notice` 都是实现名；`Passage`、`Explanation`、`Record`、`Lookup`、`Lookup Key`、`Provider Profile` 继续按词表使用。
- **接续**：本规格之后由 `/to-tickets` 切成垂直切片（不预先切片）。评审报告只存在于临时目录，有意不入库；本规格与 ADR 是持久记录。
