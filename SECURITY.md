# Security

Never post OAuth tokens, credentials, account journals, private signing keys or production state in public issues. Use synthetic reproductions and redact account identifiers.

This is an unpublished source candidate. A verified private security contact or GitHub private vulnerability reporting has not yet been deployed. The owner must deploy and test a confidential channel before public release; no address is invented here. See [release gates](docs/RELEASING.md). A SECURITY.md file alone does not enable GitHub reporting.

Installation does not authorize model requests. Real dispatch requires account identity/quota validation, explicit deployment approval, lifecycle checks and durable local/account reservations. Unknown outcomes are not automatically retried. Keep services stopped/observation-only during acceptance.

Signed installation requires an independently authenticated initial bootstrap digest and its embedded public key. Downloading a key, package and checksum from one untrusted source establishes no trust. Offline bootstrap does not discover key revocation automatically; authenticate replacement instructions through the owner's independent channel. See [key operations](docs/KEY_OPERATIONS.md).

Ordinary integrity and recovery tests cover fixed-file errors and verified installed bytes. Dynamic adversarial file-race reproduction was not completed; no penetration-test pass is claimed. A maintainer must review the documented residual coverage before release. Do not recreate restricted exploit tests.
