#!/bin/sh
# Signed release installer. The caller must obtain the publisher public key
# independently of the download source. Never starts the service or OAuth.
set -eu
lang=${KAIRO_LANG:-zh-CN}
msg() { if [ "$lang" = en-US ]; then printf '%s\n' "$2"; else printf '%s\n' "$1"; fi; }
[ "$#" -eq 4 ] || { msg '用法：one-click.sh 版本 --source-dir 目录 可信公钥 | --url HTTPS地址 可信公钥' 'usage: one-click.sh VERSION --source-dir DIRECTORY TRUSTED_PUBLIC_KEY | --url HTTPS_BASE_URL TRUSTED_PUBLIC_KEY' >&2; exit 2; }
version=$1; mode=$2; source=$3; pubkey=$4
printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || exit 2
[ "$(id -u)" -eq 0 ] || { msg '系统安装需要 root 权限' 'System installation requires root' >&2; exit 1; }
[ -f "$pubkey" ] && [ -s "$pubkey" ] || { msg '缺少独立可信的发布公钥' 'Independently trusted publisher public key is required' >&2; exit 1; }
case "$mode" in
 --source-dir) [ -d "$source" ] || { msg '发行文件目录不存在' 'Release source directory is missing' >&2; exit 1; };;
 --url) printf '%s' "$source" | grep -Eq '^https://[A-Za-z0-9.-]+(:[0-9]+)?(/[A-Za-z0-9._/-]*)?$' || { msg '仅允许 HTTPS 发布地址' 'Only HTTPS release URLs are allowed' >&2; exit 2; };;
 *) exit 2;;
esac
missing=0
for tool in openssl sha256sum tar gzip useradd getent; do command -v "$tool" >/dev/null 2>&1 || missing=1; done
[ -e /etc/localtime ] || [ -f /etc/timezone ] || missing=1
if [ "$mode" = --url ]; then
 command -v curl >/dev/null 2>&1 || missing=1
 [ -s /etc/ssl/certs/ca-certificates.crt ] || [ -s /etc/pki/tls/certs/ca-bundle.crt ] || missing=1
fi
if [ "$missing" -eq 1 ]; then
  if command -v apt-get >/dev/null 2>&1; then
   packages='openssl tar gzip coreutils tzdata passwd'
   if [ "$mode" = --url ]; then packages="$packages ca-certificates curl"; fi
   DEBIAN_FRONTEND=noninteractive timeout 180 apt-get update -qq &&
   DEBIAN_FRONTEND=noninteractive timeout 180 apt-get install -y -qq --no-install-recommends $packages || { msg '安装必要依赖失败' 'Failed to install required dependencies' >&2; exit 1; }
  elif command -v dnf >/dev/null 2>&1 || command -v microdnf >/dev/null 2>&1; then
   package_manager=dnf
   command -v dnf >/dev/null 2>&1 || package_manager=microdnf
   packages='openssl tar gzip coreutils tzdata shadow-utils'
   if [ "$package_manager" = microdnf ] && command -v rpm >/dev/null 2>&1 && rpm -q coreutils-single >/dev/null 2>&1; then
    packages='openssl tar gzip tzdata shadow-utils'
   fi
   if [ "$mode" = --url ]; then packages="$packages ca-certificates curl"; fi
   # Alma10 microdnf otherwise may spend the whole outer deadline on one mirror.
   # Keep HTTPS/GPG checks; bound each official repo so mirror fallback can proceed.
   if [ "$package_manager" = microdnf ] && grep -Eq '^ID="?almalinux"?$' /etc/os-release && grep -Eq '^VERSION_ID="?10([."]|$)' /etc/os-release; then
    timeout 180 "$package_manager" -y --setopt=baseos.timeout=10 --setopt=baseos.retries=0 --setopt=appstream.timeout=10 --setopt=appstream.retries=0 --setopt=extras.timeout=10 --setopt=extras.retries=0 install $packages || { msg 'AlmaLinux 10 minimal 依赖准备失败；此环境暂不支持自动首次安装，未创建 Kairo 用户' 'AlmaLinux 10 minimal dependency provisioning failed; automatic first installation is unsupported in this environment; no Kairo user was created' >&2; exit 1; }
   else
    timeout 180 "$package_manager" -y install $packages || { msg '安装必要依赖失败' 'Failed to install required dependencies' >&2; exit 1; }
   fi
  else
   msg '缺少受支持的系统包管理器' 'No supported system package manager' >&2
   exit 1
  fi
fi
for tool in openssl sha256sum tar gzip useradd getent; do
 command -v "$tool" >/dev/null 2>&1 || { msg "缺少依赖：$tool" "Missing dependency: $tool" >&2; exit 1; }
done
case "$mode" in
 --source-dir) ;;
 --url)
  command -v curl >/dev/null 2>&1 || { msg '缺少 curl；请先安装系统依赖' 'curl is required; install the system dependency first' >&2; exit 1; }
  host=${source#https://}; host=${host%%/*}; host=${host%%:*}
  getent ahosts "$host" >/dev/null 2>&1 || { msg '发布源 DNS 解析失败' 'Release source DNS resolution failed' >&2; exit 1; }
  ;;
 *) exit 2;;
esac
archive="kairo-$version.tar.gz"
manifest="kairo-$version.sha256"
signature="$manifest.sig"
stage=$(mktemp -d /tmp/kairo-signed-install.XXXXXXXX)
trap 'rm -rf "$stage"' EXIT
trap 'exit 130' HUP INT TERM
for name in "$manifest" "$signature" "$archive"; do
 if [ "$mode" = --source-dir ]; then
  [ -f "$source/$name" ] || { msg "发行资源缺失：$name" "Release asset is missing: $name" >&2; exit 1; }
  cp "$source/$name" "$stage/$name"
 else
  if ! curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --tlsv1.2 --max-time 90 --max-filesize 104857600 --output "$stage/$name" "${source%/}/$name"; then
   msg "下载失败或 TLS 校验失败：$name" "Download or TLS validation failed: $name" >&2
   exit 1
  fi
 fi
done
if ! openssl pkeyutl -verify -pubin -inkey "$pubkey" -rawin -in "$stage/$manifest" -sigfile "$stage/$signature" >/dev/null 2>&1; then
 msg '发布签名无效或公钥不可信' 'Release signature is invalid or the public key is untrusted' >&2
 exit 1
fi
expected=$(awk -v name="$archive" '$2==name {n++; value=$1} END {if(n==1) print value; else exit 1}' "$stage/$manifest") || { msg '签名清单中归档条目数量无效' 'Signed manifest has an invalid archive entry count' >&2; exit 1; }
printf '%s' "$expected" | grep -Eq '^[0-9a-f]{64}$' || { msg '签名清单缺少有效归档哈希' 'Signed manifest lacks a valid archive hash' >&2; exit 1; }
[ "$(sha256sum "$stage/$archive" | cut -d ' ' -f1)" = "$expected" ] || { msg '发行归档 SHA256 不匹配' 'Release archive SHA256 mismatch' >&2; exit 1; }
tar -tzf "$stage/$archive" | awk -v root="kairo-$version/" '
  $0 != root && index($0, root) != 1 {bad=1}
  /(^\/|\/\.\.\/|\/\.\.$|\/\/)/ {bad=1}
  END {if (NR==0 || bad) exit 1}
' || { msg '发行归档包含不安全路径' 'Release archive contains an unsafe path' >&2; exit 1; }
tar -xzf "$stage/$archive" -C "$stage"
root="$stage/kairo-$version"
[ -f "$root/SHA256SUMS" ] && [ -x "$root/packaging/system-first-install.sh" ] || { msg '签名发行包内容不完整' 'Signed release package is incomplete' >&2; exit 1; }
(cd "$root" && awk '
  $2 != "bin/kairo-aarch64-unknown-linux-gnu" && $2 != "bin/kairo-x86_64-unknown-linux-gnu" {bad=1}
  length($1)!=64 || $1 !~ /^[0-9a-f]+$/ {bad=1}
  {seen[$2]++; count++}
  END {if (bad || count!=2 || seen["bin/kairo-aarch64-unknown-linux-gnu"]!=1 || seen["bin/kairo-x86_64-unknown-linux-gnu"]!=1) exit 1}
' SHA256SUMS) || { msg '发行包二进制清单无效' 'Bundled binary manifest is invalid' >&2; exit 1; }
(cd "$root" && sha256sum -c SHA256SUMS) || { msg '发行包二进制哈希不匹配' 'Bundled binary SHA256 mismatch' >&2; exit 1; }
case "$(uname -m)" in
 x86_64) target=x86_64-unknown-linux-gnu;;
 aarch64|arm64) target=aarch64-unknown-linux-gnu;;
 *) msg '不支持的处理器架构' 'Unsupported CPU architecture' >&2; exit 1;;
esac
binary="$root/bin/kairo-$target"
[ -x "$binary" ] && [ "$("$binary" --version)" = "kairo $version" ] || { msg '二进制版本或架构不匹配' 'Binary version or architecture mismatch' >&2; exit 1; }
digest=$(sha256sum "$binary" | cut -d ' ' -f1)
"$root/packaging/system-first-install.sh" "$binary" "$digest"
