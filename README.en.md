# Kairo

A small Rust tool for scheduled Codex five-hour quota-window warmup and delayed confirmation. [中文](README.md)

[Download v0.2.1](https://github.com/hyd1aa/kairo-codex-scheduler/releases/tag/v0.2.1). Installation uses GitHub HTTPS and SHA256 integrity checks. Users need neither Rust/Cargo nor a hand-written systemd unit.

## Install and use

The installer detects Linux and CPU architecture, prepares OS dependencies, downloads the matching prebuilt package from GitHub Releases and verifies SHA256. It creates a dedicated `kairo` user, global command and systemd service. If the official Codex CLI is missing, it installs a pinned official native binary with SHA256 verification; existing installations are preserved. A fresh Kairo installation remains stopped and observation-only until OAuth and safety checks are complete.

Copy this single command as a VPS administrator (explicitly installs0.2.1 and handles root/sudo):

```sh
( set -eu; f=$(mktemp); trap 'rm -f "$f"' EXIT; curl -fsSL --proto '=https' --proto-redir '=https' --connect-timeout 15 --max-time 90 -o "$f" https://raw.githubusercontent.com/hyd1aa/kairo-codex-scheduler/main/install.sh; if [ "$(id -u)" -eq 0 ]; then bash "$f" --version 0.2.1; else sudo bash "$f" --version 0.2.1; fi )
```

The initial download requires curl and trusted CA certificates; the script prepares the remaining dependencies. Run `kairo` as root, or `sudo kairo` as a non-root administrator. Account/state permissions remain private.

For an already obtained and verified release: `sudo bash ./install.sh --version 0.2.1 --source-dir /path/to/release`. Isolated tests may explicitly use `--skip-codex`; it never bypasses Kairo integrity checks and does not remove the CLI requirement for actual use.

```sh
kairo
kairo --lang en-US                  # CLI language; switch menu language in the menu
kairo schedule add 02:00
kairo schedule edit 02:00 03:30
kairo schedule remove 03:30
kairo schedule preview
sudo kairo update online
sudo kairo system-uninstall --yes   # stops/removes service; retains account/state
```

[OAuth, activation, updates, rollback and removal](docs/INSTALL.md). Complete browser/MFA login through the official Codex CLI under the dedicated user. Kairo neither collects nor uploads credentials. The bilingual menu provides quotas, logs, schedules, service controls, updates and uninstall.

## Integrity and safety

Default trust relies on GitHub HTTPS and repository/Release administration; SHA256 checks file integrity, **not an independently authenticated publisher identity**. No release-key management is required. Existing Ed25519 tools remain an optional advanced path. A signature failure never triggers fallback, and SHA256 failures are never ignored.

Five-hour/weekly quota checks, system timezone, idle warmup, read-only delayed confirmation, global deduplication, durable restart recovery and server-denial protection remain intact. Unknown quota, missing authorization/binding or unmet activation gates block model requests. Kairo cannot reset, increase or bypass OpenAI quotas, and never blindly retries an uncertain request.

**Public-entry acceptance: Debian12 x86_64 in a fresh QEMU TCG guest, using real public GitHub HTTPS and systemd; this is not physical-hardware certification.** Both x86_64/aarch64 binaries passed native GitHub builds and focused tests. The installer also recognizes Debian13, Ubuntu22.04/24.04, AlmaLinux9 and Rocky9, with historical isolated compatibility evidence; those are not all claimed to have passed this public-entry test. ARM64 public online first installation remains untested. Alma/Rocky10 automatic installation is not supported. See [scope](docs/INSTALL.md).

## Development and license

Only source builds need Rust/Cargo1.99.1. Set `CARGO_BUILD_JOBS=1` on small hosts. Tests use `KAIRO_OFFLINE=1`, synthetic accounts and no real credential mounts.

```sh
cargo build --release --locked
```

[MIT](LICENSE); dependencies retain their own terms. See [notices](THIRD_PARTY_NOTICES.md), [provenance](docs/PROVENANCE.md), [file inventory](PUBLIC_FILE_MANIFEST.json) and [security policy](SECURITY.md). Historical Python/Gist applications and private development history are excluded.
