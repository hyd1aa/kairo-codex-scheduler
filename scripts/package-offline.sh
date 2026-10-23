#!/bin/sh
# Public-source package, with no private audit/history dependency.
set -eu
[ "$#" -eq 3 ] || { echo 'usage: package-offline.sh X86_BINARY ARM_BINARY OUTPUT_TAR_GZ' >&2; exit 2; }
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
x86=$(realpath -- "$1"); arm=$(realpath -- "$2"); output=$(realpath -m -- "$3")
version=$(sed -n 's/^version = "\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\)"$/\1/p' "$repo/Cargo.toml" | head -1)
[ -n "$version" ] && [ -f "$x86" ] && [ -f "$arm" ] || exit 2
stage=$(mktemp -d /tmp/kairo-package.XXXXXXXX)
trap 'rm -rf "$stage"' EXIT
root="$stage/kairo-$version"
mkdir -p "$root/bin" "$root/packaging" "$root/docs"
cp "$x86" "$root/bin/kairo-x86_64-unknown-linux-gnu"
cp "$arm" "$root/bin/kairo-aarch64-unknown-linux-gnu"
# Artifact downloads may strip mode bits; the signed archive must restore them.
chmod 755 "$root/bin/kairo-x86_64-unknown-linux-gnu" "$root/bin/kairo-aarch64-unknown-linux-gnu"
cp "$repo"/packaging/*.sh "$repo/packaging/kairo.service" "$repo/packaging/RELEASE-PUBLIC-KEY.pem" "$root/packaging/"
for name in README.md README.en.md LICENSE NOTICE THIRD_PARTY_NOTICES.md AGENTS.md SECURITY.md CONTRIBUTING.md CHANGELOG.md SOURCE_ORIGIN.json PUBLIC_FILE_MANIFEST.json; do cp "$repo/$name" "$root/"; done
cp "$repo"/docs/*.md "$repo"/docs/*.tsv "$repo/docs/THIRD_PARTY_LICENSES.txt" "$root/docs/"
cp -R "$repo/licenses" "$root/"
(cd "$root" && sha256sum bin/kairo-aarch64-unknown-linux-gnu bin/kairo-x86_64-unknown-linux-gnu > SHA256SUMS)
mkdir -p "$(dirname -- "$output")"
tar --sort=name --mtime='UTC 2026-10-20' --owner=0 --group=0 --numeric-owner -C "$stage" -cf - "kairo-$version" | gzip -n > "$output"
sha256sum "$output"
