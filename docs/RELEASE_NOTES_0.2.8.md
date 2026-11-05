# Kairo v0.2.8

Kairo now calls the official Codex CLI once per configured local time, independent of five-hour windows, quota caches and other-device history. Official quotas still apply; neither request success nor new windows are guaranteed.

- Removed deployment approval, account-wide guards, recovery, onboarding, quota gating and window-confirmation execution modules.
- One local schedule record retains pause, slot reservations, outcomes and a monotonic watermark. Durable reservation precedes process startup; no implicit retry, catch-up burst or overlapping invocation.
- Official `codex exec` uses read-only sandboxing, isolated empty work directory, ignored user config/rules, ephemeral JSON output, disabled shell/search and zero configured provider retries.
- Success requires both a successful CLI exit and valid completion evidence, and never claims a new quota window.
- Existing OAuth paths, schedules (including 06:00/11:02/16:04/21:06), delay, language, pause and historical evidence are retained. Existing CLI aliases remain available without authorization stages.
- Optional quota queries are informational. Menu times include seconds and the actual configured delay.

Tests use mock CLIs only, including schedule, failure, crash, pause, migration, menu and isolated installation cases. No production services or real OAuth are accessed, and no real model requests are sent.
