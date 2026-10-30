# Kairo

轻量 Rust 工具，自动预热 Codex 五小时额度窗口。[English](README.en.md)

安装 → 完成官方 Codex 登录 → 自动初始化和安全恢复 → 自动预热。Kairo 不会增加或突破官方额度。

## 一键安装

```sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/hyd1aa/kairo-codex-scheduler/main/install.sh | sudo bash'
```

固定命令自动获取最新稳定版、校验原生程序，准备 Codex CLI、专用用户和 systemd。**安装即授权在安全条件满足后自动预热，可随时在菜单04暂停。** 升级保留原有授权、暂停、时间设置、OAuth 和请求历史。

## 登录与自动恢复

已有服务账号 Codex 登录会直接复用。首次安装可在严格验证权限后安全导入执行安装的管理员自己的 Codex CLI OAuth，不打印令牌、不覆盖服务账号已有凭据。ChatGPT 网页登录不等于 VPS 上 Codex CLI 已授权。

交互安装缺少 OAuth 时打开官方设备登录；无交互终端时安装不会阻塞，运行 `sudo kairo`，进入 **03 账号管理** 完成登录。登录后后台继续初始化。

程序无法可靠检查其他机器。请一次确认该账号没有其他并发调度器，且恢复期间避免手动请求（**04 → 启动**，回答 `yes`）。这是完全无人值守的必要例外；非交互安装会安全等待这次确认。

缺失历史一律按未知处理，表面上全新的账号也不自动 FRESH。后台自动建立保守恢复（至少5小时6分30秒）、只读复查并在满足条件后启用；活动可能延长等待，网络失败自动退避。无需输入 FRESH / RECOVER / ENABLE，也无需到点执行命令。服务重启和升级保留进度；系统重启保留已确认的等待进度，不计无法验证的关机时间。损坏或属于其他实例的保护记录会阻止自动启用，需要修复，不会被静默覆盖。

## 简洁菜单

```sh
sudo kairo
```

| 编号 | 菜单 |
| --- | --- |
| 01 | 查看状态与额度 |
| 02 | 设置预热时间 |
| 03 | Codex 账号管理 |
| 04 | 启动 / 暂停预热 |
| 05 | 检查更新 |
| 06 | 高级设置 |
| 00 | 退出 Kairo |

数字加回车进入独立页面，00 返回。日志、诊断、维护恢复、参数、服务、语言、帮助和卸载位于高级设置。**04 的暂停同时禁止后台自动重新启用。**

## 设置时间、查看状态、更新与卸载

菜单02修改预热时间，默认按系统时区使用 06:00 / 11:00 / 16:00 / 21:00。原有高级 CLI 与 JSON 接口继续保留。

```sh
kairo status
kairo --json status
sudo kairo update online
```

一键卸载（停止服务，保留账号数据和 OAuth）：

```sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/hyd1aa/kairo-codex-scheduler/main/uninstall.sh | sudo bash'
```

仅 systemd active 不代表自动预热启用，请检查 `automatic_warmup_enabled: true`。v0.2.3 的旧观察安装通过菜单04选择一次自动启用；升级不会擅自新增授权或取消已有暂停。

[技术细节](docs/AUTOMATIC_INITIALIZATION.md) · [高级命令](docs/ADVANCED.md) · [安装详情](docs/INSTALL.md) · [最新发行](https://github.com/hyd1aa/kairo-codex-scheduler/releases/latest)
