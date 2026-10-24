# Release preparation / 发行准备

GitHub Actions CI保留Rust fmt、Clippy、测试、Gitleaks、依赖审计和只读权限。手动release.yml在x86_64/aarch64原生runner构建，准备各架构tar.gz、裸程序、兼容组合包、install.sh、SPDX SBOM、准确源码metadata及SHA256SUMS。默认不签名、不创建标签或Release。

The manual candidate workflow builds both native architectures, packages installer/binaries/SBOM/source metadata and SHA256SUMS, and uploads a private candidate artifact. It neither signs nor publishes. Existing full CI remains available; changed installer paths have focused HTTPS/install/update regressions.

维护者批准公开及正式发行后：公开源码 → Free公开仓库配置main保护/Secret Scanning/PVR → 确认成功候选绑定准确main → 人工创建指向该commit的v0.2.1注释标签 → 从同一成功run创建草稿 → 下载验SHA256/内容 → 发布 → 匿名HTTPS及隔离干净系统安装验证。草稿workflow显式approved=false，仓库层默认禁用，自动流程没有静默公开权限。无需购买私库功能套餐。

After separate authorization: publish source, configure public protections/security reporting, bind a successful candidate to the exact source, create the approved tag, prepare a draft, verify assets, publish and test anonymous download/installation. The draft workflow defaults off and never makes the repository public or publishes a draft automatically.

发布说明只陈述真实测试类型，QEMU TCG/容器/模拟不能称物理硬件通过；未公开URL和未完成真实OAuth不得标为通过。固定HTTPS+SHA256是默认信任模型，不声称独立签名认证。高级签名工具仅可选，不是发布前置。
