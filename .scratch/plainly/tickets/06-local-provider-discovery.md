# 06: 本地 provider 与端口探测

**What to build:** 我不必知道自己机器上的运行时跑在哪个端口——应用自己找出来，把可选的模型和它们的能力列给我，选中之后同一段文本走本地出解释。

**Blocked by:** 05

**Spec:** [spec.md](../spec.md) §7 默认端点与探测

**Status:** done (2026-10-07)

- [x] 探测一组常见端口（3060 / 11434 / 8080 / 1234），把发现的运行时列出来
- [x] 读模型列表端点拿到每个模型的 `loaded|unloaded` 状态与上下文长度，因此不加载模型也能判断上下文够不够
- [x] 用户能选到**具体模型**（不是只选端点），选择写进配置的 provider 段
- [x] 同一段文本在本地 provider 上产出完整 Explanation，且 provenance 显示为本地模型
- [x] 本地探测不到时给出可操作的说明，而不是静默失败或猜一个端口

## 落地形态

- `plainly-core::discover`：**去哪找**与**找到了什么**，HTTP 由 `ModelLister` 注入（缝 A）。
  - `COMMON_PORTS`＝3060（本项目自己的 llama.cpp sidecar）／11434（Ollama）／8080（llama.cpp
    默认）／1234（LM Studio），顺序即提问顺序、报告顺序与同名冲突的裁决顺序。
  - `candidates(config)`＝常见端口 **＋ 用户已配置的 loopback 端点**。后者不是猜测，是用户自己的
    答案，没有它，一个跑在自选端口上的运行时既列不出来、也选不了。去重按 **(name, endpoint)**：
    按「同一 authority」合并会让用户的某个 provider 名字变得不可选，代价只是同一服务被不同写法
    问两遍（loopback 上一次极小 GET）。
  - `scan` 只把「答了话且答的是模型列表」算作运行时：连接被拒、超时、200 但不是列表，都不是。
    空列表**是**答案（运行时在，什么都没有）。返回的是 `Scan { runtimes, unanswered }`——**没答话
    的候选者也留着**，因为「没人听」与「有人应答但拒绝了我们」是两个事实，把后者报成前者，给出的
    补救（在那儿启动一个）正好是治不了病的那一种。
  - `select(runtimes, provider, model, endpoint, configured) -> Selection` 是整条选择规则，五种
    结果：`Use`（名字所指的那个有模型）、`Silent`（用户配置的端点没答话，而同名的另一个答了）、
    `Elsewhere`（用户的端点没有这个模型，同名的另一个有）、`Unserved`（同名里谁都没有）、
    `Unanswered`（这个名字下什么都没答话）。名字的裁决顺序：先精确拼写，再「同一 service」
    （`http://127.0.0.1:8080` 与 `.../8080/v1` 是一个地方），名字指向的没答话才退到同名里第一个。
  - **凭据不由本模块决定**：`ModelLister::models_at(endpoint, provider)` 把 provider 名一起交给
    传输层，因为密钥属于名字。core 自己一个 key 都不查。
- `plainly-core::chat`：
  - `ModelReader` 是「还没有模型可选时」的读取器：只读 `{endpoint}/models`，`DISCOVERY_TIMEOUT
    = 2s`（扫描要按端口一个一个问，一个只收连接不回话的端口不该按整请求超时计费），凭据**每次
    调用传入**而不是自带——它属于候选者的名字，取哪一把是调用方的事。
  - `models_from` 补上了**本机 llama.cpp router 的真实形状**（实测，不是文档）：`loaded` 在
    `status.value`，上下文长度只在模型启动 argv 里（`--ctx-size 16384`，两种写法都读）。这正是
    「不加载模型也能判断上下文够不够」的依据：3060 上那两个模型是 `unloaded` 而上下文已经写着
    16384。
  - 判断「够不够」与预算住在一起，且**分思考档**：`completion_budget(thinking)`、
    `PROMPT_TOKENS = 2048`（出厂提示词＋150 词一块，实测 1000 出头，向上取整）、
    `min_context_length(thinking)`（off 4096／on 10240）、`context_fits(model, thinking)`。
    只给一个数字会把「思考态要按思考预算给足」（spec §7）漏掉：4096 在默认档够用，在详尽档不够。
- `plainly-core::setup`：`endpoint_of(name, config)` 是 `resolve` 里**不需要模型**的那一半——
    选择模型这件事必须先知道 provider 在哪，而本地预设故意不命名模型。两者共用一份 `sources()`
    查找，免得对「这个名字指向哪个端点」有两套说法。另有 `is_loopback`（`127.0.0.0/8`、`::1`、
  `localhost`）与 `ProviderSetup::is_local`，「本地」是端点的事实而不是名字的事实。
- `plainly` CLI：
  - `providers discover`：stdout 出列表（运行时 → 端点 → 每个模型的 `loaded|unloaded` 与
    `context N`，不够时标 `— too small for one Passage at thinking off/on`，判据用该 provider
    **配置里的**思考档），stderr 先报「在哪些端点找」，一个都没找到时退出码 3 并给出两条可操作的
    路（在那儿启动，或 `config set providers.<name>.endpoint`）。不猜端口：没答话的端口不会被写进
    任何地方。
  - `providers use <provider> <model>`：把「选中的模型」落到两个地方——`providers.<name>.model`
    与 `app.provider`（后者不写，「选中之后同一段文本走本地」就是假的）。模型必须在运行时**当场
    列出的**清单里（否则退出码 2 并列出它有什么，最多 10 个 id 加 "and N more"）；端点只在发现到
    的**不是同名端点所指的同一个服务**时才写下去。不是 loopback 的 provider 直接拒绝并指向
    `config set providers.<name>.model`。
  - **凭据**：扫描用的 `CandidateReader` 为每个候选者的**名字**解析密钥（环境变量 > keyring >
    本次会话，与一次运行同一条链）。spec §7 要求 llama.cpp sidecar 带 `--api-key` 跑在 loopback，
    不带 key 的探测对这套推荐配置只会得到 401——把「没找到」报成结论就是撒谎。
  - `explain` 的 provenance 多一句「— local model on this machine」（`ProviderSetup::is_local`）。
    **只报出处，不搬警示语**：面板正文那条「可能自信地错」属于面板（spec §12、tickets/15）。

## 给下游的交接

- **tickets/14、15（设置页与面板）**：列表来自 `discover::scan`（`Scan { runtimes, unanswered }`
  的产物：`LocalRuntime { provider, endpoint, models }` 与 `EndpointModel { id, loaded,
  context_length }`），「名字对应哪个运行时」用公开的 `discover::select` / `Selection`（不是私有的
  helper），「够不够」用 `chat::{context_fits, min_context_length, completion_budget}`，「是不是
  本地」用 `setup::is_local` 加 `presets::preset`。面板有自己的 `Secrets`，所以它要自己建一个
  `ModelLister`（`ModelReader` 已经是「每次调用传 key」的形状）。
- **tickets/07（提示词）**：未被本票碰过。
- **tickets/11（长输入）**：`PROMPT_TOKENS` / `min_context_length` 就是「上下文 × `max_tokens` ×
  150 词阈值」这三者配对的地方；改阈值要看它们。
- **tickets/17（加固）**：自动化测试只 bind 随机 loopback 端口，常见端口那一段是**手测**的——本机
  3060 上的真 llama.cpp router 被 `discover` 找到（`unloaded` + `context 16384`），`use` 选中了 4B
  模型，`echo … | plainly` 产出完整五节、冷启动 8.8 秒、provenance 写着本地模型。同机 2B 模型那几
  次**没有在 60 s 单请求上限内答话**（重试 3 次后如实报 `timeout`，`/v1/models` 显示它仍在
  `unloaded`→`loaded` 之间），所以「60 s 覆盖本地冷启动」这条 spec §7 的假设在这台机器上不是对每
  个模型都成立；要不要给本地冷启动更宽的预算，依据留给 tickets/17。

## 已知取舍

- **上下文不够只警告，不拒绝**：运行时可能服务得比它自报的多（`--ctx-size` 只是启动参数），所以
  列表与 `use` 都如实标出来，把决定留给用户。相应地，**`max_tokens` 也没有按模型上下文裁剪**——
  tickets/05 交接里「上下文与 `max_tokens` 的配对」目前兑现成「按思考档给出判据并如实告知」，不是
  「改写请求预算」。
- **Ollama 关 thinking 的 ≥ 0.31.2 版本下限仍未实现，而且现在够不到**：Ollama 预设是
  `ThinkingSwitch::Unsupported`，`chat` 从不给原生路由发 `think`，所以没有可关的东西。等真给
  Ollama 接上思考开关时，这个下限与「`num_ctx` 只能走 Modelfile／`OLLAMA_CONTEXT_LENGTH`」要一起
  处理；本票只把后者变成了选择时的一条可操作提示。
- **发现的凭据按名字解析，并发给候选端点**：常见端口上若蹲着别的进程，它会收到用户为
  `llamacpp`／`ollama` 等名字准备的那把 key。接受的理由：名字是用户或预设自己选的，而一次运行本来
  就会把同一把 key 发给同一 URL；且只扫 loopback。不这么做，本项目的推荐 sidecar 配置就发现不了。
- **去重按 (name, endpoint) 而不是按服务**：同一台机器上如果 `127.0.0.1:11434` 与
  `localhost:11434` 都被配置过，列表里会出现两遍（两次 GET）。换来的是任何一个配置过的名字都不会
  因为合并而变得不可选。
- **`use` 要求运行时当场答话**：运行没起来就没法预先选模型（这正是「不要猜」的代价）。要预先写，
  用 `plainly config set providers.<name>.model`。也没有 `--endpoint` 旗标：端点用 `config set`
  写，下一次扫描自然把它算成候选。
- **用户指过的端点不会被静默挪走**：`providers.<name>.endpoint` 已配置却没答话、而同名运行时在别处
  答了话时，`use` 拒绝（退出码 3）并给出「要在那个端点选就把它配置过去」的整条命令。只有名字本来
  只是预设的猜测（没配置过）时，才采用答话的那个并把端点写出来——那正是本票要的「我不必知道端口」。
- **扫描前在 stderr 报一行去哪找**：`discover`／`use` 的成功路径因此不是「stderr 全空」。代价是一行
  人类信息，收益是一个只收连接不回话的端口不会让人以为命令挂了。
