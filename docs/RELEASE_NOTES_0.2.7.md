# Kairo v0.2.7

Compact bilingual Rust CLI menus. No changes to scheduling, OAuth, quota APIs, request persistence, systemd or safety mechanisms.

- Seven home actions and one exit (`0`); all menus use single-digit choices and `0` to return.
- Language switching and uninstall move to home. Language preferences persist and refresh immediately.
- Compact evidence-backed last warmup and quota rows retain pending/failure/unknown results and cache age. Healthy login and active-window filler no longer take extra home rows.
- Schedule settings show current times; add/edit/remove/reset, next-check and preview remain available.
- Existing updater and uninstaller are reused. Uninstall needs one explicit confirmation and retains OAuth, settings, state and existing Codex CLI by default.
- JSON CLI and all background behavior remain compatible.

All model requests in tests are simulated. No CloudCone production instance is accessed or restarted during development.
