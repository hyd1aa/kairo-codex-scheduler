# Security

Never post OAuth tokens, credentials, account journals, private signing keys or production state in public issues. Use synthetic reproductions and redact account identifiers.

This is an unpublished source candidate. A verified private security contact or GitHub private vulnerability reporting has not yet been deployed. Enable and verify GitHub private vulnerability reporting after the repository becomes public, before promoting the first stable release; no address is invented here. See [release gates](docs/RELEASING.md). A SECURITY.md file alone does not enable GitHub reporting.

Installation does not authorize model requests. Real dispatch requires account identity/quota validation, explicit deployment approval, lifecycle checks and durable local/account reservations. Unknown outcomes are not automatically retried. Keep services stopped/observation-only during acceptance.

Default installation trusts GitHub HTTPS and repository/Release control and verifies SHA256 integrity. A checksum obtained from the same source is not independent publisher authentication. Protect repository access and keep system CA certificates current. Existing signature tools are an explicitly optional path; signature failure never falls back to checksum-only installation. No real private signing key is needed or uploaded for the default release process.

Ordinary integrity and recovery tests cover fixed-file errors and verified installed bytes. Dynamic adversarial file-race reproduction was not completed; no penetration-test pass is claimed. A maintainer must review the documented residual coverage before release. Do not recreate restricted exploit tests.
