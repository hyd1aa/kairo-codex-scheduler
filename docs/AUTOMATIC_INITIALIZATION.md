# Automatic initialization / 自动初始化

The installer records intent separately from deployment approval. A root systemd
oneshot (`kairo-onboarding.service`) is scheduled by a small timer every minute.
It only drives existing read-only verification, recovery and verified cutover APIs.
It never forces a model request. OAuth stays in the dedicated service account.
A private existing administrator Codex CLI login may be imported on a fresh install
only: owner, mode, hardlink, size and no-follow checks are required. It is never
printed, passed as an argument, or copied over an existing service credential.

安装授权与实际部署授权分开持久化。后台每分钟推进已有只读校验、恢复及正式切换接口，
不强制模型测试。缺失历史不代表新账号；全新账号也使用保守恢复，避免自动 FRESH。
账号风险确认绑定到账号指纹。未知的其他机器并发风险必须明确确认一次。

## States / 状态

- No OAuth: wait for official device login / 无 OAuth：等待官方设备登录。
- No exclusivity confirmation: wait for one confirmation / 未确认独占：等待一次确认。
- Account changed: require renewed confirmation; old local history remains protected /
  账号变化：重新确认；旧账号本地状态不覆盖，需通过原有账号管理处理。
- Unknown history: persist recovery and check quotas every minute / 历史未知：建立恢复并自动复查。
- Network/quota refusal: bounded exponential backoff, no approval / 网络及额度拒绝：退避，不授权。
- Ready: existing enable transaction stops observer, revalidates all guards and
  removes only the unchanged installer observation override, verifies the new PID /
  满足条件：复用既有事务正式切换并验证新进程，失败回退。
- User pause: durable intent is cancelled before deployment is paused; timer cannot
  reenable it / 主动暂停：先取消自动启用意愿，再关闭部署授权。
- Corrupt/foreign guards: manual repair required; no automatic evidence overwrite /
  全局保护损坏或属于其他实例时需人工修复，不自动覆盖。

## Restart and upgrade / 重启与升级

Service restarts retain monotonic waiting. OS reboots retain credited progress
from the last persisted check, but cannot count unverifiable powered-off time.
Wall-clock jumps never shorten the remaining monotonic interval. This may delay
completion, but does not restart an already credited full wait. Additional
known guard/window deadlines still apply.

服务重启保留等待。系统重启保留上次持久化的已确认进度；无法验证的关机时间不计入。
仍会遵守更晚的已知请求保护期限。不能通过改系统时间或重启跳过等待。

Existing installations without an intent record keep the v0.2.3 authorization
policy: running stays running, paused stays paused, recovery remains unchanged.
Select menu04 once to authorize automatic continuation. Repeated installs/upgrades
do not replace intent, history, OAuth, deployment or an existing service override.

旧安装没有自动意愿记录时维持原有策略。菜单04选择一次自动接续；升级本身不新增授权。

Advanced maintenance: `kairo onboard --request --confirm-exclusive-account`
records the same explicit policy as menu04. `kairo onboard` is the privileged timer
entry and rejects offline tests or nonstandard installation roots. Maintenance
commands `recover`, `activate`, `enable`, JSON status and ordinary CLI remain.
