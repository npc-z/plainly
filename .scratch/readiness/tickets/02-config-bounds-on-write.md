# 02: config: 越界数字写不进去，CLI 立刻说

**What to build:** 三个数值范围成为 core `config` 的公共常量（自动消失 `5..=300`、面板宽 `320..=900`、面板高 `240..=1200`，默认值仍是 20 / 470 / 470）；`ConfigFile::set` 在既有「0 不是时长 / 0 不是尺寸」两条规则之后校验范围；越界返回新的结构化 `ConfigError::OutOfRange`（携带 key、输入值、min、max），CLI 的错误映射把它归入 `Usage`，于是 `plainly config set app.panel_width 5000` 从 exit 0 变为 **exit 2 且不写文件**，边界值仍被接受。**读侧完全不变**：`Config::parse` 仍接受任意 u32，手改出来的越界文件仍能打开——这是「写严格，读容忍」的另一半。desktop 与 panel 本票不动（它们现有的预检/夹取仍然生效，行为不变），只有 `docs/manual-checks.md` 里「CLI 写 2000、面板按 900 收住」的说法和 `default_config.toml` 的注释要随之更新。

**Blocked by:** 无（可立即开始）。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §4、§5

**Status:** ready-for-agent

- [ ] 三个范围常量从 core 的 `config` 公共导出，值与 spec 一致；默认值不变。
- [ ] `set` 接受边界值（300 / 320 / 900 / 1200 等），拒绝边界外（4 / 301 / 319 / 901 / 239 / 1201 等）并返回 `OutOfRange`，四个事实字段正确。
- [ ] 0 的两条现有专用消息逐字保留；错误优先级符合 spec（先 0 规则，再范围）。
- [ ] `Config::parse` 仍接受越界值（用手改文件路径的测试钉住）。
- [ ] `plainly config set <key> <越界值>` 退出码 2，且配置文件不被创建/不被修改；边界值退出码 0。
- [ ] core 测试补范围矩阵与 `OutOfRange`；CLI 二进制测试补越界写入一条与边界值一条；既有 zero 测试与 `providers` 测试不改。
- [ ] `docs/manual-checks.md` 的跨表面条目改为「手工把文件改成 2000 → 面板按 900 收住；`config set 2000` 现在 exit 2」；`default_config.toml` 注释补一句 CLI 同样拒绝。
- [ ] `cargo test -p plainly-core`、`cargo test -p plainly-cli`、两个 crate 的 clippy 通过；desktop / panel 未动但仍通过。

**Comments**
