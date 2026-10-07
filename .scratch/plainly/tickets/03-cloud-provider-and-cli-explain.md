# 03: 云端 provider 与 CLI explain

**What to build:** `echo "hard sentence" | plainly` 真的从云端拿回一段解释并按五节 markdown 打印；`plainly explain file.md`、`--format json` 也能用，退出码可脚本化。

**Blocked by:** 02

**Spec:** [spec.md](../spec.md) §7 Provider 层、§10 CLI 契约

**Status:** done (2026-10-07)

- [x] 预设 provider（DeepSeek / OpenAI / llama.cpp / Ollama / LM Studio）可直接选，也能自定义 base URL 与 model id；默认 `deepseek-flash` 且默认关闭思考
- [x] 关闭思考用规范写法 `{"thinking":{"type":"disabled"}}`（不用 `{"thinking":false}` 一类的写法）
- [x] 能强制 schema 的 provider 用**嵌套**的 `response_format.json_schema.schema`；DeepSeek 只能被要求 JSON（`json_object`，答案由我们校验）；`max_tokens` ≥ 1200（默认 2048）；temperature 0
- [x] 无子命令时默认走 `explain`；输入支持 stdin 与文件路径
- [x] stdout 只出产物、stderr 出人类信息；`--format markdown|json` 都可用
- [x] 退出码：`0` 成功 · `1` provider / 生成失败 · `2` 用法错误 · `3` 未配置；无输入且 stdin 是 TTY 时报用法错误而不是默默阻塞
- [x] CLI 子进程级测试可用环回上的假 provider 跑通，不需要真实 key

## 落地形态

- `plainly-core::presets`：五个预设各自带 `endpoint`、`model`（可选）、`schema`
  （`Enforced` / `BestEffort`）、`thinking`（能否说规范写法）、`key`（是否必须有 key）。
  预设是**起点**不是静态能力表：tickets/05 的探针会覆盖 `schema`，本地预设的 `model`
  由 tickets/06 的发现填上。
- `plainly-core::setup`：`ProviderSetup::resolve(name, &Config)` 把预设与 `[providers.<name>]`
  合并，用户配置逐字段优先；名不见经传又没配置的名字、缺 endpoint、缺 model 各有各的报错。
  **能力跟着 endpoint 而不是名字走**：预设描述的是它自己那个 endpoint（按 host:port 判定，
  路径差异不算换服务）。用户把某个名字指到别的机器上时，`schema` / `thinking` / `key`
  三个能力字段一律退回「不知道就别发」的那一档（要 schema、不发未知字段、不强制 key），
  `model` 默认值保留——它是用户可覆盖的默认，不是能力。消息里的 `label` 同理：只有服务
  一致时才用厂牌名，endpoint 被改走之后用配置里的名字，免得 127.0.0.1 的 400 印成
  「DeepSeek answered」。理由与 spec §7 的「endpoint 改动使探测缓存失效」是同一条。
- `plainly-core::prompt`：出厂提示词是标定过的 v6-synthesis（两个占位符 `{{LEVEL}}` /
  `{{NATIVE}}`），`version()` 是**未替换占位符**的文本的 SHA-256——占位符的取值本来就是
  Lookup Key 的独立分量。tickets/07 把用户附录与等级描述表折进这个哈希。
- `plainly-core::chat`：唯一 HTTP 适配器（ureq，阻塞、rustls 自带根证书，因此 CLI 运行期
  只依赖 libc）。请求体是公开 seam：嵌套 schema、`temperature: 0`、`max_tokens: 2048`、
  规范 thinking 写法。答案读取覆盖 LM Studio 的 `reasoning_content` 回退（**只认解析成
  JSON 对象的**：思维链是散文，那不是答案）、OpenAI 的 refusal（含数组形态）与
  `status: "incomplete"`、`finish_reason: "length"` 的截断、以及空 content。错误体回显前
  先**按值**擦掉这次请求真正带出去的那个 key（provider 会把 key 原样写进 401，而 stderr
  会进 CI 日志），再过一遍启发式以覆盖不是我们的凭据；`ApiKey` 包了一层，使将来给客户端
  加 `Debug` 也不会打印它。
- `plainly` CLI：`explain` 的参数同时挂在根命令上（clap flatten），所以
  `echo … | plainly`、`plainly --format json`、`plainly passage.md`、
  `plainly explain passage.md` 都走同一条路径。顺序是：定输入源（TTY 报用法错误）→
  读指名文件（路径错就是用法错误）→ 读配置 / 解析 provider / 取 key → **最后**才读 stdin：
  缺 key 时不该先把一个可能很大、甚至永不结束的管道读干。配置解析失败（不是合法 TOML）
  也算「未配置」（退出码 3），脚本因此能分清"我的配置文件坏了"与"provider 挂了"。
  `thinking = on` 落在没有开关的 provider 上时在 stderr 明说一行。

## 给下游的交接

- **tickets/04（失败分类与重试）**：`ChatCompletions` 一次请求一次答案，不做重试；
  重试策略应坐在 `Provider::generate` 之上，才能同时看见传输失败与契约失败。
  截断与 `status: incomplete` 已经是**独立于契约**的失败（不会被误判成"畸形 JSON"）；
  「模型只产出了思维链」也是 provider 失败而非畸形，04 要决定它算不算可重试的一类。
  单请求 60 秒超时（`chat::TIMEOUT`）现在随传输层，因为一个会无限期挂住的阻塞客户端
  是最基本的问题；退避与错误分类仍归 04。
- **tickets/05（能力探测）**：请求形状现在取自预设的已知形态，而且**只在 endpoint 的
  host:port 与预设一致时**才生效（DeepSeek 走 `json_object`，其余走嵌套 `json_schema`）；
  探针落地后应覆盖 `SchemaSupport`，并把结论写进 cache 目录——用户把名字指到别的机器时
  现在退回谨慎档，正是探针要补上的那一格。
- **tickets/06（本地 provider）**：llamacpp / ollama / lmstudio 三个预设**故意不预设 model**，
  缺 model 时 CLI 以退出码 3 明确要求 `providers.<name>.model`。Ollama 的
  「原生 `/api/chat` + `format`」路径还没做，现在走的是它的 OpenAI 兼容面
  （也因此不会碰「关思考静默禁用 format」那个 < 0.31.2 的坑）。
- **tickets/07（提示词附录与版本）**：`ExplainRequest.system_prompt` 已经是应用填好的成品文本，
  `prompt_version` / `prompt_label` 也已随请求走；tickets/07 只需换掉 `prompt` 模块的来源与哈希输入。
- **tickets/08 / 12**：`--regenerate` 与 `--clipboard`（退出码 4）不在本票范围内。

## 已知取舍

- `--format` 与文件参数只在根命令与 `explain` 子命令两处各定义一次（clap flatten 的代价）；
  两者同时写时以子命令的为准。
- 一个不是子命令的词会被当作文件路径（`plainly frobnicate` → "cannot read frobnicate"，
  退出码 2），这是"explain 是默认命令"的直接后果，换来 `plainly passage.md` 可用。
- 「默认关闭思考」只在**知道规范写法**且 **endpoint 仍是预设那台机器**的 provider 上真的
  上到 wire（目前只有 DeepSeek）。其余情况没有已知开关，发送一个未知字段会被 400 拒绝，
  所以宁可不发——05/06 的探测来定。代价记下：把 `deepseek` 指到同一服务的另一个域名
  （代理、镜像）会同时失去 `json_object` 与「关思考」，在 05 之前要自己配
  `providers.<name>.schema` 之类的能力是做不到的（能力目前不可配置）。
- `KeyRequirement::Optional`（本地预设、未配置的 provider、以及 endpoint 被改走的预设）：
  keyring 答不上来时**不当失败**，只在 stderr 说明并按无 key 发送；`Required`（DeepSeek /
  OpenAI 两家的原装 endpoint）仍然把 keyring 故障当失败，因为那里要修的是 keyring，
  而不是把请求匿名发出去。
- 没有预设的 provider 默认按 `json_schema` 发送、且不发送 thinking 字段：我们不知道对方的
  调用面，所以不发送任何可能被 400 拒绝的东西——这正是 tickets/05 要探测出来的。
- 退出码 3 现在也覆盖「配置文件不是合法 TOML」。`ConfigError::Io`（读不出文件）与
  `ExternallyModified`（并发写冲突）仍落在 1，因为五个码里没有"本地状态写不下去"这一格；
  这条缺口留在票据里，不用第六个码糊过去。
