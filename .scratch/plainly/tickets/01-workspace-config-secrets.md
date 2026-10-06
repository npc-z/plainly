# 01: 工作区、开发环境、配置与密钥

**What to build:** 一个能构建、能跑、能记住我的设置的项目骨架——`nix develop` 进入锁定的开发环境，workspace 分成 core / CLI / 面板 / 主窗口四个 crate，其中 CLI 单独构建时不带任何图形依赖；`plainly config path` 告诉我配置文件在哪、`config set` 改得动它，而 API key 从不落进磁盘（环境变量 > keyring > 仅本次会话内存）。

**Blocked by:** None (can start immediately)

**Spec:** [spec.md](../spec.md) §2 工作区与 crate 边界、§8 配置与密钥、§14 开发环境

**Status:** done (2026-10-05)

- [x] `nix develop` 给出锁定环境（fenix 工具链、gtk3、gtk-layer-shell、webkitgtk_4_1、C 编译器），`GDK_BACKEND=wayland` 已设；`.envrc` 可用
- [x] `cargo build -p plainly-cli` 在一个没有 GTK / webkit 的环境里也能通过（不是空话，能验证）——实测 + [`scripts/check-headless-cli.sh`](../../../scripts/check-headless-cli.sh) 常驻守卫
- [x] 新克隆第一次构建不会失败（占位图标已提交进仓库）
- [x] 配置是 TOML、三段（`[app]` / `[providers.*]` / `[prompts]`），位置由 config / data / cache 三个目录决定，identifier 为 `dev.plainly.app`
- [x] `plainly config path|show|set` 与 `plainly providers key set|clear` 可用
- [x] 配置写入是原子的（临时文件 + rename），外部改动被检测到而不是被覆盖（**按内容比较**，见下）
- [x] 密钥按 环境变量 > OS keyring > 仅本次会话内存 解析，任何路径都不写明文文件；keyring 不可用时明确告知只在本次会话有效
- [x] core 不依赖 GTK / webkit / Wayland（平台能力以数据注入），可在无图形环境构建与测试

## Comments

### 2026-10-05 实现记录

**建了什么。** `flake.nix`（flake-utils + fenix，锁 `nixos-unstable`，只列 `x86_64-linux`，无 `nixConfig`，`checks = {}` 并写了 readest 那条教训）+ `.envrc` + 占位图标；workspace 四个 crate（`plainly-core` / `plainly-cli`（二进制 `plainly`）/ `plainly-panel` / `plainly-desktop`，后两者是占位二进制，各自指向票据 13 / 14）；core 的 `paths`（Tauri 同构的 config/data/cache 解析，平台输入以数据注入）、`config`（TOML 文档 + 三段 + 键类型强制 + 原子写 + 外部改动检测）、`secrets`（三级 + `SecretStore` 缝，真实 keyring 走纯 Rust D-Bus）；CLI 的 `config path|show|set` 与 `providers key set|clear`。

**证据（都跑过）。**

- flake：`nix develop` → rustc/cargo/clippy 1.101.0-nightly（fenix complete）、`GDK_BACKEND=wayland`、pkg-config 认得 gtk+-3.0 / gtk-layer-shell-0 / webkit2gtk-4.1 / libsoup-3.0、`LD_LIBRARY_PATH` 6 条；`devShells.inspect` 另加 sqlite3 3.53.3。
- headless：在一个**没有 pkg-config、没有 GTK** 的 `nix shell` 里 `cargo build -p plainly-cli` 通过；`ldd` 只有 libc / libm / libgcc；`cargo tree -p plainly-cli` 无任何图形相关 crate。
- 测试：`cargo test --workspace` **59 通过**；`cargo clippy --workspace --all-targets -- -D warnings` 干净；`cargo fmt --all --check` 干净。
- keyring 真路径：`cargo test -p plainly-core --test keyring_live -- --ignored` 在本机会话总线上通过（可存储 / 读回 / 清除）；另手工跑过一次 `providers key set|clear`，退出码 0/0，条目已清除。
- 退出码矩阵（手工）：坏值 2、未知键 2、空 stdin 2、无 keyring 1（什么都没持久化）、未知子命令 2、无子命令 2、成功 0。

**规格层补的/记下的判断。**

1. **外部改动检测按内容比较，不按 mtime**（规格 §8 的措辞是 mtime）。字节比较是同一保证的**严格更强**版本：同一时间戳刻度内的改动也抓得到。代码注释里写明了这是有意偏离。
2. **`providers key set` 在 keyring 不可用时退出 1**，不是 0。AC7 要求"明确告知只在本次会话有效"；对一次性 CLI 进程，"会话内存"等于没存，报成功就是撒谎。警告文案已按此改写（含"lives only in this process"并指向 `PLAINLY_<PROVIDER>_API_KEY`）。`PLAINLY_DISABLE_KEYRING=1` 是文档化的逃生口（headless 机器、测试）。
3. **`[prompts]` 只装用户自己的提示词数据**（`appendix` + 等级描述覆盖）；出厂默认随应用更新，所以它在代码里而不是配置里（规格 §5：出厂默认不可变、随版本更新）。有效提示词与哈希归票据 07。
4. **CLI 文案是英文**。规格"中文为主 + 文案集中"说的是面板与主窗口（票据 15 的 strings 模块）。若 CLI 也要中文，属票据 15/17 的 strings 范围。
5. **还没有 `explain`**：无子命令时把 usage 打到 stderr 并退出 2；票据 03 才把它变成默认子命令。
6. **`config path` / `config show` 只读**：文件不存在时 `show` 打印出厂默认并在 stderr 说明，`path` 不创建任何东西。

**两轴 review 的结论与处理**（`/code-review`，Standards 与 Spec 各一个子代理）。

- 修掉：配置注释里 3 处领域词漂移（"target language"→Native Language、"answer"/"cached answer"→Explanation、"comprehensible restatement"→Comprehensible English）；枚举的变体→字符串映射三处重复（补 `as_str`）；`KeySpec` 里路径与键名重复（改为由点分键直接推导路径）；`key_source` 的注释撒谎（它确实会问 keyring，只是不返回密钥），并为"仅环境变量"这一用途加了 `environment_provides`（不再为一句警告付 D-Bus 往返）；删掉无调用者的 `KeySource::as_str` 与 `MemorySecrets::holds`；`providers::run` 的中间人；`store()` 把"keyring 坏了"与"没有 keyring"混为一谈（现在把原因带进警告）；`clear` 在无 keyring 时的措辞；`std::env::vars()` 在非 UTF-8 环境变量上 panic（改 `vars_os` + 无损转换，并加了 CLI 级回归测试）；`pkgs.sqlite3` → `pkgs.sqlite`（flake eval 抓到的）。
- **AC2 从"口头声明"变成常驻守卫**：`scripts/check-headless-cli.sh`（依赖图 + 单独构建 + `ldd`）加一条读二进制字节的测试，票据 17 的 CI 直接调用它。
- **keyring 真路径进了树**：`crates/plainly-core/tests/keyring_live.rs`，`#[ignore]`，用 `cargo test -p plainly-core -- --ignored` 跑，自带清理。

**记下的两个缺口（不在本票据里悄悄糊掉）。**

1. **退出码没有"本地状态写失败"这一档**：`ConfigError::Io|Parse|ExternallyModified` 落在 1，而 §10 把 1 定义为"provider / 生成失败"。也就是说脚本目前分不开"保存被拒绝（文件被别人改了）"和"生成失败"。这是规格的码集缺一档，不改数字（不擅自发明第 6 个码），留作规格修订的输入——若脚本需要这个区分，应先在规格里定码。
2. **flake 的 `nix develop` 依赖网络**（首次拉 nixpkgs / fenix / 依赖闭包）。这与票据 13 的结论一致，不是新问题，只是记一笔：无 Cachix 也能开发，但离线机器不行。

### 2026-10-05 复核后的第二轮修改（用户复核提出，逐条处理）

1. **`the_cli_binary_carries_no_graphics_stack` 用字节扫描判断 headless** —— 那会把 `libgtk-3` 这类字符串在 `.debug_str` 或某条 `expect()` 文案里的一次出现误判成"链接了图形库"，报错还指向错的地方。判据改为二进制真正的 **DT_NEEDED**（ELF 里"这个文件要求加载器带上哪些库"的那张表）。这条在第三轮又收敛了一次，最终形态见下面第三轮第 4 条：测试用 `object::import_libraries()` 直接解析，不再 spawn `ldd`；`ldd` 只留在脚本里当第二意见（缺失时打印 `SKIPPED, NOT VERIFIED`，不冒充通过）。
2. **`check-headless-cli.sh` 写死 `target/debug/plainly`** —— 先 `--release` 再跑脚本会去查一个陈旧的 debug 产物。已改为从 `cargo build --message-format=json` 的 `"executable"` 取路径、断言可执行，并支持 `--release`（两种 profile 都实测通过）。
3. **`prompts.level_descriptors.A2.extra` 报"值错误"** —— 点分路径改成由键推导后丢掉了这一层区分。已改为：后缀里再含 `.` 即 `UnknownKey`；实测报 `unknown configuration key "prompts.level_descriptors.A2.extra"`（退出 2），并加了回归测试。
4. **环境变量 `to_string_lossy` 会篡改非 UTF-8 的密钥材料** —— 静默替换成 U+FFFD 后，存进 keyring 的已经不是用户管道喂进去的那份，而失败会迟到一个裸 401。已改为 `EnvSecrets` **保留原始 `OsString`**；`resolve()` 因此变成可失败（`Result<Option<ResolvedKey>, SecretError>`），**只有环境变量这一档会把"用不了"报出来**（其余两档的错误仍然穿透），新增 `SecretError::NotUtf8`。环境变量名本身仍做无损转换（非 UTF-8 的名字不可能是 `PLAINLY_*_API_KEY`）。CLI 里那句优先级提示走新的 `environment_provides()`，只看"设没设"，不解释值。
5. **`read_secret` 用 `read_line`，多行密钥被静默截断**（PEM / 误带 echo）—— 回答：**API key 就是单行**，但"截断"不能是静默的。已改为读完整个 stdin，含嵌入换行即**用法错误**并说明可改用 `PLAINLY_<PROVIDER>_API_KEY`；实测退出 2 且文案明说 "a secret is a single line"。
6. **`keyring_live` 在"没有 Secret Service"的机器上会红**（那是环境事实，不是代码坏）—— 已加前置探针：`get` 失败即打印 "skipping: no usable Secret Service…" 并返回，不再产生假阴性。

这一轮后：`cargo test --workspace` **62 通过**；`cargo clippy --workspace --all-targets -- -D warnings` 与 `cargo fmt --all --check` 干净；`scripts/check-headless-cli.sh` debug 与 release 两种 profile 都通过；`cargo test -p plainly-core --test keyring_live -- --ignored` 在本机通过。

以上四轮改动已随本票据的实现一起提交。

### 2026-10-05 第三轮复核（用户复核提出）

1. **`resolve` 的不对称**（环境层非 UTF-8 是硬错误，keyring 被锁却 `Ok(None)`）—— 采纳"两边都上报"。新规则写进代码注释：**每一层都被问到，第一个有答案的胜出；某层答不出来是错误，不是跳过**。代价也写明：keyring 打不开就**不会**退回到内存里的密钥——悄悄换一份凭据比报错更糟。环境变量仍然优先（先问它）。`SecretError::NotUtf8` / `Unavailable` / `Backend` 现在都能到达调用者；票据 03 因此能分开"哪儿都没配"与"钥匙串打不开"。测试同步重写：`a_broken_keyring_is_reported_rather_than_skipped`、`an_exported_key_is_found_before_the_keyring_is_consulted`，并用新增的"只读 collection"假 keyring 覆盖"keyring 正常但写入被拒 → 落到内存 → 仍能查到"。
2. **`EnvSecrets::get` 里的死分支** —— "空值不算密钥"这条规则现在只留在 `raw()` 一处，`get` 不再复述。
3. **`keyring_live` 的"跳过"在 CI 里与"通过"无法区分** —— 跳过时打印 `SKIPPED, NOT A PASS: …`，并新增 `PLAINLY_REQUIRE_KEYRING=1`：设了就要求真跑，探针失败直接 `panic!`。本机已按该变量验证通过。
4. **测试里 spawn `ldd` 是 glibc 专属** —— 改成解析 ELF 的 `DT_NEEDED`（新增 `object` 仅作 dev-dependency，用 `import_libraries()`），不再依赖外部命令，也不再受"命令找不到"影响；脚本里的 `ldd` 保留，但缺失时打印 `SKIPPED, NOT VERIFIED`，不再假装通过。
5. **脚本用 sed 解析 `--message-format=json`** —— 改成用 dev shell 里本来就有的 `node` 真正解析 JSON（取最后一条带 `executable` 的 `compiler-artifact`），并先检查 node 是否可用；路径含引号或 `"executable":null` 不再有影响。
6. **flake 加 Rust LSP** —— `fenix.packages.${system}.rust-analyzer`（与工具链同一个 nightly，避免版本错配的幽灵报错），实测 `rust-analyzer 0.0.0-nightly (65ac641 2026-10-04)`。
7. **AGENTS.md 新增 Environment 一节** —— 禁止用 `find` / `grep` / `ls` / `du` 扫描 `/nix/store`，并给出替代：`nix develop -c`、`nix shell nixpkgs#…`、`nix eval`、`nix path-info`、`nix why-depends`、`nix-locate`/`nix-index`、`nix search`；程序缺失的正解是改 `flake.nix`。

**另一处自己发现的问题**：这一轮用脚本重写 `cli.rs` 时截断了文件尾部的 `a_multi_line_secret_is_refused_rather_than_truncated`（多行密钥被拒的回归测试）。靠逐套件测试计数（16 vs 17）发现并已恢复——记在这里，因为"测试数量对不上"正是该被当成信号的东西。

这一轮后：`cargo test --workspace` **64 通过**；clippy `-D warnings` 与 `fmt --check` 干净；`scripts/check-headless-cli.sh` 退出 0（debug 与 release 都试过）；`keyring_live` 本机通过、`PLAINLY_REQUIRE_KEYRING=1` 下同样通过；两个 devShell 都能 eval。

### 2026-10-05 第四轮小修（用户复核提出）

1. **`object` 在成员 crate 里硬编码了版本号** —— 本仓库其余外部依赖（`clap` / `keyring` / `serde` / `thiserror` / `toml_edit`）都在根 `[workspace.dependencies]`，成员一律 `.workspace = true`。`object` 已上移到根（`object = { version = "0.40", default-features = false, features = ["read", "std"] }`），`plainly-cli` 只写 `object.workspace = true`——将来升版本改一处。
2. **`editorTools` 对两个 shell 无条件生效，与"只有编辑器用"的注释相矛盾** —— 改成 `mkPlainlyShell` 的参数 `editors`，由 `lib.optionals editors [ rustAnalyzer ]` 决定带不带。
   **默认值取 `false`**：我先写成 `editors ? true`，方向与注释里"谁要谁声明"相反——那样将来任何新 shell（CI、文档里的 `extraTools` 例子）都会静默继承几百 MB 的 rust-analyzer。**只有 `devShells.default` 显式写 `editors = true`**；`inspect` 不声明，因此保持精简。代价是 default 多一行，收益是"要编辑器工具"变成硬性声明而不是默认行为。
   实测：default shell 有 `rust-analyzer 0.0.0-nightly (65ac641)`；inspect shell 没有它（这正是默认值为 `false` 的证据），但仍有 sqlite3 / cargo / gtk-layer-shell。flakes 用仓库自己的 `nix fmt`（nixpkgs-fmt）格式化过。
3. **第二轮记录里第 1 条与第三轮第 4 条描述不一致** —— 那条写的是中间形态（用 `ldd` 读 DT_NEEDED），最终形态是 `object::import_libraries()` 解析、测试里完全不 spawn `ldd`。第二轮第 1 条已收敛到最终形态并指向第三轮第 4 条。

这一轮后：`cargo build --workspace`、`cargo test --workspace`（**64 通过**）、clippy `-D warnings`、`fmt --check` 全部照旧干净；两个 devShell 都 eval 通过。
