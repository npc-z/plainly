# 17: 收尾与验收

**What to build:** 一套别人能照着验的验收材料——niri 上的手测清单、GNOME 的降级行为、X11 的明确不支持、CI，以及新贡献者的上手说明。

**Blocked by:** 15, 16

**Spec:** [spec.md](../spec.md) §1 范围与平台承诺、§14 开发环境、Further Notes 的诚实记录

**Status:** ready-for-agent

- [ ] niri 手测清单可执行且写下来：键绑 spawn 面板、焦点不变、Esc 无效、✕ 可关、看门狗自退、多块堆叠、两种拒绝态、位置设置生效
- [ ] GNOME 上验证降级为取得焦点的普通窗口且取词可用；X11 会话下面板明确报不支持（CLI 不受影响）
- [ ] CI 跑 `-p plainly-cli` 的构建与测试（不整 workspace、不需要图形依赖）
- [ ] README 给出最小键绑片段（`spawn` 不走 shell、不展开 `~`、`hotkey-overlay-title`）与首次配置步骤
- [ ] 诚实记录进入面向用户的文档：等级是提示不是契约（B2 与 C1 实测分不开）、本地模型会自信地错、中文翻译不可搜、面板默认只有 ✕ 一条出路、敏感判定覆盖面有限
- [ ] 全库跑一遍 `plainly` 的端到端冒烟（CLI + 面板 + 主窗口），并记录已知未做的部分（规格的 Out of Scope）
