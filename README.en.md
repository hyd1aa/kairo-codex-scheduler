# Kairo

A small Rust tool for scheduled Codex five-hour quota-window warmup and delayed confirmation. [中文](README.md)

**This is a private release candidate. No public stable Release is available yet. The online one-command installer will be advertised only after publication and anonymous download verification.** Users need neither Rust/Cargo nor a hand-written systemd unit.

## Install and use

The installer detects Linux and CPU architecture, prepares OS dependencies, downloads the matching prebuilt package from GitHub Releases and verifies SHA256. It creates a dedicated `kairo` user, global command and systemd service. If the official Codex CLI is missing, it installs a pinned official native binary with SHA256 verification; existing installations are preserved. A fresh Kairo installation remains stopped and observation-only until OAuth and safety checks are complete.

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

The entry supports Debian12/13, Ubuntu22.04/24.04, AlmaLinux9 and Rocky9 on x86_64/aarch64, subject to binary runtime checks and the documented [validation scope](docs/INSTALL.md). Alma/Rocky10 automatic first installation is not supported. QEMU TCG, container and mocked checks are not physical-hardware certification.

## Development and license

Only source builds need Rust/Cargo1.99.1. Set `CARGO_BUILD_JOBS=1` on small hosts. Tests use `KAIRO_OFFLINE=1`, synthetic accounts and no real credential mounts.

```sh
cargo build --release --locked
```

[MIT](LICENSE); dependencies retain their own terms. See [notices](THIRD_PARTY_NOTICES.md), [provenance](docs/PROVENANCE.md), [file inventory](PUBLIC_FILE_MANIFEST.json) and [security policy](SECURITY.md). Historical Python/Gist applications and private development history are excluded.
