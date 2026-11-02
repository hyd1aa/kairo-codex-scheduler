# v0.2.6 acceptance

All identities, quota responses, requests and OAuth renewal responses in acceptance are synthetic. No model requests or production credentials are used. Installation tests run in disposable Docker containers with network disabled and mocked systemd; this is not a claim of a real VM boot or successful real-provider inference.

| Requested scenario | Result and evidence |
| --- | --- |
| Login before install | Pass: `automatic::tests::login_before_or_after_install_uses_same_automatic_path`, login-first branch; verified quota injected at the official-query boundary. |
| Install before login | Pass: same test, login-later branch; absent credentials do not query quota, appearing credentials initialize automatically. |
| Fresh VPS | Pass: empty private data/guard directories initialize without a recovery timer. |
| Reinstall without history | Pass: `reinstall_oauth_ready_scheduled_request_and_durable_receipt_without_quiet_wait`; readiness, one scheduled mock request and persisted acceptance/confirmation task. |
| Complete old state | Pass: actual SHA256-verified v0.2.5 binary → new binary → rollback/repeat, five running/pause/recovery policies; state bytes retained. |
| Partial old state | Pass: missing cursor rebuilt from surviving records; existing attempts and schedules retained. Corrupt records are blocked and preserved. This is not a promise to repair every malformed legacy file. |
| Previously paused | Pass: explicit pause survives initialization, installation, update, uninstall/reinstall and rollback. |
| Five-hour window active | Pass: positive usage and zero-percent partial windows both skip without a request; the next configured opportunity follows reset_at. |
| Five-hour window expired | Pass: one request at the next scheduled slot, followed by durable outcome. |
| Five-hour quota exhausted | Pass: no request while the exhausted window remains active. |
| Weekly quota exhausted | Pass: no request. |
| Another request activated window | Pass: positive server usage with an active reset blocks warmup; no new activation is claimed. |
| Configured activation time | Pass: fake clock advances to a configured slot and triggers exactly once. |
| Two consecutive slots | Pass: `two_consecutive_scheduled_slots_and_restart_do_not_duplicate_a_window`; two expired eligible windows get one request each, repeated ticks do not duplicate. |
| Network/restart/service failure | Pass at mocked boundaries: network errors send no request; restart/resume and process leases tested; installer service-start failure restores prior binary. Actual boot and host-specific systemd behavior remain untested locally. |
| Account switching | Pass: fingerprint mismatch blocks requests; original records retained. |
| Single instance/window deduplication | Pass: daemon lease, account execution lease, shared account reservation, durable pre-request state, crash replay and failed-write tests. Cross-machine absolute deduplication is not claimed. |
| Post-request countdown evidence | Pass: confirmation tests use stable reset and decreasing remaining time; bilingual UI never inherits an earlier success after a later failure. |
| Rust CLI and bilingual terminal | Pass: CLI/process/transaction suite, bilingual PTY navigation, NO_COLOR, dumb terminal, small-screen, EOF, SIGINT and closed-pipe regressions. README menus are compared to the shipping binary. |
| No new login/authorization on upgrade | Pass: v0.2.5 state/OAuth paths retained. Managed token renewal is delegated to official CLI account/read with refreshToken=true after HTTP401; the renewal protocol test permits only initialize/initialized/account/read. No thread/turn/reset API is called. |

The installer additionally tests SHA256, TLS failure, malformed packages, wrong versions, interrupted downloads, state retention and uninstall. CI repeats current-binary installer acceptance and previous-release compatibility. Release candidates build each real architecture on its native runner.

The local system's timezone is Asia/Shanghai. Synthetic scheduling cases specify UTC and Asia/Shanghai explicitly. Default checks retain the configured safety buffer; a window expiring just after an anchor can make the following anchor ineligible. The UI reports a later configured opportunity instead of claiming a new window was activated.

Official login/MFA, current service-side behavior and activity on other machines cannot be proven by these mock tests. OAuth revoked by the provider, unavailable quota or ambiguous/corrupt local evidence safely stops requests with diagnostics.
