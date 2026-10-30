# Kairo

A lightweight Rust tool for Codex five-hour quota-window warmup. [中文](README.md) · [Stable release](https://github.com/hyd1aa/kairo-codex-scheduler/releases/latest)

## Core features

- Daily activation times, system timezone, idle-window warmup and delayed read-only confirmation.
- Five-hour/weekly quota and server-denial guards, global deduplication, persistence and restart recovery.
- Colorful bilingual menus, systemd operation, online updates and data-preserving uninstall.

Kairo **cannot reset, increase or bypass OpenAI limits**. Missing authorization, unknown quota or unmet safety gates prevent model requests.

## One-command installation

Copy this fixed command on a supported Linux VPS; it does not change when new versions ship:

```sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/hyd1aa/kairo-codex-scheduler/main/install.sh | sudo bash'
```

The installer selects the latest stable GitHub Release, excluding drafts/prereleases, detects OS and x86_64/ARM64, verifies HTTPS/SHA256/architecture/version, and prepares dependencies, a dedicated user and systemd. **No Rust/Cargo or hand-written unit is needed.** Existing installs are safely updated without replacing OAuth, configuration or request journals. Fresh installs stay stopped/observation-only.

The initial download requires curl, trusted CA certificates, bash and root/sudo access. `pipefail` is essential: bare `curl … | sudo bash` can return success from an empty bash when curl fails before sending any bytes. The wrapper propagates that failure. The complete script is parsed before execution; child processes cannot consume shell source as interactive input. Pinned-version options remain in the [installation guide](docs/INSTALL.md).

## First OAuth login and activation

1. An existing official Codex CLI is preserved; otherwise the installer downloads a pinned official native CLI with SHA256 verification.
2. Complete official device/browser authorization and MFA yourself:

```sh
sudo kairo auth login
sudo kairo auth verify
sudo kairo doctor
```

3. `06 → 08` is for a genuinely fresh binding with no legacy scheduler/recent requests: type `FRESH`. Never clear existing state to rebind. Retain the returned `account_fingerprint`; conservative initial waiting periods remain in force.
4. After reviewing the identity, quota and history, explicitly enable actual scheduling with that fingerprint:

```sh
sudo kairo activate --account 'paste-the-returned-fingerprint' --approve-production-cutover &&
sudo rm -f /etc/systemd/system/kairo.service.d/20-mode.conf &&
sudo systemctl daemon-reload &&
sudo systemctl enable --now kairo.service
```

This is the existing runtime safety acknowledgment, not a release approval workflow. Logging in or starting the service alone does not remove the initial observation override. Never copy another server's credentials/journals or run competing automatic schedulers on the same account. Development acceptance sends no real model requests.

## The complete kairo command menu

Run `kairo` as root, or `sudo kairo` as a non-root administrator. An interactive SSH terminal shows the colorful menu. Read/configuration actions also work for the service user with access to its private data; operations marked root require administration rights.

| Number | Actual menu name | Operation | Permission |
| --- | --- | --- | --- |
| 01 | Status | View timezone, identity, window, safety gates and next check | Data access |
| 02 | Codex quota | Query five-hour/weekly usage after OAuth; unknown quota stays protected | Data access/OAuth |
| 03 | Activation schedule | Open the time submenu below | Data access |
| 04 | Service management | 01 status,02 start,03 stop,04 restart; starting is not authorization | root |
| 05 | Logs | 20 readable records per page; refresh, page, filter warnings/errors and toggle details | root/log access |
| 06 | OAuth login and accounts | 01 local status,02 server verification,03 human login,04 profiles,05 add,06 select,07 remove profile,08 fresh binding | Data access/human OAuth |
| 07 | Diagnostics and repair | 01 diagnose;02 restore defaults while retaining request history | Data access |
| 08 | Prewarm settings | 01 view,02 idle toggle,03 buffer seconds,04 weekly/denial guards,05 lifecycle,06 request-free preview,07 reset | Data access |
| 09 | Check updates / upgrade | Usually03 automatic update;01/02 advanced local manifest/file operations,04 rollback,05 transaction recovery,06 check latest stable without upgrading | root |
| 10 | 中文 / English | Switch and save menu language | Data access |
| 11 | Help and project information | Show capabilities and safety notes | Data access |
| 12 | Uninstall Kairo (preserve state) | Confirm with `yes`, then `uninstall`; stop service and preserve data | root |
| 00 | Exit Kairo | Leave the menu without stopping a running service | Data access |

Example: enter `03`, then `02`, then `02:00` to add a time. Enter `00` in a submenu to return; `10` at the main menu switches language; `00` at the main menu exits. 

## v0.2.2 interactive panel

Number + Enter opens a separate page. `00` returns one level; only the main menu exits. After an action, Enter returns to the same submenu. Invalid input stays local. The logo appears only on the main page. Interactive redraw clears the visible screen, retains scrollback and keeps normal terminal input/echo. Use subcommands without a TTY; `NO_COLOR` disables colors.

```text
[ K A I R O ]
Codex Window Scheduler
v0.2.2 | Asia/Shanghai
Service: ● Running
Scheduling: Read-only observation
Automatic warmup: Not enabled

04 → Enter
Kairo / Service management
01. Status
02. Start service
03. Stop service
04. Restart service
00. Back

02 → Enter
✓ Current service state: active
Starting the service does not authorize automatic warmup
Press Enter to return to this menu...
```

`01` shows a summary with detailed local JSON in a separate option. `02 → 01` queries quota without model requests. `05` shows20 records per page within the latest200 records; detail length is also bounded. `11` groups help by topic. `09 → 06` checks the latest stable GitHub release without upgrading; select `09 → 03` to upgrade explicitly.

OAuth discovery, server verification, local binding and production approval are separate states. Missing, corrupt and unreadable deployment/history files receive explicit explanations. Start with OAuth verification and diagnostics above. Review account history before any fresh binding; never fabricate history or clear account guards. This panel does not implement a new historical-state recovery algorithm.

## Custom activation times (03)

Defaults are `06:00,11:00,16:00,21:00` in the **system timezone**, not a fixed timezone. You are not restricted to four activations: up to48 entries are supported with at least10 minutes between adjacent entries, including across midnight.

| Number | Actual submenu name | Operation |
| --- | --- | --- |
| 03 → 01 | List all times | Show configured times |
| 03 → 02 | Add time | Enter `HH:MM` |
| 03 → 03 | Edit time | Enter old time, then new time |
| 03 → 04 | Remove time | Enter the time to remove |
| 03 → 05 | Next actual check | Show the next planned check |
| 03 → 06 | Preview next 24 hours | Read-only preview; no model request |
| 03 → 07 | Restore default times | Restore the four defaults |

Direct equivalents:

```sh
sudo kairo schedule list
sudo kairo schedule add 02:00
sudo kairo schedule edit 02:00 03:30
sudo kairo schedule remove 03:30
sudo kairo schedule next
sudo kairo schedule preview
```

## Quota and running status

Use menu `01/02/05`, or:

```sh
sudo kairo status
sudo kairo quota
sudo kairo logs
```

Server denial, exhausted/unknown quotas, recent requests and uncertain outcomes prevent blind retries. Changing times or reinstalling must not bypass these guards.

## Updates

Open `kairo` (with sudo when needed), select `09 → 03`, or run:

```sh
sudo kairo update online
```

The latest stable release is selected automatically. Failed downloads/checksums never activate a new version. Existing durable Rust transactions preserve OAuth/configuration/journals and support recovery. See [maintenance](docs/INSTALL.md) for rollback and interrupted updates.

## One-command uninstall

```sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/hyd1aa/kairo-codex-scheduler/main/uninstall.sh | sudo bash'
```

Stops/disables the service, verifies the daemon/service children have stopped, and removes installer-managed units, binaries and the global command. Repeated removal is safe. By default it retains `/var/lib/kairo`, `/var/lib/kairo-account-guards`, `/home/kairo/.codex`, the dedicated user and ownership marker. Existing Codex CLI/shared dependencies remain untouched; reinstall preserves state. Unknown files are retained and failures/residuals are explicitly reported. Data purge is a [separately confirmed advanced option](docs/INSTALL.md#高级卸载--advanced-removal).

## Verified Linux systems and notes

Real systemd lifecycle acceptance uses an isolated **Debian12 x86_64 QEMU TCG** guest, not physical-hardware certification. x86_64/aarch64 binaries passed native builds. The installer also recognizes Debian13, Ubuntu22.04/24.04, Alma9 and Rocky9 with historical compatibility evidence, not a claim all passed this online lifecycle. ARM64 public first installation remains untested; Alma/Rocky10 automatic installation is unsupported.

Default trust relies on GitHub HTTPS/repository administration; SHA256 checks integrity. Actual OAuth/account behavior remains installer-controlled. Project code is [MIT](LICENSE); dependencies retain their terms: [notices](THIRD_PARTY_NOTICES.md), [provenance](docs/PROVENANCE.md), [security reporting](SECURITY.md). Only source developers need Rust/Cargo; this change does not alter the validated Rust scheduler core.
