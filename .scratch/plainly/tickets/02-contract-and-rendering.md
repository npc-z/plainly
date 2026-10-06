# 02: Explanation 契约与五节渲染

**What to build:** core 的数据契约——模型只返回内容四件套（Comprehensible English / Gloss 列表 / Grammar / Translation），原文与全部元数据由应用附加——以及把任何 Artifact 渲染成与原型逐字同形的五节 markdown。用注入式假 provider 在 core 的 explain 路径上跑通，不需要网络。

**Blocked by:** 01

**Spec:** [spec.md](../spec.md) §3 Explanation 契约、§4 渲染（含来自原型的 schema 形状）

**Status:** ready-for-agent

- [ ] wire schema 不出现任何长度 / 模式 / 数值上下界（ADR-0001）；`glosses` 是保序的对象数组 `[{expression, gloss}]`；`grammar` 写成 `["string","null"]` 且仍列进 `required`
- [ ] 校验器能区分「JSON 解析失败」与「schema 校验失败」；字段为 null 时按「无该节」处理
- [ ] Artifact 形状含原文与全部元数据：Level、源 / 母语、provider、model、thinking、`artifact_version`（v0 = 1）、`prompt_version` 哈希与人类标签、时间戳
- [ ] 五节渲染是纯函数，输出与原型文档里的两个例子逐字一致（`grammar` 为 null 时省略该节）
- [ ] 另有「面板模式」渲染变体：`grammar` 为 null 时该节显示「本段不需要」而不是消失
- [ ] 注入式假 provider 可编程返回畸形 JSON 与错字段名，explain 路径据此返回可区分的成功 / 失败结果
- [ ] `--format json` 的文档形状与 Artifact 一致
