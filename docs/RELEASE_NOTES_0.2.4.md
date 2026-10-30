# Kairo v0.2.4

安装、登录后由后台自动接续初始化、保守等待、只读复查及正式启用。安装入口明确授权用途；无法验证其他机器的并发使用时保留一次账号独占确认。历史缺失始终视为未知，不自动 FRESH，不伪造请求历史。

- 新增轻量 systemd 初始化定时任务、持久化启用意愿及网络退避。
- 主菜单简化为六项，维护工具移入高级设置，中英文同步。
- 用户暂停取消后台自动启用；账号变化需要重新确认。
- 已有安装升级保留运行、暂停、恢复进度、OAuth、配置及全局请求账本。
- 系统重启保留已确认等待进度；不计无法验证的关机时间。
- 首次安装安全复用服务账号或执行安装的管理员自己的私有 Codex CLI OAuth。

升级：固定一键安装命令或 `sudo kairo update online`。从旧安装首次选择菜单04时自动补齐定时任务；已正式运行的旧安装无需重新授权。没有自动启用意愿的旧观察/暂停安装不因升级擅自启用，选择菜单04一次即可自动接续。

Development and acceptance use synthetic accounts, clocks, quotas and mocked services. No real Codex model requests. Automatic recovery cannot prove other machines stopped or reconstruct unknown history; corrupt or foreign guards remain fail-closed.
