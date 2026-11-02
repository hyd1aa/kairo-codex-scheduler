# Kairo v0.2.6

Installation and official Codex OAuth are enough to start automatic scheduling. Missing local history no longer creates a fixed five-hour recovery wait. The Rust daemon verifies official quota, reconstructs a local baseline and waits for a configured opportunity.

- One daemon handles login detection, initialization and scheduling. The old root onboarding timer is retired.
- New installations use the invoking user's official login directory directly. Upgrades retain their existing service user, OAuth path, schedules, explicit pauses and request evidence.
- Active windows skip the current slot. The next opportunity is a configured time after the observed reset and duplicate guards, not an invented reset-time request.
- Six-item bilingual home menu; success requires a successful request and subsequent countdown evidence. Diagnostics retain technical evidence.
- Corruption, account changes, unverifiable quota and explicit local execution conflicts stop requests. Server quota cannot guarantee cross-machine deduplication.

All model execution in tests is mocked. Official OAuth/MFA remains a user action. Real provider behavior, host-specific systemd failures and activity on other machines remain external limits.
