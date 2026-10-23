# Contributing

Read [AGENTS.md](AGENTS.md). Use synthetic fixtures, mocked Codex and `KAIRO_OFFLINE=1`. Run `CARGO_BUILD_JOBS=1 sh scripts/verify.sh`. Formatting, clippy, workspace tests and CLI/process acceptance must pass. Safety changes need meaningful failure/recovery tests. Never clear request history to make a test pass.

Linux test prerequisites: Rust/Cargo 1.99.1, a C linker, Python3, OpenSSL, timezone data and python3-yaml. Fetch locked registry packages before offline verification. Builds can use the ordinary Cargo registry cache; no excluded private file or prebuilt Kairo binary is required. Cross-compilation additionally needs the Rust target and an appropriate target C linker.

`python3 tests/workflow_policy.py`, `python3 tests/release_inventory.py` and, as root in an isolated test workspace, `python3 tests/release_trust.py target/release/kairo` check candidate release tooling using disposable fixture keys. Never replace fixture keys with a production signing secret. Do not perform a host system installation.

The public root is an export checkpoint. Subsequent development commits are real new history; do not relabel the snapshot as the original Git development chain. First-party code is MIT; third-party terms remain intact. Publication and key-custody decisions belong to the owner.
