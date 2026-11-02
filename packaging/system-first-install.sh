#!/bin/sh
# Provision a fresh dedicated Kairo account from an independently verified binary.
# The service remains disabled and in observation mode until human authorization.
set -eu
lang=${KAIRO_LANG:-zh-CN}
msg() { if [ "$lang" = en-US ]; then printf '%s\n' "$2"; else printf '%s\n' "$1"; fi; }
[ "$#" -eq 2 ] || { msg '用法：system-first-install.sh 已验证二进制 可信SHA256' 'usage: system-first-install.sh VERIFIED_BINARY TRUSTED_SHA256' >&2; exit 2; }
binary=$1
checksum=$2
[ "$(id -u)" -eq 0 ] || { msg '系统安装需要 root 权限' 'System installation requires root' >&2; exit 1; }
[ "$(uname -s)" = Linux ] || { msg '仅支持 Linux' 'Linux is required' >&2; exit 1; }
case "$(uname -m)" in
 x86_64) elf_machine=620;;
 aarch64|arm64) elf_machine=1830;;
 *) msg '不支持的处理器架构' 'Unsupported CPU architecture' >&2; exit 1;;
esac
[ -r /etc/os-release ] || { msg '无法识别 Linux 发行版' 'Cannot identify Linux distribution' >&2; exit 1; }
# /etc/os-release is supplied by the operating system, not the downloaded package.
. /etc/os-release
major=${VERSION_ID%%.*}
case "${ID:-}:$major" in
 debian:12|debian:13|ubuntu:22|ubuntu:24|almalinux:9|almalinux:10|rocky:9|rocky:10) ;;
 *) msg '不支持的 Linux 发行版或版本' 'Unsupported Linux distribution or version' >&2; exit 1;;
esac
case "${ID:-}:${VERSION_ID:-}" in
 ubuntu:22.04|ubuntu:24.04) ;;
 ubuntu:*) msg '不支持的 Ubuntu 版本' 'Unsupported Ubuntu version' >&2; exit 1;;
esac
[ -d /run/systemd/system ] && command -v systemctl >/dev/null 2>&1 || { msg '需要正在运行的 systemd' 'A running systemd is required' >&2; exit 1; }
[ -e /etc/localtime ] || [ -f /etc/timezone ] || { msg '系统时区未配置' 'System timezone is not configured' >&2; exit 1; }
printf '%s' "$checksum" | grep -Eq '^[0-9a-f]{64}$' || { msg 'SHA256 格式无效' 'Invalid SHA256 format' >&2; exit 2; }
[ -f "$binary" ] && [ "$(sha256sum "$binary" | cut -d ' ' -f1)" = "$checksum" ] || { msg '二进制 SHA256 校验失败' 'Binary SHA256 mismatch' >&2; exit 1; }
elf_magic=$(od -An -tx1 -N4 "$binary" | tr -d ' \n')
elf_class_endian=$(od -An -tu1 -j4 -N2 "$binary" | tr -d ' \n')
actual_machine=$(od -An -tu1 -j18 -N2 "$binary" | tr -d ' \n')
if [ "$elf_magic" != 7f454c46 ] || [ "$elf_class_endian" != 21 ] || [ "$actual_machine" != "$elf_machine" ]; then
 msg '二进制架构与系统不匹配' 'Binary architecture does not match this system' >&2
 exit 1
fi
if ! binary_version=$(timeout 10 "$binary" --version 2>/dev/null) || ! printf '%s\n' "$binary_version" | grep -Eq '^kairo [0-9]+\.[0-9]+\.[0-9]+$'; then
 msg '二进制无法在此系统运行或版本无效' 'Binary cannot run on this system or has an invalid version' >&2
 exit 1
fi
systemctl daemon-reload || { msg 'systemd 无法重新加载；未创建用户' 'systemd cannot reload; no user was created' >&2; exit 1; }
# Use the invoking user's official login directly; never copy OAuth credentials.
account_user=${SUDO_USER:-$(id -un)}
if [ -f /var/lib/kairo/runtime.json ] && [ ! -L /var/lib/kairo ]; then
 account_user=$(stat -c %U /var/lib/kairo)
fi
getent passwd "$account_user" >/dev/null || exit 1
mkdir -p /var/lib/kairo-installer
chmod 700 /var/lib/kairo-installer
printf 'schema=1\n' > /var/lib/kairo-installer/managed-user
chmod 600 /var/lib/kairo-installer/managed-user
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
"$script_dir/system-install.sh" "$binary" "$checksum" "$account_user"
msg '安装即授权 Kairo 在额度和安全条件允许时自动预热。未登录时，请以安装用户完成官方 Codex 登录，服务会自动检测。' 'Installation authorizes automatic warmup within quota and safety limits. Complete official Codex login as the installing user; the service detects it automatically.'
