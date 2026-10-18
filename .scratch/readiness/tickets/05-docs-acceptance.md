# 05: 收尾：文档、grep 验收、全量检查

**What to build:** 把这次的结构决定写进面向维护者的文档，并用 spec §7 的 grep 判据与全量检查证明「只有四处边缘行为变化，其余不变」。README 的「One Passage's run has one owner」段落补上 readiness（可用性是一个 core 值，三个表面只翻译它）；替换 desktop / panel 里被 03、04 取代的旧注释（「复制常量是有意」改成「写严格、读容忍」的说明）；跑全 workspace 测试、clippy、`nix build` 的 checkPhase；逐条核对四处边缘行为变化，其余任何 stdout / stderr / 退出码 / 界面差异都记为 bug 并修回；确认工作树没有残留的临时文件。

**Blocked by:** 03、04。

**Spec:** [spec.md](../spec.md) §Implementation Decisions §7、§Further Notes

**Status:** ready-for-agent

- [ ] README 的结构性段落补上 readiness 与 `local_of`（不夸大 CI 覆盖面：只写 core 测试覆盖了判定）。
- [ ] desktop / panel 里被取代的旧注释已改写成「写严格，读容忍」的说法；没有指向已删除常量的过时注释。
- [ ] grep 判据全部成立：`desktop/settings` 无 `secrets.resolve` / `environment_provides`；`desktop/view` 无 `endpoint_of`；`panel/provider` 无 `preset_requirement` / `is_loopback` / `secrets.resolve`；`panel/prefs` 无自己的范围字面量；`desktop/app` 无范围比较；范围数字只在 core `config`；凭据判定 match 只在 readiness 分类器与 `resolve_credential` 投影。
- [ ] `cargo test --workspace`、`cargo clippy`（相关 crate）、`nix build` 的 checkPhase 全绿；panel / desktop 测试在 checkPhase 下通过。
- [ ] 行为审计：只有 spec Further Notes 列出的四处边缘变化，逐条由测试或手测证据支持；任何额外的输出差异已修回。
- [ ] `git status` 干净（除本 feature 的 spec / ADR / tickets 与实现改动），没有遗留的临时恢复目录或未跟踪文件。
- [ ] 在 `docs/adr/0004-provider-readiness-is-one-value.md` 与本票 Comments 里记录实现偏差（如有）。

**Comments**
