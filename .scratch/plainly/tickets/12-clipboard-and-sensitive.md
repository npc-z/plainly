# 12: 剪贴板、敏感内容与桌面能力探测

**What to build:** 剪贴板里是被密码管理器标记过的内容时，应用不发送、不入库、明说原因；脚本能用不同的退出码把它和 provider 失败分开。

**Blocked by:** 08

**Spec:** [spec.md](../spec.md) §11 敏感剪贴板、§12 能力探测

**Status:** done (2026-10-08)

- [x] 剪贴板以**数据**形式进入 core（文本 + MIME hint 列表），平台侧只负责读，判定逻辑在 core 一处兜住
- [x] 判据 = 类型列表含 `x-kde-passwordManagerHint` **且**它的取值为 `secret`；只满足一项不判定（两条边界都有测试）
- [x] 判定后不发起任何 provider 调用、不写入任何行（**「原文总是入库」的有意例外**），并返回可区分的结果
- [x] CLI 用退出码 `4` 表达主动拒绝；`plainly explain --clipboard` 受同一规则约束
- [x] 规则与 provider 无关（本地也不放宽），且**不给**「坚持解释」的出口
- [x] 桌面能力（`ext_data_control_manager_v1` / `zwlr_data_control_manager_v1` 是否存在）以数据形式探测并**驱动降级决定**，不是只写日志
- [x] 局限写进文档：不遵守该约定的密码管理器不会被保护；文件与 stdin 输入没有类型标记，因此只覆盖剪贴板路径

## 落地形态

- `plainly-core::clipboard`（缝 A）：`Clipboard { passage, types: Vec<MimeType> }`、
  `MimeType { mime, value: Option<Vec<u8>> }`、`PASSWORD_HINT`、`PASSWORD_HINT_SECRET`、
  `Clipboard::reading() -> Reading::{Passage(String), Sensitive, Unread}`。
  - 判据只有一处，而且分成四种状态：类型没出现（`Absent`）→ 解释；类型在、值逐字节等于 `secret`
    （大小写敏感）→ `Sensitive`；类型在、值是别的（`public` 是常见那个）→ 解释；类型在、值**没读**
    （`None`）→ `Unread`。另一个类型带着 `secret` 不算——那是另一个类型的值。平台侧只负责把类型列表
    与它读到的值交上来。
  - `Unread` 是「半个判据不是放行的理由」：值不是读出来的而是猜出来的，或者是平台没读，都按**不发**处理，
    但它是**另外一条**结果（退出码 `1`），不冒充「被标记为敏感」（退出码 `4`）。把这条放进 core，
    是为了让判据真的只有一处——否则「读不出的标记」这个判断会散在每个平台读取器里。
  - `reading()` **消耗** `Clipboard`，Passage 只能从 `Reading::Passage` 里拿到：界面层想跳过这一步
    就得先把类型拆开，而 core 不给这条路。判定发生在配置、历史库与网络之前，所以拒绝之后没有东西要撤销。
  - 启发式兜底（熵 / 长度 / 无空格）**刻意不做**（issues/18）：假阳性与假阴性同时存在，最糟的是给人
    「有防护」的错觉。诚实地说「只认这个约定标记」比虚假安全感好。
- `plainly-core::desktop`（缝 A）：`DATA_CONTROL`（两个协议名）、`Desktop::new(protocols)`、
  `Desktop::data_control()`、`Desktop::presentation() -> Presentation::{Layer, Window}`。
  - 探测结果是**决定**而不是日志：有 data-control → `Layer`（无焦点 layer 表面 + data-control 取词）；
    没有 → `Window`（取得焦点的普通窗口 + GTK 剪贴板）。GNOME 上 layer-shell 与 data-control 一起缺席，
    所以这是一个判断而不是两个。
  - `data_control()` 同时是票据 16「复制自动弹」开关灰掉的依据。与票据 04 的 provider 能力探测严格分开：
    一个探端点，一个探合成器，互不当作对方的证据。
- `plainly-cli`：
  - `--clipboard` 与 stdin / 文件并列为第三种来源（无子命令时同样可用，`explain` 前后都接受）；
    **同时**给出文件和 `--clipboard` 是用法错误（退出码 `2`），不让其中一个静默胜出。
  - 平台侧读取器在 `plainly-cli/src/clipboard.rs`：跑 `wl-paste --list-types` 拿类型列表，只在列表里
    有 `PASSWORD_HINT` 时多跑一次 `wl-paste --no-newline --type x-kde-passwordManagerHint` 取它的值，
    再跑 `wl-paste --no-newline --type text` 拿正文，把三者作为数据交给 core。**值取不到就交 `None`**，
    由 core 判成 `Unread`——读取器只读，不当判官。**正文那一跑必须显式 `--type text`**：不指定类型时
    `wl-paste` 会「推断」并可能挑中剪贴板上的第一个类型，把一张图片的字节当 Passage 交上来；generic
    name `text` 让 `wl-paste` 自己挑一个 text/* 类型，挑不到就失败，于是「剪贴板上没有文本」成为一个
    被做出来的判断。CLI 因此不引入 GTK 或 Wayland 客户端库（`scripts/check-headless-cli.sh` 仍然通过）。
  - **判定在配置、历史库与网络之前**：敏感 → stderr 明说 `marked sensitive (x-kde-passwordManagerHint
    = secret): nothing was sent and nothing was stored`，退出码 `4`。`CommandError::Refused` 把「我们
    主动拒绝」与「provider 失败」分成两个码，脚本可以据此分支。**没有**「坚持解释」的出口。
  - 退出码：`wl-paste` 不在 PATH → `1` 并点名 wl-clipboard；剪贴板为空、或里面有东西但没有文本 →
    `2`（没有 Passage）。「空」有两条路：成功读到而正文是空白，以及根本没有 selection——真实
    `wl-paste` 对后者是**失败**（stderr `Nothing is copied`、退出码 1），读取器按这句话把它认成
    `Empty`（措辞变了最坏只是从 `2` 退回 `1`，不会变成误发）。标记类型在、但它的值读不出来 →
    **`4`**：这也是「我们主动不发」（core 判成 `Unread`），只是理由不同，stderr 点明区别；它和
    `Sensitive` 分属两个结果，不和 provider 失败混在一起。
  - `flake.nix` 的 `devTools` 加 `wl-clipboard`（2.3.0，正是 spec 要求的 ≥ 2.3.0），手动试
    `--clipboard` 才有 `wl-paste`。
- 测试：`crates/plainly-core/tests/clipboard.rs`（判据的四条边界）、
  `crates/plainly-core/tests/desktop.rs`（协议 → 形态）、`crates/plainly-cli/tests/clipboard.rs`
  （正常解释、`public` 不拒绝、`secret` 退出 4 且零请求零入库、本地 provider 不放宽、标记读不出则拒、
  空剪贴板、只有图片没有文本、缺 `wl-paste`、两个来源）。CLI 的测试用 PATH 上的假 `wl-paste`，
  不需要 Wayland 会话。

## 给下游的交接

- **tickets/13、15（面板）**：拒绝态用 `clipboard::Reading::Sensitive`，形态用
  `desktop::Presentation`；不要在面板里另写一遍判据或协议名判断（票据 15 只负责措辞）。
- **tickets/16（复制自动弹）**：能力开关用 `desktop::data_control()`，不具备时灰掉 + 一行说明。
- **tickets/17（加固）**：CLI 的 `--clipboard` 端到端已覆盖；真机上用 `wl-copy --sensitive` 走一遍真
  剪贴板的冒烟归 17。

## 已知取舍 / 局限（写下来，不留白）

- **不遵守该约定的密码管理器不会被保护**：判据只认 `x-kde-passwordManagerHint` = `secret`。不用这个
  约定的管理器（或平台）复制密码时，Plainly 看不到任何标记，也就没有拒绝的理由。
- **文件与 stdin 没有类型标记**：这条判据只覆盖剪贴板路径。`plainly explain file.md` 与管道输入不受它
  约束——不是漏做，是那两条路径没有可用的信号。
- **不做启发式兜底**（刻意，见上）。
- **CLI 的读取器是外部 `wl-paste`**：需要 wl-clipboard（≥ 2.3.0 才有 ext-data-control）。CLI 不链接
  Wayland 客户端库是「保持 headless」的代价；面板走 Rust 绑定直接绑协议，不依赖这个命令。**没有做版本
  探测**：版本低只影响无焦点场景，而 CLI 本来就在有焦点的终端里跑；读不到就如实报错，规则的正确性不
  依赖版本。面板的兜底路径（票据 13）才需要查版本。
- **正文在本地被读出来过一次**：core 的判据需要完整的 `Clipboard`（Passage + 类型），所以平台侧先把
  Passage 读进内存再由 core 决定；敏感时它直接随 `Reading::Sensitive` 被丢弃，不发送、不入库。这是
  「判定在 core 一处」的代价，也是它的意义。
- **CLI 的读取不是原子的一次 offer**：`wl-paste` 每次只回答一个类型，所以「类型列表」「标记的值」
  「正文」来自三次独立运行，剪贴板恰好在这几次之间变化时，读到的东西可能不属于同一份内容（真正的
  时序窗口是毫秒级）。面板不受影响——它对着**同一个 offer** 请求多个类型（票据 13）。**没有加重复
  探测**：能改动剪贴板的人本来就可以不带标记地放一段密码，那条路是上面已经写明的局限；重复读只是
  把窗口挪个位置，不解决它。真要紧的是「按一次键 → 解释刚复制的那段」这条路径，而它中间没有人再复制。
