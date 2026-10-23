# Kairo development

- Never use production credentials/state in tests or send real model requests. Set KAIRO_OFFLINE=1 and use synthetic data and mocked executors.
- Use CARGO_BUILD_JOBS=1 on resource-limited machines. Do not install/start system services on a development host; use disposable guests for installation acceptance.
- Validate with scripts/verify.sh and tests/i18n_catalog.py. Preserve safety guards, durable request history, signature verification and fail-closed recovery.
- This repository begins with an explicitly documented source snapshot, not reconstructed upstream development history. Preserve SOURCE_ORIGIN.json and the public file inventory as snapshot evidence; regenerate inventory before an approved new export.
- Do not publish repositories/releases or use signing secrets without owner authorization. Never commit OAuth, private keys, account journals, machine images or private test logs.
