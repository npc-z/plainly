# Provider readiness 规格：让 core 回答「这个 provider 现在能用吗」，并把配置数字的写侧校验收回来

Status: ready-for-agent
Labels: ready-for-agent

> 由 2026-10-18 的四轮 grilling 坍缩而成（架构评审候选 2「让 core 回答这个 provider 现在能用吗」，以及报告点到的第二处「同一判定没有 owner」：数值边界）。结构取舍见 [ADR-0004](../../docs/adr/0004-provider-readiness-is-one-value.md)。
> 领域词按仓库根 `GLOSSARY.md`；本规格不新增 domain 词条——`Readiness` 是代码名，不是学习者领域的词（同 ADR-0003 的 `Runner`）。
> 本规格装两片独立的改动、一个 spec 收口：**A. provider readiness**（含三处已分叉的边缘行为对齐）；**B. 配置数值边界的写侧**（core 拥有合法域；读侧容忍保持不变）。
> 两片可分别落地；ticket 由 `/to-tickets` 切，不预先切片。
> 前置：候选 1（`Runner`，ADR-0003）已在 `651b48e` 落地。本规格把运行路径之外仍各自推理的「可用性」收进 core，并补上候选 1 明确出界的 `providers` / desktop 判定。

## Problem Statement

同一台机器、同一份 `config.toml`，三个表面各自回答「这个 provider 现在能不能用」，而且答案不总是一样：

- **桌面设置页**自己推 `KeyView` 与 `first_run`：它先看环境变量里有没有 key，再去问 keyring；一个不是合法 UTF-8 的 key 会显示成「环境变量里有、可用」，真正运行时却得到「凭据读不到」。
- **面板**自己推 ⟳ 的云端候选：它按 preset 名字取「这个 provider 要不要 key」，而运行时是按配置里的 endpoint 解析后取 requirement 的。把一个云端 preset 的 endpoint 改到本机之后，运行时把它当成本地、不需要 key，面板却仍按 Required 判断——同一份配置，两个结论。云端候选的「云端」也按 preset 自带的 endpoint 判断，而不是按配置里实际会去的 endpoint：一个被指到 `127.0.0.1` 的云端 provider 仍可能被承诺「换成云端」。
- **CLI** 的 explain 路径已经在候选 1 里统一（`Runner` 拥有凭据三态）；但 `providers probe`、`discover`、设置页与面板仍各有一份判定。
- **本机性**有三份写法：`ProviderSetup::is_local`、桌面 `view::is_local`、面板内联的 `endpoint_of + is_loopback`；同一个 provider 在不同界面可能被贴上不同的「本机」标记。

第二处是数值边界：

- CLI 可以写进设置页拒绝的值：`plainly config set app.panel_autohide_seconds 301` 会成功（exit 0），设置页拒绝 301，而面板读到 301 时静默回退到 20 秒。用户以为自己在文件里设了 301，面板却按 20 秒在跑，设置页还把 301 原样显示出来。
- 这不是「三套数字」：设置页与面板的范围常量本来就相同（5..300 / 320..900 / 240..1200）。差别在**行为**：core 的 `set` 只拒 0，设置页写前拒绝，面板读时修复。而「读时容忍手改文件」是有意写下的决定（面板必须总能出现、总能关掉）——真正的洞只在写侧：同一把写入口，CLI 在外面没有设置页那层校验。

## Solution

core 新增一个 `readiness` module，用一个值回答一个 provider 的可用性：**能解析吗 / 要不要 key / key 在不在、存储读不读得到 / 是否本机**。三个回答者改为读这个值、只把值翻成各自的语言：

- 桌面设置页的 `KeyView`、`usable`、`first_run` 与 provider 选项的本机标记；
- 面板 ⟳ 的云端候选与本机提示；
- 桌面历史/详情里的「本机」标记；
- `Runner` 内部与 `Readiness` 共享同一个凭据分类器（`resolve_credential` 的公开形状不变），保证运行路径与展示路径永远不会给出两个规则。

它**不联网**：能力探测（`probe` / `Capability`）仍是另一条轴，由 `providers probe` 显式触发，readiness 只读配置与 secret stores。

数值边界的写侧收进 core：合法范围与校验属于 `config` 的 key 定义，`ConfigFile::set` 拒绝越界，CLI 不再能写出设置页拒绝的值。**读侧保持现状**——手改出来的越界文件仍被容忍：面板夹取宽高、时长回退到 20 秒，设置页原样显示但保存时按同一范围拒绝。规则一句话：**写严格，读容忍**。

除下面列出的四处边缘行为变化外，三个表面的输出、文案、退出码、界面状态都保持不变。

## User Stories

### 桌面设置

1. 作为桌面用户，我想在设置页看到「这个 provider 现在能不能用」的唯一答案，以便不用在三个界面之间互相印证。
2. 作为桌面用户，没有可用 provider 时我想直接落在设置页，以便第一次运行就知道该填什么。
3. 作为桌面用户，我想分清 key 在环境变量、keyring、还是只在本次会话内存里，以便知道它会不会在重启后消失。
4. 作为桌面用户，当环境变量里的 key 不是合法 UTF-8 时，我不想看到「可用」而运行时才失败，以便我在设置页就把问题修掉。
5. 作为本地模型的用户，Optional provider 的 keyring 坏掉时我仍然能跑，以便离线 runtime 不被一个读不到的存储挡住。
6. 作为云端 provider 的用户，Required key 读不到时我想知道是**存储坏了**而不是**我没设 key**，以便我修对地方。
7. 作为桌面用户，我想 provider 选项上的「本机」标记按实际 endpoint 判断，而不是按 provider 名字猜。
8. 作为桌面用户，历史与详情里的「本机」标记应与运行时看到的同一份事实一致。

### 面板

9. 作为面板用户，本地模型下面那条提示只有在**真的存在可用云端候选**时才说「换成云端」，以便按钮的承诺与行为一致。
10. 作为面板用户，当云端 provider 的 endpoint 被配置到本机时，不要再把它当作「云端」候选。
11. 作为面板用户，当 provider 的 endpoint 被指到别的远程服务时，⟳ 的可用判断应与运行时一致（endpoint 换了，preset 的「必须带 key」不再适用）。
12. 作为面板用户，keyring 读不到导致没有云端候选时，不需要额外编一句解释——运行级 notice 已经在状态行说明了配置 provider 的情况。
13. 作为面板用户，⟳ 在 hosted provider 上仍然是「用同一 provider 再答一次」，不被新的候选逻辑改变。

### CLI 与脚本

14. 作为 CLI 用户，`plainly providers probe` 对 Required / Optional 的既有 key 策略保持不变，以便脚本行为不漂移。
15. 作为脚本作者，`plainly config set` 写入设置页不接受的值时应当失败（exit 2），而不是写进一个会被其他表面静默改写的文件。
16. 作为脚本作者，边界值（300 / 320 / 900 / 1200 等）仍应被接受，以便合法配置不被误伤。
17. 作为 CLI 用户，越界写入的错误应说明该 key 的合法范围，以便我不查文档就能改对。
18. 作为 CLI 用户，`plainly providers status` / `discover` / `key` 的输出不变，以便既有管道与脚本不受影响。

### 配置与「写严格、读容忍」

19. 作为手改 `config.toml` 的用户，越界的宽高仍被面板夹到最近的边界、越界的时长仍回退到 20 秒，以便一个坏数字不会让面板消失或永远不消失。
20. 作为桌面用户，设置页仍显示文件里的原值，保存时按同一范围拒绝，以便「屏幕会拒绝的值」不可能由界面写出。
21. 作为维护者，我想「哪些值可以写」只有一个来源，以便 CLI、设置页、面板不会各学一半。
22. 作为维护者，我想「写严格、读容忍」是一条写下来的规则，而不是散落在三处注释里的巧合。

### 维护者与结构

23. 作为维护者，我想「能不能用」只有一个实现，以便改一次三个表面都变。
24. 作为维护者，我想 requirement 只有一个来源：解析后（endpoint 已过滤）的 `ProviderSetup.key`。
25. 作为维护者，我想本机性只有一个纯函数来源，以便历史行、设置页、面板与 CLI 不会各算一遍。
26. 作为维护者，我想 secret 不进入会进 `Serialize` 的展示值，以便 UI 层不可能把它带出去。
27. 作为维护者，我想 readiness 不联网，以便设置页与面板的按钮不产生没人要求的探测请求。
28. 作为维护者，我想 `Runner` 与 readiness 共用同一个凭据分类器，以便运行路径和展示路径不会再次分叉。
29. 作为维护者，我想三个范围数字只出现在 core 一次，以便边界只改一处。
30. 作为维护者，我想措辞仍归各表面（ADR-0003），core 只交事实，以便 CLI 的句子和窗口的中文不会互相绑架。
31. 作为未来实现者，我想 core 里有一个小 interface（`Readiness::of` / `usable` / `local_of`）可以读，以便不打开 GTK 窗口就能改动可用性规则。
32. 作为 CI 维护者，我想新的判定由 core 测试覆盖，不需要显示器、compositor、网络或真实 keyring。
33. 作为未来读者，我想 ADR-0004 解释为什么 readiness 是一个值，而数值边界的取舍在 spec 里，以便这两个决定都不会被下一次评审重新提出来。

## Implementation Decisions

### 1. readiness module 与职责边界

- core 新增 module `readiness`；公开类型 `Readiness` 与 `KeyPresence`；入口：
  - `Readiness::of(name: &str, config: &Config, secrets: &Secrets) -> Readiness`
  - `Readiness::usable(&self) -> bool`
  - `readiness::local_of(name: &str, config: &Config) -> bool`（纯函数，不读 keyring）
- 值的形状（决策够密，按形状记录；不是实现代码）：

  ```
  Readiness {
      name: String,
      requirement: KeyRequirement,               // 解析成功时 = setup.key
      resolution: Result<ProviderSetup, SetupError>,
      key: KeyPresence,
      local: bool,
  }
  KeyPresence = Found(KeySource) | Missing | Unreadable(SecretError)
  usable() = resolution.is_ok() && (requirement == Optional || key is Found)
  ```

- `Readiness` 是代码名，**不**进入 `GLOSSARY.md`；`Provider Profile`、`Passage`、`Explanation` 等继续按词表使用。
- surface 保留：措辞、退出码、UI 状态、面板云候选的**选择策略**（第一个可用的非本机 shipped preset）、以及面板对本机提示的排版。

### 2. readiness 的语义

- **resolution**：`ProviderSetup::resolve(name, config)` 的结果原样保留。解析失败（未知名字 / 没有 endpoint / 本地 preset 没选 model）是 `usable() == false` 的原因，`local` 仍可能为真。
- **requirement**：解析成功时取 `setup.key`（与运行路径完全一致，endpoint 被指到别处时 preset 的 `Required` 不适用）；解析失败时按同一 endpoint 过滤规则取 preset 的 requirement，没有 preset 或 endpoint 已指到别处时为 `Optional`。
- **key**：一次 `Secrets::resolve(name)`（环境变量 → keyring → 本次会话内存；某一层失败即停止，沿用现有语义），映射为 `Found(KeySource)` / `Missing` / `Unreadable(SecretError)`。
- **可用性矩阵**（`resolution` 成功时）：

  | requirement | key 找到 | 没有 key | 存储读不到 |
  |---|---|---|---|
  | Required | 可用 | 不可用（缺失） | 不可用（读不到） |
  | Optional | 可用 | 可用（无 key） | 可用（带「读不到」的原因） |

- **local**：`ProviderSetup::endpoint_of(name, config)` 解析出的 endpoint 交给 `is_loopback`；endpoint 解析不出时为 false。`local_of` 就是这一半，给只需要「本机」标记的调用方（桌面历史/详情行、provider 选项）。
- **不联网**：不调用 `probe`、不读 capability 缓存；`Capability` / `Origin` / `Resolution` 的语义不变。

### 3. 三面映射

- **desktop**
  - `KeyView` 从 `Readiness` 派生，不再直接问 `Secrets`，也不再先用 `environment_provides` 短路：
    - `Found(Environment)` → 现有「环境变量里有」文案；`Found(Keyring)` → `KEY_IN_KEYRING`；`Found(Session)` → `KEY_IN_SESSION`；
    - `Missing + Required` → `KEY_MISSING`；`Missing + Optional` → `KEY_OPTIONAL`；
    - `Unreadable` → `key_unknown(reason)`；
    - `needed = !usable()`（只有 Required + 缺失/读不到时为真；KeyView 只在解析成功时才构造）。
  - `ProvidersView::usable` = `Readiness.usable()`；`first_run = !usable()` 不变；`current_problem` 仍来自 `resolution` 的错误。
  - provider 选项的 `local` → `local_of`；历史/详情行的 `local` → `local_of(artifact.provider, config)`。
- **panel**
  - `ProviderChoice::of` 的 `local` → `local_of(config.app.provider, config)`；删除 `usable` 与 `preset_requirement`。
  - `cloud_alternative` 的条件改为：`!local_of(preset.name, config)` 且 `Readiness::of(preset.name, config, secrets).usable()`；候选项的 `local` 由 `local_of` 派生，不再硬编码 `false`。
  - 没有云端候选时不新增任何提示（按钮不出现）；配置 provider 的运行级 notice 仍由 `Runner` 发出。
  - `regenerate_recall`、`local_note`、⟳ 在 hosted provider 上的语义不变。
- **Runner**
  - `Runner::open` 与 `Runner::overridden` 的凭据判定改走同一内部分类器；`Runner` / `CredentialError` / `CredentialState` / `RunNotice` / `Turn` 的公开形状与可见行为不变。
  - `resolve_credential` 仍是公开入口（CLI 的 `providers probe` 继续用它），实现上只做「分类结果 + secret」的投影。
- **CLI**
  - `providers status` / `discover` / `key` / `use` 一律不动；`providers probe` 的 key 策略不变。
  - explain 路径已走 `Runner`，不在本规格范围内再改。

### 4. 数值边界：core 拥有写侧的合法域

- `config` module 拥有三个范围常量（沿用现有名字，作为 core 的公共常量）：自动消失 `5..=300`、面板宽 `320..=900`、面板高 `240..=1200`；默认值仍是 20 / 470 / 470。
- `ConfigFile::set` 的类型强制在既有 0 规则之后校验范围；0 的两条现有专用消息（「0 不是关掉的方式」「0 不是尺寸」）保留。
- 越界返回新的结构化错误 `ConfigError::OutOfRange`，携带 key、输入值、min、max 四个事实；Display 给 CLI 一句说明范围的英文。CLI 的错误映射把它归入 `Usage`（exit 2），与其它配置值错误一致。
- **desktop**：`SettingsPatch` 不再自己做范围比较；写入时捕获 `ConfigError::OutOfRange`，用现有的 `auto_hide_out_of_range` / `panel_size_out_of_range` 中文字符串渲染（key → 显示名），用户可见文案不变。任何 `set` 失败后重新读配置（与 save 失败同一处理），保证内存句柄不会留下「写了一半」的编辑。
- **panel**：`prefs` 的范围常量改为引用 core 常量；夹取（宽高取最近边界）与回退（时长越界 → 20 秒）的行为完全不变。
- **读侧不变**：`Config::parse` 仍接受 0..=u32::MAX；设置页仍显示文件原值；手改文件的容忍是「写严格，读容忍」的另一半。
- **文档**：`docs/manual-checks.md` 里「CLI 写 2000、面板按 900 收住」改为「手工把文件改成 2000 → 面板按 900 收住；`config set 2000` 现在 exit 2」；`default_config.toml` 的注释补一句 CLI 同样拒绝；desktop/panel 两处「复制常量是有意」的注释改写为对「写严格、读容忍」的说明。

### 5. 测试 seam

- **不新增第三条 seam**（沿用主 spec 的两条缝）。`Readiness` 的 interface 落在第一条缝（core 公共 API）上：它是 CLI、桌面、面板共同调用的最高点。
- 新增 core 集成测试 `readiness`（矩阵与等价性）；`config_file` 补范围的界内/界外；CLI 二进制测试（seam B）补越界写入；desktop 与 panel 的纯决策测试各自补齐，不新增 GTK 测试、不引入新的 mock 层。
- 已有的 `Runner` 凭据测试原样保留——它们同时是「重构没有改变运行路径」的证据。

### 6. 迁移与删除

- `readiness` module 新建；`runner` 的凭据判定改为共享分类器；`lib.rs` 导出新类型。
- desktop `settings` / `view` 删除对 `Secrets` 与 `endpoint_of` 的直接调用；`KeyView` 改为从 `Readiness` 构造。
- panel `provider` 删除 `usable` / `preset_requirement` / `is_loopback` 调用；`prefs` 改为引用 core 常量。
- `config` 新增范围常量、`OutOfRange` 与 `set` 校验；CLI 错误映射加一条；desktop `app` 删除范围比较并加错误映射；desktop `settings` 的常量移出（改为引用 core）。
- 不改：`Explanation` schema、`ARTIFACT_VERSION`、store schema / FTS、prompt、split、provider 层、`Runner` 的公开形状、任何退出码的既有含义（除越界写入的 0 → 2）。

### 7. 验收判据

- grep 可验：`desktop/settings` 不再出现 `secrets.resolve` / `environment_provides`；`desktop/view` 不再出现 `endpoint_of`；`panel/provider` 不再出现 `preset_requirement` / `is_loopback` / `secrets.resolve`；`panel/prefs` 不再有自己的范围字面量；`desktop/app` 不再有范围比较；范围数字只出现在 core 的 `config`；凭据判定 match 只出现在 `readiness` 的分类器与 `resolve_credential` 的投影。
- `cargo test -p plainly-core`、`cargo test -p plainly-cli`、`cargo clippy`（两个 crate）通过；`nix build` checkPhase 通过（含 desktop / panel 测试）。
- 四处边缘行为变化由测试钉住（见 Testing Decisions）；除此之外任何输出/文案/退出码差异都视为 bug。

## Testing Decisions

- **什么算好测试**：只测外部行为——`Readiness` 的字段与派生结论、`usable()`、`local_of`、`ConfigFile::set` 的接受/拒绝与错误事实、CLI 的 stdout/stderr/exit code、desktop 的 `SettingsView` 数据与 `update` 的结果、panel 的 `Prefs` 数据。不断言私有 helper 或调用顺序。
- **被测 module**：`readiness`（新增）、`config`（范围与 `OutOfRange`）、`runner`（既有凭据测试保持）、`plainly-cli` 二进制、desktop `settings`/`app`（数据层）、panel `provider`/`prefs`（纯决策）。
- **prior art**：
  - core 侧的注入式 secret store 与坏 keyring fake 已在 `runner` 测试与 CLI 单元测试里存在；
  - `setup` 测试已有 requirement × endpoint override 的表格式用例；
  - CLI 用真实二进制 + 临时 XDG 目录；
  - desktop 用 `with_secrets` 注入 env/keyring 替身与 `tests/window.rs` 的固定时钟；
  - panel 的 provider 测试用 `EnvSecrets` 与纯函数，不依赖显示。
- **用例清单**：

  **core `readiness`**
  1. Required × {env 找到、keyring 找到、session 找到} → 可用，`KeyPresence` 带对应的 `KeySource`。
  2. Required × {缺失、存储读不到} → 不可用；两者原因可区分（缺失带变量名，读不到带 `SecretError`）。
  3. Optional × 缺失 → 可用且 key 为 `Missing`；Optional × 读不到 → 可用且保留 `Unreadable` 原因。
  4. resolution × {未知名字、无 endpoint、本地 preset 无 model} → 不可用；local preset 无 model 时 `local` 仍为真。
  5. endpoint override：`deepseek` 指到 loopback → `local = true` 且 requirement = Optional；指到另一个远程服务 → `local = false` 且 requirement = Optional。
  6. `local_of`：`localhost`、`127.0.0.x`、`::1` 为真；LAN / 其它 DNS 名 / 未知 provider 为假。
  7. 与 `resolve_credential` 的等价性：对每个 requirement × key 状态，`Readiness.usable()` 与 `resolve_credential` 的结局同构（Required 缺失/读不到 ⇒ 不可用；Optional 读不到 ⇒ 可用）。
  8. `Readiness` 不触发 probe/网络；测试不注入 endpoint。

  **core `config_file`**
  9. 三个 key 的边界值（5/300、320/900、240/1200）接受；边界外（4/301、319/901、239/1201）拒绝。
  10. `ConfigError::OutOfRange` 携带的 key / 值 / min / max 正确；CLI 映射为 exit 2。
  11. 0 的两条专用消息不变；读路径仍接受越界值（手改文件仍能打开）。

  **CLI 二进制**
  12. `config set app.panel_width 5000` → exit 2、不创建/不修改文件；`config set app.panel_width 900` → exit 0。
  13. 现有 zero 测试与 `providers` 测试原样通过。

  **desktop**
  14. Required + 读不到 keyring → `first_run = true`、`KeyView` 为 unknown 文案；Optional + 读不到 → 仍 `usable`、文案 unknown。
  15. 非 UTF-8 的 env key → 不再显示为「环境变量里有」；Required 下 `first_run = true`。
  16. 既有 first-run / env / session / bounds 测试全部保持（文案与拒绝行为不变）。

  **panel**
  17. 坏 keyring 下云候选为 `None`（不 panic、不新增提示）；配置 provider 的 notice 仍由 Runner 决定。
  18. `deepseek` 被指到 loopback → 不再作为云候选（若 openai 可用则选 openai）。
  19. `deepseek` 被指到别的远程服务且没有 key → 仍作为云候选（requirement 为 Optional），与本机提示/⟳ 一致。
  20. `local_note` 仍只在配置 provider 为本地时出现。

- **不测**：真实 keyring、真实网络、真实显示；不新增 fake 层。
- **CI**：core / cli 在 GitHub CI；desktop / panel 测试由 `nix build` checkPhase 运行（候选 1 已确立）。

## Out of Scope

- CLI `providers status` / `discover` / `key` / `use` 的任何输出或行为变化（包括给 status 增加 key / local 行）。
- 读侧统一：`Config::parse` 校验越界、core 提供 effective getter、改变面板夹取/整体回退、改变设置页对手改值的显示。
- 其它同模式的设置判定：`app.native_language` 空串、`app.copy_auto_popup` 的能力规则、`app.panel_display` 的合法集、`prompts.level_descriptors` 的 key 集。
- 跨进程 session-tier 密钥（desktop 存的「仅本次会话」key 对 panel / CLI 不可见——这是进程内存语义，不是本规格的判定）。
- `app.config` 读取失败时 panel 整体回退到默认值的策略。
- `Runner` / `Turn` / `CredentialState` / `RunNotice` 的公开形状，`Explanation` / store / prompt / split / provider 层。
- probe / capability / discovery 的任何改动。
- 新的产品功能或 UI 改版；notice 文案与排版（行为在范围内，措辞只按本规格列出的边缘变化调整）。

## Further Notes

- **四处边缘行为变化，其余不变**：
  1. panel 云候选的 requirement 改为 endpoint 过滤后的结果；
  2. panel 云候选的本机性改为按配置 endpoint 判断（被指到本机的 provider 不再是「云端」；被指到别的远程服务的 provider 可能无 key 也被提供）；
  3. desktop 的非 UTF-8 env key 从「可用」变为 unknown（Required → first_run，Optional → 仍可用）；
  4. `plainly config set` 的越界值从 exit 0 变为 exit 2（且不写文件）。
  除这四条外，现有测试是「没有别的变化」的证据；任何额外的 stdout/stderr/退出码/界面差异都应被视为 bug。
- **为什么现在做**：候选 1 把「一次运行」收进 core 之后，三个表面仍在各自回答「现在能不能用」；报告里点到的第二处（数值边界）又暴露了同一类「判定没有 owner」的问题。两片都小，且都在候选 1 明确出界的范围里。
- **结构记录**：ADR-0004 记录 `Readiness` 这个值与拒绝掉的替代（每面各判、只给布尔、值里带 secret、把探活折进来、放进 `setup` / `runner`）。数值边界的取舍（写严格、读容忍）记在本规格 §4 与代码注释里，不另开 ADR。
- **词表**：`GLOSSARY.md` 不变；`Readiness` / `KeyPresence` 是实现名。
- **评审报告**：候选 2 来自架构评审报告（临时文件，有意不入库）；本规格与 ADR 是持久记录。
- **接续**：`/to-tickets` 会切成垂直切片（不预先切片）；预期至少分为「core readiness」「config bounds」「desktop/panel 迁移」三片，`desktop/panel 迁移` 依赖前两片合并后再动 `settings` / `prefs`，避免同一文件冲突。
