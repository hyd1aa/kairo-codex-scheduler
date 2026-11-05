# Kairo

轻量 Rust 工具，在用户设定的时间调用一次官方 Codex CLI。[English](README.en.md)

**Kairo 是定时调用 Codex 的工具，不保证每次调用都能启动新五小时窗口，也不保证请求必然成功。官方额度和限流仍然适用。**

## 一键安装

```sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/hyd1aa/kairo-codex-scheduler/main/install.sh | sudo bash'
```

下载 GitHub 最新正式版，验证 SHA256，安装程序并配置 systemd 开机自启。**安装即授权 Kairo 在预设时间自动调用官方 Codex CLI；可以随时暂停。** 官方 OAuth 和 MFA 由用户本人完成。

先登录再安装：自动使用安装用户现有的官方登录。先安装再登录：在安装用户下运行 `codex login`，或使用菜单 4；服务无需重新绑定、重启或启用。Kairo 不复制、输出或上传 OAuth Token，也不修改 Codex 的用户配置。

## 定时调用

默认时间为系统本地时区的 06:00、11:00、16:00、21:00，可新增、修改和删除。原有自定义时间会保留，例如 06:00、11:02、16:04、21:06。默认缓冲为 30 秒，所以实际调用为 06:00:30、11:02:30、16:04:30、21:06:30；菜单和计划预览明确显示秒数。

每个计划时点独立尝试一次，不读取五小时 reset_at 来决定是否跳过，也不判断其他设备是否使用过 Codex。只要未暂停、官方 CLI 已登录可用、本机该时点尚未处理且没有同一 Kairo 调用正在执行，就启动一次 `codex exec`。

调用使用空的私有工作目录、只读沙箱、临时会话、禁用用户配置和规则、禁用命令及网页搜索工具，并明确关闭官方 OpenAI 提供方的请求和流重试。固定指令为“请仅回复：Kairo ping。不要调用工具，不要修改文件。”Kairo 不进行额外模型验证或窗口确认，不重复启动同一时点。只有退出码为 0、收到有效非空回复及有效 `turn.completed` 用量事件，且没有失败或工具执行事件，才显示调用成功。

本地 `daemon.lock` 防止多个守护进程；`call.lock` 防止调用重叠。`schedule.json` 在启动模型进程前原子写入并同步磁盘，记录该计划时点及单调时间水位。失败、崩溃或结果未知也不会重发同一时点。进程有超时和父进程死亡清理。上一调用仍运行时记录跳过原因，等待下一时点。

服务重启和升级只安排未来时点，关机期间错过的任务不集中补发。网络失败、登录过期、服务端限流或额度耗尽均记录结果，等待下一个计划时点。手动暂停在重启、升级和重新登录后继续保留，只有主动启动才取消。

额度查询是独立的可选只读功能。首页保留缓存时间及过期标记；查询失败不阻止定时调用。额度仅供参考，不能保证下一次请求成功。Kairo 不保证跨机器防重复；本机至多启动一次依赖本地记录和文件系统正常持久化。删除本地数据或丢失整个磁盘不可能保留过去的防重证据，新安装从未来时点开始。

## 菜单

```sh
sudo kairo
```

| 编号 | 菜单 |
| --- | --- |
| 1 | 查看调用记录与额度 |
| 2 | 设置调用时间 |
| 3 | 启动 / 暂停 |
| 4 | Codex 登录与账号 |
| 5 | 更新 / 高级设置 |
| 6 | 中文 / English |
| 7 | 卸载 Kairo |
| 0 | 退出 |

所有菜单使用单数字加 Enter，`0` 返回，主菜单的 `0` 退出。操作后 Enter 返回当前菜单。支持中英文、NO_COLOR、SSH、tmux 和窄屏终端。非交互环境使用子命令或 `--json`。

## 升级与兼容

```sh
sudo kairo update online
sudo kairo --version
sudo kairo status
kairo --json status
```

升级保留 OAuth 路径、用户时间、缓冲、语言、手动暂停和已处理时点。旧的部署、窗口、账号保护和恢复记录不再控制定时调用；旧请求记录只读导入显示，原件保留，不创建部署授权，不需要 FRESH、RECOVER、ENABLE 或五小时等待。

原有 `enable`、`activate` 是启动别名；`onboard --request` 启动，`onboard` 仅查看状态。`recover status/check/begin/wizard` 兼容返回无需恢复，不更改暂停。旧原始导入命令已退休；升级直接从原数据目录读取记录。旧的空闲窗口开关保留兼容字段，但不影响调用，`prewarm --buffer` 仍可设置调用延迟。

普通二进制回滚不会回退新执行记录；回滚到旧版本将恢复旧版本行为。升级事务失败时也不删除本机时点记录。

卸载保留 OAuth、用户配置和状态，不删除已有 Codex CLI：

```sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/hyd1aa/kairo-codex-scheduler/main/uninstall.sh | sudo bash'
```

## 开发

```sh
KAIRO_OFFLINE=1 CARGO_BUILD_JOBS=1 sh scripts/verify.sh
```

测试使用合成状态和带明确标记的模拟 CLI；离线模式拒绝调用真实 Codex。安装验收在隔离容器中使用模拟 systemd，不访问正式服务、真实 OAuth 或模型。

[验收报告](docs/ACCEPTANCE_0.2.8.md) · [发行说明](docs/RELEASE_NOTES_0.2.8.md) · [MIT License](LICENSE) · [源码来源](SOURCE_ORIGIN.json)
