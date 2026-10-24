# Kairo 0.2.1 — release candidate / 发行候选

轻量Rust Codex五小时额度窗口预热工具：中英文菜单、每日激活时间、时区、五小时/周额度与服务端拒绝保护、持久化防重、延迟只读确认及重启恢复。不能强制重置、增加或绕过OpenAI额度。

A small Rust Codex quota-window warmup scheduler with bilingual menus, configurable daily times, quota/denial guards, durable deduplication and restart recovery. It cannot increase or bypass OpenAI limits.

新增GitHub HTTPS＋SHA256默认在线安装及菜单更新，按CPU下载原生包，自动准备依赖/独立用户/systemd；缺少官方Codex时下载经固定SHA256核验的官方原生CLI。首次服务停用/观察，OAuth及安全检查通过后才由管理员启用。签名工具仅为可选路径；SHA256完整性不冒充独立发布者身份。

Adds HTTPS/SHA256 installation and menu updates, architecture packages, dependency/user/unit provisioning and a pinned official Codex CLI when missing. Fresh installs stay stopped/observation-only until human OAuth and activation checks. Advanced signatures are optional.

修复ARM64下硬编码O_NOFOLLOW，改用libc常量；新增固定文件拒绝回归。更新复用原Rust事务，失败恢复旧程序，保留用户状态/请求历史；可从菜单卸载及重装。

Fixes ARM64 O_NOFOLLOW portability. Updates reuse durable transactions and preserve request history. Menu uninstall preserves account/state for safe reinstall.

范围：现有QEMU TCG、容器、原生构建及模拟证据，不是物理硬件认证。Alma/Rocky10自动首装暂不支持。真实首次OAuth及公开下载入口仍须实际验收；当前候选尚未公开发布。先前受平台限制的动态竞态测试未完成，普通固定文件/校验/恢复检查不能证明所有恶意并发行为不存在。MIT，第三方许可保留。
