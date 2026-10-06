# 12: 剪贴板、敏感内容与桌面能力探测

**What to build:** 剪贴板里是被密码管理器标记过的内容时，应用不发送、不入库、明说原因；脚本能用不同的退出码把它和 provider 失败分开。

**Blocked by:** 08

**Spec:** [spec.md](../spec.md) §11 敏感剪贴板、§12 能力探测

**Status:** ready-for-agent

- [ ] 剪贴板以**数据**形式进入 core（文本 + MIME hint 列表），平台侧只负责读，判定逻辑在 core 一处兜住
- [ ] 判据 = 类型列表含 `x-kde-passwordManagerHint` **且**它的取值为 `secret`；只满足一项不判定（两条边界都有测试）
- [ ] 判定后不发起任何 provider 调用、不写入任何行（**「原文总是入库」的有意例外**），并返回可区分的结果
- [ ] CLI 用退出码 `4` 表达主动拒绝；`plainly explain --clipboard` 受同一规则约束
- [ ] 规则与 provider 无关（本地也不放宽），且**不给**「坚持解释」的出口
- [ ] 桌面能力（`ext_data_control_manager_v1` / `zwlr_data_control_manager_v1` 是否存在）以数据形式探测并**驱动降级决定**，不是只写日志
- [ ] 局限写进文档：不遵守该约定的密码管理器不会被保护；文件与 stdin 输入没有类型标记，因此只覆盖剪贴板路径
