#!/bin/sh
# Independently verified checksum required; no service start/enable or OAuth.
set -eu
kairo_lang=${KAIRO_LANG:-zh-CN}
msg() { if [ "$kairo_lang" = en-US ]; then printf '%s\n' "$2"; else printf '%s\n' "$1"; fi; }
[ "$#" -eq 4 ] || [ "$#" -eq 5 ] || { msg '用法：bootstrap.sh 所有者/仓库 版本 可信SHA256 安装目录 | --system 现有用户' 'usage: bootstrap.sh OWNER/REPO VERSION TRUSTED_SHA256 PREFIX | --system EXISTING_USER' >&2; exit 2; }
repo=$1
version=$2
checksum=$3
mode=local
if [ "$#" -eq 5 ]; then
 [ "$4" = --system ] || exit 2
 mode=system
 account_user=$5
else
 [ "$4" != --system ] || exit 2
 prefix=$4
fi
if ! printf '%s' "$repo" | grep -Eq '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$' || ! printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || ! printf '%s' "$checksum" | grep -Eq '^[0-9a-f]{64}$'; then msg '仓库、版本或校验值格式无效' 'Invalid repository, version or checksum' >&2; exit 2; fi
[ "$(uname -s)" = Linux ] || { msg '仅支持 Linux' 'Linux is required'; exit 1; }
case "$(uname -m)" in x86_64) target=x86_64-unknown-linux-gnu;; aarch64|arm64) target=aarch64-unknown-linux-gnu;; *) msg '不支持的处理器架构' 'Unsupported architecture'; exit 1;; esac
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
[ -x "$script_dir/install.sh" ] || { msg '请从校验后的 Kairo 发布包运行' 'Run from a verified Kairo release package'; exit 1; }
if [ "$mode" = system ]; then
 [ -x "$script_dir/system-install.sh" ] && [ -f "$script_dir/kairo.service" ] || { msg '发布包缺少系统安装文件' 'Release package is missing system installation files' >&2; exit 1; }
fi
command -v curl >/dev/null 2>&1 || { msg '缺少 curl；请先安装 curl，再运行已验证的发布包' 'curl is required; install curl before running the verified release package' >&2; exit 1; }
staging=$(mktemp -d /tmp/kairo-bootstrap.XXXXXXXX)
trap 'rm -rf "$staging"' EXIT
trap 'exit 130' HUP INT TERM
if ! curl --fail --silent --location --proto '=https' --proto-redir '=https' --tlsv1.2 --max-time 60 --max-filesize 104857600 --output "$staging/kairo" "https://github.com/$repo/releases/download/v$version/kairo-$version-$target"; then msg '发布文件下载失败' 'Release download failed' >&2; exit 1; fi
chmod 700 "$staging/kairo"
if [ "$mode" = system ]; then
 "$script_dir/system-install.sh" "$staging/kairo" "$checksum" "$account_user"
else
 "$script_dir/install.sh" "$staging/kairo" "$checksum" "$prefix"
fi
