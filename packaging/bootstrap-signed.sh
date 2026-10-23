#!/bin/sh
# Initial script SHA256 MUST be verified through an independent owner channel.
set -eu
[ "$#" -eq 3 ] || { echo 'usage: bootstrap-signed.sh VERSION --source-dir DIRECTORY | VERSION --url HTTPS_BASE' >&2; exit 2; }
version=$1; mode=$2; source=$3
printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || exit 2
[ "$(uname -s)" = Linux ] && [ "$(id -u)" -eq 0 ] || exit 1
for tool in openssl sha256sum; do command -v "$tool" >/dev/null || { echo "Bootstrap prerequisite missing: $tool" >&2; exit 1; }; done
stage=$(mktemp -d /tmp/kairo-bootstrap-signed.XXXXXXXX)
trap 'rm -rf "$stage"' EXIT
trap 'exit 130' HUP INT TERM
case "$mode" in
 --source-dir) ;;
 --url) printf '%s' "$source" | grep -Eq '^https://[A-Za-z0-9.-]+(:[0-9]+)?(/[A-Za-z0-9._/-]*)?$' || exit 2; command -v curl >/dev/null || exit 1;;
 *) exit 2;;
esac
for asset in "kairo-$version.sha256" "kairo-$version.sha256.sig" one-click.sh release-auth.sh signed-update.sh "kairo-$version.tar.gz"; do
 if [ "$mode" = --source-dir ]; then cp "$source/$asset" "$stage/$asset"; else
  curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --tlsv1.2 --max-time 90 --max-filesize 104857600 -o "$stage/$asset" "${source%/}/$asset"
 fi
done
verify_asset() (
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

)
for trusted_script in one-click.sh release-auth.sh signed-update.sh; do verify_asset "$version" "$stage" "$trusted_script" >/dev/null; done
verify_asset "$version" "$stage" "kairo-$version.tar.gz" >/dev/null
cat >"$stage/trusted.pem" <<'PUBLIC_KEY'
-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEAVN+fLWicPzLsIQdl0Zce1ghgLy9HirNi8gRMNMheoac=
-----END PUBLIC KEY-----
PUBLIC_KEY
trust_dir=/opt/kairo/trust
if [ -e "$trust_dir" ] || [ -L "$trust_dir" ]; then
 [ -d "$trust_dir" ] && [ ! -L "$trust_dir" ] && [ "$(stat -c %u "$trust_dir")" = 0 ] && [ $((0$(stat -c %a "$trust_dir") & 022)) -eq 0 ] || exit 1
fi
for trusted_script in release-auth.sh signed-update.sh; do
 destination="$trust_dir/$trusted_script"
 if [ -e "$destination" ] || [ -L "$destination" ]; then
  [ -f "$destination" ] && [ ! -L "$destination" ] && [ "$(stat -c %u "$destination")" = 0 ] && [ $((0$(stat -c %a "$destination") & 022)) -eq 0 ] || exit 1
 fi
done
sh "$stage/one-click.sh" "$version" --source-dir "$stage" "$stage/trusted.pem"
# Retain authenticated root-owned update verifiers across binary version switches.
mkdir -p "$trust_dir"
chmod 755 "$trust_dir"
for trusted_script in release-auth.sh signed-update.sh; do
 temporary=$(mktemp "$trust_dir/.new.XXXXXXXX")
 cat "$stage/$trusted_script" >"$temporary"
 chmod 755 "$temporary"
 mv -fT "$temporary" "$trust_dir/$trusted_script"
done
sync -f "$trust_dir"
