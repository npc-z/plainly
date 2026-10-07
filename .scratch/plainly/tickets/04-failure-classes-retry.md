# 04: 失败分类、重试与不静默回退

**What to build:** 面对畸形输出、系统性漂移、不受支持的参数、限流与超时，应用各走各的路，并把「发生了什么、重试过几次」如实带回用户面前。

**Blocked by:** 03

**Spec:** [spec.md](../spec.md) §7 错误分类与重试

**Status:** done (2026-10-07)

- [x] 输出畸形（JSON 解析 / schema 校验失败）重试，最多 2 次额外
- [x] 第二次错误与第一次**完全相同**时立即停止，不再重试（断言实际调用次数，不白跑）
- [x] 429 / 5xx / 超时退避重试，最多 2 次额外，间隔约 1 秒 / 4 秒；单请求超时 60 秒（覆盖本地冷启动）
- [x] HTTP 4xx 参数不受支持时不重试，而是**分类为**「能力假设错了」交给 tickets/05 降级并重新探测
- [x] 失败结果是结构化的（人类可读原因 + 是否已重试 + 可区分的分类），并携带「什么都没存、输入没被改动」这一事实
- [x] 不做静默回退：失败暴露在真正失败的那次查询上（reason 取自最后一次尝试）
- [x] 思考态下的 token 预算被照顾到（`reasoning_tokens` 可占 completion 的 90%+），长句不会因截断被误判成「畸形」

## 落地形态

- `plainly-core::retry`：策略本体。`retry::explain(provider, request, now, pause)`
  是表面唯一该调的入口——它把「一次尝试」（`explain::explain`）与「契约校验」一起包在
  循环里，因为重试只有同时看见传输失败与契约失败才能分类。等待通过 `pause` 闭包注入，
  所以测试读的是**schedule 本身**而不是真的睡过去；时钟仍是调用方给的 instant。
  - 四类各走各的路：畸形/空答案 → 立即再来（最多 2 次额外）；答案形状连续两次完全相同
    → 停（`Stopped::Repeated`）；400/422 拒绝请求形状 → 一次都不重试（`NotRetryable`）；
    429/5xx/超时/连接断开 → 退避 1 秒、4 秒（`Stopped::Exhausted`）。
  - **重复即停只适用于答案形状**。429 重复正是退避存在的理由，把它按「系统性漂移」掐掉
    会让 spec 那一行退避表变成死代码——这条判断写在 `FailureKind::repeats_mean_drift` 上。
  - 退避序列按**已经等过几次**索引，不按总尝试次数：一次畸形重试不会把第一次 429 推到 4 秒。
- `plainly-core::provider::ProviderError` 从一个不透明的 message 变成
  `kind` + `message`，kind 就是策略分支的依据：`UnsupportedParameter` / `Unavailable` /
  `Misconfigured` / `Empty` / `Refused` / `Truncated`。tickets/03 的「provider 层分类随 04 到来」
  到此兑现。
- `plainly-core::retry::Failure`：给表面用的结构化结果——`kind`、`reason`（最后一次尝试的
  人话原因）、`attempts`、`stopped`（为什么不再试），外加 `retried()`。失败的唯一产物就是它，
  **没有 Artifact**；而 Artifact 是 Record 的唯一来源（tickets/08），所以「什么都没存」
  是结构性的而不是一个字段：面板拿到 `Err` 就等于拿到这个事实，措辞归 tickets/15。
  CLI 把它说出来（见下）。
- `plainly-core::chat`：把 HTTP 状态与传输错误翻译成 kind：400/422 → `UnsupportedParameter`；
  408/429/5xx → `Unavailable`；401/403/404/413 → `Misconfigured`。**传输错误按 io kind 判**，
  不按 ureq 的变体判：DNS 解析失败在 ureq 3.4 里走的是 `Error::Io`（`resolver.rs` 的
  `to_socket_addrs()?`），`HostNotFound` 只在「解析成功但结果为空」时出现。只有
  「重发同一条请求可能成功」的 kind（ConnectionRefused / Reset / Aborted、TimedOut、
  WouldBlock、Interrupted、UnexpectedEof）才算 `Unavailable`；其余 io 错误、`HostNotFound`、
  `BadUri` 都是 `Misconfigured`。一个拼错的域名因此**不再白等 5 秒**再告诉用户「稍后重试」，
  而一个还没起来的本地运行时仍然拿到重试。
  **信封本身坏掉**（200 却不是 JSON、没有 choices、没有 message）归 `Empty`——不是模型的错，
  但可能是一次抖动，问第二遍很便宜，而重复即停让确定性的坏信封只花两次调用。
  **未标名的截断**（Ollama 的 OpenAI 兼容层在 length 截断上返回 `finish_reason: null`）
  靠 `serde_json` 的 `is_eof()` 认出来——内容是一个在中间断掉的 JSON 前缀——并归
  `Truncated`，否则它会被契约层报成畸形答案，重试策略再拿同一个已经耗尽的预算去问第二遍。
  `answer_from` 是客户端的方法，因此截断错误**指名当时那档预算**。思考档的 `max_tokens`
  是 `MAX_TOKENS_THINKING = 8192`（普通档仍是 2048），两个下限都写成 const 断言。
- `plainly` CLI：`retry::explain` + 真实 `pause`（等待 ≥1 秒时在 stderr 说一行「asking again
  in Ns」，于是重试看起来不像卡住）。失败时输出：原因 +（tried N times … / not retried …）
  + 「Nothing was stored and the input was not changed.」。

## 给下游的交接

- **tickets/05（能力探测与降级）**：`FailureKind::UnsupportedParameter` 就是探针要挂的钩子
  ——「这个 endpoint 不接受我们假设的形状」。本票只做到「不重试 + 如实分类」，降级与
  重新探测（含写 cache）归 05；它可以直接用 `retry::Failure.kind` 分支。
- **tickets/08（历史与 Lookup Key）**：失败路径没有 Artifact，因此没有任何行可写。若将来
  要把「已重试」记进历史，`attempts` 已经在 `Failure` 里。
- **tickets/11（长输入切分与 CLI 批次）**：批次的每一块各自调用 `retry::explain`，
  所以「一块失败其余继续」时每块的重试预算互不干扰。
- **tickets/15（面板状态）**：面板的「已重试情况 + 剪贴板内容未变、什么都没存」直接由
  `Failure` 的字段 + 面板自己的剪贴板知识组成；本票只提供事实，不写界面文案。

## 已知取舍

- **400/422 一律算「形状被拒」，不去嗅探 body**。误判的代价不对称：猜「形状」最坏是多做
  一次廉价探针（05），猜「不是形状」最坏是某个 endpoint 永远用不了。模型名写错通常会
  返回 404 或 400 带 "model"；前者已经是 `Misconfigured`，后者会走一次降级后照样把真实
  报错带给用户。
- **`max_tokens` 的两个值仍与 150 词阈值成对**（spec §6）：普通档 2048、思考档 8192。
  改任何一个都要看切分阈值。
- **预算跟着用户的档位（`setup.thinking`）走，不跟「这个 endpoint 会不会自己思考」走**。
  一个默认就会推理的模型落在没有开关的 endpoint 上时，默认档仍只给 2048，于是会截断——
  但截断现在是**被正确分类的**失败，错误里指名当时的预算，打开详尽档即可拿到 8192。
  反过来（switchless 一律给 8192）会在小上下文的本地运行时上把整条请求顶掉
  （Ollama 默认 4k）；上下文与预算的真正配对是 tickets/06 的探针要做的事。
- **重复即停比较的是结构化的 signature，不是 Display 文本**（`retry::Signature`）：
  契约失败按「serde 停在哪」或「schema 报了哪些问题」比较，provider 失败只按 kind 比较。
  provider 的 message 会带 response body、request id、摘录，同一桩麻烦每次措辞都可能不同；
  拿文本当判据会让系统性失败一路重试到预算耗尽。代价是同一类的两次空答案即使措辞不同
  也算重复——这是想要的方向（宁可少花调用）。
- **JSON 前缀一律算截断**，不管 `finish_reason` 写了什么（`length`、`null`、还是撒谎的
  `stop`）：`is_eof()` 区分的是「解析器把输入用完了」与「遇到不是 JSON 的东西」。代价是
  一个只吐了 `{` 的坏答案被报成截断而不是畸形，因而不重试——而这类答案重试也确实没用。
- **超时（60 秒）随传输层**，不随重试次数放大：三次尝试最坏 3×60 秒 + 5 秒退避。上限
  写在 `chat::TIMEOUT`，tickets/04 没有把它做成可配置项——v0 没有人要求过。
- `Failure` 不保留每一次尝试的错误，只保留最后一次的 reason 与次数。要展示完整历史是
  tickets/15 的界面问题，而现在没有第二个消费者。
