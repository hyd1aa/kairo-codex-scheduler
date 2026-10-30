#!/bin/sh
set -eu
cargo fmt --check
python3 scripts/i18n-catalog.py --check
python3 tests/i18n_catalog.py
python3 tests/installer_serialization.py
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked
KAIRO_OFFLINE=1 python3 tests/acceptance.py target/release/kairo
KAIRO_OFFLINE=1 python3 tests/panel_navigation.py target/release/kairo
python3 scripts/check-secrets.py
python3 tests/release_secrets.py
for script in packaging/*.sh; do sh -n "$script"; done
