# 05 CLI 命令面与守护进程触发

Type: grilling
Status: open

## Question

定下 CLI 的**子命令与 I/O 契约**，以及它作为守护进程触发器的角色。

- 输入：stdin / 文件路径 / 系统剪贴板。
- 输出：markdown / json（原始结构）/ 仅退出码（脚本用）。
- 子命令与选项（`explain`、`history`、`config` 之类）以及**退出码语义**。
- `plainly --explain-clipboard` 如何把请求投递给**已在运行的桌面进程**：倾向 `tauri-plugin-single-instance` 的 argv 传递（回调能拿到第二个进程的 argv，无需自建 IPC 服务）；备选是 `$XDG_RUNTIME_DIR` 下的 unix socket。决定用哪个、以及桌面进程没在跑时的行为（自己起、还是纯 CLI 输出）。
- CLI 是 v0 的一等公民（core 的验证器），不是附属品——但界面能力以文本为准。
