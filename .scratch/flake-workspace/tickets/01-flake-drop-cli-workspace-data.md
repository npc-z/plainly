# 01: nix: 把 CLI 子集移出 flake（生成物 + 生成器 + 收口）

**What to build:** CLI 的合成 workspace（`plainly-core` + `plainly-cli`）的 manifest 与 lock 不再是 flake 里的手抄字面量：新增 `scripts/generate-cli-workspace.sh`，从根 `Cargo.toml` / `Cargo.lock` 派生并提交 `nix/cli-workspace/{Cargo.toml,Cargo.lock}`；`flake.nix` 删掉 `cliManifest` / `cliLock` 两个绑定（约 2281 行）改成 `cp ${./nix/cli-workspace/...}`，并更新一次 `cliCargoDeps.hash`。做完后 `nix build .#plainly` / `.#default` 与今天等价：同一个二进制（CLI 依赖版本按 spec §7 统一到根 lock）、同一个无图形栈闭包、checkPhase 照跑测试；flake.nix 从 2754 行回到 ≤ 500 行，接口（`.#plainly` / `.#default` / `nix develop` / `nix run` / `checks` 为空）不变。

**Blocked by:** 无（可立即开始）。

**Spec:** [spec.md](../spec.md) §1、§2、§4、§7

**Status:** done (2026-10-18，工作区改动未提交)

- [x] `scripts/generate-cli-workspace.sh`（bash + 内联 `python3`/`tomllib`）按 spec §2 四步实现：派生 manifest、从根 lock 剪种子（传递闭包，处理 `"name version"` 形式的依赖串）、临时目录里用 `cargo metadata` 让 cargo 规范化（`--offline` 优先，失败回退并说明用到网络）、写回并打印 `git diff --stat`；`--check` 只比较不写；`cargo` / `python3` 缺失时给明确报错；失败不写文件、exit 1、保留临时目录路径。
- [x] 生成器在干净树上产出的 `nix/cli-workspace/Cargo.toml`：header 注释（generated / do not edit / 重新生成的命令）、`[workspace]` 的 `resolver` 与 2 个 members、`[workspace.package]` 4 个字段、`[workspace.dependencies]` 只有两个 crate 以 `workspace = true` 继承的 11 项（不含 `gtk` / `gtk-layer-shell` / `wayland-client` / `wl-clipboard-rs`，不含未使用的 `async-channel`）。
- [x] 生成器产出的 `nix/cli-workspace/Cargo.lock` 是 cargo 自己写的普通 lock（cargo 头、v4、236 包；数量由 cargo 决定，不写死），且 `cargo metadata --locked` 在 2-crate 工作区上通过。
- [x] `flake.nix` 删除 `cliManifest`（216–244 行）与 `cliLock`（245–2496 行）两个绑定及「second source of truth / 手同步 / 粘贴」注释；改写为一段短注释指向生成器、生成物位置与守卫；**保留**「为什么需要合成 workspace」的理由（`libdbus-sys` 沙箱 panic、ticket 02 Q5）。顶部工具链注释（40 行）里「`rust-version` … `Cargo.toml` 与手抄 `cliManifest` 两处」改成一处。
- [x] `cliSource` 改为 `cp ${./nix/cli-workspace/Cargo.toml} $out/Cargo.toml` 与 `cp ${./nix/cli-workspace/Cargo.lock} $out/Cargo.lock`；`cliCargoDeps.hash` 更新为 nix 报出的新值（记录新旧值）。
- [x] 生成器在提交后的树上重跑一次，`git diff` 为空（可复现）。
- [x] 根 `Cargo.lock` 未改；`guiCargoDeps.hash` 未变；GUI 依赖缓存输出不变。
- [x] `nix build .#plainly` 通过（checkPhase 跑 core + cli 测试，`cargo test --locked`；记录测试数）；`nix build .#default` 通过；`nix run .#plainly -- --version` 可用；`nix develop -c cargo --version` 仍为 1.99.1。
- [x] `scripts/check-flake-packages.sh --no-build` 通过（CLI 闭包无 GTK/WebKit/Wayland/dbus、两包闭包大小不同、面板 `env -i` 启动、CLI 无环境读 Passage 等既有断言）。
- [x] `grep -n 'cliManifest\|cliLock\|\[\[package\]\]' flake.nix` 为空；`wc -l flake.nix` ≤ 500。
- [x] 记录实际数字到本票 Comments：flake 行数前后、`cliCargoDeps` 新旧哈希、`plainly-deps` 新旧输出路径、生成器耗时、`nix build` 耗时、测试数、CLI lock 包数与相对今天字面量的版本差（预期 15 个，见 spec §7）。
- [x] 核对 spec §1/§2/§4/§7 与实现无出入；实现中若发现规格需要修正，改 spec 而不是绕过。

## Comments

**2026-10-18 — 实现（按用户要求不提交代码，改动留在工作区）**

- **生成物**：`nix/cli-workspace/Cargo.toml`（28 行：header 2 行、2 个 members、4 个 `[workspace.package]` 字段、11 个 workspace 依赖，无 GUI 名字、无未使用的 `async-channel`）与 `nix/cli-workspace/Cargo.lock`（cargo 写出的 v4、236 包、2246 行）。在 2-crate 工作区上 `cargo metadata --locked` 通过；与旧 flake 字面量相比 15 个共享包换版本（`cc` 1.6.0、`js-sys` 0.3.100、`num-conv` 0.2.2、`smallvec` 1.16.2、`syn` 3.0.6、`time` 0.3.55、`time-core` 0.1.9、`time-macros` 0.2.32、`toml_edit` 0.25.15、`toml_parser` 1.1.4、`wasm-bindgen`/`-macro`/`-macro-support`/`-shared` 0.2.123、`zeroize` 1.9.0），包数仍 236。
- **生成器**：`scripts/generate-cli-workspace.sh`（bash + 内联 python3；投影规则与守卫共用 `scripts/cli_workspace.py`）。在仓库上跑一次约 1 s（离线、cache 命中）：种子 237 包 → cargo 规范化 236 包；`--check` 通过、无 diff（可复现）。只接受与 `flake.nix` 的 `channel` 一致的 cargo（版本从 flake 读取，不再写死；实测在宿主 cargo 1.98 上 exit 1 并说明原因）。`--check` 不可用于生成；失败时不写文件并保留临时目录。
- **flake**：2754 → **472 行**，删除约 2280 行内联数据；`cliSource` 改为 `cp ${./nix/cli-workspace/...}`；`cliCargoDeps.hash` 从 `sha256-PL9gVEXFHjTzIVXu1VjEK/lDC80/FGeU4md1QnSLk6U=` 更新为 `sha256-OVl064aNTeMQsK/SGdS5W/WI2ofO0zYlb6zWTbJrhqM=`；顶部工具链注释改为「rust-version 只在根 Cargo.toml 一处」。`grep -n 'cliManifest\|cliLock\|\[\[package\]\]' flake.nix` 为空；`nix fmt -- --check flake.nix` 报告 0/1 would have been reformatted。
- **构建**：`nix build .#plainly` 2m17s（新 vendor fetch + 依赖编译 + 测试；日志 31 个 `test result` 行、464 passed、0 failed）。`nix build .#default` 首次 3m33s；GUI 的 `watch::tests::a_window_asked_for_without_a_watcher_ends_the_listener` 在并发跑 3 个 nix 任务时失败一次——时序敏感测试，与本次改动无关（GUI 代码与根 lock 都未动），重跑通过，且单独重跑 5/5 通过；GUI 的存储日志 36 个 `test result` 行、607 passed、0 failed。两个产物现在都已在 store（`nix build` 无动作）。
- **闭包**：`scripts/check-flake-packages.sh --no-build` 通过——CLI 闭包 graphics-free（5 paths，default 187；24 个直接 build inputs 无图形库）、`nix run .` / `nix run .#plainly` → `plainly 0.1.0`、default 三件套 + 图标、面板与主窗口 `env -i` 可起、67 / 156 个库无缺失、CLI 无环境读 Passage 给出可用句子。
- **Review 修订**：投影规则（members / 继承依赖 / manifest 渲染）抽到 `scripts/cli_workspace.py`，生成器与守卫共用；dev shell 校验的版本改为从 `flake.nix` 的 `channel` 读取（不再写死 `1.99.1`）；删掉「cargo 规范化必须改变种子」的错误断言（no-op 规范化是合法的）；根 lock 缺成员 / 缺依赖 / CLI 闭包为空时给 `FAIL:` 而不是 traceback；导入共享模块时不写 `__pycache__`。产物字节未变：生成器 `--check` 通过。
- **未动**：根 `Cargo.lock` / `Cargo.toml`（`git diff HEAD --` 为空）；`guiCargoDeps.hash` 未变（`sha256-GVGZ...`）；`nix develop -c cargo --version` = 1.99.1。

## 不在本票据里

- `scripts/check-cli-workspace.sh`、`check-incremental-build.sh` 的自检改写、`docs/manual-checks.md`（票 02）。
- `.scratch/plainly/spec.md` §14、历史票的 Comments、最终验收记录（票 03）。
- 任何产品代码、CI、`checks` 非空、新增 flake 输出、GUI 侧数据。
