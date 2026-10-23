# Kairo

A Rust scheduler for Codex five-hour allowance windows. [简体中文](README.md)

Version 0.2.1 is an unpublished source candidate, not a live GitHub Release. New installations observe only and never automatically enable/start the service. OAuth and live deployment approval are separate administrator steps. Kairo cannot increase, bypass or reset subscription allowances.

See [installation/OAuth/upgrade/uninstall](docs/INSTALL.md), [security](SECURITY.md), [provenance](docs/PROVENANCE.md) and [releasing](docs/RELEASING.md). Authenticate the bootstrap digest through an independent owner channel before executing the signed installer. No real public download endpoint is claimed.

`kairo` opens the bilingual menu. Commands include `status`, `schedule list/add/edit/remove/preview`, `quota`, `auth`, `doctor`, `service` and `update`. Use `--lang en-US` or `--lang zh-CN`. Unknown timezone, identity/quota failures, missing approval/history and backward clocks block requests. Durable reservations precede model execution; unknown outcomes are not replayed. Delayed window confirmation performs read-only checks without model retries.

Build on Linux with Rust/Cargo 1.99.1, a C linker, Python3, OpenSSL and python3-yaml:

```sh
export CARGO_BUILD_JOBS=1 KAIRO_OFFLINE=1
cargo fetch --locked
sh scripts/verify.sh
```

Tests use synthetic accounts and mocked executors only. The offline environment flag fences Rust adapters, not arbitrary external programs or the whole OS. Never mount real credentials into tests. See [contributing](CONTRIBUTING.md).

Independent project code retains MIT. Dependencies retain their own licenses; see [file inventory](PUBLIC_FILE_MANIFEST.json), [NOTICE](NOTICE), [third-party notices](THIRD_PARTY_NOTICES.md). Comparison against a fixed Gist supports independent Rust expression and identified no permission-requiring reuse. Neither that script nor historical Python code is distributed. Full early Python history remains unverified; no rights over it are claimed. Maintainer confirmation of first-party licensing authority and publication identity is still required.

Prior isolation evidence includes software-emulated QEMU guests, containers and mocks, not physical hardware certification. Independent candidate build checks do not expand that coverage. Full AlmaLinux10 minimal system installation remains unverified.
