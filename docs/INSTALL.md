# 安装与维护 / Installation and maintenance

当前私有候选没有已经验收的公开下载入口。上线后才公布在线命令。默认采用GitHub HTTPS＋SHA256，不要求签名私钥或独立信任公告；同源SHA不提供独立发布者身份认证。必须信任仓库管理者及GitHub/TLS。高级签名链路保持可选且独立，失败不降级。

This private candidate has no validated public download endpoint. The default installer trusts GitHub HTTPS/repository control and verifies SHA256 integrity. It does not require offline signing. The advanced signed path remains separate with no fallback.

## 首次安装 / First installation

root或sudo、systemd、支持发行版、DNS/HTTPS及官方包源可用。下载入口命令本身需要curl和CA；脚本准备python3、curl、CA、tar/gzip、SHA256、用户管理、时区和锁等依赖。不安装Rust/Cargo。下载并验完正确架构包后才创建kairo用户/安装unit；原有未知命令或用户不会被覆盖。

Requires root/sudo, systemd and working DNS/HTTPS/package repositories. The initial download command needs curl/CA; the script prepares the other OS dependencies. No Rust/Cargo is needed. Package integrity is checked before provisioning the dedicated user and unit.

缺少Codex时从 `openai/codex` 官方GitHub Release安装原生CLI0.164.0，使用固定架构摘要验证，不运行登录或模型命令；现有 `/usr/local/bin/codex`、`/usr/bin/codex`、服务用户本地CLI不覆盖。CLI是独立第三方依赖，Kairo不在源码/发行包中转发它。安装器只管理自己安装的Kairo，卸载不删除官方CLI或用户认证。

If absent, the installer fetches official Codex0.164.0 from `openai/codex`, verifies its pinned architecture SHA256 and installs the native CLI. Existing usable installations are preserved. Codex is downloaded directly rather than bundled; Kairo uninstall preserves it and authentication data. [Official CLI documentation](https://developers.openai.com/codex/cli).

离线准备好发行目录时：

```sh
sudo bash ./install.sh --version 0.2.1 --source-dir /path/to/release --skip-codex
```

`--skip-codex`只用于已有CLI或无网准备，不绕过Kairo校验。首次服务停用/观察，不自动OAuth或真实调度。

## OAuth与启用 / OAuth and activation

```sh
sudo -u kairo -H kairo auth login
sudo -u kairo -H kairo auth verify
sudo -u kairo -H kairo doctor
sudo kairo
```

本人完成官方浏览器/MFA。登录凭据由官方CLI在服务用户HOME中管理，不复制其他服务器的auth.json或账本。若现有CLI位于其他绝对路径，用 `sudo -u kairo -H kairo runtime --codex /absolute/path/to/codex` 设置；keyring-only等凭证存储是否兼容须实际验证，失败应停住，不转抄凭据。

菜单“OAuth与账号管理”完成状态核验和绑定；新账户确认只适用于确实没有近期请求的账户，迁移账户不得清空历史冒充新账户。诊断、额度和账户/近期请求门禁满足后，管理员才明确启用调度并启动服务。`kairo activate --help`列出原有显式启用参数；安装和登录本身不构成真实请求授权。开发验收只使用模拟账户，首次真实OAuth仍需本人操作，不发送模型测试请求。

Complete browser/MFA yourself. Credentials stay with the official CLI under the service user's HOME. Use the account menu to verify/bind and review quota/history checks before explicitly enabling live scheduling and starting the service. A successful login is not itself permission to send requests. Never erase request history to impersonate a fresh account. Real OAuth remains a separate human acceptance item.

## 更新与回滚 / Updates and rollback

菜单“检查更新/升级程序 → 下载并自动升级”或：

```sh
sudo kairo update online
```

安装时保留的本地安装器查询GitHub latest稳定Release，校验完整清单/目标架构包、包内二进制、版本与安装后哈希。校验失败不停止旧服务；验证成功后才停止原本活动的服务，复用Rust持久化安装事务，成功后恢复原服务状态。失败尝试恢复旧版本；恢复失败必须报错且不启动错误哈希的程序。显式在线降级被拒绝。重复同版本同字节安装幂等。下载新installer脚本不会替代已安装入口，直到其所属发行包通过校验。

The retained installer checks the latest stable GitHub release, package/binary hashes and version before stopping an active service. It reuses the existing durable Rust transaction and restores the previous service state. Errors attempt recovery; failed recovery stays an error and never starts a binary with the wrong expected hash. Online downgrades are refused. Same-byte reinstall is idempotent.

```sh
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
