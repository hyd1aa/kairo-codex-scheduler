# Kairo

A small Rust tool that automatically warms Codex five-hour quota windows. [中文](README.md)

Install → complete official Codex OAuth → automatic initialization and safe recovery → automatic warmup. Kairo never increases or bypasses official quotas.

## Install

```sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/hyd1aa/kairo-codex-scheduler/main/install.sh | sudo bash'
```

The fixed command selects the latest stable release, verifies the native binary and prepares Codex CLI, the dedicated service account and systemd. **Installing authorizes automatic warmup once safe. Pause any time in menu04.** Upgrades preserve existing authorization, pauses, schedules, OAuth and request history.

## Read the dashboard

```text
Automatic warmup: Enabled; running normally
Last warmup: Today 06:08
Warmup result: ✓ Request succeeded; five-hour countdown confirmed (time-correlated; attribution uncertain)
Next scheduled check: Today 11:00
Five-hour quota: Remaining 100.0%
Weekly quota: Remaining 60.0%
Quota last queried: Today 06:10 (cached, not live)
```

Illustrative layout only: all actual results, times and percentages come from saved evidence. **A running service or 100% remaining quota is not proof of a successful warmup.** A green result requires a successful request plus the existing validated countdown confirmation. Time correlation does not prove exclusive causation; other clients may affect the window.

Menu **01** refreshes quota read-only and shows the latest request, confirmation time, saved recent history and failure details. ○ means awaiting/unverified confirmation or an active window that needs no request; ! means a safety gate prevented execution; ✗ means the request failed. Missing old evidence says “No verifiable history”, never an invented success. Confirmed results survive restarts and upgrades.

“Next scheduled check” is a check, not a promise to send a model request. Completed recovery deadlines are hidden. Cached quota always shows its query time; old caches are labelled stale. Raw JSON and logs remain in **06 More settings**.

## Codex login and automatic recovery

The installer reuses an existing service-account Codex login. On a fresh installation it can securely import the invoking administrator’s private Codex CLI OAuth, without printing tokens or overwriting service credentials. Browser login is not VPS Codex CLI authorization.

In an interactive terminal, missing OAuth opens the official device login. Without a terminal, installation does not block: run `sudo kairo`, choose **03**, then login. After login, initialization continues in the background.

Other machines cannot be inspected reliably. Confirm once that this account has no competing schedulers, and avoid manual requests during recovery (**04 → Start**, answer `yes`). This confirmation is the only required exception to unattended enablement. Noninteractive installation waits safely for it.

Missing history is always unknown, including apparently new accounts. The program automatically starts conservative recovery (at least 5h6m30s), rechecks read-only quotas and enables scheduling when safe. Activity may extend the wait; network failures back off. No FRESH / RECOVER / ENABLE strings or deadline commands are needed. Service restart and upgrade preserve progress. After an OS reboot, credited waiting is retained; unverifiable downtime does not count. Corrupt or foreign guard records require repair and are never silently overwritten.

## Menu

```sh
sudo kairo
```

| Number | Menu |
| --- | --- |
| 01 | Quota and warmup history |
| 02 | Warmup schedule |
| 03 | Codex account |
| 04 | Start / pause warmup |
| 05 | Check updates |
| 06 | More settings |
| 00 | Exit Kairo |

Number + Enter opens a separate page; 00 goes back. Advanced settings contains logs, diagnostics, maintenance recovery, parameters, services, language, help and uninstall. **04 pauses automatic initialization as well as warmup.**

## Schedule, status, update and uninstall

Use menu02 to change warmup times. Defaults: 06:00 / 11:00 / 16:00 / 21:00 in the system timezone. Advanced CLI commands remain compatible.

```sh
kairo status
kairo --json status
sudo kairo update online
```

Uninstall (stops services, keeps account data and OAuth):

```sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/hyd1aa/kairo-codex-scheduler/main/uninstall.sh | sudo bash'
```

An active systemd service alone does not prove warmup is enabled: check `automatic_warmup_enabled: true`. Existing v0.2.3 observers opt in once via menu04; an upgrade never grants new authorization or cancels an existing pause.

[Technical details](docs/AUTOMATIC_INITIALIZATION.md) · [Advanced commands](docs/ADVANCED.en.md) · [Install details](docs/INSTALL.md) · [Latest release](https://github.com/hyd1aa/kairo-codex-scheduler/releases/latest)
