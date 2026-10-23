#!/bin/sh
# Trust this program through an independent bootstrap digest before executing it.
# No network key download, TOFU, environment key override or unsigned fallback.
set -eu
[ "$#" -eq 3 ] || { echo 'usage: release-auth.sh VERSION DIRECTORY ASSET' >&2; exit 2; }
version=$1; source=$2; asset=$3
printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || exit 2
case "$asset" in
 "kairo-$version.tar.gz"|"kairo-$version-x86_64-unknown-linux-gnu"|"kairo-$version-aarch64-unknown-linux-gnu"|one-click.sh|bootstrap-signed.sh|signed-update.sh|release-auth.sh|sbom.spdx.json|release-metadata.json) ;;
 *) echo 'Unapproved release asset' >&2; exit 2;;
esac
stage=$(mktemp -d /tmp/kairo-auth.XXXXXXXX)
trap 'rm -rf "$stage"' EXIT
trap 'exit 130' HUP INT TERM
cat >"$stage/public.pem" <<'PUBLIC_KEY'
-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEAVN+fLWicPzLsIQdl0Zce1ghgLy9HirNi8gRMNMheoac=
-----END PUBLIC KEY-----
PUBLIC_KEY
manifest="$source/kairo-$version.sha256"
[ -f "$manifest" ] && [ -f "$manifest.sig" ] || exit 1
[ "$(wc -c <"$manifest")" -le 16384 ] || exit 1
openssl pkeyutl -verify -pubin -inkey "$stage/public.pem" -rawin -in "$manifest" -sigfile "$manifest.sig" >/dev/null 2>&1 || { echo 'Release signature rejected' >&2; exit 1; }
digest=$(awk -v name="$asset" '$2==name {n++; value=$1; if(NF!=2) bad=1} END {if(n!=1 || bad) exit 1; print value}' "$manifest") || exit 1
printf '%s' "$digest" | grep -Eq '^[0-9a-f]{64}$' || exit 1
[ -f "$source/$asset" ] && [ ! -L "$source/$asset" ] || exit 1
[ "$(sha256sum "$source/$asset" | cut -d ' ' -f1)" = "$digest" ] || { echo 'Release hash rejected' >&2; exit 1; }
printf '%s\n' "$digest"
