# 03: 云端 provider 与 CLI explain

**What to build:** `echo "hard sentence" | plainly` 真的从云端拿回一段解释并按五节 markdown 打印；`plainly explain file.md`、`--format json` 也能用，退出码可脚本化。

**Blocked by:** 02

**Spec:** [spec.md](../spec.md) §7 Provider 层、§10 CLI 契约

**Status:** ready-for-agent

- [ ] 预设 provider（DeepSeek / OpenAI / llama.cpp / Ollama / LM Studio）可直接选，也能自定义 base URL 与 model id；默认 `deepseek-flash` 且默认关闭思考
- [ ] 关闭思考用规范写法 `{"thinking":{"type":"disabled"}}`（不用 `{"thinking":false}` 一类的写法）
- [ ] 请求用**嵌套**的 `response_format.json_schema.schema`；`max_tokens` ≥ 1200（默认 2048）；temperature 0
- [ ] 无子命令时默认走 `explain`；输入支持 stdin 与文件路径
- [ ] stdout 只出产物、stderr 出人类信息；`--format markdown|json` 都可用
- [ ] 退出码：`0` 成功 · `1` provider / 生成失败 · `2` 用法错误 · `3` 未配置；无输入且 stdin 是 TTY 时报用法错误而不是默默阻塞
- [ ] CLI 子进程级测试可用环回上的假 provider 跑通，不需要真实 key
