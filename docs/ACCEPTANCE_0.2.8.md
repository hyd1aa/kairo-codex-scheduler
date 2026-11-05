# v0.2.8 acceptance

All model execution uses a marked synthetic Codex CLI with no network implementation. The real Rust scheduler launches that mock process; this is not only a mocked scheduler decision. No production OAuth or services are used.

| Requested case | Acceptance evidence |
| --- | --- |
| Fresh install | Empty state initializes local schedule; isolated installer enables service. |
| Login before install | Mock official login status is recognized; no deployment or quota is needed. |
| Login after install | A logged-out slot is recorded; later login executes the next slot without enable or restart. |
| Reinstall/no history | Missing old files do not gate a future slot or create recovery waiting. |
| Upgrade | Actual SHA256-verified previous binaries upgrade/rollback/repeat while preserving old records. |
| Custom times | 06:00/11:02/16:04/21:06 persist; 30-second delayed instants are shown explicitly. |
| On-time call | Fake clock crosses the configured instant; exactly one mock exec starts. |
| Active five-hour window | The scheduled mock exec still starts, independent of cached reset or use. |
| Expired five-hour window | Uses the same execution path. |
| Weekly exhausted | Mock official CLI returns weekly rejection; recorded without retry. |
| Duplicate slot | Repeated tick, clock rollback and repeated fixture processes start no second exec. |
| Restart/upgrade dedup | Persisted reservation/watermark survives restart and binary transactions. |
| Consecutive slots | Each configured slot receives one call, independent of five-hour spacing. |
| CLI failure | Exit failure, malformed JSON, tool events and missing completion evidence cannot show success. |
| Expired OAuth | Official CLI login failure/rejection is recorded, then waits for the next slot. |
| Pause | Pause survives startup, migration, upgrade and login; only explicit start resumes. |
| SSH disconnect | Foreground terminal is unrelated to systemd; daemon lease and parent-death cleanup are tested. |
| VPS restart | Startup sets a future baseline; old slots are not replayed. Actual VM boot is not claimed. |
| Bilingual menu | Real PTY navigation, 24-column width, NO_COLOR, non-TTY CLI, EOF and Ctrl+C. |
| Old/new history | Old files remain byte-identical; imported display records coexist with new durable slots. |

Additional tests cover file-lock exclusion, symlink/hardlink refusal, failed pre-exec persistence, process-group cleanup, quota-query independence and interrupted installer/update transactions. The existing SHA256, TLS, archive validation, secret and package checks remain.

Systemd installation acceptance uses disposable Docker containers and mocked systemctl; it cannot prove every real host boot or SSH client. Public official Codex 0.164.0 was SHA256-verified and checked only with `exec --help` under empty HOME/CODEX_HOME; this sent no model requests. Current flags and JSON completion evidence were checked against [official noninteractive documentation](https://learn.chatgpt.com/docs/non-interactive-mode), [CLI reference](https://learn.chatgpt.com/docs/developer-commands?surface=cli) and [configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference).

At-most-once local startup is conditional on retained local records and correct filesystem persistence. Records cannot guarantee cross-machine exclusion or survive deliberate data erasure. Calls remain subject to official limits, authentication and server availability.
