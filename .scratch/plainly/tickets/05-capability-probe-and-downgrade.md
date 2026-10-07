# 05: 能力探测、缓存与降级

**What to build:** 应用自己试一次就知道这家 provider 能不能强制 schema，把结论缓存起来并如实告诉我「哪些 provider 敢承诺契约、哪些只是尽力而为」。

**Blocked by:** 04

**Spec:** [spec.md](../spec.md) §7 Provider 层（探测、缓存、各 provider 怪癖）

**Status:** done (2026-10-07)

- [x] 探针发一个极小请求（`json_schema` 试探 + 读模型列表 / 上下文），结论写进 cache 目录，**不**写进配置文件
- [x] DeepSeek 的 `json_schema` → HTTP 400 会落到「提示词 + 校验 + 重试」档并如实告知；llama.cpp → 200 走强制档
- [x] 不支持或探测失败时降级为 `json_object` + 校验 + 重试，且界面上与强制档可区分
- [x] 4xx 参数不受支持、手动重探、endpoint / model 改动都会让缓存失效；清空缓存不丢用户的 provider 选择
- [x] 各 provider 调用怪癖各有一条测试：LM Studio 的 `content` 为空时回退 `reasoning_content`；OpenAI 解析前先查 refusal 与 `status: incomplete`；Ollama 优先原生 `/api/chat` + `format`（`/v1` 作回退）；DeepSeek 只发 `json_object`
- [x] 思考被建模为 `on | off | unsupported`，默认 off，接口能表达它（以后才加得上「详尽档」）
- [x] `plainly providers` 显示当前 provider、模型、thinking 与探测结论；`providers probe [name]` 强制重探

## 落地形态

- `plainly-core::probe`：结论的类型与策略本体，HTTP 一行都不碰。
  - `Capability { schema, thinking, models, probed_at }`——**这就是**「这家 endpoint 敢承诺什么」。
    `SchemaSupport::Enforced` 与 `BestEffort` 两档就是「强制档」与「提示词 + 校验 + 重试」档；
    `ThinkingSwitch` 表达 thinking 是「说得动」（`Canonical`）还是「没有已知开关」（`Unsupported`），
    与用户设置 `Thinking::{On, Off}`（默认 off）正交——spec §7 的 `on | off | unsupported`
    在代码里就是「设置 × 能力」这一对，`unsupported` 是能力那一侧、不写进配置。
  - `ProbeEndpoint`（`probe` + `models`）是探针的传输半；`Endpoint: Provider + ProbeEndpoint`
    是「一次运行」需要的全部传输，`ChatCompletions` 与测试替身都自动满足（blanket impl）。
  - 探测顺序，以及每一步为什么：
    1. **先发最强形状**（嵌套 `json_schema`，不带 thinking）。预设的 `schema` 只是**离线兜底**：
       探针的存在理由就是「同一家 API 会变」，所以不拿预设当证据（spec §7 明确拒绝静态能力表）。
    2. 收到 400/422 → 再发一次 `json_object`。200 即 `BestEffort`；**这正是 DeepSeek 的信号**
       （`json_schema` 400、`json_object` 200）。两次都被拒 = endpoint 什么形状都不接受：
       当作「探测失败」（`Origin::Assumed` + 谨慎档），**不写缓存**。
    3. **只有在预设敢承诺规范写法、且这条路由真能携带那个字段时才试 thinking**
       （目前只有 DeepSeek，而它走的就是兼容面）。一次 200 **证明不了**开关有效——endpoint
       完全可以收下字段再忽略它（`{"enable_thinking":false}` 就是这种），所以 200 不是证据、
       400 才是：探测这一步只为**收回**预设的说法。Ollama 的原生路由不带规范写法，去问就是
       在问一个没人会发的请求，所以那里连问都不问，预设的说法原样保留为**未验证**。
    4. 读 `{endpoint}/models`，记下 id 与运行时自愿给出的 `loaded` / 上下文长度
       （`max_context_length`、`context_length`、`n_ctx`、`meta.n_ctx`）。模型列表是 discovery
       不是 capability：读不到只是列表短一点（tickets/06 在此之上做端口扫描与选择界面）。
       读不到记成 `models: None` 而**不是空列表**——「这家什么都没有」与「谁也没读到」
       是两个事实，报告与 tickets/06 要分开说。
  - **写缓存的判据是「endpoint 答了话」，不是「答得好看」**：`json_schema` 或 `json_object`
    被接受 → 写；两种都被 400 → 不写（什么都没学到）；传输错误 / 401 / 5xx → 不写。
    理由：缓存**没有 TTL**，把一次偶发失败写成「这家只能尽力而为」就等于永久降级。
    **thinking 那一问没问成时整份结论都不写**（`Resolution::unwritten` 说明原因）：schema 那一问
    是答了，但把「今天没验证过的厂牌说法」写进无 TTL 的缓存，就等于把一次打嗝变成「已探测」的
    事实。这一跑仍然用那个说法（打嗝不该等于「这家说不动能不说」），下一跑再花一次廉价探测。
  - `Cache`：`appCacheDir/providers/<name>.json`，一个 provider 一个文件。文件里记着
    **endpoint + model**，读的时候对不上就是 miss——`endpoint` / `model` 改动使缓存失效**是结构性
    的，不靠谁记得去删。读不出来 / 解析失败 / 将来版本的字段变化一律当 miss（结论可重算，
    所以「意外」的答案是**再探一次**，不是报错）。写失败不致命：`Resolution::unwritten`
    把原因带出去，由表面说出来。
  - 缓存文件的两处额外小心，都不在票据字面上但很便宜：provider 名来自配置或命令行，
    写文件名前把非 `[A-Za-z0-9_-]` 换成 `_`（否则 `../x` 能把缓存写到别处）；写入走
    同目录临时文件 + rename，临时名带 pid（CLI 与面板可以同时探同一个 provider）。
  - `probe.rs` 只回答「这家 endpoint 是什么」；**拿它怎么办**在
    `plainly-core::explain::run`：`explain::explain` 是一次尝试，`explain::run` 是表面真正
    调用的整条运行（探针 → 重试策略 → 一次降级重问），所以 CLI 与面板不会各长一份。
    `connect` 闭包是唯一注入点（API key 属于表面）。`FailureKind::UnsupportedParameter`
    （tickets/04 留的钩子）触发降级：先 **`cache.forget`**（错的结论不能再用第二次），再强制重探；
    **只有重探「答了话」、且新结论会真的改变下一次请求**才重问一次。两个条件各有理由：
    重探自己没答上话时（`Origin::Assumed`）谨慎档只是个猜测；而新结论改不动请求时
    （`Thinking::On` 下 thinking 字段本来就不发、或路由根本不携带它）重问的是一条逐字节相同的
    请求——正是 tickets/04 拒绝浪费的那一次调用。两种情况都不重问，如实报错，
    并把重探结果与新结论一起带在 `RunFailure` 里。
    **失败时报告的是「那次被拒的请求所携带的能力」**（`RunFailure::resolution`），不是重探后的
    猜测；重探结果单独放在 `RunFailure::reprobe`，两者一起交给表面措辞。
  - `Result<Run, Box<RunFailure>>`：错误里带着当时的能力与降级事实，装箱是因为它是
    每请求一次的事件，不值得为省一个指针去裁字段。
- `plainly-core::chat`：
  - `Route::{OpenAi, OllamaNative}` + `RouteOutcome::{Answer, Missing, Failed}`：
    **404 不是答案**（endpoint 没有这条路由），所以换下一条路由再问；其余错误照实返回。
    这条区分让「回退」不是「重试」。
  - **Ollama 原生面**：`format` 承载 schema（对象 = 强制档，字符串 `"json"` = 尽力档），
    `stream: false`，预算与 temperature 进 `options.num_predict` / `options.temperature`。
    **不发 `think`**：0.31.2 以下它会连 `format` 一起静默关掉，版本下限归 tickets/06。
    答案从 `message.content` 读，`done_reason: "length"` 与截断的 JSON 前缀都报为截断。
  - `ProbeEndpoint`：`probe` 只**看状态码**（探针问的是调用面，不是答案好坏）并走
    路由回退；`models` 读 `{endpoint}/models`，**只读兼容面**（见已知取舍）。
- `plainly-core::setup`：`ProviderSetup` 多一个 `surface`，多一个
  `with_capability(schema, thinking)`——**只**替换探针能回答的两个字段，其余（模型、thinking 设置、
  是否需要 key、endpoint）一个都不动，免得某个表面重新拼一遍时静默丢字段。
- `plainly` CLI：
  - `plainly providers`（无子命令）读缓存并如实报告：provider / endpoint / model / thinking +
    switch / contract 档位 + 探测时间 + 模型列表；**没探过就说「not probed yet」并指向
    `plainly providers probe`**，而不是显示一个没人建立过的能力。它**不发网络请求**：
    展示不该替用户去打 endpoint。
  - `plainly providers probe [name]` 强制重探（忽略缓存），报告走 stdout、进度走 stderr。
    探不到（`Origin::Assumed`）时**退出码 1**：脚本问「这个 endpoint 能用吗」应当听到回答；
    报告仍然先打出来，写清探测失败的原因与临时采用的档位。
  - `explain` 的 stderr 每跑至少说一次**这一跑被哪一档兜着**
    （`contract: enforced by the endpoint …` / `contract: best effort (json_object, then our
    own validation and retry)`）——「endpoint 敢承诺契约」与「我们自己校验并重试」不是同一个
    承诺，不说的那一跑让人分不出来（spec §7、user story 42）。顺序是**先因后果**：
    「探测不到」/「endpoint 拒了形状」两行在前，`contract:` 一行在后（没有异常时它就只剩
    这一行），再往后才是重探结果、缓存写不下去的提示与 thinking 开关的说明。
    `Failure` 措辞、重试提示、退出码都不变（tickets/03/04 的契约）。

## 给下游的交接

- **tickets/06（本地 provider 与端口探测）**：`Capability.models` 已经是标准形状
  （`Option<Vec<EndpointModel>>`，`EndpointModel { id, loaded, context_length }`，
  `chat::models_from` 容忍各家字段名、已从 crate 根再导出），
  端口扫描与「选中具体模型写进配置」在它之上做。Ollama 的「关 thinking 要求 ≥ 0.31.2」
  与 `num_ctx` 的位置（Modelfile / `OLLAMA_CONTEXT_LENGTH`）都还没做，`native_request_body`
  里留了说明。上下文与 `max_tokens` 的配对（tickets/04 的已知取舍）也归它。
- **tickets/15（面板状态）**：面板要说的档位、降级事实、探测失败原因都在
  `Resolution` / `Downgrade` / `RunFailure` 上，措辞由面板写；「本地模型可能自信地错」
  那条提示属于面板自己的知识（`setup.surface` 与 provider 名都在手上）。
- **tickets/07（提示词）**：`ExplainRequest` 与 `prompt::*` 没被本票碰过。

## 已知取舍

- **冷缓存的第一跑要多发几个极小请求**（最多 3 个探测 + 1 个模型列表 + 真正那一次）。
  这是 spec §7「首次配置某 provider 时发一个极小探针请求」的直接代价，换来的是不靠静态能力表。
  缓存无 TTL，所以每个 endpoint 只付一次。
- **surface 跟着 provider 名，不跟着 endpoint 走**（与 `schema` / `thinking` / `key` 相反）。
  理由：surface 是「Plainly 怎么和这家说话」，不是「这家接受什么」；把 `ollama` 指到远端
  Ollama 仍然要 `/api/chat`，而真的不是 Ollama 的 endpoint 由 `/api/chat` 的 404 + `/v1` 回退
  兜住。代价诚实记下：**把一个不是 Ollama 的 endpoint 配成 `ollama` 时，每次请求会多一次
  注定的 404**——这条路径的收益（远端 Ollama 可用）大于代价（自己配错的 endpoint 多一个廉价请求），
  且不做「路由结论也进缓存」这种第二套缓存。
- **探测失败不写缓存**，所以 endpoint 不可达时每跑一次就重探一次。这是有意的：把
  「暂时问不到」写成结论会让一次网络抖动变成永久降级。反过来，**被 400 拒到底也不写缓存**，
  因为那种情况等于什么都没学到。
- **thinking 只能被证伪，不能被证明**。200 可能只是 endpoint 收下字段后忽略它，
  所以只有预设敢承诺的写法才会被发出去，探针只能把它收回。想给更多 provider 加「详尽档」
  得先有人在真机上验证写法，而不是靠试探。
- **探测顺序里 thinking 放在 schema 之后**：两条假设不能混在一次请求里问，
  否则一个 400 说不清是谁被拒（DeepSeek 上就会把 thinking 开关错判成不支持）。
- **降级重问上限是一次**。第二次仍被拒就如实报告，不做循环；`Max_extra_attempts`
  与退避预算仍归 tickets/04，降级不会放大它们（两层各自算）。
- **`RunFailure` 是装箱的**，因为错误里有整份能力（含模型列表）。调用方多一次解引用，
  换来的是不必为了 clippy 的尺寸阈值裁掉「当时用的是哪个形状」这个事实。
- **模型列表只读 OpenAI 兼容面**（`{endpoint}/models`），不读 Ollama 原生 `/api/tags`：
  两家都服务兼容面，而两份可能互相矛盾的列表需要一个「谁赢」的规则，v0 不值当。
- **回退条件就是 404，不是「任何 Misconfigured」**：401/403 在原生面上直接报错，
  不会再去 `/v1` 问一遍同一件不行的事。代价是「原生路由存在但要求另一种鉴权」这类
  设想要靠 404 之外的行为自己浮出来，收益是不为一个猜想多花一次请求。
- **缓存文件的两处小心属于「便宜的正确性」，不是本票要求的功能**：名字转义与临时文件 +
  rename。它们各自五行左右，挡掉的是「命令行传进来的名字把缓存写到别处」与「两个进程同时
  探测同一个 provider 读到一个写了一半的文件」。不加它们也能跑，但会留下两个只在特定时刻
  才现形的问题；记在这里，免得读代码的人以为票据要求过。
- **`explain` 每一跑都在 stderr 说一次档位**，哪怕没探测、没降级：这是「如实告知」的落地，
  代价是 stderr 多一行固定文案（stdout 仍然只有产物）。
