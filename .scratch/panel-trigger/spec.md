# Panel trigger 规格：一次触发的迁移归 state

Status: ready-for-agent
Labels: ready-for-agent

> 由 2026-10-18 的三轮 grilling 坍缩而成（架构评审候选 5「面板的触发策略搬进 state，Plan 并进 Showing」；13 个决定，见文末溯源表）。
> 不新增 ADR：ADR 记的是跨表面的规则 / 契约（[ADR-0003](../../docs/adr/0003-the-runner-owns-one-runs-machine-state.md) 运行语义、[ADR-0004](../../docs/adr/0004-provider-readiness-is-one-value.md) 可用性、[ADR-0005](../../docs/adr/0005-the-rendered-shape-is-a-value-core-owns.md) 渲染形状）；这次没有新规则越过 panel 的边界，删掉 Plan 的理由与被否掉的替代记在本规格。
> 领域词按仓库根 `GLOSSARY.md`；本规格不新增 domain 词条——`Showing` / `Trigger` / `Press` / `Recall` / `Plan` 都是代码名（同 ADR-0003 的 `Runner`；tickets/15 已把 `Showing` / `Recall` / `Trigger` 定为代码名）。需要复核的只是 `Reading` / `Copy Listener` / `Arm` 三条不变。
> 硬约束：**零可感知行为变化**。面板的按键、⟳、复制、⤢、✕、自动消失、位置与每一句文案都不动；这次只搬决策、删一套词汇。唯一需要在记录里点名的边界见 Further Notes「触发配置的快照时点」。
> 不在范围：面板 `Prefs` 与 worker「各读一次配置」的窗口（[runner/spec.md](../runner/spec.md) 的 Out of Scope 单列的那条）、`Runner` 的接口与语义、渲染形态、任何 GTK 视觉改动。

## Problem Statement

「按一下键 / ⟳ 意味着什么」和「读回来的剪贴板意味着什么」是**纯决策**，改动前都住在 GTK 文件里（下面是改动前的快照，行号与文件位置都按改动前；`plan.rs` 现已删除）：

- [panel.rs](../../crates/plainly-panel/src/panel.rs) 当时 1598 行（GTK 实现 1487 行、测试 111 行）。`Press { Refresh, Regenerate }` 在 94–111 行；`start` 把 Press 解析成 `Recall` 在 484–487 行；读回结果的完整分发（`clipboard::Error` → `Nothing` / `Unreadable`，`Clipboard` → Plan → Showing）在 633–663 行，夹在线程、generation 与 widget 代码之间。要测它们只能构造窗口。
- `plan.rs` 的 `Plan` 有 5 个变体，与 `Showing` 的 5 个非运行状态一一对应；panel.rs 的 644–662 行拿到 Plan 之后立刻一对一重匹配成 Showing。第二套词汇只为跨过模块边界存在。
- [`state.rs`](../../crates/plainly-panel/src/state.rs) 的模块注释（3–8 行）已经写着「panel.rs then owns widgets and nothing else — it draws what this says and asks it what an action means」；今天这句话只对 `Showing` 成立，trigger 的迁移还在 GTK 侧。
- 后果：改一次触发语义要同时改不可测的一侧；评审点名的三类判断——「本地 ⟳ 换云端」「空剪贴板 vs 读失败」「TooLong」——今天没有任何无显示测试盯着 panel.rs 里的落点。

## Solution

一次触发的迁移收进 [`state.rs`](../../crates/plainly-panel/src/state.rs) 的 `Trigger`，`panel.rs` 退回 I/O、widget 与按键接线：

- `Press { Refresh, Regenerate }` 从 `panel.rs` 搬进 `state.rs`。
- `Trigger::press(press: Press, prefs: Prefs) -> Trigger`：离开时是 `Showing::Loading`，内部按配置解析 `Recall`（`Refresh → Recall::Reuse`；`Regenerate → prefs.regenerate_recall()`），`prefs` 快照随 trigger 走（今天它已经在 `Trigger.prefs` 里）。
- `Trigger::read(&mut self, result: Result<Clipboard, clipboard::Error>) -> Option<Vec<String>>`：把读取结果落成 `Showing`——`Sensitive` / `Unread` / `Nothing` / `TooLong` / `Stack`，`NoPassage` 落 `Nothing`、其它错误落 `Unreadable`；`Some(passages)` 表示这条触发要为这些 Passage 跑一次，`None` 表示已落成一句状态。`Plan` 与 `plan.rs` 删除，判断与构造合一。
- `Trigger.showing` 私有，只有迁移能写；`Trigger::stack` 收为私有。
- `panel.rs` 的 `Surface::show` / `Surface::stack` 改成只读绘制（`draw_message` / `draw_stack`），不再写 state；`pending: RefCell<Option<Recall>>` 换成 `pending: RefCell<Option<Trigger>>`——等待 focus 的是 `Trigger::press` 在按下时就建好的完整 Trigger（recall 按触发开始时的配置解析），由 `read` 安装；上一条运行在等待期间继续拥有当前 trigger。

## User Stories

### 维护者

1. 作为维护者，我想要「按键意味着什么」和「读回结果意味着什么」只在一个已测 module 里表达，以便改触发语义时不用打开 GTK 文件。
2. 作为维护者，我想要删掉与 `Showing` 一一对应的 `Plan` 词汇，以便一个状态只有一个名字。
3. 作为维护者，我想要 `Trigger.showing` 的写者只有迁移，以便「state 拥有触发」是编译期事实而不是注释。
4. 作为维护者，我想要这次不新增 ADR，以便 `docs/adr/` 继续只记跨表面的规则与契约。
5. 作为维护者，我想要 spec 记下被否掉的替代（保留 Plan、读取结果用中立类型、pending 继续带 Recall、绘制统一入口），以便后来人不会再把它们加回来。

### 面板用户

6. 作为面板用户，我想要每次按键、⟳、复制、⤢、✕、自动消失、位置与文案逐字不变，以便这次重构不需要重新学习界面。
7. 作为面板用户，我想要「本地 ⟳ 换云端」仍然在按下时按**这次触发开始时**读到的配置决定，以便设置页里改的东西下一次按键就生效。

### CI 与未来实现

8. 作为 CI 维护者，我想要这次不引入需要 display 的测试，以便 `cargo test -p plainly-panel` 的纯决策测试在没有显示的机器上照旧能跑。
9. 作为未来实现 agent，我想要 state 的迁移接口只收值、不碰 GTK，以便构造窗口不是测一条触发规则的先决条件。
10. 作为未来实现 agent，我想要 grep 就能验证「GTK 文件不再做触发决策」，以便接缝不靠阅读维持。

## Implementation Decisions

### 1. 读取结果直接是 `clipboard::Error`

`Trigger::read` 的参数是 `Result<Clipboard, crate::clipboard::Error>`，`state.rs` 直接 `use crate::clipboard;`。映射与今天逐条相同：

| 读取结果 | Showing |
| --- | --- |
| `Err(Error::NoPassage(_))` | `Nothing`（reason 与本日一致地不显示） |
| `Err(other)` | `Unreadable(other.to_string())` |
| `Ok(clipboard)` 且 `Clipboard::reading()` 为 `Sensitive` / `Unread` | `Sensitive` / `Unread` |
| `Ok(clipboard)` 且 `split::panel_chunks()` 为 `Err(too_many)` | `TooLong(too_many)` |
| `Ok(clipboard)` 切出空列表 | `Nothing` |
| `Ok(clipboard)` 切出 Passage 列表 | `Stack`，返回 `Some(passages)` |

拒绝「state 定义中立类型（如 `Read { Got, Empty, Failed }`），panel 在 I/O 边翻译」：翻译层只做改名，而它落脚的 panel 正是不可测的一侧；`clipboard::Error` 是普通数据 enum，测试构造它不会初始化 GTK。

### 2. `Press` 与 `Trigger::press`

- `pub enum Press { Refresh, Regenerate }` 从 `panel.rs` 搬进 `state.rs`，为什么是 Press 而不是 Recall 的注释一起搬（recall 依赖触发开始时读到的配置，提前解析就是「云端 provider 取自上一次触发」那个 bug）。
- `pub fn press(press: Press, prefs: Prefs) -> Self`：`Showing::Loading`；`Refresh → Recall::Reuse`；`Regenerate → prefs.regenerate_recall()`；`prefs` 存进 trigger。
- `Trigger::loading(Recall, Prefs)` 保留 `pub`：`Panel::new` 的初始占位（「任何触发之前」）、`Trigger::press` 的构造体，以及测试的直接构造。
- `Trigger::read` 返回的 `Option<Vec<String>>` 是交给 I/O 的下一手：`Some` 的列表就是 `Stack` 的 slot 顺序；`read` 一进来替换的就是 panel 装好的 `Loading` trigger（等待 focus 的那份也由 `read` 安装），即便如此，它在非 `Loading` 状态被调用也按覆盖处理、不 panic——与 `commit` / `fail` 的宽松风格一致，覆盖路径由 `a_read_on_a_failed_trigger_overwrites_it` 钉住。

### 3. `showing` 私有，绘制只读

- `pub fn showing(&self) -> &Showing`；`showing` 字段私有。`recall` / `prefs` 同样私有，配 `pub fn recall()` / `pub fn prefs()`（panel 只读并 clone）；三个字段的写者都只剩迁移。
- `Trigger::stack` 私有，只有 `Trigger::read` 用。
- `panel.rs`：`Surface::show` → `draw_message(&self, message: &str)`，`Surface::stack` → `draw_stack(&self, passages: &[String])`；两者只画、不写 state。写 state 只发生在 `Trigger::press` / `Trigger::read` / `state::apply`（`commit` / `fail`）。

### 4. `panel.rs` 的触发路径

- `Panel::start(press)`：`reread_prefs()` → `reset_dismiss()` → 建 `Trigger::press(press, prefs)` → 按 reader 分支：`DataControl` 装进 `surface.trigger` 并 `read()`；`Gtk` 先 `present()`，已 active 则同样装并 `read()`，否则把 Trigger 存进 `pending`、`waiting = true`（**不安装**：等待期间当前 trigger 仍属于上一条运行，否则它的流式事件会画进新 `Loading`）。
- focus-in 处理器：`if waiting.replace(false)` → 取出 `pending` 的 Trigger 装进 `surface.trigger`，再 `Panel::read(self)`。
- `Panel::read(&self)` 不再带 `Recall` 参数：trigger 已在 cell 里（`start` 建好；立即读的路径由 `start` 装，等待路径由 focus 处理器装），照旧重置 READING 文案、状态行、复制按钮与 chip、推进 generation，然后读剪贴板。结果回来且 generation 仍当前时，`let to_run = trigger.read(result)`；`Some(passages)` → `draw_stack` 并从 trigger 取 recall 起 worker；`None` → `draw_message(showing.message())`。
- `pending` 从 `Option<Recall>` 换成 `Option<Trigger>`；`waiting: Cell<bool>` 保留。
- `Press` / `use crate::plan` 从 panel.rs 删除。

### 5. `plan.rs` 删除

`crates/plainly-panel/src/plan.rs` 删除，`main.rs` 的 `mod plan;` 一并删除；它唯一的调用点（panel.rs 的 Plan 匹配）在本规格第 1、4 条里消失。

### 6. 测试

- `plan.rs` 的 6 条测试迁进 `state.rs` 新建的 `#[cfg(test)] mod trigger_tests`，按新词汇改名（如 `a_secret_clipboard_settles_as_a_refusal`）。
- 新增同模块测试：`Press::Refresh → Recall::Reuse`；`Press::Regenerate` 的三种配置（云端 provider → `Bypass { provider: None }`；本地 + 可用云端 → `Bypass { provider: Some(name) }`；本地 + 无云端 → `Bypass { provider: None }`）；读取结果的每个落点（含 `NoPassage` → `Nothing`、其它错误 → `Unreadable`），以及 `Some` 的 Passage 顺序。
- 现有 `mod tests`（Showing / Trigger 的既有性质）与 `planning_tests`（`first_occurrences` / `unreported`）原样不动；`panel.rs` 的 GTK 测试不动（无 display 时按既有方式跳过并打印 `PLAINLY_TEST_NO_DISPLAY`）。

## Testing Decisions

- 证明方式：迁移后的纯决策测试（无显示）+ 现有面板测试全绿 + clippy 零新警告 + 两条 grep 验收。
- 命令：`nix develop -c cargo test -p plainly-panel`；`nix develop -c cargo clippy --workspace --all-targets`。
- grep 验收（在仓库根执行，输出为空即通过）：
  - `panel.rs` 里不再出现 `plan::`、`.showing =`、`= Showing::`、`regenerate_recall`，也没有 `match press`；`Press::` 只作为 `Panel::start` 的实参出现在按键 / ⟳ 的接线处，不再定义或匹配；
  - `plan.rs` 文件与 `mod plan` 不再存在。
- 真机 smoke（显示 Explanation、按 ⟳）**不作为完成条件**：行为必须逐字不变，自动化证明的是决策搬走了、迁移正确。可在 ticket 的 Comments 里留一次人工记录；本机有 `wayland-1` 会话时可做。
- 面板不在 CI 里；`nix build` 的 checkPhase 用 `PLAINLY_TEST_NO_DISPLAY=1` 跑（GTK 测试被明确跳过）。这条是既有事实，本次既不改变也不假装改变。

## Out of Scope

- 面板 `Prefs` 与 worker「各读一次配置」的窗口（[runner/spec.md](../runner/spec.md) 的 Out of Scope 单列的一条）——收它要改 `Runner` 的打开路径并带来可感知行为变化。
- `Runner` / `explain` / `readiness` / `render` / store / config / prompt 的任何接口或语义。
- 任何可感知的行为、文案、状态、几何、按键或视觉变化。
- README、`docs/manual-checks.md` 的新条目：没有可感知变化，不增删。若 spec 的某个 claim 在实现中漂移，只改 spec 自身。
- ADR：本规格即记录。
- `Surface` 之外的 widget 重组；`worker` 的事件映射（`state::apply` 已拥有它）。

## Further Notes

- **触发配置的快照时点**：`start` 现在把 `Trigger::press(press, prefs)` 装好再等 focus，所以 trigger 的 `prefs` 是**触发开始时**那一份；旧的 `read(recall)` 在 focus 到达后才抓 live `prefs`。只有「focus 等待期间显示器插拔（`watch_monitors` 重读 Prefs）」这条路径能看出差别，且新行为才是 spec §12「每次触发开始时重读配置」的字面意思。这是本次唯一的边界收紧，记录在此而不是当行为变化宣传。
- **等待 focus 的 Trigger**：Q9 的初版把 Trigger 在 `start` 就装进 cell，评审指出这在 GTK 降级路径上有一处真实差异——上一条运行的事件还在用当时仍当前的 generation 往新 `Loading` trigger 上画（chip 重置、复制变灰、`Failed` 落成句子而正文仍是旧 stack）。改成「`pending` 存整个 Trigger、`read` 才安装」后与旧行为一致：等待期间当前 trigger 继续归属上一条运行，它的 chip、复制与失败落点都照旧；`pending` 里也不再是第二份 recall，而是一个由 state 建好的完整 Trigger。
- **词表**：`GLOSSARY.md` 不变；`Showing` / `Trigger` / `Press` / `Recall` 是代码名；`Plan` 从代码里消失。
- **被否掉的替代**：保留 `plan.rs` 只删类型（留下从 plan 指向 state 的横向模块）；读取结果用中立类型（翻译层住在不可测侧）；`read` 不回传要跑的 Passage、由 panel match `Showing`（决策回到 GTK 文件）；`Surface::draw(&Showing)` 单入口（要么让「画」返回要跑的东西，要么再加 `to_run()`）；`pending` 继续带 `Recall`（同一判断两处表达）；全部字段继续 `pub`（约定而非编译期事实）；删 `Press` 改两个构造（少一套词汇却没省任何判断）。
- **接续**：本规格的 tickets 在 `tickets/`：01（读取结果 → Showing）、02（Press → state）、03（验收与记录）；依赖序 01 → 02 → 03，每张都保持 workspace 可测。实现完成后在 03 的 Comments 里记验收输出。
- **行号与评审 HTML**：§Problem Statement 是改动前的快照（panel.rs 当时 1598 行、plan.rs 137 行），不随后续改动重锚；评审 HTML 自己的引用（说 panel.rs 1446 行、1447–1544 是触发逻辑）同样漂移过，结构判断不受影响。评审 HTML 在仓库根且未被跟踪，沿用前四个候选的惯例不入库：提交时显式列出路径，不 `git add -A`。
- **溯源（三轮 13 个决定）**：

| # | 决定 | 落在 |
| --- | --- | --- |
| 1 | 完整搬迁：Press→Recall、读取结果→Showing、Plan 并入 | Solution；§1–§5 |
| 2 | 不收 Prefs/worker 双读窗口 | Out of Scope |
| 3 | 删 `plan.rs` 与 `Plan`，测试迁入 state | §5、§6 |
| 4 | 只写 spec，不新增 ADR | 开头 blockquote；Further Notes |
| 5 | 两段式提交（规划 commit + 实现 commit） | 接续 |
| 6 | 自动化验收；真机 smoke 可选 | Testing Decisions |
| 7 | 读取结果直接收 `clipboard::Error` | §1 |
| 8 | `Trigger::press` + `Trigger::read -> Option<Vec<String>>`，保留 `Press` | §2、§4 |
| 9 | `pending` 从 recall 改为整个 `Trigger`，由 `read` 安装（评审修正：等待期间当前 trigger 仍属上一条运行） | §4、Further Notes |
| 10 | `showing` / `recall` / `prefs` 私有 + getter；`stack` 私有；`loading` 留 pub | §3 |
| 11 | 绘制只读：`draw_message` / `draw_stack`，不新增 `match Showing` | §3、§4 |
| 12 | 名字用 `press` / `read` | §2 |
| 13 | 测试放 `mod trigger_tests`，plan 测试按新词汇改名 | §6 |
