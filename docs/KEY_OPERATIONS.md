# Release key custody and trust

The public key is pinned in packaging/RELEASE-PUBLIC-KEY.pem and embedded in bootstrap-signed.sh/release-auth.sh. Its PEM-byte SHA256 must be independently authenticated by the owner; test signatures and a colocated public key do not establish publisher identity. The candidate key's publisher identity/custody confirmation remains pending.

Keep the real private key outside all checkouts, build artifacts, logs and images, in an owner-only directory/file. Use an encrypted offline backup and perform an owner recovery drill that compares only public fingerprints and signs a harmless challenge. Do not put a secret/passphrase in CLI arguments or shell tracing. The signer enforces private permissions, checkout exclusion and a matching pinned public key.

Planned rotation requires an exact statement with old/new PEM fingerprints, sequence, effective version and new bootstrap digest. When the old key remains trusted, cross-sign identical statement bytes with both keys and authenticate the transition independently. Update both embedded keys together and rerun affected trust/install checks; old installed verifiers reject new-key signatures until explicitly replaced through the authenticated bootstrap.

On compromise, halt signing/publishing, disable affected hosted secrets, and issue an independently authenticated revocation identifying affected fingerprints/releases and recovery steps. An old-key signature alone is insufficient once that key is compromised. Offline bootstrap has no automatic revocation lookup or expiry: an isolated old copy cannot learn revocation. Users must consult the independent owner channel and authenticate the replacement bootstrap. Do not claim automatic revocation enforcement.
