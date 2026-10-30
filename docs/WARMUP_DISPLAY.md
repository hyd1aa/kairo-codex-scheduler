# Warmup evidence / 预热结果证据

The CLI is a read-only projection of state.json attempts and the existing validated
confirmations.json ledger. It joins by account, anchor and request timestamp;
an older successful confirmation can never attach to a newer failed request.
The immediate five_hour_window_confirmed outcome describes an observed window
change, not a verified decreasing countdown. The delayed confirmation must exist
before the dashboard claims countdown confirmation. Time-correlated results keep
their attribution caveat. No extra model request or quota check is performed on
opening the dashboard.

最近预热时间采用已保存的请求完成时间，确认时间采用最后一条已校验确认样本。
失败原因只显示已有证据；旧记录没有保存更具体原因时明确说明，不猜测。
请求已成功但确认超时/达到检查次数上限时显示“倒计时未能确认”，不永久称为等待。
历史未知恢复基线、缺失/损坏/账号不匹配的记录不能当作从未预热或成功。

Existing natural-acceptance.json supplies the latest durable scheduled decision.
The receipt now continues updating after the first request, preserving both its
first and latest check. Saved request history and the latest saved decision are
listed; older skipped/blocked decisions that were never persisted cannot be
reconstructed. This change introduces no new ledger or state schema.

缓存按当前账号隔离，明确标注最近查询时间及过期状态；不把100%剩余解释为请求成功。
恢复复查只在 recovery_waiting 且未批准、期限仍在将来时显示。所有普通界面时间
按系统时区转为本地自然语言；原JSON保留原字段和时间表示。

Linux SIGPIPE uses an async-signal-safe _exit(0) handler, installed before CLI
output. A consumer closing stdout is ordinary output termination, not a business
failure or Rust panic. Open-pipe business errors retain their existing error JSON
and nonzero exit codes. Tests use a closed pipe, without installing jq.
