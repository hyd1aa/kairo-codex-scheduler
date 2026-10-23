# Kairo

Rust 编写的 Codex 五小时额度窗口调度器。[English](README.en.md)

当前为 0.2.1 未公开源码候选，不是已发布的 GitHub Release。新安装默认观察，服务不自动启动；授权和真实调度激活分别需要管理员操作。Kairo 不能增加、绕过或重置订阅额度。

## 安装与维护

普通二进制用户无需 Rust/Cargo。只有经独立渠道认证 bootstrap 摘要后，才运行其固定公钥签名安装链路。实际仓库和下载入口尚未部署，不能将示例当作已验收入口。

[首次安装、OAuth、升级和卸载](docs/INSTALL.md) · [安全报告](SECURITY.md) · [来源与许可](docs/PROVENANCE.md) · [发布流程](docs/RELEASING.md)

全局命令 `kairo` 打开菜单，`kairo --lang en-US` 使用英文。支持状态、五小时/周额度、动态激活时间、系统时区、服务管理、日志、OAuth、诊断及升级。例：

```sh
kairo status
kairo schedule list
kairo schedule add 02:00
kairo schedule edit 11:00 12:30
kairo schedule remove 21:00
kairo schedule preview
```

未知时区、账号不匹配、额度未知、未批准部署、历史不完整或时钟回拨都会阻止真实请求。请求先持久化预留；未知结果不会自动重发。延迟窗口确认只读查询，不补发模型请求。

## 从源码构建

Linux、Rust/Cargo 1.99.1、C 链接器、Python3、OpenSSL及测试所需python3-yaml。使用锁定依赖；资源有限时串行构建。

```sh
export CARGO_BUILD_JOBS=1 KAIRO_OFFLINE=1
cargo fetch --locked
sh scripts/verify.sh
```

测试只使用合成账户和模拟 Codex。`KAIRO_OFFLINE=1`限制Rust网络适配器，不是操作系统级网络隔离；不要向测试挂载真实凭据或模型执行器。测试/构建说明见 [开发](CONTRIBUTING.md)。

## 许可与支持范围

自有源码保留MIT，第三方继续适用各自条款，见[逐文件公开清单](PUBLIC_FILE_MANIFEST.json)、[NOTICE](NOTICE)及[第三方通知](THIRD_PARTY_NOTICES.md)。当前Rust与固定Gist比较有独立实现依据，未发现需要其作者许可的表达复用；不分发原脚本或早期Python实现。完整早期Python历史未重建，不声称取得了其再发布许可。最终公开仍需维护者确认自有代码许可权与发布身份。

既有隔离测试包括QEMU软件模拟、容器和离线模拟；不是物理硬件验收。此候选另行验证独立构建/测试，不扩大硬件支持承诺；AlmaLinux10 minimal完整系统首装仍未验证。
