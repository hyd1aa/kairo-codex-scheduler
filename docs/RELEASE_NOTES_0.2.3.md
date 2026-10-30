# Kairo v0.2.3

Fix existing accounts that cannot resume safely after reinstalling without deployment/history/guard records.

- New `recover status/begin/check/wizard` commands and menu06→09 distinguish missing OAuth, unknown history, corrupt data, pending recovery and missing approval. Complete account history cannot be inferred from quota alone; existing users must never choose FRESH.
- Persist an account-wide unknown-history barrier for at least5h6m30s; fresh official read-only identity/quota checks and new activity can extend it. Valid requests/anchors remain intact, damaged local bytes are archived privately, and corrupt global guards are never silently replaced.
- Explicit `enable --approve-production-cutover` completes guarded recovery, handles the unchanged managed observation override and verifies fresh live status for the current systemd PID. Failure revokes approval/restores observation; custom service overrides are preserved.
- Doctor/status separately report existing-record integrity, complete scheduler readiness, production approval and confirmed live operation.

No forced model tests, quota endpoint changes or unrelated scheduling redesign. Once an administrator explicitly enables production, future eligible natural checks may send real warmup requests. History before recovery stays unknown, and account locks cover competing data directories on the same host; operators must stop other-machine schedulers/manual requests during recovery. Existing configuration, OAuth and durable account/request protection are preserved.

Upgrade: `sudo kairo update online`. Existing-account recovery: `sudo kairo recover wizard` or menu06→09. Review the bilingual README before approving production. Publishing this release does not upgrade or activate the development server.
