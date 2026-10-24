# Security

Never post OAuth tokens, credentials, account journals, private signing keys or production state in public issues. Use synthetic reproductions and redact account identifiers.

Use [GitHub private vulnerability reporting](https://github.com/hyd1aa/kairo-codex-scheduler/security/advisories/new) for confidential reports (sign-in required). It is enabled for this public repository. Do not report credentials or vulnerabilities through public issues. No email address is invented here; separate-account notification delivery has not been independently exercised.

Installation does not authorize model requests. Real dispatch requires account identity/quota validation, explicit deployment approval, lifecycle checks and durable local/account reservations. Unknown outcomes are not automatically retried. Keep services stopped/observation-only during acceptance.

Default installation trusts GitHub HTTPS and repository/Release control and verifies SHA256 integrity. A checksum obtained from the same source is not independent publisher authentication. Protect repository access and keep system CA certificates current. Existing signature tools are an explicitly optional path; signature failure never falls back to checksum-only installation. No real private signing key is needed or uploaded for the default release process.

Ordinary integrity and recovery tests cover fixed-file errors and verified installed bytes. Dynamic adversarial file-race reproduction was not completed; no penetration-test pass is claimed. A maintainer must review the documented residual coverage before release. Do not recreate restricted exploit tests.
