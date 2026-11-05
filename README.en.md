# Kairo

A lightweight Rust tool that calls the official Codex CLI once at each scheduled time. [中文](README.md)

**Kairo schedules Codex calls. It does not guarantee that a call starts a new five-hour window or succeeds. Official quotas and rate limits still apply.**

## One-command installation

```sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/hyd1aa/kairo-codex-scheduler/main/install.sh | sudo bash'
```

Downloads the latest stable release, checks SHA256 and provisions systemd boot startup. **Installation authorizes scheduled official Codex calls; pause at any time.** Users complete official OAuth and MFA themselves.

Existing login is reused as the installing user. If installed first, run `codex login` as that user or use menu 4. Later login works without binding, restart or enable commands. Kairo never copies, prints or uploads OAuth tokens or edits Codex user configuration.

## Scheduled calls

Defaults are 06:00, 11:00, 16:00 and 21:00 in the system timezone. Existing custom times survive upgrades, including 06:00, 11:02, 16:04 and 21:06. The default delay is 30 seconds: actual calls occur at 06:00:30, 11:02:30, 16:04:30 and 21:06:30. Menus and previews show the delay and seconds explicitly.

Each slot is independent. Active windows, reset_at, other devices and cached quota never gate execution. When unpaused, the official CLI is logged in and available, the local slot is unprocessed and no Kairo invocation overlaps, Kairo starts one `codex exec`.

Calls use an empty private working directory, read-only sandbox, ephemeral session, ignored user configuration/rules and disabled shell/search tools. OpenAI provider request and stream retries are explicitly set to zero. The fixed instruction asks for only “Kairo ping”, with no tools or file edits. Kairo performs no additional model validation or window confirmation. Success requires exit code zero, a nonempty assistant reply and a valid `turn.completed` usage event, with no failure or tool execution event.

`daemon.lock` excludes duplicate daemons and `call.lock` excludes overlapping invocations. The slot reservation and monotonic watermark are atomically persisted and fsynced in `schedule.json` before model process startup. Failure, crash or unknown outcomes never retry that slot. Processes have bounded lifetimes and parent-death cleanup. An overlapping call causes a recorded skip until the next slot.

Restart and upgrade schedule future slots only; downtime does not cause catch-up bursts. Network failures, expired login, rate limits and exhausted quota are recorded until the next slot. Explicit pause survives restart, upgrade and login until an explicit start.

Quota is optional read-only information. Cache time and stale markers remain visible; quota query failure never blocks calls. Neither quota nor CLI login status guarantees the next request will succeed. Cross-machine deduplication is not promised. Local at-most-once startup depends on intact durable local records and filesystem guarantees; erased data cannot preserve past evidence. A fresh installation starts with future slots.

## Menu

```sh
sudo kairo
```

| Number | Menu |
| --- | --- |
| 1 | Call history and quota |
| 2 | Call schedule |
| 3 | Start / pause |
| 4 | Codex login and account |
| 5 | Updates / advanced settings |
| 6 | 中文 / English |
| 7 | Uninstall Kairo |
| 0 | Exit |

Enter a single digit and press Enter. `0` returns; only home `0` exits. Press Enter after actions to return. Supports Chinese, English, NO_COLOR, SSH, tmux and narrow terminals. Noninteractive callers use subcommands or `--json`.

## Upgrade and compatibility

```sh
sudo kairo update online
sudo kairo --version
sudo kairo status
kairo --json status
```

Upgrades retain OAuth paths, times, delay, language, explicit pause and processed slots. Legacy deployment, window, account-guard and recovery records no longer authorize calls. Old call history is imported for display without rewriting originals. No deployment approval, FRESH, RECOVER, ENABLE or five-hour waiting is required.

`enable` and `activate` are resume aliases. `onboard --request` resumes; `onboard` reads readiness. Legacy `recover status/check/begin/wizard` report no recovery needed and do not change pause. Raw legacy import commands are retired; normal upgrades read the original data directory directly. The old idle-window field remains for compatibility and is ignored; `prewarm --buffer` still sets the call delay.

Binary rollback never rewinds new call records; rolling back to old code restores old behavior. Failed upgrade transactions also retain local slot records.

Uninstall preserves OAuth, configuration and state, including the existing Codex CLI:

```sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/hyd1aa/kairo-codex-scheduler/main/uninstall.sh | sudo bash'
```

## Development

```sh
KAIRO_OFFLINE=1 CARGO_BUILD_JOBS=1 sh scripts/verify.sh
```

Tests use synthetic state and explicitly marked mock CLIs. Offline mode rejects real Codex execution. Disposable installation containers mock systemd; no production service, real OAuth or model requests are used.

[Acceptance](docs/ACCEPTANCE_0.2.8.md) · [Release notes](docs/RELEASE_NOTES_0.2.8.md) · [MIT License](LICENSE) · [Source origin](SOURCE_ORIGIN.json)
