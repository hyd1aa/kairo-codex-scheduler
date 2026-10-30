# 安装与维护 / Installation and maintenance

[最新稳定发行](https://github.com/hyd1aa/kairo-codex-scheduler/releases/latest)。使用[README中的固定命令](../README.md)自动选择最新稳定正式版，固定版本仍为高级选项。默认GitHub HTTPS＋SHA256完整性检查，不要求发行签名；同源SHA不提供独立发布者身份认证。必须信任仓库管理者及GitHub/TLS。高级签名路径保持独立且失败不降级。

Use the fixed latest-stable command in the [README](../README.en.md); pinning a version remains an advanced option. Default trust is GitHub HTTPS/repository control with SHA256 integrity, not independent publisher authentication. Optional signatures are a separate, fail-closed path.

## 首次安装 / First installation

root或sudo、systemd、支持发行版、DNS/HTTPS及官方包源可用。下载入口命令本身需要curl和CA；脚本准备python3、curl、CA、tar/gzip、SHA256、用户管理、时区和锁等依赖。不安装Rust/Cargo。下载并验完正确架构包后才创建kairo用户/安装unit；原有未知命令或用户不会被覆盖。

Requires root/sudo, systemd and working DNS/HTTPS/package repositories. The initial download command needs curl/CA; the script prepares the other OS dependencies. No Rust/Cargo is needed. Package integrity is checked before provisioning the dedicated user and unit.

缺少Codex时从 `openai/codex` 官方GitHub Release安装原生CLI0.164.0，使用固定架构摘要验证，不运行登录或模型命令；现有 `/usr/local/bin/codex`、`/usr/bin/codex`、服务用户本地CLI不覆盖。CLI是独立第三方依赖，Kairo不在源码/发行包中转发它。安装器只管理自己安装的Kairo，卸载不删除官方CLI或用户认证。

If absent, the installer fetches official Codex0.164.0 from `openai/codex`, verifies its pinned architecture SHA256 and installs the native CLI. Existing usable installations are preserved. Codex is downloaded directly rather than bundled; Kairo uninstall preserves it and authentication data. [Official CLI documentation](https://developers.openai.com/codex/cli).

离线准备好发行目录时：

```sh
sudo bash ./install.sh --version 0.2.4 --source-dir /path/to/release --skip-codex
```

`--skip-codex`只用于已有CLI或无网准备，不绕过Kairo校验。首次安装记录安全自动预热意愿；OAuth 未完成或安全条件未满足时禁止模型请求。交互安装从控制终端引导官方 OAuth；非交互安装不阻塞。

## OAuth与启用 / OAuth and activation

普通用户运行 `sudo kairo`，03 完成必要登录，04 启动或暂停。交互首装会引导缺失的官方设备 OAuth；已有服务账号登录直接复用，管理员自己的私有 Codex CLI 登录可在严格权限验证后安全导入。网页登录不能代替 VPS 上 Codex CLI OAuth，不复制其他机器的凭据或请求账本。

安装授权安全条件满足后自动预热。其他机器是否仍在调度无法验证时，必须对当前账号确认一次独占调度。之后由轻量 systemd timer 自动建立未知历史恢复、等待、只读复查和切换正式服务，无需 FRESH / RECOVER / ENABLE。系统重启保留已确认等待进度，无法验证的关机时间不计入。旧安装升级保留已有授权、暂停和恢复进度；没有自动意愿的旧观察安装在菜单04选择一次自动接续。

Use `sudo kairo`: 03 for necessary official login, 04 to start/pause. Interactive first installation guides official device OAuth, while noninteractive installation never waits for input. Existing private service OAuth or a securely validated local administrator Codex CLI login can be reused. Installation authorizes safe automatic warmup. A one-time account exclusivity confirmation remains necessary when other machines cannot be verified. The timer then handles unknown-history recovery, waiting, read-only checks and verified enablement. Upgrades never cancel a pause or infer new authorization. [State machine and restart details](AUTOMATIC_INITIALIZATION.md).

高级命令仍保留，见[人工维护说明](ADVANCED.md)。若官方 CLI 不在标准位置，可设置 `sudo kairo runtime --codex /absolute/path/to/codex`。keyring-only 等非文件 OAuth 需要管理员使用受支持的官方 CLI 登录方式，不绕过权限或手工转抄令牌。测试仅使用模拟账号，不发送真实模型请求。

## 更新与回滚 / Updates and rollback

菜单“检查更新/升级程序 → 下载并自动升级”或：

```sh
sudo kairo update online
```

安装时保留的本地安装器查询GitHub latest稳定Release，校验完整清单/目标架构包、包内二进制、版本与安装后哈希。校验失败不停止旧服务；验证成功后才停止原本活动的服务，复用Rust持久化安装事务，成功后恢复原服务状态。失败尝试恢复旧版本；恢复失败必须报错且不启动错误哈希的程序。显式在线降级被拒绝。重复同版本同字节安装幂等。下载新installer脚本不会替代已安装入口，直到其所属发行包通过校验。

The retained installer checks the latest stable GitHub release, package/binary hashes and version before stopping an active service. It reuses the existing durable Rust transaction and restores the previous service state. Errors attempt recovery; failed recovery stays an error and never starts a binary with the wrong expected hash. Online downgrades are refused. Same-byte reinstall is idempotent.

```sh
sudo kairo pause
sudo systemctl stop kairo.service
sudo kairo update rollback --prefix /opt/kairo
sudo kairo update recover --prefix /opt/kairo
```

回滚保留当前请求历史，不自动批准调度；检查状态后决定是否恢复服务。卸载可从菜单选择并输入uninstall确认，或：

```sh
sudo kairo system-uninstall --yes
```

停止并移除服务、全局命令和Kairo程序，保留状态、独立用户、OAuth和请求账本以便安全重装。不会删除官方Codex。不要为了重装清空账本。

## 验收范围 / Validation scope

既有发行版/QEMU安装矩阵不重复。此次入口修改需要单独的下载校验、幂等、更新/失败恢复、卸载及干净systemd来宾回归，结果以本轮交付报告为准。aarch64采用原生构建；不是物理硬件验收。Debian12/13、Ubuntu22.04/24.04、Alma/Rocky9通过运行探测后可安装；Alma/Rocky10暂拒绝自动首装，避免把minimal依赖的有限证据冒称完整支持。

### v0.2.1真实公开入口结果 / Actual public-entry result

Debian12 x86_64干净QEMU TCG来宾（1vCPU/1024MiB）从真正GitHub公网下载脚本、对应架构包及官方Codex依赖，端到端TLS/发布SHA256校验，验证systemd/global命令、中英文菜单、时间增删改及无OAuth时禁止请求。受限代理只转发真实GitHub/官方Debian源，不替换文件或证书，不是本地回环发行模拟。此来宾完成缺curl时真实APT恢复、兼容旧RC与正式包不同哈希间的在线更新/回滚、损坏SHA256拒绝；历史不兼容0.2.0验证拒绝及自动恢复，不宣称成功迁移。

Debian12 x86_64 public installation passed in a fresh QEMU TCG guest over real public GitHub TLS. Missing-curl APT recovery, compatible RC-to-release binary updates/rollback, checksum rejection and observation-only systemd were checked. The proxy only forwards actual approved upstream traffic; it neither serves fixture releases nor replaces TLS certificates. Historical0.2.0 is incompatible and was tested for safe refusal/recovery. Other OS/ARM64 public-entry installs and physical hardware remain unverified; native ARM64 build/test is not an online system-install pass. Real OAuth/MFA remains installer-controlled and was not performed during acceptance.

## 管道入口 / Streaming entry points

首页安装和卸载命令固定且不带版本。保留高级固定版本：

```sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/hyd1aa/kairo-codex-scheduler/main/install.sh | sudo bash -s -- --version 0.2.4'
```

不使用 `sudo -S` 读取密码，以免与脚本源流混用。sudo正常从终端读取密码；安装器不从脚本 stdin 读取认证或确认，只在存在真实控制终端时使用独立终端描述符。网络/下载/校验失败返回非零，HUP/INT/TERM分别传播129/130/143并清理已建立的私有暂存目录；SIGKILL或断电不能执行shell清理钩子，重跑依靠已有持久化恢复，不宣称能截获SIGKILL。

Use the homepage pipefail commands for reliable pipeline status. Plain `curl | bash` cannot detect a producer that fails before sending a script. No installer prompts consume the script stream. The complete function is parsed before any installation/removal action; child commands receive /dev/null. Official OAuth remains a human authentication operation, guided during interactive installation or through menu03 later. Existing v0.2.1 binaries/releases are immutable: rerunning the current online bootstrap also refreshes its managed maintenance helpers without replacing user data, even when the binary is already current.

## 高级卸载 / Advanced removal

默认卸载保留凭据、配置、请求历史和专用用户。只有明确决定永久清除这些数据时才使用：

```sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/hyd1aa/kairo-codex-scheduler/main/uninstall.sh | sudo bash -s -- --purge-data --confirm DELETE-KAIRO-DATA'
```

This explicitly deletes /var/lib/kairo, /var/lib/kairo-account-guards and /home/kairo/.codex, including authentication and request history. It keeps the dedicated user/ownership marker for safe reinstall and never removes the Codex CLI or shared dependencies. Do not purge to bypass safety history. Foreign unit/command replacements, unknown files or active unmanaged Kairo processes are refused/preserved and reported as incomplete removal; stop/review them before retrying. There is no implicit confirmation read from the download pipe.
