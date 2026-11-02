# Kairo

A lightweight Rust tool that warms up Codex five-hour quota windows at configured times. [中文](README.md)

Install → detect official Codex login → stand by automatically → check quota and warm up when eligible.

## Install

```sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/hyd1aa/kairo-codex-scheduler/main/install.sh | sudo bash'
```

Downloads the latest stable GitHub release, verifies SHA256 and provisions a native binary with systemd boot startup. **Installing authorizes automatic warmup within quota and safety limits. Pause at any time in menu03.** Users complete official OAuth and MFA themselves.

New installations use the installing user's official Codex directory (SUDO_USER under sudo), without copying or editing official credentials. Login before installation is detected and verified. Login after installation is detected automatically; no restart, binding or enable command is needed. Run `codex login` as the installing user, or use menu04. Upgrades preserve the existing service user and OAuth directory.

## Scheduling

Missing local history does not trigger a fixed five-hour wait. Kairo verifies credential ownership, server account identity and both quota windows, then establishes a local baseline. Unverifiable quotas, changed accounts and explicit local scheduler conflicts stop requests with a reason.

Default local times are 06:00, 11:00, 16:00 and 21:00. Add, edit or remove them in menu02. If a window remains active at16:00, that slot is skipped and a later configured opportunity is selected. No implicit reset-time request is sent and skipping is never reported as successful activation. Each real request rechecks quota and persists its reservation first. Failed requests retain evidence and apply a safe backoff. Missed slots are not replayed in a burst after downtime.

The server can describe an idle window with zero usage and a fresh full duration. Warmup requires valid quota evidence and local duplicate guards. Kairo does not reset, accelerate or bypass official limits. Server quota cannot prove complete request history on other machines or identify every concurrent scheduler. Cross-machine absolute deduplication is not guaranteed.

## Menu

```sh
sudo kairo
```

| Number | Menu |
| --- | --- |
| 01 | Status and quota |
| 02 | Warmup schedule |
| 03 | Start / pause |
| 04 | Codex login and account |
| 05 | Updates / more settings |
| 06 | Exit |
| 00 | Exit Kairo |

The home screen shows enabled status, login, last request result, next opportunity and actual cached quota. Local times use today/tomorrow. Quota includes its read time; stale data is labelled. Chinese, English, SSH, tmux and small terminals use numbers plus Enter, with00 to return. Logs, service controls, language, diagnostics and uninstall are in menu05.

Success requires an actual successful request and subsequent countdown evidence. Until then the result remains pending or unconfirmed. Advanced diagnostics retain evidence and attribution limitations.

## Upgrade and compatibility

```sh
kairo status
kairo --json status
sudo kairo update online
```

Upgrades preserve schedules, OAuth paths, request/confirmation history, account guards and explicit pauses. Partial state is reconstructed from surviving valid records and verified server quota. Corrupt or mismatched records block execution and remain intact. Old recovery records remain as migration evidence; their fixed timer leaves the normal execution path. `onboard` and `enable` remain compatible start commands without production approval or exclusivity confirmation.

If v0.2.5's cached updater rejects missing records before loading the new binary, rerun the install command above to use the verified new updater. No recovery command or new OAuth is needed. Only absent primary records can use valid backups; corrupt originals are never silently replaced.

Uninstall preserves state by default:

```sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/hyd1aa/kairo-codex-scheduler/main/uninstall.sh | sudo bash'
```

## Development

```sh
KAIRO_OFFLINE=1 CARGO_BUILD_JOBS=1 sh scripts/verify.sh
```

Tests use synthetic identities, mocked official quotas and mocked requests, never real model quota. Installation acceptance belongs in disposable containers or guests, never production instances or credentials.

[MIT License](LICENSE) · [Source provenance](SOURCE_ORIGIN.json)

[Full mocked acceptance results](docs/ACCEPTANCE_0.2.6.md)
