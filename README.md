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

## 首次启用与已有账号重装恢复

安装或升级后，以管理员身份运行统一向导：

```sh
sudo kairo recover wizard
```

也可输入 `sudo kairo`，选择 **06 → 09**。向导包含人工 OAuth 登录、官方只读身份/额度校验、新账号绑定、已有账号恢复、等待复查及正式启用；不要求编辑 JSON 或手工删除 systemd 文件。

**首次新账号**：完成登录和服务端校验后，仅确认这个账号从未运行 Kairo、没有旧调度和近期请求时，选择首次绑定并输入 `FRESH`。这是管理员的声明，额度接口不能证明账号的完整历史。已有状态或门禁不会被 FRESH 覆盖。然后在向导中输入 `ENABLE` 明确授权；已有首次绑定冷却、防重、周额度和服务端拒绝保护继续生效。

**已有账号重装、没有可恢复的旧本地记录**：不要 FRESH。先停用其他机器/数据目录上的调度器，并在恢复期间停止其他自动和手动模型请求。向导选择“旧账号历史丢失”，输入 `RECOVER`，或执行：

```sh
sudo kairo recover begin --acknowledge-unknown-history
sudo kairo recover status
```

此时只创建持久化的未知历史风险屏障，**不授权模型请求，不把旧历史当作空历史**。至少等待 **5 小时 6 分 30 秒**（完整五小时窗口 + 最大缓冲 300 秒 + 执行超时 90 秒）；已观察窗口、旧门禁和配置缓冲可能使实际请求更晚。服务/进程重启或重复 begin 不缩短/重置等待；单调时钟保证系统向前校时不能跳过等待，系统重启会在下次复查时保守重新计时；若等待期间保护文件丢失，显式重新 begin 会重新开始保守等待。

到提示时间后复查：

```sh
sudo kairo recover check
```

使用官方只读额度接口重新验证当前账号、五小时和每周窗口；身份不符、额度未知、拒绝、周额度耗尽或时钟回拨都会阻止批准。观察到新的用量或活跃窗口变化会延长等待；零用量空闲账号正常滚动的重置时间不会无休止延期。等待结束不会自动批准。

最后由管理员明确启用：

```sh
sudo kairo enable --approve-production-cutover
sudo kairo status
sudo kairo doctor
```

`enable` 同样可在向导中输入 `ENABLE` 完成。它只支持标准安装器管理的 root/systemd 安装：验证 unit/目录，停止服务后保存恢复基线和授权，只移除原样的安装器观察覆盖，重载并启动服务，确认**当前 PID 的新 `live` 状态**。自定义 unit/覆盖会保留并拒绝自动接管。失败时关闭授权并恢复观察配置；错误会说明回退是否完整。启用后自然计划可能发出真实请求；回退不能撤销已发生的请求，已有持久化请求保护始终保留。向导本身不发送模型测试请求。

确认真正启用时，`kairo --json status` 应同时显示 `scheduler_state_ready: true`、`recovery_status.production_authorized: true`、`service_mode.daemon_live_confirmed: true`、`automatic_warmup_enabled: true`，并且 `status_file.execution` 为 `live`、`status_file.daemon_pid` 对应 systemd 当前 PID。只有 systemd active 或 OAuth 校验通过都不足以证明自动预热已启用。服务刚启动、状态过旧或仍有 `--observe` 时不会显示已确认正式运行。授权仍在但 live 未确认时，界面会明确提示可能执行请求；这不是安全暂停，需要撤权时运行 `kairo pause`。

恢复保留有效请求和账号锚点；替换损坏的本地状态前，将原始字节保留为私有 `recovery-evidence-*` 文件。全局门禁损坏、账号不匹配、危险文件类型/权限不能自动覆盖，需要保留原件并核查备份。历史未知标记在恢复后仍保留；全局冷却字段保存的是风险/预算保留时间，不伪造成功请求或旧调用记录。防重和执行权锁在**同机所有数据目录**间按账号共享；额度接口既不提供完整请求账本，也不能证明其他机器已经停用，管理员必须保证不运行竞争调度器。

`doctor` 的 `existing_records_valid` 仅表示本地已有配置/记录可校验；`state_valid` 和 `scheduler_state_ready` 表示完整绑定、状态、游标及全局门禁就绪，仍不等于生产授权或实际 `live`。doctor 退出成功表示报告已生成，不表示所有项目已就绪。

## 输入 kairo：完整命令菜单

root 用户输入 `kairo`；非 root 管理员输入 `sudo kairo`。SSH 交互终端显示彩色菜单。只读和配置操作也可由具有数据权限的 `kairo` 服务用户执行；下表标为 root 的项目需管理员权限。

| 编号 | 实际菜单名称 | 功能和操作方法 | 权限 |
| --- | --- | --- | --- |
| 01 | 查看运行状态 | 查看时区、账号、窗口、安全门禁及下一次计划 | 数据权限 |
| 02 | 查询 Codex 额度 | 已授权后查询五小时／每周额度；未知即保持保护 | 数据权限／OAuth |
| 03 | 管理激活时间 | 进入下表的时间管理子菜单 | 数据权限 |
| 04 | 服务管理 | 子项01状态、02启动、03停止、04重启；启动不等于授权 | root |
| 05 | 查看运行日志 | 每页20条可读日志，支持翻页、刷新、错误过滤和详情 | root／日志权限 |
| 06 | OAuth 登录与账号管理 | 01本地状态、02服务端校验、03人工登录、04列表、05新增、06切换、07删除配置、08首次绑定，09首次启用/重装恢复向导 | 数据权限／人工OAuth |
| 07 | 系统诊断与修复 | 01诊断；02安全恢复默认配置，保留请求历史 | 数据权限 |
| 08 | 预热参数设置 | 01查看、02空闲预热开关、03缓冲秒数、04周额度/拒绝保护、05生命周期、06无请求预览、07重置参数 | 数据权限 |
| 09 | 检查更新 / 升级程序 | 普通用户选03自动更新；01/02为本地清单/文件高级操作，04回滚，05恢复中断事务，06只读查询最新稳定版 | root |
| 10 | 中文 / English 切换 | 立即切换并保存菜单语言 | 数据权限 |
| 11 | 帮助与项目说明 | 查看项目能力及安全说明 | 数据权限 |
| 12 | 卸载 Kairo（保留状态） | 先输入 `yes`，再输入 `uninstall` 确认；停止服务并保留用户数据 | root |
| 00 | 退出 Kairo | 退出菜单，不停止已运行的服务 | 数据权限 |

例如输入 `03` 进入时间管理，输入 `02` 后填写 `02:00` 新增；子菜单输入 `00` 返回，主菜单输入 `10` 切换语言，主菜单 `00` 退出。

## 交互面板

数字 + Enter 打开独立页面；子菜单 `00` 返回上一级，只有主菜单 `00` 退出。操作结果后按 Enter 回到当前子菜单，支持连续操作；非法输入留在本级。Logo 仅显示在主菜单。交互终端只清理当前可见屏幕，不清除终端历史，不切换原始输入模式；非 TTY 请使用子命令，`NO_COLOR` 禁用颜色。

```text
[ K A I R O ]
Codex Window Scheduler
v0.2.3 | Asia/Shanghai
服务状态：● 运行中
调度模式：只读观察
自动预热：尚未启用

04 → Enter
Kairo / 服务管理
01. 查看状态
02. 启动服务
03. 停止服务
04. 重启服务
00. 返回

02 → Enter
✓ 当前服务状态：运行中
启动服务不等于取得自动预热授权
按 Enter 返回当前菜单...
```

`01` 展示状态摘要，详细本地 JSON 放在二级选项；`02 → 01` 发起只读额度查询，不发送模型请求。`05` 默认每页20条，最多浏览最近200条日志；详情同样有长度限制。`11` 按主题展示帮助。`09 → 06` 查询 GitHub 最新稳定版，不会执行升级；升级仍需选择 `09 → 03`。

OAuth 已发现、服务端校验、本地绑定、生产授权分别展示。部署或历史文件缺失、损坏、读取失败会明确提示。首次使用先完成上面的 OAuth 校验和诊断；已有账号缺失历史时先核查，不要用 FRESH、清空保护记录或伪造状态恢复。恢复向导保留历史未知事实，并建立保守且受保护的新基线，不重建旧请求。

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
