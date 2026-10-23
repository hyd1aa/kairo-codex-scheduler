# Future release procedure

This is local preparation, not approval to create/push a repository, formal tag or Release. Choose the real repository identity only after owner approval. Keep the private evidence repository and its objects/bundles separate; use only the reviewed public snapshot/history.

## Owner gates

Confirm authority to license the included first-party code under existing MIT and the provenance record; uncertain historical Python material is excluded, not retroactively cleared. Gist permission is required only if actual protected reuse/distribution is identified, not merely for independent ideas/interfaces. Apache is a future owner licensing decision and is not applied here.

Confirm signing identity/custody, encrypted backup/recovery and an independent channel publishing the exact bootstrap digest and public-key fingerprint. Deploy a verified private reporting address or, once a real repository exists, enable GitHub private vulnerability reporting and test a harmless report from a second account, including confidential visibility and notification delivery. SECURITY.md alone does not enable it. Assign responsible maintainers. Accept or remediate the residual uncompleted adversarial file-race coverage through an allowed independent review; never rerun restricted exploit scripts. Human first OAuth/storage acceptance must leave services observation-only and model request count zero.

## Build and unsigned artifacts

Use Rust 1.99.1 and Cargo.lock. Set CARGO_BUILD_JOBS=1 and KAIRO_OFFLINE=1. Fetch Cargo registry packages as needed; no private files are required. Build each architecture natively or with an appropriate Rust target and C linker. `scripts/verify.sh` runs offline unit/CLI checks. Example packaging after both binaries exist:

```sh
sh scripts/package-offline.sh X86_BINARY ARM_BINARY OUTPUT_DIRECTORY/kairo-0.2.1.tar.gz
python3 scripts/release-sbom.py OUTPUT_DIRECTORY/sbom.spdx.json
```

The manual release.yml workflow builds both architectures, audits dependencies, checks sources/history for secret patterns, validates SBOM and uploads unsigned candidates. Optional signing is restricted to main and the release-signing environment. The separate publish.yml workflow defaults off and requires explicit owner approval/public-release environment, a successful exact-source candidate run, independently reviewed manifest hash and a pre-existing matching formal tag. It creates a draft only; it neither creates a tag nor publishes the draft. Naming an environment in YAML does not configure reviewers: enforce them in the actual repository before enabling secrets or dispatching signing/draft jobs.

Offline signing uses the same code as hosted signing:

```sh
python3 scripts/sign-release.py "$CANDIDATE" "$PRIVATE_KEY"
python3 scripts/verify-release.py "$CANDIDATE" --source "$REVIEWED_COMMIT" --manifest-sha256 "$REVIEWED_MANIFEST_SHA256"
```

A proper candidate also needs both raw binaries, version/source metadata, and SBOM. Signing copies trusted installer scripts and binds all nine assets. No real private signing key belongs in this source snapshot. SBOM is a lockfile inventory including build/dev/cross-target packages, not a per-binary link attestation.

## Only after real repository/endpoint deployment

Verify hosted dual-architecture jobs and audit/scan/SBOM results, branch/tag rules, reviewer protections, push protection and private reporting delivery. Reverify the approved draft/tag/source and every signed asset. Public Release publication requires separate owner approval. Only after public assets are online can unauthenticated real HTTPS download, redirects/errors, independent initial trust and both architecture paths be accepted. Local archives/localhost are not proof of a public endpoint. Keep model executors and credentials out of download/install test guests. No physical hardware claim follows from QEMU/container/mock results.
