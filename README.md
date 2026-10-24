# Kairo

轻量级 Rust Codex 五小时额度窗口自动预热工具。[English](README.en.md) · [正式发行](https://github.com/hyd1aa/kairo-codex-scheduler/releases/latest)

## 核心功能

- 每日激活时间、系统时区、空闲窗口预热和延迟只读确认。
- 五小时／每周额度与服务端拒绝保护，全局请求防重、持久化和重启恢复。
- 彩色中英文菜单、systemd 常驻、在线更新和保留数据的卸载。

Kairo **不能强制重置、增加或绕过 OpenAI 额度**。额度未知、未授权或安全门禁不满足时不发模型请求。

## 一键安装

在支持的 Linux VPS 上复制这一条命令，以后发布新版也无需改版本号：

```sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/hyd1aa/kairo-codex-scheduler/main/install.sh | sudo bash'
```

安装器自动取 GitHub 最新稳定正式 Release（不含 draft/prerelease），识别系统与 x86_64/ARM64，校验 HTTPS、SHA256、架构和版本，准备依赖、独立用户及 systemd。**不需 Rust/Cargo，不需手写 unit。** 已有安装按安全更新处理，保留 OAuth、配置和请求账本；首次服务保持停用／观察。

首次下载需 curl、可信 CA、bash 和 root/sudo。外层 `pipefail` 很重要：裸 `curl … | sudo bash` 在 curl 没下载到任何内容时可能返回空 bash 的成功码；包装后下载失败明确非零。脚本完整接收后才执行，子命令不会把脚本流当交互输入。固定版本等高级选项见[安装说明](docs/INSTALL.md)。

## 首次 OAuth 登录及启用

1. 安装器保留已有 Codex CLI；缺少时下载经固定 SHA256 校验的官方原生 CLI。
2. 运行下面命令，由本人完成官方设备码／浏览器和 MFA；不上传凭据给 Kairo 项目：

```sh
sudo kairo auth login
sudo kairo auth verify
sudo kairo doctor
```

3. 菜单 `06 → 08` 仅用于确实无旧调度、无近期请求的首次绑定，输入 `FRESH`；已有状态不能清空重绑。绑定返回 `account_fingerprint`，首次有保守等待期，按状态提示等待。
4. 真正启用自动请求前，核对账号、额度和历史后，使用返回的指纹显式启用：

```sh
sudo kairo activate --account '这里填写绑定返回的指纹' --approve-production-cutover &&
sudo rm -f /etc/systemd/system/kairo.service.d/20-mode.conf &&
sudo systemctl daemon-reload &&
sudo systemctl enable --now kairo.service
```

这是现有运行时的显式安全确认，不是发布审批。登录或点击“启动服务”本身不会解除初始观察模式。不要复制其他机器的 OAuth 或账本，也不要让同一账号同时运行多个自动调度器。真实 OAuth 由使用者授权，开发验收不发送模型测试请求。

## 输入 kairo：完整命令菜单

root 用户输入 `kairo`；非 root 管理员输入 `sudo kairo`。SSH 交互终端显示彩色菜单。只读和配置操作也可由具有数据权限的 `kairo` 服务用户执行；下表标为 root 的项目需管理员权限。

| 编号 | 实际菜单名称 | 功能和操作方法 | 权限 |
| --- | --- | --- | --- |
| 01 | 查看运行状态 | 查看时区、账号、窗口、安全门禁及下一次计划 | 数据权限 |
| 02 | 查询 Codex 额度 | 已授权后查询五小时／每周额度；未知即保持保护 | 数据权限／OAuth |
| 03 | 管理激活时间 | 进入下表的时间管理子菜单 | 数据权限 |
| 04 | 启动 / 停止 / 重启服务 | 子项01状态、02启动、03停止、04重启；启动不等于授权 | root |
| 05 | 查看运行日志 | 显示 kairo.service 最近100行日志 | root／日志权限 |
| 06 | OAuth 登录与账号管理 | 01本地状态、02服务端校验、03人工登录、04列表、05新增、06切换、07删除配置、08首次绑定 | 数据权限／人工OAuth |
| 07 | 系统诊断与修复 | 01诊断；02安全恢复默认配置，保留请求历史 | 数据权限 |
| 08 | 预热参数设置 | 01查看、02空闲预热开关、03缓冲秒数、04周额度/拒绝保护、05生命周期、06无请求预览、07重置参数 | 数据权限 |
| 09 | 检查更新 / 升级程序 | 普通用户选03自动更新；01/02为本地清单/文件高级操作，04回滚，05恢复中断事务 | root |
| 10 | 中文 / English 切换 | 立即切换并保存菜单语言 | 数据权限 |
| 11 | 帮助与项目说明 | 查看项目能力及安全说明 | 数据权限 |
| 12 | 卸载 Kairo（保留状态） | 输入 `uninstall` 确认，停止服务及子进程并保留用户数据 | root |
| 00 | 退出 Kairo | 退出菜单，不停止已运行的服务 | 数据权限 |

例如输入 `03` 进入时间管理，输入 `02` 后填写 `02:00` 新增；子菜单输入 `00` 返回，主菜单输入 `10` 切换语言，主菜单 `00` 退出。**12 是当前 v0.2.1 真正存在的选项，并非规划功能。**

## 自定义激活时间（03）

默认 `06:00、11:00、16:00、21:00`，均按**系统时区**解释，不固定为北京时间。可以自行管理，不要求每天恰好四次；最多48项，任意相邻时间含跨午夜至少间隔10分钟。

| 编号 | 实际子菜单名称 | 操作 |
| --- | --- | --- |
| 03 → 01 | 查看全部激活时间 | 查看当前列表 |
| 03 → 02 | 新增时间 | 输入 `HH:MM` |
| 03 → 03 | 修改时间 | 依次输入旧时间、新时间 |
| 03 → 04 | 删除时间 | 输入要删除的时间 |
| 03 → 05 | 下次实际检查 | 查看下一次实际计划 |
| 03 → 06 | 未来24小时预览 | 只读预览，不发模型请求 |
| 03 → 07 | 恢复默认时间 | 恢复四个默认时间 |

也可直接运行：

```sh
sudo kairo schedule list
sudo kairo schedule add 02:00
sudo kairo schedule edit 02:00 03:30
sudo kairo schedule remove 03:30
sudo kairo schedule next
sudo kairo schedule preview
```

## 查看额度与运行状态

菜单 `01/02/05` 对应状态、额度、日志：

```sh
sudo kairo status
sudo kairo quota
sudo kairo logs
```

服务端拒绝、额度用尽或未知、近期请求以及不确定请求结果都会阻止盲目重发；不能用改时间或重装绕过这些门禁。

## 更新方法

输入 `kairo`（非root加sudo），选择 `09 → 03`，或：

```sh
sudo kairo update online
```

自动选择最新正式版，不改安装命令里的版本。下载/校验失败不启用新版本；复用原Rust持久化事务，保留 OAuth、配置、请求账本并支持失败恢复。回滚和恢复见[维护说明](docs/INSTALL.md)。

## 一键卸载

```sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/hyd1aa/kairo-codex-scheduler/main/uninstall.sh | sudo bash'
```

停止并禁用服务，确认 daemon／服务子进程停止后，删除安装器管理的 unit、程序和全局命令。重复卸载安全。默认保留 `/var/lib/kairo`、`/var/lib/kairo-account-guards`、`/home/kairo/.codex`、专用用户及安装归属标记；不删除已有 Codex CLI 或共享依赖，重装继续保留原状态。未知文件不乱删，残留或失败会明确报错。彻底清除仅提供[单独确认的高级选项](docs/INSTALL.md#高级卸载--advanced-removal)。

## 已验证系统和注意事项

真实systemd安装与维护在 **Debian12 x86_64 QEMU TCG** 隔离来宾验证，不是物理硬件认证。x86_64/aarch64预编译程序已原生构建。安装器还识别Debian13、Ubuntu22.04/24.04、Alma9、Rocky9；有历史兼容性证据，不冒称全部通过本轮在线闭环。ARM64公开首装未实测，Alma/Rocky10自动首装暂不支持。

默认信任GitHub HTTPS及仓库管理权限，SHA256用于完整性。初次真实OAuth/具体账户额度仍由使用者检查。自有代码[MIT](LICENSE)，第三方保留各自条款，见[通知](THIRD_PARTY_NOTICES.md)和[来源](docs/PROVENANCE.md)。[安全报告](SECURITY.md)。源码开发才需Rust/Cargo；本轮不修改已验收的Rust调度核心。
