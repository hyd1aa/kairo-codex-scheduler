#!/bin/sh
# Local signed assets: download them separately; no credential-bearing HTTP.
set -eu
[ "$#" -eq 4 ] || { echo 'usage: signed-update.sh VERSION ASSET_DIRECTORY PREFIX DATA_DIRECTORY' >&2; exit 2; }
version=$1; source=$2; prefix=$3; data=$4
scripts=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
case "$(uname -m)" in x86_64) target=x86_64-unknown-linux-gnu;; aarch64|arm64) target=aarch64-unknown-linux-gnu;; *) exit 2;; esac
asset="kairo-$version-$target"
stage=$(mktemp -d /tmp/kairo-signed-update.XXXXXXXX)
trap 'rm -rf "$stage"' EXIT
trap 'exit 130' HUP INT TERM
# Authenticate a private snapshot; never pass a mutable download directory onward.
for name in "kairo-$version.sha256" "kairo-$version.sha256.sig" "$asset"; do
 cp "$source/$name" "$stage/$name"
done
digest=$(sh "$scripts/release-auth.sh" "$version" "$stage" "$asset")
chmod 700 "$stage/$asset"
[ "$("$stage/$asset" --version)" = "kairo $version" ] || { echo 'Signed release version rejected' >&2; exit 1; }
# Existing durable updater retains its rollback, ownership and request-history rules.
"$prefix/current/bin/kairo" --data-dir "$data" update apply "$stage/$asset" --sha256 "$digest" --prefix "$prefix"
