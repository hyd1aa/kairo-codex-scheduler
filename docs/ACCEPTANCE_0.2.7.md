# v0.2.7 CLI acceptance

Scope: Rust CLI presentation and interactive navigation only, plus patch-version packaging. Scheduling, official OAuth, quota queries, request ledger, systemd and safety implementations are unchanged from v0.2.6.

| Scenario | Evidence |
| --- | --- |
| Home 0–7 | Bilingual PTY sessions visit every action, one exit row, no padded numbers; 6 switches and persists language; 7 enters one-confirmation uninstall and safely cancels. |
| Secondary menus | PTY sessions return through every advanced page, schedule, start/pause, account, history and legacy maintenance wizard. |
| Continuous actions | Schedule add/edit/delete/reset/next/preview, pause/start, status and diagnostics execute against temporary synthetic data. |
| Invalid input | Empty, text, 00 and out-of-range inputs retry in the same page. |
| EOF / Ctrl+C | Home, schedule, account, update and uninstall confirmation exit without changing terminal modes. |
| Terminal compatibility | 24-column ioctl PTY, ANSI screen clearing, NO_COLOR, dumb terminal and noninteractive subcommand behavior checked. |
| Evidence display | Real binary with synthetic records verifies confirmed/pending/failed/unknown/unconfirmed results, account conflict/expired login, cache age and remaining quotas. No active-window filler on home. |
| Automation compatibility | Existing JSON status, schedule and all CLI/process/transaction tests retained. |
| Background regression | Actual v0.2.6 → v0.2.7 upgrade/rollback/repeat preserves state bytes, including paused installations. Existing scheduling, guards, update/rollback and installer suites retained; fmt, Clippy and Rust workspace tests required. |

Run `KAIRO_OFFLINE=1 CARGO_BUILD_JOBS=1 sh scripts/verify.sh`. Installation acceptance runs only in disposable containers with mocked systemd. No real OAuth, model requests or production services are used. Actual SSH clients and real-provider behavior are not claimed by PTY tests.
