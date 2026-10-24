# Kairo

轻量级 Rust 工具：按每日激活时间管理 Codex 五小时额度窗口的预热与延迟确认。[English](README.en.md)

[下载 v0.2.1](https://github.com/hyd1aa/kairo-codex-scheduler/releases/tag/v0.2.1)。使用 GitHub HTTPS 下载与 SHA256 完整性校验，普通用户无需 Rust/Cargo 或手写 systemd unit。

## 安装与使用

安装器自动识别 Linux/CPU、准备系统依赖，从 GitHub Releases 下载对应的预编译包并校验 SHA256，创建独立 `kairo` 用户、全局命令及 systemd 服务。缺少官方 Codex CLI 时安装固定版本官方原生程序并核对 SHA256；已有 CLI 不覆盖。首次安装保持停用、观察模式，完成 OAuth 和安全检查前不会自动发模型请求。

管理员在支持的 Linux VPS 上复制这一条命令（版本明确为0.2.1，自动适配root/sudo）：

```sh
( set -eu; f=$(mktemp); trap 'rm -f "$f"' EXIT; curl -fsSL --proto '=https' --proto-redir '=https' --connect-timeout 15 --max-time 90 -o "$f" https://raw.githubusercontent.com/hyd1aa/kairo-codex-scheduler/main/install.sh; if [ "$(id -u)" -eq 0 ]; then bash "$f" --version 0.2.1; else sudo bash "$f" --version 0.2.1; fi )
```

最初下载需要curl和可信CA；脚本准备其余系统依赖。管理操作需要root权限：root用户输入 `kairo`，其他管理员输入 `sudo kairo`，不放宽账户/状态目录权限。

离线安装已取得并核验的候选包：`sudo bash ./install.sh --version 0.2.1 --source-dir /path/to/release`。无网测试可显式加 `--skip-codex`，这不会绕过 Kairo 校验，也不能用于跳过实际运行所需的 CLI。

```sh
kairo                              # 中英文管理菜单
kairo --lang en-US                  # 英文CLI；菜单语言可在菜单切换
kairo schedule add 02:00
kairo schedule edit 02:00 03:30
kairo schedule remove 03:30
kairo schedule preview
sudo kairo update online
sudo kairo system-uninstall --yes    # 停止服务并卸载，保留状态/账户数据
```

[首次 OAuth、启用、更新、回滚、卸载](docs/INSTALL.md)。首次登录使用官方 Codex CLI，在独立用户下人工完成浏览器/MFA；Kairo 不收集或上传凭据。菜单可查询额度、查看日志、管理时间、启动/停止服务、更新及卸载。

## 校验与安全

默认信任 GitHub HTTPS 和仓库/Release 管理权限，SHA256 用于文件完整性；**同源校验值不是独立发布者签名**。不要求用户维护签名密钥。原 Ed25519 签名工具仅为高级可选路径；签名路径失败不会自动退回默认路径，默认安装也不会忽略 SHA256 错误。

保留五小时/周额度检查、系统时区、空闲窗口预热、延迟只读确认、全局防重、持久化/重启恢复及服务端拒绝保护。未授权、未绑定、额度未知或部署安全门禁不满足时不发模型请求。Kairo **不能强制重置、增加或绕过 OpenAI 额度**；未知结果不会自动补发。

**当前公开入口验收：Debian12 x86_64，全新QEMU TCG来宾，真实GitHub公网HTTPS下载及systemd；不是物理硬件认证。** x86_64/aarch64预编译程序均通过GitHub原生构建和定向检查。安装器还识别Debian13、Ubuntu22.04/24.04、AlmaLinux9、Rocky9；这些目标有历史隔离兼容性证据，但不冒称全部通过本次公开入口首装。ARM64公开在线首装尚未实测。Alma/Rocky10暂不支持自动首装，详见[验收范围](docs/INSTALL.md)。

## 开发与许可

源码构建才需 Rust/Cargo1.99.1。资源有限时 `CARGO_BUILD_JOBS=1`；测试使用 `KAIRO_OFFLINE=1` 和合成账户，不挂载真实凭据。

```sh
cargo build --release --locked
```

[MIT](LICENSE)；第三方保留各自条款，见[通知](THIRD_PARTY_NOTICES.md)、[来源](docs/PROVENANCE.md)及[逐文件清单](PUBLIC_FILE_MANIFEST.json)。不分发早期Python/Gist应用源码或私人开发历史。[安全报告](SECURITY.md)。
